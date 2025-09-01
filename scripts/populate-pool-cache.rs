#!/usr/bin/env rust-script
//! Pool Cache Population Tool
//! 
//! This tool pre-populates the pool cache with all known DEX pools on Polygon
//! to avoid RPC rate limiting during runtime discovery.
//!
//! ```cargo
//! [dependencies]
//! tokio = { version = "1", features = ["full"] }
//! web3 = "0.19"
//! serde = { version = "1", features = ["derive"] }
//! serde_json = "1"
//! hex = "0.4"
//! anyhow = "1"
//! ```

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PoolEntry {
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

#[derive(Debug, Serialize, Deserialize)]
struct PoolCache {
    version: u32,
    chain_id: u64,
    pools: Vec<PoolEntry>,
}

// Known token decimals for Polygon
fn get_token_decimals(token_address: &str) -> u8 {
    let addr = token_address.to_lowercase();
    match addr.as_str() {
        // Stablecoins
        "0x2791bca1f2de4661ed88a30c99a7a9449aa84174" => 6,  // USDC
        "0xc2132d05d31c914a87c6611c10748aeb04b58e8f" => 6,  // USDT
        "0x8f3cf7ad23cd3cadbd9735aff958023239c6a063" => 18, // DAI
        
        // Native & Wrapped
        "0x0d500b1d8e8ef31e21c99d1db9a6444d3adf1270" => 18, // WMATIC
        "0x7ceb23fd6bc0add59e62ac25578270cff1b9f619" => 18, // WETH
        "0x1bfd67037b42cf73acf2047067bd4f2c47d9bfd6" => 8,  // WBTC
        
        // DeFi tokens
        "0xd6df932a45c0f255f85145f286ea0b292b21c90b" => 18, // AAVE
        "0x53e0bca35ec356bd5dddfebbdfc0fd03fabad39"=> 18, // LINK
        "0xb33eaad8d922b1083446dc23f610c2567fb5180f" => 18, // UNI
        "0x0b3f868e0be5597d5db7feb59e1cadbb0fdda50a" => 18, // SUSHI
        
        // Default to 18 (most ERC20 tokens)
        _ => 18,
    }
}

// All discovered pools from logs
fn get_all_known_pools() -> Vec<PoolEntry> {
    let mut pools = Vec::new();
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u64;
    
    // Pools actively trading (from logs)
    let active_pools = vec![
        ("0x9b08288c3be4f62bbf8d1c20ac9c5e6f9467d8b7", "0x53e0bca35ec356bd5dddfebbd1fc0fd03fabad39", "0x2791bca1f2de4661ed88a30c99a7a9449aa84174"), // LINK/USDC
        ("0x45dda9cb7c25131df268515131f647d726f50608", "0x7ceb23fd6bc0add59e62ac25578270cff1b9f619", "0x2791bca1f2de4661ed88a30c99a7a9449aa84174"), // WETH/USDC V3
        ("0x86f1d8390222a3691c28938ec7404a1661e618e0", "0x0d500b1d8e8ef31e21c99d1db9a6444d3adf1270", "0x7ceb23fd6bc0add59e62ac25578270cff1b9f619"), // WMATIC/WETH V3
        ("0x50e7899355133de7bb3e754025bad11bad89302b", "0x0d500b1d8e8ef31e21c99d1db9a6444d3adf1270", "0x8f3cf7ad23cd3cadbd9735aff958023239c6a063"), // WMATIC/DAI
        ("0xa374094527e1673a86de625aa59517c5de346d32", "0x0d500b1d8e8ef31e21c99d1db9a6444d3adf1270", "0xc2132d05d31c914a87c6611c10748aeb04b58e8f"), // WMATIC/USDT
        ("0xf23c42f2bcc62c090316ad7ea7220697d4432576", "0x7ceb23fd6bc0add59e62ac25578270cff1b9f619", "0x0d500b1d8e8ef31e21c99d1db9a6444d3adf1270"), // WETH/WMATIC
        ("0x479e1b71a702a595e19b6d5932cd5c863ab57ee0", "0x2791bca1f2de4661ed88a30c99a7a9449aa84174", "0xc2132d05d31c914a87c6611c10748aeb04b58e8f"), // USDC/USDT V3
        ("0xdac8a8e6dbf8c690ec6815e0ff03491b2770255d", "0x2791bca1f2de4661ed88a30c99a7a9449aa84174", "0xc2132d05d31c914a87c6611c10748aeb04b58e8f"), // USDC/USDT V3
        ("0x1a34eabbe928bf431b679959379b2225d60d9cda", "0x7ceb23fd6bc0add59e62ac25578270cff1b9f619", "0x1bfd67037b42cf73acf2047067bd4f2c47d9bfd6"), // WETH/WBTC
        ("0x6b75f2189f0e11c52e814e09e280eb1a9a8a094a", "0x0d500b1d8e8ef31e21c99d1db9a6444d3adf1270", "0x2791bca1f2de4661ed88a30c99a7a9449aa84174"), // WMATIC/USDC
        
        // Add all other discovered pools from logs
        ("0x00604dcc4463d6c39d957bc25c7b6683ddd2ba90", "", ""),
        ("0x00a59c2d0f0f4837028d47a391decbffc1e10608", "", ""),
        ("0x00e0f57d0928e01d92c1135b4ae12a48a8e995a2", "", ""),
        ("0x0a28c2f5e0e8463e047c203f00f649812ae67e4f", "", ""),
        ("0x0e3eb2c75bd7dd0e12249d96b1321d9570764d77", "", ""),
        ("0x0e7754127dedd4097be750825dbb4669bc32c956", "", ""),
        ("0x22177148e681a6ca5242c9888ace170ee7ec47bd", "", ""),
        ("0x2392a3787523a9bd7f24651d14615fe5b74ef515", "", ""),
        ("0x254aa3a898071d6a2da0db11da73b02b4646078f", "", ""),
        ("0x25fb97799f80433e422f47e75173314e54dae174", "", ""),
        ("0x296b95dd0e8b726c4e358b0683ff0b6d675c35e9", "", ""),
        ("0x31083a78e11b18e450fd139f9abea98cd53181b7", "", ""),
        ("0x32fae204835e08b9374493d6b4628fd1f87dd045", "", ""),
        ("0x33b08488bfba59299ac01c6acf33d506f2715c45", "", ""),
        ("0x3bfcb475e528f54246f1847ec0e7b53dd88bda4e", "", ""),
        ("0x3dc10d7bfb94eeb009203e84a653e5764f71771d", "", ""),
        ("0x4ed052a9880d21d59addda012abe367cf03a85aa", "", ""),
        ("0x50eaedb835021e4a108b7290636d62e9765cc6d7", "", ""),
        ("0x519596c983929b23113d660bb1c96eb29c9b54e3", "", ""),
        ("0x5b41eedcfc8e0ae47493d4945aa1ae4fe05430ff", "", ""),
        ("0x6669b4706cc152f359e947bca68e263a87c52634", "", ""),
        ("0x781067ef296e5c4a4203f81c593274824b7c185d", "", ""),
        ("0x7b41801b47f8279c3a9fae6aa61e8ae141d90608", "", ""),
        ("0x7b925e617aefd7fb3a93abe3a701135d7a1ba710", "", ""),
        ("0x8983fad9adfbfd3ccc5f0e2173cddbd940fbd23c", "", ""),
        ("0x96239bd7ae3d9bc253b1cc7cf7a84f3a67ca5369", "", ""),
        ("0x9ceff2f5138fc59eb925d270b8a7a9c02a1810f2", "", ""),
        ("0xa4d8c89f0c20efbe54cba9e7e7a7e509056228d9", "", ""),
        ("0xa6aedf7c4ed6e821e67a6bfd56fd1702ad9a9719", "", ""),
        ("0xab52931301078e2405c3a3ebb86e11ad0dfd2cfd", "", ""),
        ("0xac4494e30a85369e332bdb5230d6d694d4259dbc", "", ""),
        ("0xae81fac689a1b4b1e06e7ef4a2ab4cd8ac0a087d", "", ""),
        ("0xb6e57ed85c4c9dbfef2a68711e9d6f36c56e0fcb", "", ""),
        ("0xb89daec63973eea96bb69979db772b028f0e5d1e", "", ""),
        ("0xbb98b3d2b18aef63a3178023a920971cf5f29be4", "", ""),
        ("0xc7ed6c48ce8894d63b58c39fc2056963a3c545ab", "", ""),
        ("0xce67850420c82db45eb7feeccd2d181300d2bdb3", "", ""),
        ("0xd04491e3868e6ddb8fe6075fe9d2557f809c248d", "", ""),
        ("0xd29c2df656b2e4ae6b6817ccc2ebe932fc6a950b", "", ""),
        ("0xd36ec33c8bed5a9f7b6630855f1533455b98a418", "", ""),
        ("0xdb975b96828352880409e86d5ae93c23c924f812", "", ""),
        ("0xeecb5db986c20a8c88d8332e7e252a9671565751", "", ""),
        ("0xeffa9e5e63ba18160ee26bda56b42f3368719615", "", ""),
        ("0xf1a12338d39fc085d8631e1a745b5116bc9b2a32", "", ""),
        ("0xf95ab46ec5c9fec3e40e8255ec7490095a8011ff", "", ""),
    ];
    
    // Main DEX factory pools (QuickSwap V2, SushiSwap, etc.)
    let quickswap_v2_pools = vec![
        ("0x6e7a5fafcec6bb1e78bae2a1f0b612012bf14827", "0x0d500b1d8e8ef31e21c99d1db9a6444d3adf1270", "0x2791bca1f2de4661ed88a30c99a7a9449aa84174"), // WMATIC/USDC
        ("0x853ee4b2a13f8a742d64c8f088be7ba2131f670d", "0x7ceb23fd6bc0add59e62ac25578270cff1b9f619", "0x2791bca1f2de4661ed88a30c99a7a9449aa84174"), // WETH/USDC
        ("0xdc9232e2df177d7a12fdff6ecbab114e2231198d", "0x1bfd67037b42cf73acf2047067bd4f2c47d9bfd6", "0x7ceb23fd6bc0add59e62ac25578270cff1b9f619"), // WBTC/WETH
        ("0x2cf7252e74036d1da831d11089d326296e64a728", "0x2791bca1f2de4661ed88a30c99a7a9449aa84174", "0xc2132d05d31c914a87c6611c10748aeb04b58e8f"), // USDC/USDT
        ("0xc4e595acdd7d12fec385e5da5d43160e8a0bac0e", "0x0d500b1d8e8ef31e21c99d1db9a6444d3adf1270", "0x7ceb23fd6bc0add59e62ac25578270cff1b9f619"), // WMATIC/WETH
        ("0xf04adbf75cdfc5ed26eea4bbbb991db002036bdd", "0x8f3cf7ad23cd3cadbd9735aff958023239c6a063", "0x2791bca1f2de4661ed88a30c99a7a9449aa84174"), // DAI/USDC
    ];
    
    for (pool_addr, token0, token1) in active_pools.into_iter().chain(quickswap_v2_pools) {
        // Skip empty token addresses
        if token0.is_empty() || token1.is_empty() {
            // For pools without token info, we'll need RPC discovery
            continue;
        }
        
        let dec0 = get_token_decimals(token0);
        let dec1 = get_token_decimals(token1);
        
        // Determine protocol based on patterns (V3 pools typically have different fee tiers)
        let (protocol, fee_tier) = if pool_addr.contains("45dda9") || pool_addr.contains("86f1d8") {
            ("V3".to_string(), 3000) // 0.3% for V3
        } else {
            ("V2".to_string(), 30) // 0.3% for V2
        };
        
        pools.push(PoolEntry {
            pool_address: pool_addr.to_string(),
            token0: token0.to_string(),
            token1: token1.to_string(),
            token0_decimals: dec0,
            token1_decimals: dec1,
            protocol,
            fee_tier,
            discovered_at: timestamp,
            venue: "Polygon".to_string(),
        });
    }
    
    pools
}

fn main() -> Result<()> {
    println!("🚀 Populating Polygon Pool Cache...");
    
    // Create cache directory
    let cache_dir = PathBuf::from("/tmp/alphapulse/pool_cache");
    fs::create_dir_all(&cache_dir)?;
    
    // Get all known pools
    let pools = get_all_known_pools();
    println!("📊 Found {} pools to cache", pools.len());
    
    // Create cache structure
    let cache = PoolCache {
        version: 1,
        chain_id: 137,
        pools,
    };
    
    // Save to JSON
    let cache_file = cache_dir.join("polygon_pools_complete.json");
    let json = serde_json::to_string_pretty(&cache)?;
    fs::write(&cache_file, json)?;
    
    println!("✅ Pool cache saved to: {}", cache_file.display());
    println!("📝 Note: The Rust adapter will convert this to TLV format on first load");
    
    Ok(())
}