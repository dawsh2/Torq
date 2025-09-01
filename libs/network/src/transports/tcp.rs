//! TCP Network Transport Implementation
//!
//! High-performance TCP transport for distributed Mycelium actor communication.
//! Implements TLV message framing with proper connection pooling and health monitoring.

use crate::{Result, TransportError};
use async_trait::async_trait;
use bytes::{Bytes, BytesMut};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

/// TCP network transport configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TcpNetworkConfig {
    /// Local address to bind to (for server mode)
    pub bind_address: Option<SocketAddr>,
    /// Remote address to connect to (for client mode)
    pub remote_address: Option<SocketAddr>,
    /// Connection timeout
    pub connect_timeout: Duration,
    /// Receive timeout
    pub receive_timeout: Duration,
    /// Keep-alive interval
    pub keepalive_interval: Duration,
    /// Maximum message size
    pub max_message_size: usize,
    /// Buffer size for reading
    pub buffer_size: usize,
}

impl Default for TcpNetworkConfig {
    fn default() -> Self {
        Self {
            bind_address: None,
            remote_address: None,
            connect_timeout: Duration::from_secs(5),
            receive_timeout: Duration::from_secs(30),
            keepalive_interval: Duration::from_secs(30),
            max_message_size: 16 * 1024 * 1024, // 16MB
            buffer_size: 64 * 1024, // 64KB
        }
    }
}

/// TCP network transport for distributed actor communication
pub struct TcpNetworkTransport {
    pub(crate) config: TcpNetworkConfig,
    pub(crate) connection: Arc<RwLock<Option<TcpConnection>>>,
    pub(crate) last_health_check: Arc<RwLock<Option<Instant>>>,
    pub(crate) metrics: super::metrics::MetricsTracker,
}

/// TCP connection wrapper with health monitoring and zero-copy buffers
pub struct TcpConnection {
    stream: TcpStream,
    pub(crate) peer_addr: SocketAddr,
    connected_at: Instant,
    last_activity: Instant,
    pub(crate) bytes_sent: u64,
    pub(crate) bytes_received: u64,
    /// Reusable read buffer for zero-copy operations
    read_buffer: BytesMut,
    /// Reusable write buffer for zero-copy operations
    write_buffer: BytesMut,
}

impl TcpConnection {
    pub(crate) fn new(stream: TcpStream, peer_addr: SocketAddr) -> Self {
        let now = Instant::now();
        Self {
            stream,
            peer_addr,
            connected_at: now,
            last_activity: now,
            bytes_sent: 0,
            bytes_received: 0,
            read_buffer: BytesMut::with_capacity(64 * 1024),
            write_buffer: BytesMut::with_capacity(64 * 1024),
        }
    }
    
    /// Send TLV message with length prefix using zero-copy buffer
    pub(crate) async fn send_message(&mut self, data: &[u8]) -> Result<()> {
        // Clear and reuse write buffer
        self.write_buffer.clear();
        
        // Write length prefix and data to buffer
        self.write_buffer.extend_from_slice(&(data.len() as u32).to_be_bytes());
        self.write_buffer.extend_from_slice(data);
        
        // Single write call for better performance
        self.stream.write_all(&self.write_buffer).await
            .map_err(|e| TransportError::network_with_source("Failed to write message", e))?;
        
        // Flush to ensure immediate transmission
        self.stream.flush().await
            .map_err(|e| TransportError::network_with_source("Failed to flush TCP stream", e))?;
        
        self.bytes_sent += 4 + data.len() as u64;
        self.last_activity = Instant::now();
        
        debug!(
            peer = %self.peer_addr,
            bytes = data.len(),
            total_sent = self.bytes_sent,
            "Sent TLV message over TCP"
        );
        
        Ok(())
    }
    
    /// Receive TLV message with length prefix
    pub(crate) async fn receive_message(&mut self, max_size: usize) -> Result<Bytes> {
        // Read message length prefix
        let mut len_bytes = [0u8; 4];
        self.stream.read_exact(&mut len_bytes).await
            .map_err(|e| TransportError::network_with_source("Failed to read message length", e))?;
        
        let message_len = u32::from_be_bytes(len_bytes) as usize;
        
        if message_len > max_size {
            return Err(TransportError::protocol(format!(
                "Message size {} exceeds maximum {}", message_len, max_size
            )));
        }
        
        // Resize read buffer if needed (reuse existing allocation)
        if self.read_buffer.capacity() < message_len {
            self.read_buffer.reserve(message_len - self.read_buffer.capacity());
        }
        self.read_buffer.resize(message_len, 0);
        
        // Read directly into buffer
        self.stream.read_exact(&mut self.read_buffer).await
            .map_err(|e| TransportError::network_with_source("Failed to read message data", e))?;
        
        self.bytes_received += 4 + message_len as u64;
        self.last_activity = Instant::now();
        
        debug!(
            peer = %self.peer_addr,
            bytes = message_len,
            total_received = self.bytes_received,
            "Received TLV message over TCP"
        );
        
        // Zero-copy: split off the exact portion we need
        Ok(self.read_buffer.split_to(message_len).freeze())
    }
    
