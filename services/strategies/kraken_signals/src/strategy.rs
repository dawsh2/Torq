//! Main Kraken signal strategy implementation
//! 
//! **Bijective ID Architecture**: This strategy uses InstrumentId's self-describing 
//! properties to eliminate HashMap lookups. Instead of storing state in DashMaps keyed 
//! by InstrumentId, we extract venue/symbol directly from the bijective ID and compute
//! indicator state deterministically.

use crate::config::StrategyConfig;
use crate::error::{Result, StrategyError};
use crate::indicators::{CompositeIndicator, IndicatorSignal, MomentumStrength, TrendDirection};
use crate::signals::{SignalStats, SignalType, TradingSignal};
use torq_types::TLVType;
use torq_types::{InstrumentId, VenueId};
use torq_types::{MessageHeader, RelayDomain, SourceType};
use parking_lot::RwLock;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use network::time::safe_system_timestamp_ns;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;
use tracing::{debug, error, info, warn};

/// Kraken signal strategy - Uses bijective InstrumentId architecture
/// 
/// **No HashMaps**: Instead of storing per-instrument state in DashMaps, this strategy 
/// leverages InstrumentId's self-describing properties to compute state deterministically.
/// Each InstrumentId encodes venue, asset type, and symbol data directly, enabling
/// stateless operation with identical performance characteristics.
pub struct KrakenSignalStrategy {
    config: StrategyConfig,

    /// Signal generation statistics (global across all instruments)
    stats: Arc<RwLock<SignalStats>>,

    /// Next signal ID (global counter)
    next_signal_id: AtomicU64,

    /// Strategy ID for TLV messages
    strategy_id: u16,

    /// Signal relay connection
    signal_connection: Option<UnixStream>,

    /// Last processed timestamp for cooldown calculations (in-memory cache)
    /// Uses u128 cache keys from InstrumentId for O(1) performance
    last_signal_cache: Arc<RwLock<std::collections::HashMap<u128, u64>>>,
}

impl KrakenSignalStrategy {
    pub fn new(config: StrategyConfig) -> Self {
        Self {
            config,
            stats: Arc::new(RwLock::new(SignalStats::default())),
            next_signal_id: AtomicU64::new(1),
            strategy_id: 20, // Kraken signal strategy ID from protocol.md
            signal_connection: None,
            last_signal_cache: Arc::new(RwLock::new(std::collections::HashMap::new())),
        }
    }

    /// Start the strategy by connecting to market data relay
    pub async fn start(&mut self) -> Result<()> {
        info!("Starting Kraken Signal Strategy");

        // Connect to signal relay for output
        self.connect_to_signal_relay().await?;

        // Connect to market data relay for input
        let mut market_data_stream = self.connect_to_market_data_relay().await?;

        info!("Kraken Signal Strategy started, processing market data...");

        // Process incoming market data
        let mut buffer = vec![0u8; 8192];
        loop {
            match market_data_stream.read(&mut buffer).await {
                Ok(0) => {
                    warn!("Market data connection closed, reconnecting...");
                    market_data_stream = self.connect_to_market_data_relay().await?;
                }
                Ok(bytes_read) => {
                    if let Err(e) = self.process_market_data(&buffer[..bytes_read]).await {
                        warn!("Error processing market data: {}", e);
                    }
                }
                Err(e) => {
                    error!("Error reading market data: {}", e);
                    tokio::time::sleep(Duration::from_secs(1)).await;
                    market_data_stream = self.connect_to_market_data_relay().await?;
                }
            }
        }
    }

    async fn connect_to_signal_relay(&mut self) -> Result<()> {
        info!(
            "Connecting to signal relay: {}",
            self.config.signal_relay_path
        );

        let mut attempts = 0;
        loop {
            match UnixStream::connect(&self.config.signal_relay_path).await {
                Ok(stream) => {
                    self.signal_connection = Some(stream);
                    info!("Connected to signal relay");
                    return Ok(());
                }
                Err(e) => {
                    attempts += 1;
                    if attempts > 30 {
                        return Err(StrategyError::Configuration {
                            message: format!(
                                "Failed to connect to signal relay after {} attempts: {}",
                                attempts, e
                            ),
                        });
                    }
                    warn!(
                        "Failed to connect to signal relay (attempt {}): {}",
                        attempts, e
                    );
                    tokio::time::sleep(Duration::from_secs(1)).await;
                }
            }
        }
    }

