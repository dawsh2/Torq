//! DEX event structures and decoders
//!
//! Provides semantic validation and type-safe decoding of blockchain events
//! using ethabi, preventing manual byte parsing errors and data truncation.

use super::uniswap_v2;
use super::uniswap_v3;
use super::DEXProtocol;
use ethabi::RawLog;
use web3::types::{Log, H160, U256};

/// Error types for ABI decoding
#[derive(Debug, thiserror::Error)]
pub enum DecodingError {
    #[error("Unknown event signature: {0}")]
    UnknownEventSignature(String),

    #[error("ABI parsing failed: {0}")]
    AbiParsingError(String),

    #[error("Missing required field: {0}")]
    MissingField(String),

    #[error("Value overflow: {value} exceeds u128::MAX")]
    ValueOverflow { value: String },

    #[error("Invalid token order in event")]
    InvalidTokenOrder,

    #[error("Unsupported DEX protocol: {0:?}")]
    UnsupportedProtocol(DEXProtocol),
}

/// Validated swap data with semantic correctness
#[derive(Debug, Clone)]
pub struct ValidatedSwap {
    pub pool_address: [u8; 20],
    pub amount_in: u128,
    pub amount_out: u128,
    pub token_in_is_token0: bool,
    pub sqrt_price_x96_after: u128,
    pub tick_after: i32,
    pub liquidity_after: u128,
    pub dex_protocol: DEXProtocol,
}

/// Validated mint data
#[derive(Debug, Clone)]
pub struct ValidatedMint {
    pub pool_address: [u8; 20],
    pub liquidity_provider: [u8; 20],
    pub liquidity_delta: u128,
    pub amount0: u128,
    pub amount1: u128,
    pub tick_lower: i32,
    pub tick_upper: i32,
    pub dex_protocol: DEXProtocol,
}

/// Validated burn data
#[derive(Debug, Clone)]
pub struct ValidatedBurn {
    pub pool_address: [u8; 20],
    pub liquidity_provider: [u8; 20],
    pub liquidity_delta: u128,
    pub amount0: u128,
    pub amount1: u128,
    pub tick_lower: i32,
    pub tick_upper: i32,
    pub dex_protocol: DEXProtocol,
}

/// Detect DEX protocol from pool address and log structure
pub fn detect_dex_protocol(pool_address: &H160, log: &Log) -> DEXProtocol {
    // V3 Swap events have distinctive characteristics:
    // 1. Data field contains exactly 5 words (160 bytes): amount0, amount1, sqrtPriceX96, liquidity, tick
    // 2. Topics: [event_sig, sender, recipient]
    //
    // V2 Swap events:
    // 1. Data field contains 4 words (128 bytes): amount0In, amount1In, amount0Out, amount1Out
    // 2. Topics: [event_sig, sender, to]
    
    let data_len = log.data.0.len();
    let topics_len = log.topics.len();
    
    // Check by data length - most reliable indicator
    if data_len == 160 && topics_len == 3 {
        // V3 swap: 5 * 32 bytes = 160 bytes
        // Check address patterns for specific V3 implementations
        let addr_bytes = pool_address.as_bytes();
        if addr_bytes[0] == 0x5C || addr_bytes[0] == 0x45 {
            DEXProtocol::QuickswapV3
        } else {
            DEXProtocol::UniswapV3
        }
    } else if data_len == 128 && topics_len == 3 {
        // V2 swap: 4 * 32 bytes = 128 bytes
        let addr_bytes = pool_address.as_bytes();
        if addr_bytes[0] == 0xc3 {
            DEXProtocol::SushiswapV2
        } else if addr_bytes[0] == 0x80 || addr_bytes[0] == 0xa5 {
            DEXProtocol::QuickswapV2
        } else {
            DEXProtocol::UniswapV2
        }
    } else {
        // Default to V2 for unknown formats
        DEXProtocol::UniswapV2
    }
}

