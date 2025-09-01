//! Pool Cache TLV Structures
//!
//! Binary TLV format for persisting discovered pool → token mappings.
//! Uses full 20-byte addresses for execution compatibility.

use crate::protocol::message::header::precise_timestamp_ns as fast_timestamp_ns;
use crate::{define_tlv, VenueId};
use zerocopy::{AsBytes, FromBytes, FromZeroes};

/// Pool type for cache records (renamed to avoid conflict with pool_state)
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CachePoolType {
    UniswapV2 = 1,
    UniswapV3 = 2,
    QuickSwapV2 = 3,
    QuickSwapV3 = 4,
    SushiSwapV2 = 5,
    CurveV2 = 6,
    BalancerV2 = 7,
}

impl TryFrom<u8> for CachePoolType {
    type Error = String;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(CachePoolType::UniswapV2),
            2 => Ok(CachePoolType::UniswapV3),
            3 => Ok(CachePoolType::QuickSwapV2),
            4 => Ok(CachePoolType::QuickSwapV3),
            5 => Ok(CachePoolType::SushiSwapV2),
            6 => Ok(CachePoolType::CurveV2),
            7 => Ok(CachePoolType::BalancerV2),
            _ => Err(format!("Invalid pool type: {}", value)),
        }
    }
}

// Individual pool information record using macro for consistency
define_tlv! {
    /// Individual pool information record (Type 200, 88 bytes)
    /// Stores full 20-byte addresses for pool and both tokens
    PoolInfoTLV {
        u64: {
            discovered_at: u64, // Unix timestamp when discovered
            last_seen: u64      // Last time this pool had activity
        }
        u32: { fee_tier: u32 } // Fee in basis points (30 = 0.3%)
        u16: { venue: u16 }    // VenueId enum
        u8: {
            tlv_type: u8,        // 200
            tlv_length: u8,      // 88 bytes
            token0_decimals: u8, // Token0 decimal places (e.g., 18 for WETH, 6 for USDC)
            token1_decimals: u8, // Token1 decimal places
            pool_type: u8,       // CachePoolType enum discriminant
            _padding: u8         // Explicit padding
        }
        special: {
            pool_address: [u8; 20],   // Full pool contract address
            token0_address: [u8; 20], // Full token0 address
            token1_address: [u8; 20]  // Full token1 address
        }
    }
}

/// Configuration for creating PoolInfoTLV
#[derive(Debug, Clone)]
pub struct PoolInfoConfig {
    pub pool_address: [u8; 20],
    pub token0_address: [u8; 20],
    pub token1_address: [u8; 20],
    pub token0_decimals: u8,
    pub token1_decimals: u8,
    pub pool_type: CachePoolType,
    pub fee_tier: u32,
    pub venue: VenueId,
    pub discovered_at: u64,
    pub last_seen: u64,
}

impl PoolInfoTLV {
    pub const TYPE: u8 = 200;
    pub const SIZE: usize = std::mem::size_of::<Self>(); // Actual size with padding

    /// Create a new pool info record from config (semantic constructor)
    pub fn new(config: PoolInfoConfig) -> Self {
        Self::from_config(config)
    }

    /// Create a new pool info record from config
    pub fn from_config(config: PoolInfoConfig) -> Self {
        // Use macro-generated new_raw() with proper field order
        Self::new_raw(
            config.discovered_at,
            config.last_seen,
            config.fee_tier,
            config.venue as u16,
            Self::TYPE,
            (Self::SIZE - 2) as u8, // Exclude type and length fields
            config.token0_decimals,
            config.token1_decimals,
            config.pool_type as u8,
            0, // _padding
            config.pool_address,
            config.token0_address,
            config.token1_address,
        )
    }

