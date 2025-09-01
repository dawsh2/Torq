//! Polygon Enriched Adapter
//! 
//! Single binary that combines PolygonAdapter and PoolMetadataAdapter
//! for maximum performance. Events are enriched in-process with no IPC overhead.

use anyhow::Result;
use polygon_adapter::PolygonAdapter;
use pool_metadata_adapter::{PoolMetadataAdapter, PoolMetadataConfig};
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{info, warn, debug};
use types::tlv::PoolSwapTLV;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter("polygon_enriched=debug")
        .init();
    
    info!("🚀 Starting Polygon Enriched Adapter (combined binary)");
    
    // Create shared pool metadata adapter
    let metadata_config = PoolMetadataConfig::default();
    let pool_metadata = Arc::new(PoolMetadataAdapter::new(metadata_config)?);
    info!("✅ Pool metadata cache initialized");
    
    // Create polygon adapter  
    let polygon_config = Default::default();
    let mut polygon = PolygonAdapter::new(polygon_config)?;
    info!("✅ Polygon adapter initialized");
    
    // Create internal channel for raw events (no IPC!)
    let (raw_tx, mut raw_rx) = mpsc::channel::<RawSwapEvent>(1000);
    
    // Start polygon adapter in background
    let polygon_handle = tokio::spawn(async move {
        polygon.start_streaming(raw_tx).await
    });
    
    // Main enrichment loop - all in same process!
    info!("📊 Starting enrichment loop");
    while let Some(raw_swap) = raw_rx.recv().await {
        // Get metadata - this is just a function call, no IPC!
        match pool_metadata.get_or_discover(raw_swap.pool_address).await {
            Ok(metadata) => {
                // Create enriched event
                let enriched = EnrichedSwapTLV {
                    pool_address: raw_swap.pool_address,
                    token0: metadata.token0,
                    token1: metadata.token1,
                    token0_decimals: metadata.token0_decimals,
                    token1_decimals: metadata.token1_decimals,
                    amount0_in: raw_swap.amount0_in,
                    amount1_in: raw_swap.amount1_in,
                    amount0_out: raw_swap.amount0_out,
                    amount1_out: raw_swap.amount1_out,
                    timestamp: raw_swap.timestamp,
                    block_number: raw_swap.block_number,
                    protocol: metadata.protocol,
                };
                
                // Send to relay (only external communication)
                send_to_relay(enriched).await?;
                
                debug!(
                    "Enriched swap from pool 0x{}: {}/{} decimals", 
                    hex::encode(&raw_swap.pool_address[..8]),
                    metadata.token0_decimals,
                    metadata.token1_decimals
                );
            }
            Err(e) => {
                warn!("Failed to enrich swap: {}", e);
                // Could still forward raw event if desired
            }
        }
    }
    
    polygon_handle.await??;
    Ok(())
}

// Dummy types for illustration - would come from actual adapter code
struct RawSwapEvent {
    pool_address: [u8; 20],
    amount0_in: u128,
    amount1_in: u128,
    amount0_out: u128,
    amount1_out: u128,
    timestamp: u64,
    block_number: u64,
}

struct EnrichedSwapTLV {
    pool_address: [u8; 20],
    token0: [u8; 20],
    token1: [u8; 20],
    token0_decimals: u8,
    token1_decimals: u8,
    amount0_in: u128,
    amount1_in: u128,
    amount0_out: u128,
    amount1_out: u128,
    timestamp: u64,
    block_number: u64,
    protocol: String,
}

async fn send_to_relay(enriched: EnrichedSwapTLV) -> Result<()> {
    // Send to market data relay
    // This is the only IPC - everything else is in-process!
    Ok(())
}