/// ABI decoder for Swap events
pub struct SwapEventDecoder;

impl SwapEventDecoder {
    /// Decode swap event based on protocol type
    pub fn decode_swap_event(
        log: &Log,
        protocol: DEXProtocol,
    ) -> Result<ValidatedSwap, DecodingError> {
        let raw_log = RawLog {
            topics: log.topics.clone(),
            data: log.data.0.clone(),
        };

        match protocol {
            DEXProtocol::UniswapV3 | DEXProtocol::QuickswapV3 => {
                Self::decode_v3_swap(log.address, raw_log, protocol)
            }
            DEXProtocol::UniswapV2 | DEXProtocol::SushiswapV2 | DEXProtocol::QuickswapV2 => {
                Self::decode_v2_swap(log.address, raw_log, protocol)
            }
        }
    }

    /// Decode V3 swap event
    fn decode_v3_swap(
        pool_address: H160,
        raw_log: RawLog,
        protocol: DEXProtocol,
    ) -> Result<ValidatedSwap, DecodingError> {
        let event = uniswap_v3::swap_event();
        let decoded = event
            .parse_log(raw_log)
            .map_err(|e| DecodingError::AbiParsingError(e.to_string()))?;

        // Extract amounts (can be negative in V3)
        let amount0 = decoded
            .params
            .get(2)
            .and_then(|p| p.value.clone().into_int())
            .ok_or(DecodingError::MissingField("amount0".to_string()))?;

        let amount1 = decoded
            .params
            .get(3)
            .and_then(|p| p.value.clone().into_int())
            .ok_or(DecodingError::MissingField("amount1".to_string()))?;

        // Determine trade direction based on signs
        let (amount_in, amount_out, token_in_is_token0) = if amount0 > U256::zero() {
            // Token0 in (positive), Token1 out (negative)
            (amount0, amount1.overflowing_neg().0, true)
        } else {
            // Token1 in (positive), Token0 out (negative)
            (amount1, amount0.overflowing_neg().0, false)
        };

        // Check for overflow before converting to u128
        let amount_in_u128 = Self::safe_u256_to_u128(amount_in)?;
        let amount_out_u128 = Self::safe_u256_to_u128(amount_out)?;

        // Extract V3-specific fields
        let sqrt_price_x96 = decoded
            .params
            .get(4)
            .and_then(|p| p.value.clone().into_uint())
            .ok_or(DecodingError::MissingField("sqrtPriceX96".to_string()))?;

        let liquidity = decoded
            .params
            .get(5)
            .and_then(|p| p.value.clone().into_uint())
            .ok_or(DecodingError::MissingField("liquidity".to_string()))?;

        let tick = decoded
            .params
            .get(6)
            .and_then(|p| p.value.clone().into_int())
            .ok_or(DecodingError::MissingField("tick".to_string()))?;

        let tick_value = Self::safe_u256_to_tick(tick);
        
        // Safely convert sqrt_price_x96 and liquidity to u128
        // These can be very large values in V3 pools
        let sqrt_price_x96_u128 = Self::safe_u256_to_u128(sqrt_price_x96)?;
        let liquidity_u128 = Self::safe_u256_to_u128(liquidity)?;
        
        Ok(ValidatedSwap {
            pool_address: pool_address.0,
            amount_in: amount_in_u128,
            amount_out: amount_out_u128,
            token_in_is_token0,
            sqrt_price_x96_after: sqrt_price_x96_u128,
            tick_after: tick_value,
            liquidity_after: liquidity_u128,
            dex_protocol: protocol,
        })
    }