    /// Validate the TLV structure
    pub fn validate(&self) -> Result<(), String> {
        if self.tlv_type != Self::TYPE {
            return Err(format!(
                "Invalid TLV type: expected {}, got {}",
                Self::TYPE,
                self.tlv_type
            ));
        }

        if self.tlv_length != (Self::SIZE - 2) as u8 {
            return Err(format!(
                "Invalid TLV length: expected {}, got {}",
                Self::SIZE - 2,
                self.tlv_length
            ));
        }

        // Validate decimals are reasonable
        if self.token0_decimals > 30 || self.token1_decimals > 30 {
            return Err(format!(
                "Invalid decimals: token0={}, token1={}",
                self.token0_decimals, self.token1_decimals
            ));
        }

        // Validate fee tier is reasonable (0-10000 basis points = 0-100%)
        if self.fee_tier > 10000 {
            let fee_tier = self.fee_tier; // Copy to avoid packed field reference
            return Err(format!("Invalid fee tier: {} basis points", fee_tier));
        }

        Ok(())
    }
}

/// Pool cache file header (Not a TLV itself, but file metadata)
/// Fields ordered to eliminate padding: u64 → [u8;32] → u32 → [u8;4]
#[repr(C)]
#[derive(Debug, Clone, Copy, AsBytes, FromBytes, FromZeroes)]
pub struct PoolCacheFileHeader {
    // Group 64-bit fields first
    pub chain_id: u64,     // Blockchain chain ID (137 for Polygon)
    pub last_updated: u64, // Unix timestamp of last update

    // Then large arrays (naturally aligned)
    pub reserved: [u8; 32], // Reserved for future use

    // Then 32-bit fields
    pub version: u32,    // Cache format version (currently 1)
    pub pool_count: u32, // Number of pools in cache
    pub checksum: u32,   // CRC32 checksum of all pool data

    // Finally small arrays
    pub magic: [u8; 4], // "POOL" magic bytes
}

impl PoolCacheFileHeader {
    pub const MAGIC: [u8; 4] = *b"POOL";
    pub const VERSION: u32 = 1;
    pub const SIZE: usize = std::mem::size_of::<Self>();

    /// Create a new cache file header
    pub fn new(chain_id: u64, pool_count: u32, checksum: u32) -> Self {
        Self {
            magic: Self::MAGIC,
            version: Self::VERSION,
            chain_id,
            pool_count,
            last_updated: (fast_timestamp_ns() / 1_000_000_000), // Convert ns to seconds
            checksum,
            reserved: [0; 32],
        }
    }

    /// Validate the header
    pub fn validate(&self) -> Result<(), String> {
        if self.magic != Self::MAGIC {
            return Err(format!(
                "Invalid magic bytes: expected {:?}, got {:?}",
                Self::MAGIC,
                self.magic
            ));
        }

        if self.version != Self::VERSION {
            let version = self.version; // Copy to avoid packed field reference
            return Err(format!("Unsupported version: {}", version));
        }

        Ok(())
    }
}

/// Journal entry for incremental updates
/// Fields ordered to eliminate padding: u64 → PoolInfoTLV → u8
#[repr(C)]
#[derive(Debug, Clone, Copy, AsBytes, FromBytes, FromZeroes)]
pub struct PoolCacheJournalEntry {
    // Group 64-bit fields first
    pub timestamp: u64, // When this operation occurred

    // Then embedded struct (already aligned)
    pub pool_info: PoolInfoTLV, // The pool data

    // Finally 8-bit fields (need 7 bytes to align to 8-byte boundary)
    pub operation: u8,     // 1=add, 2=update, 3=delete
    pub _padding: [u8; 7], // Explicit padding
}

impl PoolCacheJournalEntry {
    pub const OP_ADD: u8 = 1;
    pub const OP_UPDATE: u8 = 2;
    pub const OP_DELETE: u8 = 3;
    pub const SIZE: usize = std::mem::size_of::<Self>();

    pub fn new_add(pool_info: PoolInfoTLV) -> Self {
        Self {
            operation: Self::OP_ADD,
            timestamp: (fast_timestamp_ns() / 1_000_000_000), // Convert ns to seconds
            pool_info,
            _padding: [0; 7],
        }
    }

    pub fn new_update(pool_info: PoolInfoTLV) -> Self {
        Self {
            operation: Self::OP_UPDATE,
            timestamp: (fast_timestamp_ns() / 1_000_000_000), // Convert ns to seconds
            pool_info,
            _padding: [0; 7],
        }
    }

