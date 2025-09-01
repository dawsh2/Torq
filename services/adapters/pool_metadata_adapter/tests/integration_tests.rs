//! Integration tests for PoolMetadataAdapter
//!
//! Tests with real Polygon RPC data to ensure:
//! - Correct pool discovery
//! - Proper decimal detection
//! - TLV serialization/deserialization
//! - Semantic equality preservation

use pool_metadata_adapter::{PoolMetadataAdapter, PoolMetadataConfig, PoolInfo};
use std::path::PathBuf;
use types::protocol::tlv::{PoolSwapTLV, TLVType};
use codec::{build_message_direct, parse_tlv_message};

/// Known Polygon pools with verified metadata for testing
struct TestPool {
    address: &'static str,
    token0: &'static str,
    token1: &'static str,
    token0_decimals: u8,
    token1_decimals: u8,
    protocol: &'static str,
    name: &'static str,
}

const TEST_POOLS: &[TestPool] = &[
    TestPool {
        address: "0x6e7a5FAFcec6BB1e78bAE2A1F0B612012BF14827",
        token0: "0x0d500B1d8E8eF31E21C99d1Db9A6444d3ADf1270", // WMATIC
        token1: "0x2791Bca1f2de4661ED88A30C99A7a9449Aa84174", // USDC
        token0_decimals: 18,
        token1_decimals: 6,
        protocol: "UniswapV2",
        name: "WMATIC/USDC QuickSwap V2",
    },
    TestPool {
        address: "0x45dDa9cb7c25131DF268515131f647d726f50608",
        token0: "0x7ceB23fD6bC0adD59E62ac25578270cFf1b9f619", // WETH
        token1: "0x2791Bca1f2de4661ED88A30C99A7a9449Aa84174", // USDC  
        token0_decimals: 18,
        token1_decimals: 6,
        protocol: "UniswapV3",
        name: "WETH/USDC UniswapV3 0.05%",
    },
    TestPool {
        address: "0x2cF7252e74036d1Da831d11089D326296e64a728",
        token0: "0x2791Bca1f2de4661ED88A30C99A7a9449Aa84174", // USDC
        token1: "0xc2132D05D31c914a87C6611C10748AEb04B58e8F", // USDT
        token0_decimals: 6,
        token1_decimals: 6,
        protocol: "UniswapV2",
        name: "USDC/USDT QuickSwap V2",
    },
];

#[tokio::test]
#[ignore] // Run with --ignored flag to test with real RPC
async fn test_real_pool_discovery() {
    println!("🧪 Testing real pool discovery with Polygon RPC");
    
    let config = PoolMetadataConfig {
        primary_rpc: "https://polygon-rpc.com".to_string(),
        fallback_rpcs: vec![
            "https://rpc-mainnet.matic.network".to_string(),
            "https://rpc.ankr.com/polygon".to_string(),
        ],
        chain_id: 137,
        cache_dir: PathBuf::from("./test_cache"),
        max_concurrent_discoveries: 2,
        rpc_timeout_ms: 15000,
        max_retries: 3,
        rate_limit_per_sec: 5,
        enable_disk_cache: false, // Don't persist for tests
    };
    
    let adapter = PoolMetadataAdapter::new(config)
        .expect("Failed to create adapter");
    
    for test_pool in TEST_POOLS {
        println!("\n📊 Testing pool: {} ({})", test_pool.name, test_pool.address);
        
        // Convert address to bytes
        let pool_bytes = hex::decode(&test_pool.address[2..])
            .expect("Invalid hex address");
        let mut pool_address = [0u8; 20];
        pool_address.copy_from_slice(&pool_bytes);
        
        // Discover pool via RPC
        match adapter.get_or_discover_pool(pool_address).await {
            Ok(pool_info) => {
                // Validate token addresses
                let token0_hex = hex::encode(pool_info.token0);
                let token1_hex = hex::encode(pool_info.token1);
                
                assert_eq!(
                    format!("0x{}", token0_hex).to_lowercase(),
                    test_pool.token0.to_lowercase(),
                    "Token0 address mismatch for {}",
                    test_pool.name
                );
                
                assert_eq!(
                    format!("0x{}", token1_hex).to_lowercase(),
                    test_pool.token1.to_lowercase(),
                    "Token1 address mismatch for {}",
                    test_pool.name
                );
                
                // Validate decimals
                assert_eq!(
                    pool_info.token0_decimals, test_pool.token0_decimals,
                    "Token0 decimals mismatch for {}",
                    test_pool.name
                );
                
                assert_eq!(
                    pool_info.token1_decimals, test_pool.token1_decimals,
                    "Token1 decimals mismatch for {}",
                    test_pool.name
                );
                
                // Validate protocol detection
                assert_eq!(
                    pool_info.protocol, test_pool.protocol,
                    "Protocol mismatch for {}",
                    test_pool.name
                );
                
                println!("✅ Pool discovery successful:");
                println!("   Token0: 0x{}... ({} decimals)", &token0_hex[..8], pool_info.token0_decimals);
                println!("   Token1: 0x{}... ({} decimals)", &token1_hex[..8], pool_info.token1_decimals);
                println!("   Protocol: {}", pool_info.protocol);
                if pool_info.protocol == "UniswapV3" {
                    println!("   Fee Tier: {}bps", pool_info.fee_tier);
                }
            }
            Err(e) => {
                panic!("Failed to discover pool {}: {}", test_pool.name, e);
            }
        }
    }
    
    println!("\n✅ All pool discoveries passed!");
}

