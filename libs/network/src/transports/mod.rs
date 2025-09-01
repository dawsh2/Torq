//! Unified Transport Layer
//!
//! This module provides a unified transport abstraction for the network layer,
//! consolidating previously scattered transport implementations into a coherent system.

use crate::{Result, TransportError};
use async_trait::async_trait;
use bytes::Bytes;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

pub mod tcp;
pub mod udp;
pub mod unix;
pub mod metrics;
pub mod pool;

#[cfg(test)]
mod tests;

// Re-export transport types
pub use tcp::{TcpNetworkConfig, TcpNetworkTransport, TcpConnectionStats};
pub use udp::{UdpConfig, UdpTransport};
pub use unix::{UnixSocketConfig, UnixSocketTransport, UnixSocketConnection};
pub use pool::{ConnectionPool, PooledTransport, PoolStats};
pub use metrics::MetricsTracker;

/// Unified Transport trait for all transport implementations
#[async_trait]
pub trait Transport: Send + Sync {
    /// Send message over the transport
    async fn send(&self, message: &[u8]) -> Result<()>;
    
    /// Send message with timeout
    async fn send_timeout(&self, message: &[u8], timeout: Duration) -> Result<()> {
        tokio::time::timeout(timeout, self.send(message)).await
            .map_err(|_| TransportError::timeout("send", timeout.as_millis() as u64))?
    }
    
    /// Receive message from the transport
    async fn receive(&self) -> Result<bytes::Bytes>;
    
    /// Receive message with timeout
    async fn receive_timeout(&self, timeout: Duration) -> Result<bytes::Bytes> {
        tokio::time::timeout(timeout, self.receive()).await
            .map_err(|_| TransportError::timeout("receive", timeout.as_millis() as u64))?
    }
    
    /// Try to receive message without blocking (returns None if no message available)
    async fn try_receive(&self) -> Result<Option<bytes::Bytes>>;
    
    /// Check if transport is healthy
    fn is_healthy(&self) -> bool;
    
    /// Get transport-specific information
    fn transport_info(&self) -> TransportInfo;
    
    /// Get performance metrics
    async fn get_metrics(&self) -> TransportMetrics;
}


/// Transport type enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TransportType {
    /// TCP network transport
    Tcp,
    /// UDP network transport  
    Udp,
    /// Unix domain socket transport
    Unix,
}

/// Transport configuration enum
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TransportConfig {
    /// TCP configuration
    Tcp(TcpNetworkConfig),
    /// UDP configuration  
    Udp(UdpConfig),
    /// Unix socket configuration
    Unix(UnixSocketConfig),
}

/// Transport information for monitoring
#[derive(Debug, Clone)]
pub struct TransportInfo {
    pub transport_type: TransportType,
    pub local_address: Option<String>,
    pub remote_address: Option<String>,
    pub connection_count: usize,
    pub bytes_sent: u64,
    pub bytes_received: u64,
}

/// Transport performance metrics
#[derive(Debug, Clone, Default)]
pub struct TransportMetrics {
    pub messages_sent: u64,
    pub messages_received: u64,
    pub bytes_sent: u64,
    pub bytes_received: u64,
    pub errors: u64,
    pub last_send_latency_ns: u64,
    pub avg_send_latency_ns: u64,
    pub p95_send_latency_ns: u64,
    pub p99_send_latency_ns: u64,
    pub last_activity: Option<Instant>,
}

/// Transport factory for creating transport instances
pub struct TransportFactory;

impl TransportFactory {
    /// Create transport from configuration
    pub async fn create_transport(config: TransportConfig) -> Result<Box<dyn Transport>> {
        match config {
            TransportConfig::Tcp(tcp_config) => {
                let transport = TcpNetworkTransport::from_config(tcp_config);
                Ok(Box::new(transport))
            }
            TransportConfig::Udp(udp_config) => {
                let transport = UdpTransport::new(udp_config).await?;
                Ok(Box::new(transport))
            }
            TransportConfig::Unix(unix_config) => {
                // For Unix sockets, check if we're in client mode (path exists)
                let is_client = unix_config.path.exists();
                
                if is_client {
                    let transport = UnixSocketTransport::new_client(unix_config.path).await?;
                    Ok(Box::new(transport))
                } else {
                    // Server mode - just create the transport
                    let transport = UnixSocketTransport::new(unix_config)?;
                    Ok(Box::new(transport))
                }
            }
        }
    }
    
    /// Create TCP transport for client connections
    pub fn create_tcp_client(remote_address: SocketAddr) -> Box<dyn Transport> {
        Box::new(TcpNetworkTransport::new_client(remote_address))
    }
    
    /// Create TCP transport for server connections  
    pub fn create_tcp_server(bind_address: SocketAddr) -> Box<dyn Transport> {
        Box::new(TcpNetworkTransport::new_server(bind_address))
    }
    
    /// Create Unix socket transport
    pub fn create_unix_socket(path: PathBuf) -> Result<Box<dyn Transport>> {
        let config = UnixSocketConfig {
            path,
            ..Default::default()
        };
        let transport = UnixSocketTransport::new(config)?;
        Ok(Box::new(transport))
    }
}



