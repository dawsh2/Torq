//! Connection Pool for Transport Reuse
//!
//! High-performance connection pooling to avoid connection overhead and
//! maintain <35μs hot path operations.

use crate::{Result, TransportError};
use super::{Transport, TransportConfig, TransportType};
use async_trait::async_trait;
use bytes::Bytes;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Semaphore;

/// Connection pool for transport reuse with backpressure handling
pub struct ConnectionPool {
    /// Pool of active transports by key
    connections: Arc<RwLock<HashMap<String, PooledConnection>>>,
    
    /// Maximum connections per endpoint
    max_per_endpoint: usize,
    
    /// Maximum total connections
    max_total: usize,
    
    /// Connection idle timeout
    idle_timeout: Duration,
    
    /// Semaphore for limiting total connections
    semaphore: Arc<Semaphore>,
    
    /// Wait queue for when pool is exhausted
    wait_queue: Arc<tokio::sync::Mutex<Vec<tokio::sync::oneshot::Sender<()>>>>,
    
    /// Maximum time to wait for a connection
    max_wait_time: Duration,
}

/// Pooled connection wrapper with proper shared ownership
struct PooledConnection {
    transport: Arc<Box<dyn Transport>>,
    last_used: Instant,
    in_use: bool,
    created_at: Instant,
    /// Permit from semaphore to track resource usage
    _permit: Option<tokio::sync::OwnedSemaphorePermit>,
}

impl ConnectionPool {
    /// Acquire permit with wait queue support
    async fn acquire_permit_with_queue(&self) -> Result<tokio::sync::OwnedSemaphorePermit> {
        // Try to acquire immediately
        match self.semaphore.clone().try_acquire_owned() {
            Ok(permit) => Ok(permit),
            Err(_) => {
                // Add to wait queue
                let (tx, rx) = tokio::sync::oneshot::channel();
                {
                    let mut queue = self.wait_queue.lock().await;
                    queue.push(tx);
                }
                
                // Wait for notification or try periodically
                tokio::select! {
                    _ = rx => {
                        // Notified that a connection is available
                        self.semaphore.clone().acquire_owned().await
                            .map_err(|_| TransportError::resource_exhausted(
                                "connection_pool",
                                "Failed to acquire after notification"
                            ))
                    }
                    permit = self.semaphore.clone().acquire_owned() => {
                        permit.map_err(|_| TransportError::resource_exhausted(
                            "connection_pool",
                            "Maximum connections reached"
                        ))
                    }
                }
            }
        }
    }
    
    /// Notify waiting requests when a connection becomes available
    async fn notify_waiters(&self) {
        let mut queue = self.wait_queue.lock().await;
        while let Some(tx) = queue.pop() {
            if tx.send(()).is_ok() {
                // Successfully notified a waiter
                break;
            }
            // Receiver dropped, try next
        }
    }

    /// Create new connection pool
    pub fn new(max_per_endpoint: usize, max_total: usize, idle_timeout: Duration) -> Self {
        Self {
            connections: Arc::new(RwLock::new(HashMap::new())),
            max_per_endpoint,
            max_total,
            idle_timeout,
            semaphore: Arc::new(Semaphore::new(max_total)),
            wait_queue: Arc::new(tokio::sync::Mutex::new(Vec::new())),
            max_wait_time: Duration::from_secs(30),
        }
    }
    
    /// Get or create a connection with proper ownership and wait queue support
    pub async fn get_connection(&self, config: &TransportConfig) -> Result<Arc<Box<dyn Transport>>> {
        let key = Self::config_to_key(config);
        
        // Fast path: try to get an existing connection
        {
            let mut pool = self.connections.write();
            if let Some(conn) = pool.get_mut(&key) {
                if !conn.in_use && conn.transport.is_healthy() {
                    // Check if connection is not idle
                    if conn.last_used.elapsed() < self.idle_timeout {
                        conn.in_use = true;
                        conn.last_used = Instant::now();
                        // Return a clone of the Arc, properly sharing ownership
                        return Ok(Arc::clone(&conn.transport));
                    } else {
                        // Connection is idle, remove it
                        pool.remove(&key);
                    }
                }
            }
        }
        
        // Try to acquire a permit, with wait queue support
        let permit = match tokio::time::timeout(
            self.max_wait_time,
            self.acquire_permit_with_queue()
        ).await {
            Ok(Ok(permit)) => permit,
            Ok(Err(e)) => return Err(e),
            Err(_) => {
                return Err(TransportError::timeout(
                    "connection_pool_wait",
                    self.max_wait_time.as_millis() as u64
                ));
            }
        };
        
        // Create new transport
        let transport = match super::TransportFactory::create_transport(config.clone()).await {
            Ok(t) => Arc::new(t),
            Err(e) => {
                // Permit automatically released when dropped
                return Err(e);
            }
        };
        
        // Store in pool with proper Arc sharing
        let transport_clone = Arc::clone(&transport);
        {
            let mut pool = self.connections.write();
            
            // Check per-endpoint limit
            let endpoint_count = pool.iter()
                .filter(|(k, _)| k.starts_with(&key[..key.len().min(20)]))
                .count();
            
            if endpoint_count >= self.max_per_endpoint {
                // Permit automatically released when dropped
                return Err(TransportError::resource_exhausted(
                    "connection_pool",
                    format!("Maximum connections per endpoint ({}) reached", self.max_per_endpoint)
                ));
            }
            
            pool.insert(key.clone(), PooledConnection {
                transport: transport_clone,
                last_used: Instant::now(),
                in_use: true,
                created_at: Instant::now(),
                _permit: Some(permit),
            });
        }
        
        Ok(transport)
    }
    