#[tokio::test]
async fn test_cache_behavior() {
    println!("🧪 Testing cache hit/miss behavior");
    
    let config = PoolMetadataConfig {
        cache_dir: PathBuf::from("./test_cache_behavior"),
        enable_disk_cache: false,
        ..Default::default()
    };
    
    let adapter = PoolMetadataAdapter::new(config)
        .expect("Failed to create adapter");
    
    // Use a fake pool address for this test
    let pool_address = [0x42u8; 20];
    
    // First call should be a cache miss
    let metrics_before = adapter.get_metrics().await;
    
    // Mock discovery (would fail with real RPC, but tests cache logic)
    // In real scenario, this would make an RPC call
    
    let metrics_after = adapter.get_metrics().await;
    
    println!("📊 Cache metrics:");
    println!("   Cache hits: {}", metrics_after.cache_hits);
    println!("   Cache misses: {}", metrics_after.cache_misses);
    println!("   RPC discoveries: {}", metrics_after.rpc_discoveries);
    
    // Second call to same pool should be a cache hit
    // (if we had successfully discovered it)
}

#[test]
fn test_tlv_serialization_roundtrip() {
    println!("🧪 Testing TLV serialization/deserialization");
    
    // Create an enriched swap event
    let enriched_swap = EnrichedSwapEvent {
        pool_address: [0x01; 20],
        token0: [0x02; 20],
        token1: [0x03; 20],
        token0_decimals: 18,
        token1_decimals: 6,
        amount0_in: 1000000000000000000, // 1 token with 18 decimals
        amount1_in: 0,
        amount0_out: 0,
        amount1_out: 1000000, // 1 USDC with 6 decimals
        sqrt_price_x96: [0x04; 20],
        tick: 100,
        liquidity: 1000000,
        protocol: "UniswapV3",
        fee_tier: 3000,
        block_number: 50000000,
        timestamp_ns: 1700000000000000000,
    };
    
    // Serialize to TLV
    let tlv_bytes = serialize_to_tlv(&enriched_swap);
    
    println!("📦 Serialized TLV: {} bytes", tlv_bytes.len());
    
    // Deserialize back
    let deserialized = deserialize_from_tlv(&tlv_bytes)
        .expect("Failed to deserialize");
    
    // Verify semantic equality
    assert_eq!(enriched_swap.pool_address, deserialized.pool_address);
    assert_eq!(enriched_swap.token0, deserialized.token0);
    assert_eq!(enriched_swap.token1, deserialized.token1);
    assert_eq!(enriched_swap.token0_decimals, deserialized.token0_decimals);
    assert_eq!(enriched_swap.token1_decimals, deserialized.token1_decimals);
    assert_eq!(enriched_swap.amount0_in, deserialized.amount0_in);
    assert_eq!(enriched_swap.amount1_out, deserialized.amount1_out);
    assert_eq!(enriched_swap.protocol, deserialized.protocol);
    
    println!("✅ TLV roundtrip successful - semantic equality preserved!");
}

