//! Polygon Adapter Binary
//!
//! Standalone binary for the Polygon DEX adapter plugin.

use torq_polygon_adapter::{PolygonAdapter, PolygonConfig};
use adapter_service::{Adapter, output::RelayOutput};
use types::RelayDomain;
use clap::Parser;
use std::{path::PathBuf, sync::Arc};
use tracing::{error, info};

#[derive(Parser)]
#[command(name = "polygon_adapter")]
#[command(about = "Polygon DEX Adapter for Torq Protocol V2")]
struct Args {
    /// Configuration file path
    #[arg(short, long, default_value = "config.toml")]
    config: PathBuf,

    /// Enable debug logging
    #[arg(short, long)]
    debug: bool,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    // Initialize logging
    let level = if args.debug {
        tracing::Level::DEBUG
    } else {
        tracing::Level::INFO
    };
    
    tracing_subscriber::fmt()
        .with_max_level(level)
        .init();

    info!("🚀 Starting Polygon DEX Adapter");

    // Load configuration
    let config = PolygonConfig::from_file(&args.config)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to load config: {}", e))?;

    info!("📋 Configuration loaded from: {:?}", args.config);

    // Create RelayOutput for MarketData relay
    let relay_output = Arc::new(RelayOutput::new(
        "/tmp/torq/market_data.sock".to_string(),
        RelayDomain::MarketData,
    ));

    // Connect to relay
    info!("🔌 Connecting to MarketData relay...");
    if let Err(e) = relay_output.connect().await {
        error!("❌ Failed to connect to MarketData relay: {}", e);
        return Err(e.into());
    }
    info!("✅ Connected to MarketData relay");

    // Start health monitor for automatic reconnection
    info!("🏥 Starting relay health monitor for automatic reconnection");
    relay_output.clone().spawn_health_monitor();

    // Create adapter with relay output
    let mut adapter = PolygonAdapter::new(config, Some(relay_output))?;

    info!("Adapter created: polygon_adapter");

    // Start adapter
    match adapter.start().await {
        Ok(()) => {
            info!("✅ Polygon adapter started successfully");
            
            // Keep running until interrupted
            tokio::signal::ctrl_c().await?;
            info!("📡 Received shutdown signal");
            
            // Stop adapter
            adapter.stop().await?;
            info!("✅ Polygon adapter stopped gracefully");
        }
        Err(e) => {
            error!("🔥 Failed to start Polygon adapter: {}", e);
            return Err(e.into());
        }
    }

    Ok(())
}