    /// Decode V2 swap event
    fn decode_v2_swap(
        pool_address: H160,
        raw_log: RawLog,
        protocol: DEXProtocol,
    ) -> Result<ValidatedSwap, DecodingError> {
        let event = uniswap_v2::swap_event();
        let decoded = event
            .parse_log(raw_log)
            .map_err(|e| DecodingError::AbiParsingError(e.to_string()))?;

        // Extract all amounts
        let amount0_in = decoded
            .params
            .get(1)
            .and_then(|p| p.value.clone().into_uint())
            .ok_or(DecodingError::MissingField("amount0In".to_string()))?;

        let amount1_in = decoded
            .params
            .get(2)
            .and_then(|p| p.value.clone().into_uint())
            .ok_or(DecodingError::MissingField("amount1In".to_string()))?;

        let amount0_out = decoded
            .params
            .get(3)
            .and_then(|p| p.value.clone().into_uint())
            .ok_or(DecodingError::MissingField("amount0Out".to_string()))?;

        let amount1_out = decoded
            .params
            .get(4)
            .and_then(|p| p.value.clone().into_uint())
            .ok_or(DecodingError::MissingField("amount1Out".to_string()))?;

        // Determine trade direction
        let (amount_in, amount_out, token_in_is_token0) = if amount0_in > U256::zero() {
            (amount0_in, amount1_out, true)
        } else if amount1_in > U256::zero() {
            (amount1_in, amount0_out, false)
        } else {
            return Err(DecodingError::InvalidTokenOrder);
        };

        // Safe conversion with overflow check
        let amount_in_u128 = Self::safe_u256_to_u128(amount_in)?;
        let amount_out_u128 = Self::safe_u256_to_u128(amount_out)?;

        Ok(ValidatedSwap {
            pool_address: pool_address.0,
            amount_in: amount_in_u128,
            amount_out: amount_out_u128,
            token_in_is_token0,
            sqrt_price_x96_after: 0, // V2 doesn't have this
            tick_after: 0,           // V2 doesn't have ticks
            liquidity_after: 0,      // V2 doesn't expose this in swap
            dex_protocol: protocol,
        })
    }

    /// Safely convert U256 to u128 with overflow detection
    pub fn safe_u256_to_u128(value: U256) -> Result<u128, DecodingError> {
        if value > U256::from(u128::MAX) {
            return Err(DecodingError::ValueOverflow {
                value: format!("{}", value),
            });
        }
        Ok(value.as_u128())
    }

    /// Safely convert U256 tick value to i32 with proper bounds checking
    /// 
    /// Uniswap V3 ticks are int24 values ranging from -887,272 to +887,272.
    /// On-chain they're stored as U256 but need to be interpreted as signed integers.
    /// Large U256 values (> 0x7FFFFFFF) represent negative ticks using two's complement.
    pub fn safe_u256_to_tick(value: U256) -> i32 {
        const MIN_TICK: i32 = -887272;
        const MAX_TICK: i32 = 887272;
        
        // Handle two's complement - if the value is very large, it's a negative tick
        if value > U256::from(i32::MAX) {
            // This is a negative value in two's complement
            // Convert U256 to u64 and then interpret as i32
            let low_u64 = value.low_u64();
            let tick = low_u64 as i32;
            tick.clamp(MIN_TICK, MAX_TICK)
        } else {
            // Positive tick value
            let tick = value.as_u64() as i32;
            tick.clamp(MIN_TICK, MAX_TICK)
        }
    }

    /// Legacy function for backward compatibility - deprecated
    #[deprecated(note = "Use safe_u256_to_u128 instead to preserve full precision")]
    pub fn safe_u256_to_i64(value: U256) -> Result<i64, DecodingError> {
        if value > U256::from(i64::MAX) {
            // For very large values, truncate to i64::MAX with warning
            tracing::warn!("Value overflow: {} > i64::MAX, truncating", value);
            Ok(i64::MAX)
        } else {
            Ok(value.as_u64() as i64)
        }
    }
}

/// ABI decoder for Mint events
pub struct MintEventDecoder;