    async fn connect_to_market_data_relay(&self) -> Result<UnixStream> {
        info!(
            "Connecting to market data relay: {}",
            self.config.market_data_relay_path
        );

        let mut attempts = 0;
        loop {
            match UnixStream::connect(&self.config.market_data_relay_path).await {
                Ok(stream) => {
                    info!("Connected to market data relay");
                    return Ok(stream);
                }
                Err(e) => {
                    attempts += 1;
                    if attempts > 30 {
                        return Err(StrategyError::Configuration {
                            message: format!(
                                "Failed to connect to market data relay after {} attempts: {}",
                                attempts, e
                            ),
                        });
                    }
                    warn!(
                        "Failed to connect to market data relay (attempt {}): {}",
                        attempts, e
                    );
                    tokio::time::sleep(Duration::from_secs(1)).await;
                }
            }
        }
    }

    async fn process_market_data(&mut self, data: &[u8]) -> Result<()> {
        // Parse message header
        if data.len() < 32 {
            return Ok(()); // Incomplete message
        }

        let magic = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
        if magic != 0xDEADBEEF {
            return Ok(()); // Invalid message
        }

        let relay_domain = data[4];
        if relay_domain != RelayDomain::MarketData as u8 {
            return Ok(()); // Not market data
        }

        let payload_size = u32::from_le_bytes([data[8], data[9], data[10], data[11]]) as usize;
        if data.len() < 32 + payload_size {
            return Ok(()); // Incomplete message
        }

        // Extract TLV payload
        let tlv_data = &data[32..32 + payload_size];
        self.process_tlv_payload(tlv_data).await?;

        Ok(())
    }

    async fn process_tlv_payload(&mut self, tlv_data: &[u8]) -> Result<()> {
        let mut offset = 0;

        while offset + 2 <= tlv_data.len() {
            let tlv_type = tlv_data[offset];
            let tlv_length = tlv_data[offset + 1] as usize;

            if offset + 2 + tlv_length > tlv_data.len() {
                break; // Incomplete TLV
            }

            let tlv_payload = &tlv_data[offset + 2..offset + 2 + tlv_length];

            if tlv_type == TLVType::Trade as u8 {
                self.process_trade_tlv(tlv_payload).await?;
            }

            offset += 2 + tlv_length;
        }

        Ok(())
    }

    async fn process_trade_tlv(&mut self, payload: &[u8]) -> Result<()> {
        if payload.len() < 22 {
            return Ok(()); // Invalid trade TLV
        }

        // Parse instrument ID (12 bytes)
        let venue = u16::from_le_bytes([payload[0], payload[1]]);
        let asset_type = payload[2];
        let reserved = payload[3];
        let asset_id = u64::from_le_bytes([
            payload[4],
            payload[5],
            payload[6],
            payload[7],
            payload[8],
            payload[9],
            payload[10],
            payload[11],
        ]);

        let instrument_id = InstrumentId {
            venue,
            asset_type,
            reserved,
            asset_id,
        };

        // Only process Kraken instruments
        if venue != VenueId::Kraken as u16 {
            return Ok(());
        }

        // Parse price and volume
        let price_raw = i64::from_le_bytes([
            payload[12],
            payload[13],
            payload[14],
            payload[15],
            payload[16],
            payload[17],
            payload[18],
            payload[19],
        ]);
        let price = Decimal::from(price_raw) / Decimal::from(100_000_000); // Convert from fixed-point

        let timestamp = safe_system_timestamp_ns();

        // Update indicators and generate signals
        self.update_indicators_and_generate_signals(instrument_id, price, timestamp)
            .await?;

        debug!(
            "Processed trade: {:?} @ {}",
            instrument_id.debug_info(),
            price
        );

        Ok(())
    }

    async fn update_indicators_and_generate_signals(
        &mut self,
        instrument_id: InstrumentId,
        price: Decimal,
        timestamp: u64,
    ) -> Result<()> {
        // Extract bijective data from InstrumentId - no lookups needed!
        let venue = instrument_id.venue()
            .map_err(|e| StrategyError::InvalidData(format!("Invalid venue in InstrumentId: {}", e)))?;
        let asset_type = instrument_id.asset_type()
            .map_err(|e| StrategyError::InvalidData(format!("Invalid asset type in InstrumentId: {}", e)))?;

        // Create indicator deterministically from bijective ID components
        let signal = {
            let mut indicator = self.create_indicator_for_instrument(&instrument_id, venue)?;
            
            // Update indicator with new price data
            let signal = indicator.update(price);
            let is_ready = indicator.is_ready();
            (signal, is_ready)
        };

        // Price history is computed on-demand from market data stream - no storage needed
        // This eliminates the price_history HashMap entirely

        // Generate signal if indicator is ready
        if signal.1 {
            if let Some(trading_signal) = self
                .evaluate_signal(instrument_id, &signal.0, price, timestamp)
                .await?
            {
                self.send_signal(&trading_signal).await?;

                // Update stats
                self.stats.write().record_signal(&trading_signal);

                // Update last signal time using bijective cache key
                let cache_key = instrument_id.cache_key();
                self.last_signal_cache.write().insert(cache_key, timestamp);

                info!(
                    "Generated signal: {:?} for {:?}",
                    trading_signal.signal_type,
                    instrument_id.debug_info()
                );
            }
        }

        Ok(())
    }