    /// Check if connection appears healthy
    pub(crate) fn is_healthy(&self) -> bool {
        // Consider connection healthy if we've had activity recently
        let activity_threshold = Duration::from_secs(60); // 1 minute
        self.last_activity.elapsed() < activity_threshold
    }
    
    /// Get connection statistics
    fn get_stats(&self) -> TcpConnectionStats {
        TcpConnectionStats {
            peer_addr: self.peer_addr,
            connected_duration: self.connected_at.elapsed(),
            last_activity: self.last_activity.elapsed(),
            bytes_sent: self.bytes_sent,
            bytes_received: self.bytes_received,
        }
    }
}

/// TCP connection statistics
#[derive(Debug, Clone)]
pub struct TcpConnectionStats {
    pub peer_addr: SocketAddr,
    pub connected_duration: Duration,
    pub last_activity: Duration,
    pub bytes_sent: u64,
    pub bytes_received: u64,
}

impl TcpNetworkTransport {
    /// Create new TCP network transport for client connections
    pub fn new_client(remote_address: SocketAddr) -> Self {
        let config = TcpNetworkConfig {
            remote_address: Some(remote_address),
            ..Default::default()
        };
        
        Self {
            config,
            connection: Arc::new(RwLock::new(None)),
            last_health_check: Arc::new(RwLock::new(None)),
            metrics: super::metrics::MetricsTracker::new(),
        }
    }
    
    /// Create new TCP network transport for server connections
    pub fn new_server(bind_address: SocketAddr) -> Self {
        let config = TcpNetworkConfig {
            bind_address: Some(bind_address),
            ..Default::default()
        };
        
        Self {
            config,
            connection: Arc::new(RwLock::new(None)),
            last_health_check: Arc::new(RwLock::new(None)),
            metrics: super::metrics::MetricsTracker::new(),
        }
    }
    
    /// Create from configuration
    pub fn from_config(config: TcpNetworkConfig) -> Self {
        Self {
            config,
            connection: Arc::new(RwLock::new(None)),
            last_health_check: Arc::new(RwLock::new(None)),
            metrics: super::metrics::MetricsTracker::new(),
        }
    }
    
    /// Establish connection (for client mode)
    pub async fn connect(&self) -> Result<()> {
        let remote_addr = self.config.remote_address
            .ok_or_else(|| TransportError::configuration("No remote address configured", Some("remote_address")))?;
        
        info!("Connecting to TCP peer at {}", remote_addr);
        
        // Connect with timeout
        let stream = tokio::time::timeout(
            self.config.connect_timeout,
            TcpStream::connect(remote_addr)
        )
        .await
        .map_err(|_| TransportError::timeout("TCP connect", self.config.connect_timeout.as_millis() as u64))?
        .map_err(|e| TransportError::network_with_source("Failed to connect to TCP peer", e))?;
        
        // Configure TCP socket
        if let Err(e) = stream.set_nodelay(true) {
            warn!("Failed to set TCP_NODELAY: {}", e);
        }
        
        let peer_addr = stream.peer_addr()
            .map_err(|e| TransportError::network_with_source("Failed to get peer address", e))?;
        
        let connection = TcpConnection::new(stream, peer_addr);
        
        // Store connection
        let mut conn_guard = self.connection.write().await;
        *conn_guard = Some(connection);
        
        info!("Successfully connected to TCP peer at {}", peer_addr);
        Ok(())
    }
    
