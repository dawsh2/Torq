//! Relay Output Adapter - Sends Protocol V2 binary messages directly to relay sockets
//!
//! This adapter allows collectors to send their Protocol V2 messages (built with
//! TLVMessageBuilder) directly to the appropriate relay (MarketData, Signal, or Execution).
//!
//! The collector builds messages using TLVMessageBuilder::build() which returns Vec<u8>,
//! then sends them through this adapter for direct relay delivery.

use std::sync::Arc;
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use tokio::net::UnixStream;
use tokio::sync::Mutex;
use tokio::time::{sleep, Instant};
use tracing::{debug, error, info, warn};

use crate::Result;
use types::RelayDomain;

/// Output adapter that sends Protocol V2 binary messages to a relay socket
pub struct RelayOutput {
    socket_path: String,
    stream: Arc<Mutex<Option<UnixStream>>>,
    relay_domain: RelayDomain,
    messages_sent: Arc<Mutex<u64>>,
    reconnect_attempts: Arc<Mutex<u32>>,
    last_reconnect: Arc<Mutex<Option<Instant>>>,
}

impl RelayOutput {
    /// Create a new relay output adapter
    /// Note: source_id parameter removed - messages should include their own source in the header
    pub fn new(socket_path: String, relay_domain: RelayDomain) -> Self {
        Self {
            socket_path,
            stream: Arc::new(Mutex::new(None)),
            relay_domain,
            messages_sent: Arc::new(Mutex::new(0)),
            reconnect_attempts: Arc::new(Mutex::new(0)),
            last_reconnect: Arc::new(Mutex::new(None)),
        }
    }

    /// Connect to the relay socket with retry logic
    pub async fn connect(&self) -> Result<()> {
        self.connect_with_retry().await
    }

    /// Internal connect with exponential backoff retry
    async fn connect_with_retry(&self) -> Result<()> {
        let mut attempts = *self.reconnect_attempts.lock().await;
        const MAX_ATTEMPTS: u32 = 10;
        const BASE_DELAY_MS: u64 = 100;
        const MAX_DELAY_MS: u64 = 30000; // 30 seconds max

        loop {
            info!("🔌 Connecting to relay at: {} (attempt {})", self.socket_path, attempts + 1);

            match UnixStream::connect(&self.socket_path).await {
                Ok(mut stream) => {
                    info!("✅ Connected to {:?} relay", self.relay_domain);

                    // Send a small identification message immediately to be classified as publisher
                    // This is a minimal Protocol V2 header (32 bytes) with zero payload
                    let identification_header = [
                        // magic: 0xDEADBEEF (4 bytes, little endian)
                        0xEF, 0xBE, 0xAD, 0xDE,
                        // relay_domain: MarketData (1 byte)
                        0x01,
                        // version: 1 (1 byte)
                        0x01,
                        // source: PolygonCollector (1 byte)
                        0x02,
                        // flags: 0 (1 byte)
                        0x00,
                        // sequence: 0 (8 bytes)
                        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                        // timestamp: 0 (8 bytes)
                        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                        // payload_size: 0 (4 bytes)
                        0x00, 0x00, 0x00, 0x00,
                        // checksum: 0 (4 bytes)
                        0x00, 0x00, 0x00, 0x00,
                    ];

                    match stream.write_all(&identification_header).await {
                        Ok(()) => debug!("📡 Sent identification message to establish publisher role"),
                        Err(e) => warn!("Failed to send identification message: {}", e),
                    }

                    *self.stream.lock().await = Some(stream);
                    *self.reconnect_attempts.lock().await = 0; // Reset on successful connection
                    *self.last_reconnect.lock().await = Some(Instant::now());
                    return Ok(());
                }
                Err(e) => {
                    attempts += 1;
                    *self.reconnect_attempts.lock().await = attempts;

                    if attempts >= MAX_ATTEMPTS {
                        error!("❌ Failed to connect to relay after {} attempts: {}", MAX_ATTEMPTS, e);
                        return Err(crate::AdapterError::Io(e));
                    }

                    // Calculate exponential backoff delay
                    let delay_ms = (BASE_DELAY_MS * 2_u64.pow(attempts - 1)).min(MAX_DELAY_MS);
                    warn!(
                        "⚠️ Connection attempt {} failed, retrying in {}ms: {}",
                        attempts, delay_ms, e
                    );
                    
                    sleep(Duration::from_millis(delay_ms)).await;
                }
            }
        }
    }