impl MintEventDecoder {
    /// Decode mint event based on protocol
    pub fn decode_mint_event(
        log: &Log,
        protocol: DEXProtocol,
    ) -> Result<ValidatedMint, DecodingError> {
        let raw_log = RawLog {
            topics: log.topics.clone(),
            data: log.data.0.clone(),
        };

        match protocol {
            DEXProtocol::UniswapV3 | DEXProtocol::QuickswapV3 => {
                Self::decode_v3_mint(log.address, raw_log, protocol)
            }
            DEXProtocol::UniswapV2 | DEXProtocol::SushiswapV2 | DEXProtocol::QuickswapV2 => {
                Self::decode_v2_mint(log.address, raw_log, protocol)
            }
        }
    }

    /// Decode V3 mint event
    fn decode_v3_mint(
        pool_address: H160,
        raw_log: RawLog,
        protocol: DEXProtocol,
    ) -> Result<ValidatedMint, DecodingError> {
        let event = uniswap_v3::mint_event();
        let decoded = event
            .parse_log(raw_log)
            .map_err(|e| DecodingError::AbiParsingError(e.to_string()))?;

        // Extract liquidity provider from owner (indexed)
        let owner = decoded
            .params
            .get(1)
            .and_then(|p| p.value.clone().into_address())
            .ok_or(DecodingError::MissingField("owner".to_string()))?;

        // Extract tick range (indexed)
        let tick_lower = decoded
            .params
            .get(2)
            .and_then(|p| p.value.clone().into_int())
            .ok_or(DecodingError::MissingField("tickLower".to_string()))?;

        let tick_upper = decoded
            .params
            .get(3)
            .and_then(|p| p.value.clone().into_int())
            .ok_or(DecodingError::MissingField("tickUpper".to_string()))?;

        // Extract liquidity and amounts
        let liquidity = decoded
            .params
            .get(4)
            .and_then(|p| p.value.clone().into_uint())
            .ok_or(DecodingError::MissingField("amount".to_string()))?;

        let amount0 = decoded
            .params
            .get(5)
            .and_then(|p| p.value.clone().into_uint())
            .ok_or(DecodingError::MissingField("amount0".to_string()))?;

        let amount1 = decoded
            .params
            .get(6)
            .and_then(|p| p.value.clone().into_uint())
            .ok_or(DecodingError::MissingField("amount1".to_string()))?;

        // Safely convert U256 values to u128
        let liquidity_u128 = SwapEventDecoder::safe_u256_to_u128(liquidity)?;
        let amount0_u128 = SwapEventDecoder::safe_u256_to_u128(amount0)?;
        let amount1_u128 = SwapEventDecoder::safe_u256_to_u128(amount1)?;
        
        Ok(ValidatedMint {
            pool_address: pool_address.0,
            liquidity_provider: owner.0,
            liquidity_delta: liquidity_u128,
            amount0: amount0_u128,
            amount1: amount1_u128,
            tick_lower: SwapEventDecoder::safe_u256_to_tick(tick_lower),
            tick_upper: SwapEventDecoder::safe_u256_to_tick(tick_upper),
            dex_protocol: protocol,
        })
    }

    /// Decode V2 mint event
    fn decode_v2_mint(
        pool_address: H160,
        raw_log: RawLog,
        protocol: DEXProtocol,
    ) -> Result<ValidatedMint, DecodingError> {
        let event = uniswap_v2::mint_event();
        let decoded = event
            .parse_log(raw_log)
            .map_err(|e| DecodingError::AbiParsingError(e.to_string()))?;

        // Extract sender as liquidity provider
        let sender = decoded
            .params
            .get(0)
            .and_then(|p| p.value.clone().into_address())
            .ok_or(DecodingError::MissingField("sender".to_string()))?;

        // Extract amounts
        let amount0 = decoded
            .params
            .get(1)
            .and_then(|p| p.value.clone().into_uint())
            .ok_or(DecodingError::MissingField("amount0".to_string()))?;

        let amount1 = decoded
            .params
            .get(2)
            .and_then(|p| p.value.clone().into_uint())
            .ok_or(DecodingError::MissingField("amount1".to_string()))?;

        // V2 doesn't have ticks, use full range
        // Safely convert U256 values to u128
        let amount0_u128 = SwapEventDecoder::safe_u256_to_u128(amount0)?;
        let amount1_u128 = SwapEventDecoder::safe_u256_to_u128(amount1)?;
        
        Ok(ValidatedMint {
            pool_address: pool_address.0,
            liquidity_provider: sender.0,
            liquidity_delta: 0, // V2 doesn't expose liquidity in mint
            amount0: amount0_u128,
            amount1: amount1_u128,
            tick_lower: -887272, // MIN_TICK for V2
            tick_upper: 887272,  // MAX_TICK for V2
            dex_protocol: protocol,
        })
    }
}