    /// Clean up abandoned connections (connections marked in_use but not actually being used)
    pub async fn cleanup_abandoned(&self, timeout: Duration) {
        let mut to_release = Vec::new();
        
        {
            let pool = self.connections.read();
            let now = Instant::now();
            
            for (key, conn) in pool.iter() {
                if conn.in_use && (now - conn.last_used) > timeout {
                    // Connection has been "in use" for too long, likely abandoned
                    to_release.push(key.clone());
                }
            }
        }
        
        // Release abandoned connections
        if !to_release.is_empty() {
            let mut pool = self.connections.write();
            for key in to_release {
                if let Some(conn) = pool.get_mut(&key) {
                    conn.in_use = false;
                    conn.last_used = Instant::now();
                }
            }
        }
        
        // Notify waiters
        self.notify_waiters().await;
    }
    
    /// Release a connection back to the pool and notify waiters
    pub async fn release_connection(&self, config: &TransportConfig) {
        let key = Self::config_to_key(config);
        {
            let mut pool = self.connections.write();
            
            if let Some(conn) = pool.get_mut(&key) {
                conn.in_use = false;
                conn.last_used = Instant::now();
            }
        }
        
        // Notify any waiting requests
        self.notify_waiters().await;
    }
    
    /// Clean up idle connections
    pub async fn cleanup_idle(&self) {
        let mut to_remove = Vec::new();
        
        {
            let pool = self.connections.read();
            let now = Instant::now();
            
            for (key, conn) in pool.iter() {
                if !conn.in_use && (now - conn.last_used) > self.idle_timeout {
                    to_remove.push(key.clone());
                }
            }
        }
        
        if !to_remove.is_empty() {
            let mut pool = self.connections.write();
            for key in to_remove {
                pool.remove(&key);
            }
        }
    }
    
    /// Get pool statistics
    pub fn get_stats(&self) -> PoolStats {
        let pool = self.connections.read();
        
        PoolStats {
            total_connections: pool.len(),
            active_connections: pool.values().filter(|c| c.in_use).count(),
            idle_connections: pool.values().filter(|c| !c.in_use).count(),
            max_total: self.max_total,
            max_per_endpoint: self.max_per_endpoint,
        }
    }
    
    /// Generate unique key for transport config
    fn config_to_key(config: &TransportConfig) -> String {
        match config {
            TransportConfig::Tcp(tcp) => {
                format!("tcp:{}:{}", 
                    tcp.remote_address.map(|a| a.to_string()).unwrap_or_default(),
                    tcp.bind_address.map(|a| a.to_string()).unwrap_or_default())
            }
            TransportConfig::Udp(udp) => {
                format!("udp:{}:{}", 
                    udp.remote_address.map(|a| a.to_string()).unwrap_or_default(),
                    udp.bind_address.to_string())
            }
            TransportConfig::Unix(unix) => {
                format!("unix:{}", unix.path.display())
            }
        }
    }
}

/// Pool statistics
#[derive(Debug, Clone)]
pub struct PoolStats {
    pub total_connections: usize,
    pub active_connections: usize,
    pub idle_connections: usize,
    pub max_total: usize,
    pub max_per_endpoint: usize,
}

impl Default for ConnectionPool {
    fn default() -> Self {
        Self::new(10, 100, Duration::from_secs(300)) // 5 min idle timeout
    }
}

/// Pooled transport wrapper that implements Transport trait
pub struct PooledTransport {
    inner: Arc<Box<dyn Transport>>,
    pool: Arc<ConnectionPool>,
    config: TransportConfig,
}

impl PooledTransport {
    pub fn new(inner: Arc<Box<dyn Transport>>, pool: Arc<ConnectionPool>, config: TransportConfig) -> Self {
        Self { inner, pool, config }
    }
}

#[async_trait]
impl Transport for PooledTransport {
    async fn send(&self, message: &[u8]) -> Result<()> {
        self.inner.send(message).await
    }
    
    async fn receive(&self) -> Result<Bytes> {
        self.inner.receive().await
    }
    
    async fn try_receive(&self) -> Result<Option<Bytes>> {
        self.inner.try_receive().await
    }
    
    fn is_healthy(&self) -> bool {
        self.inner.is_healthy()
    }
    
    fn transport_info(&self) -> super::TransportInfo {
        self.inner.transport_info()
    }
    
    async fn get_metrics(&self) -> super::TransportMetrics {
        self.inner.get_metrics().await
    }
}

impl Drop for PooledTransport {
    fn drop(&mut self) {
        // Release connection back to pool when dropped
        // Note: We can't await in drop, so we spawn a task
        let pool = Arc::clone(&self.pool);
        let config = self.config.clone();
        tokio::spawn(async move {
            pool.release_connection(&config).await;
        });
    }
}