    /// Send a Protocol V2 binary message to the relay
    /// The message should be built using TLVMessageBuilder::build() which returns Vec<u8>
    /// This message already contains the complete Protocol V2 header and TLV payload
    pub async fn send_bytes(&self, message_bytes: &[u8]) -> Result<()> {
        // Ensure we're connected
        let mut stream_guard = self.stream.lock().await;
        if stream_guard.is_none() {
            drop(stream_guard);
            self.connect().await?;
            stream_guard = self.stream.lock().await;
        }

        if let Some(ref mut stream) = *stream_guard {
            // Send the pre-built Protocol V2 message directly
            match stream.write_all(&message_bytes).await {
                Ok(()) => {
                    let mut count = self.messages_sent.lock().await;
                    *count += 1;
                    let total = *count;
                    drop(count);

                    debug!(
                        "📨 Sent Protocol V2 message #{} to {:?} relay ({} bytes)",
                        total,
                        self.relay_domain,
                        message_bytes.len()
                    );

                    if total <= 5 || total % 1000 == 0 {
                        info!(
                            "📊 RelayOutput stats: {} messages sent to {:?} relay",
                            total, self.relay_domain
                        );
                    }

                    Ok(())
                }
                Err(e) => {
                    error!("❌ Failed to send to relay, will attempt reconnection: {}", e);
                    // Reset connection on error
                    *stream_guard = None;
                    drop(stream_guard);
                    
                    // Try to reconnect immediately
                    if let Err(reconnect_err) = self.connect_with_retry().await {
                        error!("❌ Failed to reconnect after send error: {}", reconnect_err);
                        return Err(crate::AdapterError::Io(e));
                    }
                    
                    // Retry sending after reconnection
                    stream_guard = self.stream.lock().await;
                    if let Some(ref mut stream) = *stream_guard {
                        match stream.write_all(&message_bytes).await {
                            Ok(()) => {
                                info!("✅ Successfully sent message after reconnection");
                                let mut count = self.messages_sent.lock().await;
                                *count += 1;
                                Ok(())
                            }
                            Err(retry_err) => {
                                error!("❌ Failed to send even after reconnection: {}", retry_err);
                                *stream_guard = None;
                                Err(crate::AdapterError::Io(retry_err))
                            }
                        }
                    } else {
                        Err(crate::AdapterError::Io(e))
                    }
                }
            }
        } else {
            Err(crate::AdapterError::ConnectionTimeout {
                venue: types::VenueId::Generic, // Use Generic venue for relay
                timeout_ms: 0,
            })
        }
    }

    /// Get statistics
    pub async fn stats(&self) -> RelayOutputStats {
        RelayOutputStats {
            connected: self.stream.lock().await.is_some(),
            messages_sent: *self.messages_sent.lock().await,
            relay_domain: self.relay_domain,
            socket_path: self.socket_path.clone(),
        }
    }

    /// Check connection health and reconnect if necessary
    pub async fn ensure_connected(&self) -> Result<()> {
        let stream_guard = self.stream.lock().await;
        if stream_guard.is_none() {
            drop(stream_guard);
            info!("🔄 Connection lost, attempting to reconnect...");
            self.connect_with_retry().await
        } else {
            Ok(())
        }
    }

    /// Start a background task that monitors connection health
    /// This should be spawned as a separate task
    pub fn spawn_health_monitor(self: Arc<Self>) {
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(30));
            loop {
                interval.tick().await;
                
                // Check if we should reconnect
                let last_reconnect = *self.last_reconnect.lock().await;
                let should_check = match last_reconnect {
                    Some(instant) => instant.elapsed() > Duration::from_secs(5),
                    None => true,
                };

                if should_check {
                    if let Err(e) = self.ensure_connected().await {
                        error!("❌ Health check failed to reconnect: {}", e);
                    } else {
                        let stats = self.stats().await;
                        if stats.connected {
                            debug!(
                                "✅ Health check: {:?} relay healthy, {} messages sent",
                                self.relay_domain, stats.messages_sent
                            );
                        }
                    }
                }
            }
        });
    }
}

/// Statistics for relay output
#[derive(Debug, Clone)]
pub struct RelayOutputStats {
    /// Whether the relay is currently connected
    pub connected: bool,
    /// Total messages sent to this relay
    pub messages_sent: u64,
    /// Domain this relay serves (MarketData, Signal, or Execution)
    pub relay_domain: RelayDomain,
    /// Unix socket path for relay connection
    pub socket_path: String,
}