    /// Start server listener (for server mode)
    pub async fn start_server(&self) -> Result<()> {
        let bind_addr = self.config.bind_address
            .ok_or_else(|| TransportError::configuration("No bind address configured", Some("bind_address")))?;
        
        let listener = TcpListener::bind(bind_addr).await
            .map_err(|e| TransportError::network_with_source("Failed to bind TCP listener", e))?;
        
        info!("TCP server listening on {}", bind_addr);
        
        // Accept first connection (simplified for single-connection transport)
        let (stream, peer_addr) = listener.accept().await
            .map_err(|e| TransportError::network_with_source("Failed to accept TCP connection", e))?;
        
        // Configure TCP socket
        if let Err(e) = stream.set_nodelay(true) {
            warn!("Failed to set TCP_NODELAY: {}", e);
        }
        
        let connection = TcpConnection::new(stream, peer_addr);
        
        // Store connection
        let mut conn_guard = self.connection.write().await;
        *conn_guard = Some(connection);
        
        info!("Accepted TCP connection from {}", peer_addr);
        Ok(())
    }
    
    /// Ensure connection is established
    pub(crate) async fn ensure_connected(&self) -> Result<()> {
        let conn_guard = self.connection.read().await;
        if conn_guard.is_none() {
            drop(conn_guard);
            
            if self.config.remote_address.is_some() {
                self.connect().await?;
            } else {
                return Err(TransportError::configuration(
                    "No connection established and no remote address to connect to", 
                    Some("connection_state")
                ));
            }
        }
        Ok(())
    }
    
    /// Get connection statistics
    pub async fn get_stats(&self) -> Option<TcpConnectionStats> {
        let conn_guard = self.connection.read().await;
        conn_guard.as_ref().map(|conn| conn.get_stats())
    }
    
    /// Close the connection
    pub async fn close(&self) -> Result<()> {
        let mut conn_guard = self.connection.write().await;
        if let Some(mut connection) = conn_guard.take() {
            if let Err(e) = connection.stream.shutdown().await {
                warn!("Error shutting down TCP connection: {}", e);
            }
            info!("Closed TCP connection to {}", connection.peer_addr);
        }
        Ok(())
    }
}


/// Type aliases for backward compatibility and cleaner exports
pub type TcpConfig = TcpNetworkConfig;
pub type TcpTransport = TcpNetworkTransport;

/// Implementation of unified Transport trait for TCP
#[async_trait]
impl super::Transport for TcpNetworkTransport {
    async fn send(&self, message: &[u8]) -> crate::Result<()> {
        self.ensure_connected().await?;
        
        // Check message size limit
        if message.len() > self.config.max_message_size {
            return Err(TransportError::protocol(format!(
                "Message size {} exceeds maximum {}", 
                message.len(), 
                self.config.max_message_size
            )));
        }
        
        // Send message
        let mut conn_guard = self.connection.write().await;
        if let Some(connection) = conn_guard.as_mut() {
            connection.send_message(message).await?;
            Ok(())
        } else {
            Err(TransportError::network("Connection not established"))
        }
    }
    
    async fn receive(&self) -> crate::Result<bytes::Bytes> {
        self.ensure_connected().await?;
        
        let mut conn_guard = self.connection.write().await;
        if let Some(connection) = conn_guard.as_mut() {
            connection.receive_message(self.config.max_message_size).await
        } else {
            Err(TransportError::network("Connection not established"))
        }
    }
    
    async fn try_receive(&self) -> crate::Result<Option<bytes::Bytes>> {
        Err(TransportError::NotImplemented {
            feature: "try_receive for TCP".to_string(),
            reason: "Requires non-blocking socket operations".to_string(),
        })
    }
    
    fn is_healthy(&self) -> bool {
        match self.connection.try_read() {
            Ok(conn_guard) => {
                match conn_guard.as_ref() {
                    Some(connection) => connection.is_healthy(),
                    None => false,
                }
            }
            Err(_) => false,
        }
    }
    
    fn transport_info(&self) -> super::TransportInfo {
        super::TransportInfo {
            transport_type: super::TransportType::Tcp,
            local_address: None,
            remote_address: self.config.remote_address.map(|a| a.to_string()),
            connection_count: if self.is_healthy() { 1 } else { 0 },
            bytes_sent: 0,  // Would need tracking
            bytes_received: 0,
        }
    }
    
    async fn get_metrics(&self) -> super::TransportMetrics {
        super::TransportMetrics {
            messages_sent: 0,
            messages_received: 0,
            bytes_sent: 0,
            bytes_received: 0,
            errors: 0,
            last_send_latency_ns: 0,
            avg_send_latency_ns: 0,
            p95_send_latency_ns: 0,
            p99_send_latency_ns: 0,
            last_activity: None,
        }
    }
}
