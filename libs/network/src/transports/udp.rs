//! UDP Network Transport Implementation
//!
//! High-performance UDP transport with TLV message framing for datagram communication.
//! Implements multicast support for service discovery and broadcast messaging.

use crate::{Result, TransportError};
use bytes::{Bytes, BytesMut};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::net::UdpSocket;
use tokio::sync::{Mutex, RwLock};
use tokio::time::timeout;
use tracing::{debug, info, warn};

/// UDP transport configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UdpConfig {
    /// Local address to bind to
    pub bind_address: SocketAddr,
    /// Remote address for connected mode (optional)
    pub remote_address: Option<SocketAddr>,
    /// Buffer size for reading
    pub buffer_size: usize,
    /// Maximum message size
    pub max_message_size: usize,
    /// Enable multicast
    pub multicast: Option<MulticastConfig>,
    /// Send/receive timeout
    pub timeout: Duration,
}

impl Default for UdpConfig {
    fn default() -> Self {
        Self {
            bind_address: "0.0.0.0:0".parse().unwrap(),
            remote_address: None,
            buffer_size: 64 * 1024,             // 64KB
            max_message_size: 64 * 1024,        // 64KB max for UDP
            multicast: None,
            timeout: Duration::from_secs(5),
        }
    }
}

/// Multicast configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MulticastConfig {
    /// Multicast group address
    pub group: SocketAddr,
    /// Interface for multicast
    pub interface: Option<std::net::Ipv4Addr>,
    /// TTL for multicast packets
    pub ttl: Option<u32>,
}

/// UDP transport statistics
#[derive(Debug, Clone, Default)]
pub struct UdpStats {
    pub packets_sent: u64,
    pub packets_received: u64,
    pub bytes_sent: u64,
    pub bytes_received: u64,
    pub errors: u64,
    pub last_activity: Option<Instant>,
}

/// UDP transport for datagram communication
pub struct UdpTransport {
    pub(crate) config: UdpConfig,
    socket: Arc<UdpSocket>,
    stats: Arc<RwLock<UdpStats>>,
    send_buffer: Arc<Mutex<BytesMut>>,
    recv_buffer: Arc<Mutex<BytesMut>>,
    pub(crate) metrics: super::metrics::MetricsTracker,
}

impl UdpTransport {
    /// Create new UDP transport
    /// Note: This must be called from within a tokio runtime context
    pub async fn new(config: UdpConfig) -> Result<Self> {
        // Validate configuration
        if config.max_message_size > 65507 {
            return Err(TransportError::configuration(
                "UDP max message size cannot exceed 65507 bytes",
                Some("max_message_size"),
            ));
        }

        // Bind socket using existing runtime
        let socket = UdpSocket::bind(config.bind_address).await
            .map_err(|e| TransportError::network_with_source(
                format!("Failed to bind UDP socket on {}", config.bind_address),
                e
            ))?;
        
        // Set up multicast if configured
        if let Some(ref multicast) = config.multicast {
            if let SocketAddr::V4(group_addr) = multicast.group {
                let interface = multicast.interface.unwrap_or(std::net::Ipv4Addr::UNSPECIFIED);
                socket.join_multicast_v4(group_addr.ip().clone(), interface)
                    .map_err(|e| TransportError::network_with_source(
                        format!("Failed to join multicast group {}", group_addr),
                        e
                    ))?;
                
                if let Some(ttl) = multicast.ttl {
                    socket.set_multicast_ttl_v4(ttl)
                        .map_err(|e| TransportError::network_with_source(
                            format!("Failed to set multicast TTL to {}", ttl),
                            e
                        ))?;
                }
                
                info!("Joined multicast group: {}", group_addr);
            }
        }
        
        // Connect to remote if specified
        if let Some(remote) = config.remote_address {
            socket.connect(remote).await
                .map_err(|e| TransportError::network_with_source(
                    format!("Failed to connect UDP socket to {}", remote),
                    e
                ))?;
            info!("UDP socket connected to: {}", remote);
        }

        info!("UDP transport listening on: {}", config.bind_address);

        Ok(Self {
            config: config.clone(),
            socket: Arc::new(socket),
            stats: Arc::new(RwLock::new(UdpStats::default())),
            send_buffer: Arc::new(Mutex::new(BytesMut::with_capacity(config.buffer_size))),
            recv_buffer: Arc::new(Mutex::new(BytesMut::with_capacity(config.buffer_size))),
            metrics: super::metrics::MetricsTracker::new(),
        })
    }
    