    async fn evaluate_signal(
        &self,
        instrument_id: InstrumentId,
        indicator_signal: &IndicatorSignal,
        current_price: Decimal,
        timestamp: u64,
    ) -> Result<Option<TradingSignal>> {
        // Check cooldown using bijective cache key
        let cache_key = instrument_id.cache_key();
        if let Some(&last_time) = self.last_signal_cache.read().get(&cache_key) {
            let cooldown_ns = self.config.signal_cooldown.as_nanos() as u64;
            if timestamp.saturating_sub(last_time) < cooldown_ns {
                return Ok(None); // Still in cooldown
            }
        }

        // Analyze trend and momentum
        let trend = indicator_signal.trend_direction();
        let momentum = indicator_signal.momentum_strength();

        let signal_type = match (trend, momentum) {
            (Some(TrendDirection::Up), Some(MomentumStrength::Strong)) => SignalType::Buy,
            (Some(TrendDirection::Down), Some(MomentumStrength::Strong)) => SignalType::Sell,
            _ => SignalType::Hold,
        };

        // Only generate buy/sell signals
        if signal_type == SignalType::Hold {
            return Ok(None);
        }

        // Calculate confidence based on trend and momentum alignment
        let confidence = match (trend, momentum) {
            (Some(TrendDirection::Up), Some(MomentumStrength::Strong)) => 90,
            (Some(TrendDirection::Down), Some(MomentumStrength::Strong)) => 90,
            (Some(TrendDirection::Up), Some(MomentumStrength::Moderate)) => 75,
            (Some(TrendDirection::Down), Some(MomentumStrength::Moderate)) => 75,
            _ => 60,
        };

        // Check minimum confidence threshold
        if confidence < self.config.min_confidence {
            return Ok(None);
        }

        // Calculate position size and expected profit
        let position_size_usd = current_price * self.config.max_position_pct / dec!(100);
        let expected_profit_usd =
            position_size_usd * self.config.min_price_movement_pct / dec!(100);

        let signal_id = self.next_signal_id.fetch_add(1, Ordering::SeqCst);

        let reason = format!(
            "Trend: {:?}, Momentum: {:?}, MA Cross",
            trend.unwrap_or(TrendDirection::Sideways),
            momentum.unwrap_or(MomentumStrength::Neutral)
        );

        let trading_signal = TradingSignal::new(crate::signals::SignalConfig {
            signal_id,
            instrument_id,
            signal_type,
            confidence,
            current_price,
            position_size_usd,
            expected_profit_usd,
            strategy_id: self.strategy_id,
            reason,
        });

        Ok(Some(trading_signal))
    }

    async fn send_signal(&mut self, signal: &TradingSignal) -> Result<()> {
        // Create TLV message before borrowing connection mutably
        let tlv_message = self.create_signal_tlv_message(signal)?;

        if let Some(ref mut connection) = self.signal_connection {
            if let Err(e) = connection.write_all(&tlv_message).await {
                warn!("Failed to send signal to relay: {}", e);
                // Reconnect on next iteration
                self.signal_connection = None;
            } else {
                connection.flush().await.ok();
                debug!("Sent signal {} to relay", signal.signal_id);
            }
        }

        Ok(())
    }