    /// Validate the journal entry
    pub fn validate(&self) -> Result<(), String> {
        if self.operation < 1 || self.operation > 3 {
            return Err(format!("Invalid operation: {}", self.operation));
        }

        // Validate embedded pool info
        self.pool_info.validate()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pool_info_tlv_size() {
        assert_eq!(std::mem::size_of::<PoolInfoTLV>(), PoolInfoTLV::SIZE);
    }

    #[test]
    fn test_pool_cache_header_size() {
        assert_eq!(
            std::mem::size_of::<PoolCacheFileHeader>(),
            PoolCacheFileHeader::SIZE
        );
    }

    #[test]
    fn test_pool_info_validation() {
        let config = PoolInfoConfig {
            pool_address: [0x45; 20],   // pool address
            token0_address: [0x11; 20], // token0
            token1_address: [0x22; 20], // token1
            token0_decimals: 18,        // WETH decimals
            token1_decimals: 6,         // USDC decimals
            pool_type: CachePoolType::UniswapV3,
            fee_tier: 30, // 0.3% fee
            venue: VenueId::Polygon,
            discovered_at: 1700000000,
            last_seen: 1700000000,
        };
        let pool_info = PoolInfoTLV::new(config);

        assert!(pool_info.validate().is_ok());
    }

    #[test]
    fn test_header_validation() {
        let header = PoolCacheFileHeader::new(137, 1000, 0xDEADBEEF);
        assert!(header.validate().is_ok());

        // Test invalid magic
        let mut bad_header = header;
        bad_header.magic = *b"BADM";
        assert!(bad_header.validate().is_err());
    }

    #[test]
    fn test_journal_entry_creation() {
        let config = PoolInfoConfig {
            pool_address: [0x45; 20],   // pool address
            token0_address: [0x11; 20], // token0
            token1_address: [0x22; 20], // token1
            token0_decimals: 18,        // WETH decimals
            token1_decimals: 6,         // USDC decimals
            pool_type: CachePoolType::UniswapV3,
            fee_tier: 30, // 0.3% fee
            venue: VenueId::Polygon,
            discovered_at: 1700000000,
            last_seen: 1700000000,
        };
        let pool_info = PoolInfoTLV::new(config);

        let journal_entry = PoolCacheJournalEntry::new_add(pool_info);
        assert_eq!(journal_entry.operation, PoolCacheJournalEntry::OP_ADD);
        assert!(journal_entry.validate().is_ok());
    }

    #[test]
    fn test_journal_entry_size() {
        // Verify the journal entry has the expected size
        assert_eq!(
            std::mem::size_of::<PoolCacheJournalEntry>(),
            PoolCacheJournalEntry::SIZE
        );

        // Size includes padding for struct alignment
        assert!(
            PoolCacheJournalEntry::SIZE >= 94,
            "Should be at least 94 bytes (1 + 8 + 85), actual: {}",
            PoolCacheJournalEntry::SIZE
        );
    }

    #[test]
    fn test_pool_info_bijection() {
        let config = PoolInfoConfig {
            pool_address: [0x45; 20],   // pool address
            token0_address: [0x11; 20], // token0
            token1_address: [0x22; 20], // token1
            token0_decimals: 18,        // WETH decimals
            token1_decimals: 6,         // USDC decimals
            pool_type: CachePoolType::UniswapV3,
            fee_tier: 30, // 0.3% fee
            venue: VenueId::Polygon,
            discovered_at: 1700000000,
            last_seen: 1700000000,
        };
        let original = PoolInfoTLV::new(config);

        // Convert to bytes and back
        let bytes = original.as_bytes();
        let reconstructed = zerocopy::Ref::<_, PoolInfoTLV>::new_from_prefix(bytes)
            .unwrap()
            .0;

        // Should be identical
        assert_eq!(original.pool_address, reconstructed.pool_address);
        assert_eq!(original.token0_address, reconstructed.token0_address);
        assert_eq!(original.token1_address, reconstructed.token1_address);

        // Copy values from packed structs to avoid alignment issues
        let original_fee_tier = original.fee_tier;
        let reconstructed_fee_tier = reconstructed.fee_tier;
        assert_eq!(original_fee_tier, reconstructed_fee_tier);
    }
}