    /// Create new UDP transport (blocking version for initialization only)
    /// 
    /// # WARNING: INITIALIZATION ONLY
    /// This method is provided ONLY for initialization code where an async context
    /// is not available. It MUST NOT be used in hot paths or production code paths.
    /// Using this in performance-critical code will violate the <35μs latency requirement.
    /// 
    /// # Panics
    /// Will panic if no tokio runtime is available.
    #[deprecated(since = "0.2.0", note = "Use async new() instead. This is for init only.")]
    pub fn new_blocking_init_only(config: UdpConfig) -> Result<Self> {
        tokio::runtime::Handle::try_current()
            .map_err(|_| TransportError::configuration(
                "No tokio runtime found. Use new() from async context or create runtime first",
                Some("runtime")
            ))?
            .block_on(Self::new(config))
    }

    /// Send TLV-framed message
    pub(crate) async fn send_message(&self, data: &[u8]) -> Result<()> {
        if data.len() > self.config.max_message_size {
            return Err(TransportError::protocol(format!(
                "Message size {} exceeds maximum {}",
                data.len(),
                self.config.max_message_size
            )));
        }

        let mut buffer = self.send_buffer.lock().await;
        buffer.clear();
        
        // Add TLV framing: 4-byte length prefix + data
        buffer.extend_from_slice(&(data.len() as u32).to_be_bytes());
        buffer.extend_from_slice(data);

        // Send with timeout
        let timeout_ms = self.config.timeout.as_millis() as u64;
        let bytes_sent = timeout(self.config.timeout, self.socket.send(&buffer))
            .await
            .map_err(|_| TransportError::timeout("UDP send", timeout_ms))?
            .map_err(|e| TransportError::network_with_source("Failed to send UDP packet", e))?;

        // Update statistics
        let mut stats = self.stats.write().await;
        stats.packets_sent += 1;
        stats.bytes_sent += bytes_sent as u64;
        stats.last_activity = Some(Instant::now());

        debug!(
            "Sent UDP packet: {} bytes (payload: {} bytes)",
            bytes_sent,
            data.len()
        );

        Ok(())
    }

    /// Send to specific address (for unconnected mode)
    pub async fn send_to(&self, data: &[u8], addr: SocketAddr) -> Result<()> {
        if data.len() > self.config.max_message_size {
            return Err(TransportError::protocol(format!(
                "Message size {} exceeds maximum {}",
                data.len(),
                self.config.max_message_size
            )));
        }

        let mut buffer = self.send_buffer.lock().await;
        buffer.clear();
        
        // Add TLV framing
        buffer.extend_from_slice(&(data.len() as u32).to_be_bytes());
        buffer.extend_from_slice(data);

        // Send with timeout
        let timeout_ms = self.config.timeout.as_millis() as u64;
        let bytes_sent = timeout(self.config.timeout, self.socket.send_to(&buffer, addr))
            .await
            .map_err(|_| TransportError::timeout("UDP send_to", timeout_ms))?
            .map_err(|e| TransportError::network_with_source("Failed to send UDP packet", e))?;

        // Update statistics
        let mut stats = self.stats.write().await;
        stats.packets_sent += 1;
        stats.bytes_sent += bytes_sent as u64;
        stats.last_activity = Some(Instant::now());

        debug!("Sent UDP packet to {}: {} bytes", addr, bytes_sent);

        Ok(())
    }