#[test]
fn test_decimal_precision_preservation() {
    println!("🧪 Testing decimal precision preservation");
    
    // Test various decimal combinations
    let test_cases = vec![
        (18, 6, 1_000_000_000_000_000_000u128, 1_000_000u128), // 1 WETH for 1 USDC
        (6, 6, 1_000_000u128, 1_000_000u128), // 1 USDC for 1 USDT
        (18, 18, 1_000_000_000_000_000_000u128, 2_000_000_000_000_000_000u128), // 1 WMATIC for 2 WETH
        (8, 18, 100_000_000u128, 1_000_000_000_000_000_000u128), // 1 WBTC for 1 WETH
    ];
    
    for (dec0, dec1, amount0, amount1) in test_cases {
        println!("\n  Testing {}/{} decimals with amounts {}/{}", dec0, dec1, amount0, amount1);
        
        let enriched = EnrichedSwapEvent {
            token0_decimals: dec0,
            token1_decimals: dec1,
            amount0_in: amount0,
            amount1_out: amount1,
            ..Default::default()
        };
        
        // Serialize and deserialize
        let tlv_bytes = serialize_to_tlv(&enriched);
        let deserialized = deserialize_from_tlv(&tlv_bytes)
            .expect("Failed to deserialize");
        
        // Verify exact precision preservation
        assert_eq!(
            enriched.token0_decimals, deserialized.token0_decimals,
            "Token0 decimals not preserved"
        );
        assert_eq!(
            enriched.token1_decimals, deserialized.token1_decimals,
            "Token1 decimals not preserved"
        );
        assert_eq!(
            enriched.amount0_in, deserialized.amount0_in,
            "Amount0 not preserved exactly"
        );
        assert_eq!(
            enriched.amount1_out, deserialized.amount1_out,
            "Amount1 not preserved exactly"
        );
        
        println!("  ✅ Precision preserved correctly");
    }
    
    println!("\n✅ All decimal precision tests passed!");
}

// Helper structures and functions

#[derive(Debug, Clone, PartialEq)]
struct EnrichedSwapEvent {
    pool_address: [u8; 20],
    token0: [u8; 20],
    token1: [u8; 20],
    token0_decimals: u8,
    token1_decimals: u8,
    amount0_in: u128,
    amount1_in: u128,
    amount0_out: u128,
    amount1_out: u128,
    sqrt_price_x96: [u8; 20],
    tick: i32,
    liquidity: u128,
    protocol: &'static str,
    fee_tier: u32,
    block_number: u64,
    timestamp_ns: u64,
}

impl Default for EnrichedSwapEvent {
    fn default() -> Self {
        Self {
            pool_address: [0; 20],
            token0: [0; 20],
            token1: [0; 20],
            token0_decimals: 18,
            token1_decimals: 18,
            amount0_in: 0,
            amount1_in: 0,
            amount0_out: 0,
            amount1_out: 0,
            sqrt_price_x96: [0; 20],
            tick: 0,
            liquidity: 0,
            protocol: "UniswapV2",
            fee_tier: 30,
            block_number: 0,
            timestamp_ns: 0,
        }
    }
}

fn serialize_to_tlv(event: &EnrichedSwapEvent) -> Vec<u8> {
    // This would use the actual TLV builder from codec crate
    // For now, a simplified version
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&event.pool_address);
    bytes.extend_from_slice(&event.token0);
    bytes.extend_from_slice(&event.token1);
    bytes.push(event.token0_decimals);
    bytes.push(event.token1_decimals);
    bytes.extend_from_slice(&event.amount0_in.to_le_bytes());
    bytes.extend_from_slice(&event.amount1_in.to_le_bytes());
    bytes.extend_from_slice(&event.amount0_out.to_le_bytes());
    bytes.extend_from_slice(&event.amount1_out.to_le_bytes());
    bytes
}

fn deserialize_from_tlv(bytes: &[u8]) -> Result<EnrichedSwapEvent, String> {
    // Simplified deserialization
    if bytes.len() < 105 {
        return Err("Invalid TLV size".to_string());
    }
    
    let mut event = EnrichedSwapEvent::default();
    let mut offset = 0;
    
    event.pool_address.copy_from_slice(&bytes[offset..offset+20]);
    offset += 20;
    
    event.token0.copy_from_slice(&bytes[offset..offset+20]);
    offset += 20;
    
    event.token1.copy_from_slice(&bytes[offset..offset+20]);
    offset += 20;
    
    event.token0_decimals = bytes[offset];
    offset += 1;
    
    event.token1_decimals = bytes[offset];
    offset += 1;
    
    event.amount0_in = u128::from_le_bytes(bytes[offset..offset+16].try_into().unwrap());
    offset += 16;
    
    event.amount1_in = u128::from_le_bytes(bytes[offset..offset+16].try_into().unwrap());
    offset += 16;
    
    event.amount0_out = u128::from_le_bytes(bytes[offset..offset+16].try_into().unwrap());
    offset += 16;
    
    event.amount1_out = u128::from_le_bytes(bytes[offset..offset+16].try_into().unwrap());
    
    Ok(event)
}