/// ABI decoder for Burn events
pub struct BurnEventDecoder;

impl BurnEventDecoder {
    /// Decode burn event based on protocol
    pub fn decode_burn_event(
        log: &Log,
        protocol: DEXProtocol,
    ) -> Result<ValidatedBurn, DecodingError> {
        let raw_log = RawLog {
            topics: log.topics.clone(),
            data: log.data.0.clone(),
        };

        match protocol {
            DEXProtocol::UniswapV3 | DEXProtocol::QuickswapV3 => {
                Self::decode_v3_burn(log.address, raw_log, protocol)
            }
            DEXProtocol::UniswapV2 | DEXProtocol::SushiswapV2 | DEXProtocol::QuickswapV2 => {
                Self::decode_v2_burn(log.address, raw_log, protocol)
            }
        }
    }

    /// Decode V3 burn event
    fn decode_v3_burn(
        pool_address: H160,
        raw_log: RawLog,
        protocol: DEXProtocol,
    ) -> Result<ValidatedBurn, DecodingError> {
        let event = uniswap_v3::burn_event();
        let decoded = event
            .parse_log(raw_log)
            .map_err(|e| DecodingError::AbiParsingError(e.to_string()))?;

        // Extract owner
        let owner = decoded
            .params
            .get(0)
            .and_then(|p| p.value.clone().into_address())
            .ok_or(DecodingError::MissingField("owner".to_string()))?;

        // Extract tick range
        let tick_lower = decoded
            .params
            .get(1)
            .and_then(|p| p.value.clone().into_int())
            .ok_or(DecodingError::MissingField("tickLower".to_string()))?;

        let tick_upper = decoded
            .params
            .get(2)
            .and_then(|p| p.value.clone().into_int())
            .ok_or(DecodingError::MissingField("tickUpper".to_string()))?;

        // Extract liquidity and amounts
        let liquidity = decoded
            .params
            .get(3)
            .and_then(|p| p.value.clone().into_uint())
            .ok_or(DecodingError::MissingField("amount".to_string()))?;

        let amount0 = decoded
            .params
            .get(4)
            .and_then(|p| p.value.clone().into_uint())
            .ok_or(DecodingError::MissingField("amount0".to_string()))?;

        let amount1 = decoded
            .params
            .get(5)
            .and_then(|p| p.value.clone().into_uint())
            .ok_or(DecodingError::MissingField("amount1".to_string()))?;

        // Safely convert U256 values to u128
        let liquidity_u128 = SwapEventDecoder::safe_u256_to_u128(liquidity)?;
        let amount0_u128 = SwapEventDecoder::safe_u256_to_u128(amount0)?;
        let amount1_u128 = SwapEventDecoder::safe_u256_to_u128(amount1)?;
        
        Ok(ValidatedBurn {
            pool_address: pool_address.0,
            liquidity_provider: owner.0,
            liquidity_delta: liquidity_u128,
            amount0: amount0_u128,
            amount1: amount1_u128,
            tick_lower: SwapEventDecoder::safe_u256_to_tick(tick_lower),
            tick_upper: SwapEventDecoder::safe_u256_to_tick(tick_upper),
            dex_protocol: protocol,
        })
    }