    /// Receive TLV-framed message
    pub async fn receive_message(&self) -> Result<Bytes> {
        let mut buffer = self.recv_buffer.lock().await;
        buffer.resize(self.config.buffer_size, 0);

        // Receive with timeout
        let timeout_ms = self.config.timeout.as_millis() as u64;
        let bytes_received = timeout(self.config.timeout, self.socket.recv(&mut buffer))
            .await
            .map_err(|_| TransportError::timeout("UDP receive", timeout_ms))?
            .map_err(|e| TransportError::network_with_source("Failed to receive UDP packet", e))?;

        // Parse TLV framing
        if bytes_received < 4 {
            return Err(TransportError::protocol("UDP packet too small for TLV header"));
        }

        let length = u32::from_be_bytes([buffer[0], buffer[1], buffer[2], buffer[3]]) as usize;
        
        if bytes_received != length + 4 {
            return Err(TransportError::protocol(format!(
                "TLV length mismatch: expected {}, got {}",
                length + 4,
                bytes_received
            )));
        }

        // Extract payload
        let payload = Bytes::copy_from_slice(&buffer[4..bytes_received]);

        // Update statistics
        let mut stats = self.stats.write().await;
        stats.packets_received += 1;
        stats.bytes_received += bytes_received as u64;
        stats.last_activity = Some(Instant::now());

        debug!("Received UDP packet: {} bytes (payload: {} bytes)", bytes_received, length);

        Ok(payload)
    }

    /// Receive from any address (returns data and sender address)
    pub async fn receive_from(&self) -> Result<(Bytes, SocketAddr)> {
        let mut buffer = self.recv_buffer.lock().await;
        buffer.resize(self.config.buffer_size, 0);

        // Receive with timeout
        let timeout_ms = self.config.timeout.as_millis() as u64;
        let (bytes_received, sender) = timeout(self.config.timeout, self.socket.recv_from(&mut buffer))
            .await
            .map_err(|_| TransportError::timeout("UDP recv_from", timeout_ms))?
            .map_err(|e| TransportError::network_with_source("Failed to receive UDP packet", e))?;

        // Parse TLV framing
        if bytes_received < 4 {
            return Err(TransportError::protocol("UDP packet too small for TLV header"));
        }

        let length = u32::from_be_bytes([buffer[0], buffer[1], buffer[2], buffer[3]]) as usize;
        
        if bytes_received != length + 4 {
            return Err(TransportError::protocol(format!(
                "TLV length mismatch: expected {}, got {}",
                length + 4,
                bytes_received
            )));
        }

        // Extract payload
        let payload = Bytes::copy_from_slice(&buffer[4..bytes_received]);

        // Update statistics
        let mut stats = self.stats.write().await;
        stats.packets_received += 1;
        stats.bytes_received += bytes_received as u64;
        stats.last_activity = Some(Instant::now());

        debug!("Received UDP packet from {}: {} bytes", sender, bytes_received);

        Ok((payload, sender))
    }

    /// Broadcast message to multicast group
    pub async fn broadcast(&self, data: &[u8]) -> Result<()> {
        if let Some(ref multicast) = self.config.multicast {
            self.send_to(data, multicast.group).await
        } else {
            Err(TransportError::configuration(
                "Multicast not configured for broadcast",
                Some("multicast"),
            ))
        }
    }

    /// Get transport statistics
    pub async fn get_stats(&self) -> UdpStats {
        self.stats.read().await.clone()
    }

    /// Check if transport is healthy
    pub(crate) async fn is_healthy(&self) -> bool {
        let stats = self.stats.read().await;
        if let Some(last_activity) = stats.last_activity {
            // Consider healthy if we've had activity in the last 60 seconds
            last_activity.elapsed() < Duration::from_secs(60)
        } else {
            // No activity yet, but socket is bound
            true
        }
    }

    /// Get local address
    pub fn local_addr(&self) -> Result<SocketAddr> {
        self.socket.local_addr()
            .map_err(|e| TransportError::network_with_source("Failed to get local address", e))
    }
}
