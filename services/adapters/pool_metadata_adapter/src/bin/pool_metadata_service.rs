//! Pool Metadata Service
//!
//! Standalone service that provides pool metadata discovery and caching.
//! Can be run as a separate process or integrated into other services.

use anyhow::Result;
use pool_metadata_adapter::{PoolMetadataAdapter, PoolMetadataConfig};
use std::path::PathBuf;
use tokio::signal;
use tracing::{info, warn};

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("pool_metadata_adapter=debug".parse()?)
        )
        .init();
    
    info!("🚀 Starting Pool Metadata Service");
    
    // Load configuration
    let config = PoolMetadataConfig {
        primary_rpc: std::env::var("POLYGON_RPC_URL")
            .unwrap_or_else(|_| "https://polygon-rpc.com".to_string()),
        fallback_rpcs: vec![
            "https://rpc-mainnet.matic.network".to_string(),
            "https://rpc.ankr.com/polygon".to_string(),
        ],
        chain_id: 137,
        cache_dir: PathBuf::from("./data/pool_cache"),
        max_concurrent_discoveries: 5,
        rpc_timeout_ms: 10000,
        max_retries: 3,
        rate_limit_per_sec: 10,
        enable_disk_cache: true,
    };
    
    // Create adapter
    let adapter = PoolMetadataAdapter::new(config)?;
    
    info!("✅ Pool Metadata Service initialized");
    info!("📁 Cache directory: ./data/pool_cache");
    
    // Example: Pre-load some known pools
    let known_pools = vec![
        // WMATIC/USDC QuickSwap V2
        hex::decode("6e7a5FAFcec6BB1e78bAE2A1F0B612012BF14827")?,
        // WETH/USDC QuickSwap V2  
        hex::decode("853Ee4b2A13f8a742d64C8F088bE7bA2131f670d")?,
    ];
    
    for pool_hex in known_pools {
        let mut pool_address = [0u8; 20];
        pool_address.copy_from_slice(&pool_hex);
        
        match adapter.get_or_discover_pool(pool_address).await {
            Ok(info) => {
                info!(
                    "Loaded pool 0x{}: tokens 0x{}/0x{}", 
                    hex::encode(pool_address),
                    hex::encode(&info.token0[..8]),
                    hex::encode(&info.token1[..8]),
                );
            }
            Err(e) => {
                warn!("Failed to load pool 0x{}: {}", hex::encode(pool_address), e);
            }
        }
    }
    
    // Print metrics
    let metrics = adapter.get_metrics().await;
    info!("📊 Initial metrics: {:?}", metrics);
    
    // Wait for shutdown signal
    info!("Service running. Press Ctrl+C to stop.");
    signal::ctrl_c().await?;
    
    info!("🛑 Shutting down Pool Metadata Service");
    
    // Save cache before exit
    adapter.save_cache().await?;
    
    info!("✅ Cache saved. Service stopped.");
    
    Ok(())
}