    /// Decode V2 burn event
    fn decode_v2_burn(
        pool_address: H160,
        raw_log: RawLog,
        protocol: DEXProtocol,
    ) -> Result<ValidatedBurn, DecodingError> {
        let event = uniswap_v2::burn_event();
        let decoded = event
            .parse_log(raw_log)
            .map_err(|e| DecodingError::AbiParsingError(e.to_string()))?;

        // Extract sender
        let _sender = decoded
            .params
            .get(0)
            .and_then(|p| p.value.clone().into_address())
            .ok_or(DecodingError::MissingField("sender".to_string()))?;

        // Extract amounts
        let amount0 = decoded
            .params
            .get(1)
            .and_then(|p| p.value.clone().into_uint())
            .ok_or(DecodingError::MissingField("amount0".to_string()))?;

        let amount1 = decoded
            .params
            .get(2)
            .and_then(|p| p.value.clone().into_uint())
            .ok_or(DecodingError::MissingField("amount1".to_string()))?;

        // Extract recipient
        let to = decoded
            .params
            .get(3)
            .and_then(|p| p.value.clone().into_address())
            .ok_or(DecodingError::MissingField("to".to_string()))?;

        // Safely convert U256 values to u128  
        let amount0_u128 = SwapEventDecoder::safe_u256_to_u128(amount0)?;
        let amount1_u128 = SwapEventDecoder::safe_u256_to_u128(amount1)?;
        
        Ok(ValidatedBurn {
            pool_address: pool_address.0,
            liquidity_provider: to.0, // Use recipient as LP
            liquidity_delta: 0,       // V2 doesn't expose liquidity in burn
            amount0: amount0_u128,
            amount1: amount1_u128,
            tick_lower: -887272, // MIN_TICK for V2
            tick_upper: 887272,  // MAX_TICK for V2
            dex_protocol: protocol,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use web3::types::{Bytes, H256};

    fn create_test_log(topics: Vec<H256>, data: Vec<u8>) -> Log {
        Log {
            address: H160::from_low_u64_be(0x1234),
            topics,
            data: Bytes(data),
            block_hash: None,
            block_number: None,
            transaction_hash: None,
            transaction_index: None,
            log_index: None,
            transaction_log_index: None,
            log_type: None,
            removed: None,
        }
    }

    #[test]
    fn test_safe_u256_to_u128() {
        // Test normal value
        let normal = U256::from(1000000u64);
        assert_eq!(SwapEventDecoder::safe_u256_to_u128(normal).unwrap(), 1000000u128);

        // Test max u128 value
        let max_safe = U256::from(u128::MAX);
        assert_eq!(
            SwapEventDecoder::safe_u256_to_u128(max_safe).unwrap(),
            u128::MAX
        );

        // Test large value that fits in u128
        let large_value = U256::from_dec_str("340282366920938463463374607431768211455").unwrap(); // u128::MAX
        assert_eq!(
            SwapEventDecoder::safe_u256_to_u128(large_value).unwrap(),
            u128::MAX
        );

        // Test overflow (value larger than u128::MAX)
        let overflow = U256::from(u128::MAX) + U256::from(1u64);
        assert!(SwapEventDecoder::safe_u256_to_u128(overflow).is_err());
    }

    #[test]
    fn test_safe_u256_to_i64_legacy() {
        // Test normal value
        let normal = U256::from(1000000);
        assert_eq!(SwapEventDecoder::safe_u256_to_i64(normal).unwrap(), 1000000);

        // Test max i64 value
        let max_safe = U256::from(i64::MAX);
        assert_eq!(
            SwapEventDecoder::safe_u256_to_i64(max_safe).unwrap(),
            i64::MAX
        );

        // Test overflow
        let overflow = U256::from(i64::MAX) + U256::from(1);
        assert_eq!(
            SwapEventDecoder::safe_u256_to_i64(overflow).unwrap(),
            i64::MAX
        );
    }

    #[test]
    fn test_safe_u256_to_tick() {
        // Test positive tick values
        let positive_tick = U256::from(12345u64);
        assert_eq!(SwapEventDecoder::safe_u256_to_tick(positive_tick), 12345);

        // Test zero tick
        let zero_tick = U256::zero();
        assert_eq!(SwapEventDecoder::safe_u256_to_tick(zero_tick), 0);

        // Test maximum valid positive tick
        let max_tick = U256::from(887272u64);
        assert_eq!(SwapEventDecoder::safe_u256_to_tick(max_tick), 887272);

        // Test minimum valid negative tick (represented as large U256 in two's complement)
        // -887272 in two's complement as U256
        let min_tick_u256 = U256::from_str_radix("FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF274B8", 16).unwrap();
        assert_eq!(SwapEventDecoder::safe_u256_to_tick(min_tick_u256), -887272);

        // Test negative tick value (e.g., -1000)
        // -1000 in two's complement as U256 
        let negative_tick_u256 = U256::from_str_radix("FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFC18", 16).unwrap();
        assert_eq!(SwapEventDecoder::safe_u256_to_tick(negative_tick_u256), -1000);

        // Test out-of-range positive tick (should be clamped to MAX_TICK)
        let too_large_tick = U256::from(1000000u64);
        assert_eq!(SwapEventDecoder::safe_u256_to_tick(too_large_tick), 887272);

        // Test out-of-range negative tick (should be clamped to MIN_TICK)
        // Very negative value that would exceed valid range
        let too_negative_u256 = U256::from_str_radix("FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF00000", 16).unwrap();
        assert_eq!(SwapEventDecoder::safe_u256_to_tick(too_negative_u256), -887272);
    }

    #[test]
    fn test_overflow_prevention_demo() {
        // This test demonstrates that our safe conversion prevents the panic
        // that would have occurred with the old `.as_u32()` approach
        
        // Create a U256 value that would cause overflow with as_u32()
        // This represents a negative tick in two's complement (-1001)
        let problematic_tick = U256::MAX - U256::from(1000u64);
        
        // The old unsafe approach would have panicked here:
        // let bad_result = problematic_tick.as_u32() as i32;  // PANIC!
        
        // Our safe approach handles it gracefully:
        let safe_result = SwapEventDecoder::safe_u256_to_tick(problematic_tick);
        
        // This represents -1001 which is valid, so no clamping needed
        assert_eq!(safe_result, -1001);
        
        // Test another problematic case - a value that would be interpreted as -1 in i32
        let edge_case = U256::from(u32::MAX);  // 0xFFFFFFFF -> -1 when cast to i32
        let safe_edge_result = SwapEventDecoder::safe_u256_to_tick(edge_case);
        
        // u32::MAX (0xFFFFFFFF) becomes -1 when interpreted as i32, which is valid
        assert_eq!(safe_edge_result, -1);
        
        // Test a truly out-of-range negative value that should be clamped
        let way_too_negative = U256::MAX - U256::from(2000000u64); // Much more negative than MIN_TICK
        let clamped_result = SwapEventDecoder::safe_u256_to_tick(way_too_negative);
        assert_eq!(clamped_result, -887272); // Should be clamped to MIN_TICK
    }

    #[test]
    fn test_dex_protocol_detection() {
        let v3_log = create_test_log(vec![H256::zero(); 3], vec![0u8; 150]);
        let v2_log = create_test_log(vec![H256::zero(); 3], vec![0u8; 64]);

        let v3_addr = H160::from_low_u64_be(0x1234);
        let v2_addr = H160::from_low_u64_be(0x5678);

        assert_eq!(
            detect_dex_protocol(&v3_addr, &v3_log),
            DEXProtocol::UniswapV3
        );
        assert_eq!(
            detect_dex_protocol(&v2_addr, &v2_log),
            DEXProtocol::UniswapV2
        );
    }
}