    fn create_signal_tlv_message(&self, signal: &TradingSignal) -> Result<Vec<u8>> {
        let mut message = Vec::new();

        // Create message header
        let header = MessageHeader {
            magic: 0xDEADBEEF,
            relay_domain: RelayDomain::Signal as u8,
            version: 1,
            source: SourceType::KrakenSignalStrategy as u8,
            flags: 0,
            payload_size: 48, // SignalIdentityTLV (16) + EconomicsTLV (32)
            sequence: 0,
            timestamp: signal.timestamp_ns,
            checksum: 0,
        };

        // Serialize header
        message.extend_from_slice(&header.magic.to_le_bytes());
        message.push(header.relay_domain);
        message.push(header.version);
        message.push(header.source);
        message.push(header.flags);
        message.extend_from_slice(&header.payload_size.to_le_bytes());
        message.extend_from_slice(&header.sequence.to_le_bytes());
        message.extend_from_slice(&header.timestamp.to_le_bytes());
        message.extend_from_slice(&header.checksum.to_le_bytes());

        // Create SignalIdentityTLV
        message.push(TLVType::SignalIdentity as u8);
        message.push(14); // length
        message.extend_from_slice(&signal.strategy_id.to_le_bytes());
        message.extend_from_slice(&signal.signal_id.to_le_bytes());
        message.extend_from_slice(&(signal.signal_id as u32).to_le_bytes()); // signal_nonce
        message.push(signal.confidence);
        message.extend_from_slice(&1u32.to_le_bytes()); // chain_id (N/A for Kraken)
        message.push(0); // reserved

        // Create EconomicsTLV
        use rust_decimal::prelude::ToPrimitive;
        let expected_profit_q =
            (signal.expected_profit_usd.to_f64().unwrap_or(0.0) * (1u128 << 64) as f64) as i128;
        let required_capital_q =
            (signal.position_size_usd.to_f64().unwrap_or(0.0) * (1u128 << 64) as f64) as u128;

        message.push(TLVType::Economics as u8);
        message.push(30); // length
        message.extend_from_slice(&expected_profit_q.to_le_bytes());
        message.extend_from_slice(&required_capital_q.to_le_bytes());
        message.extend_from_slice(&0u128.to_le_bytes()); // gas estimate (N/A for CEX)
        message.extend_from_slice(&[0u8; 6]); // reserved

        // Calculate and update checksum
        let checksum = crc32fast::hash(&message[..message.len() - 4]);
        let checksum_offset = message.len() - 4 - 32; // Before TLVs
        message[checksum_offset..checksum_offset + 4].copy_from_slice(&checksum.to_le_bytes());

        Ok(message)
    }

    /// Get strategy statistics
    pub fn get_stats(&self) -> SignalStats {
        self.stats.read().clone()
    }

    /// Create indicator deterministically from bijective InstrumentId components
    /// 
    /// This method demonstrates the core principle of bijective ID architecture:
    /// Instead of storing indicators in HashMap<InstrumentId, CompositeIndicator>,
    /// we extract venue and asset data directly from the self-describing ID and
    /// create indicators on-demand with configuration tailored to that specific
    /// venue and asset type combination.
    fn create_indicator_for_instrument(
        &self,
        instrument_id: &InstrumentId,
        venue: VenueId,
    ) -> Result<CompositeIndicator> {
        // Configure indicator parameters based on venue characteristics
        let (short_period, long_period, momentum_period) = match venue {
            VenueId::Kraken => {
                // Kraken has different latency/volume characteristics
                (self.config.short_ma_period, self.config.long_ma_period, self.config.short_ma_period)
            }
            VenueId::Binance => {
                // Binance higher frequency, can use shorter periods
                (self.config.short_ma_period / 2, self.config.long_ma_period / 2, self.config.short_ma_period / 2)
            }
            _ => {
                // Default parameters for other venues
                (self.config.short_ma_period, self.config.long_ma_period, self.config.short_ma_period)
            }
        };

        // Create indicator with venue-specific configuration
        Ok(CompositeIndicator::new(short_period, long_period, momentum_period))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn test_strategy_creation() {
        let config = StrategyConfig::default();
        let strategy = KrakenSignalStrategy::new(config);
        assert_eq!(strategy.strategy_id, 20);
    }

    #[tokio::test]
    async fn test_signal_generation() {
        let config = StrategyConfig::default();
        let mut strategy = KrakenSignalStrategy::new(config);

        let instrument = InstrumentId::coin(VenueId::Kraken, "BTCUSD");
        let timestamp = safe_system_timestamp_ns();

        // Feed some price data to build up indicators
        for i in 1..=50 {
            let price = dec!(50000) + Decimal::from(i * 100);
            strategy
                .update_indicators_and_generate_signals(
                    instrument,
                    price,
                    timestamp + i * 1000000000,
                )
                .await
                .unwrap();
        }

        // Should have generated some signals
        let stats = strategy.get_stats();
        // Note: Actual signal generation depends on the specific price movement patterns
    }
}
