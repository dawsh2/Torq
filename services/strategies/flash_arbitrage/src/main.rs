mod arbitrage_calculator;
mod config;
mod detector;
mod gas_price;
mod pool_loader;
mod relay_consumer;
mod signal_output;

use detector::OpportunityDetector;
use relay_consumer::RelayConsumer;
use signal_output::SignalOutput;
use state_market::pool_state::PoolStateManager;
use anyhow::{Context, Result};
use std::sync::Arc;
use tracing::{info, warn};
use tracing_subscriber;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt::init();

    info!("🚀 Starting Flash Arbitrage Service...");

    // Create shared components
    let pool_manager = Arc::new(PoolStateManager::new());
    
    // Load cached pools if available
    let cache_path = std::path::PathBuf::from("./data/pool_cache/polygon_pools.json");
    if cache_path.exists() {
        info!("📂 Loading pool cache from {:?}", cache_path);
        match pool_loader::load_pool_cache(&cache_path) {
            Ok(cached_pools) => {
                let pool_count = cached_pools.len();
                pool_manager.initialize_from_cached_pools(cached_pools);
                info!("✅ Loaded {} pools into pool state manager", pool_count);
            }
            Err(e) => {
                warn!("Failed to load pool cache: {}", e);
                info!("Starting with empty pool state");
            }
        }
    } else {
        info!("No pool cache found at {:?}, starting with empty pool state", cache_path);
    }
    
    info!("✅ Pool state manager initialized");

    // Create opportunity detector with pool manager and default config
    let detector = Arc::new(OpportunityDetector::new(
        pool_manager.clone(),
        Default::default(), // Use default detector configuration
    ));
    info!("✅ Opportunity detector initialized");

    // Create signal output component
    let signal_output = Arc::new(SignalOutput::new(
        "/tmp/torq/signals.sock".to_string(),
    ));
    info!("✅ Signal output configured for Signal Relay");

    // Create relay consumer with all components
    let mut consumer = RelayConsumer::new(
        "/tmp/torq/market_data.sock".to_string(),
        pool_manager,
        detector,
        signal_output,
    );

    info!("✅ Flash Arbitrage Service initialized successfully");
    info!("📡 Listening for pool events on Market Data Relay");
    info!("📊 Analyzing ALL swaps for arbitrage opportunities");
    info!("🎯 Sending signals to Signal Relay → Dashboard");

    // Start consuming and analyzing pool events
    consumer.start().await
        .context("Failed to start flash arbitrage relay consumer")?;

    Ok(())
}
