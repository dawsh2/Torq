use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use state_market::pool_state::CachedPoolInfo;
use types::protocol::tlv::pool_state::DEXProtocol;
use std::fs;
use std::path::Path;
use rust_decimal::Decimal;
use tracing::{info, warn};

#[derive(Debug, Deserialize, Serialize)]
struct PoolCacheJson {
    version: u32,
    chain_id: u32,
    pools: Vec<PoolJson>,
}

#[derive(Debug, Deserialize, Serialize)]
struct PoolJson {
    pool_address: String,
    token0: String,
    token1: String,
    token0_decimals: u8,
    token1_decimals: u8,
    protocol: String,
    fee_tier: u32,
    discovered_at: u64,
    venue: String,
}

/// Parse hex address string to 20-byte array
fn parse_hex_address(addr_str: &str) -> Result<[u8; 20]> {
    let cleaned = if addr_str.starts_with("0x") || addr_str.starts_with("0X") {
        &addr_str[2..]
    } else {
        addr_str
    };
    
    let mut bytes = [0u8; 20];
    hex::decode_to_slice(cleaned, &mut bytes)
        .context("Failed to parse hex address")?;
    
    Ok(bytes)
}

/// Load pool cache from JSON file
pub fn load_pool_cache(cache_path: &Path) -> Result<Vec<CachedPoolInfo>> {
    info!("Loading pool cache from {:?}", cache_path);
    
    // Read JSON file
    let json_content = fs::read_to_string(cache_path)
        .context("Failed to read pool cache file")?;
    
    // Parse JSON
    let cache: PoolCacheJson = serde_json::from_str(&json_content)
        .context("Failed to parse pool cache JSON")?;
    
    info!("Found {} pools in cache for chain {}", cache.pools.len(), cache.chain_id);
    
    // Convert to CachedPoolInfo format
    let mut cached_pools = Vec::new();
    
    for pool_json in cache.pools {
        // Parse addresses
        let pool_address = match parse_hex_address(&pool_json.pool_address) {
            Ok(addr) => addr,
            Err(e) => {
                warn!("Skipping pool with invalid address {}: {}", pool_json.pool_address, e);
                continue;
            }
        };
        
        let token0_address = match parse_hex_address(&pool_json.token0) {
            Ok(addr) => addr,
            Err(e) => {
                warn!("Skipping pool with invalid token0 {}: {}", pool_json.token0, e);
                continue;
            }
        };
        
        let token1_address = match parse_hex_address(&pool_json.token1) {
            Ok(addr) => addr,
            Err(e) => {
                warn!("Skipping pool with invalid token1 {}: {}", pool_json.token1, e);
                continue;
            }
        };
        
        // Map protocol string to enum
        let protocol = match pool_json.protocol.as_str() {
            "V2" => DEXProtocol::UniswapV2,
            "V3" => DEXProtocol::UniswapV3,
            "SushiV2" => DEXProtocol::SushiswapV2,
            "QuickV3" => DEXProtocol::QuickswapV3,
            _ => {
                warn!("Unknown protocol '{}', defaulting to UniswapV2", pool_json.protocol);
                DEXProtocol::UniswapV2
            }
        };
        
        // Create CachedPoolInfo with zero reserves (will be updated from events)
        let cached_pool = CachedPoolInfo {
            pool_address,
            token0_address,
            token1_address,
            protocol,
            fee_tier: pool_json.fee_tier,
            reserve0: Decimal::ZERO,
            reserve1: Decimal::ZERO,
        };
        
        cached_pools.push(cached_pool);
    }
    
    info!("Successfully loaded {} pools from cache", cached_pools.len());
    
    // Log token pair statistics
    let mut pair_counts = std::collections::HashMap::new();
    for pool in &cached_pools {
        let pair = format!("{:?}/{:?}", 
            hex::encode(&pool.token0_address[..4]),
            hex::encode(&pool.token1_address[..4])
        );
        *pair_counts.entry(pair).or_insert(0) += 1;
    }
    
    for (pair, count) in pair_counts {
        if count > 1 {
            info!("  Token pair {} has {} pools - good for arbitrage!", pair, count);
        }
    }
    
    Ok(cached_pools)
}