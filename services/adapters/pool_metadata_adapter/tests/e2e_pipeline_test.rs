//! End-to-End Pipeline Test
//!
//! Simulates the complete flow:
//! Raw Event → Enrichment → TLV → Relay → Consumer

use pool_metadata_adapter::{PoolMetadataAdapter, PoolMetadataConfig};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{info, debug};

/// Simulated raw swap event from blockchain
#[derive(Debug, Clone)]
struct RawSwapEvent {
    pool_address: [u8; 20],
    sender: [u8; 20],
    amount0_in: [u8; 32],  // U256 as bytes
    amount1_in: [u8; 32],   // U256 as bytes
    amount0_out: [u8; 32],  // U256 as bytes
    amount1_out: [u8; 32],  // U256 as bytes
    block_number: u64,
    log_index: u32,
    transaction_hash: [u8; 32],
}

/// Enriched swap with metadata
#[derive(Debug, Clone)]
struct EnrichedSwap {
    // Original event data
    raw: RawSwapEvent,
    // Enriched metadata
    token0: [u8; 20],
    token1: [u8; 20],
    token0_decimals: u8,
    token1_decimals: u8,
    token0_symbol: String,
    token1_symbol: String,
    protocol: String,
    fee_tier: u32,
}

#[tokio::test]
#[ignore] // Run with --ignored for real RPC test
async fn test_e2e_pipeline_with_real_events() {
    tracing_subscriber::fmt()
        .with_env_filter("pool_metadata_adapter=debug")
        .init();
    
    info!("🚀 Starting E2E pipeline test");
    
    // Step 1: Create Pool Metadata Adapter
    let metadata_config = PoolMetadataConfig {
        primary_rpc: "https://polygon-rpc.com".to_string(),
        cache_dir: PathBuf::from("./test_e2e_cache"),
        enable_disk_cache: true,
        ..Default::default()
    };
    
    let metadata_adapter = Arc::new(
        PoolMetadataAdapter::new(metadata_config)
            .expect("Failed to create metadata adapter")
    );
    
    // Step 2: Create channels to simulate relay communication
    let (raw_tx, mut raw_rx) = mpsc::channel::<RawSwapEvent>(100);
    let (enriched_tx, mut enriched_rx) = mpsc::channel::<EnrichedSwap>(100);
    
    // Step 3: Spawn enrichment task (simulates enrichment service)
    let metadata_adapter_clone = metadata_adapter.clone();
    let enrichment_task = tokio::spawn(async move {
        info!("📊 Enrichment service started");
        
        while let Some(raw_event) = raw_rx.recv().await {
            debug!("Received raw swap from pool 0x{}", hex::encode(&raw_event.pool_address[..8]));
            
            // Enrich with metadata
            match metadata_adapter_clone.get_or_discover_pool(raw_event.pool_address).await {
                Ok(metadata) => {
                    let enriched = EnrichedSwap {
                        raw: raw_event.clone(),
                        token0: metadata.token0,
                        token1: metadata.token1,
                        token0_decimals: metadata.token0_decimals,
                        token1_decimals: metadata.token1_decimals,
                        token0_symbol: get_token_symbol(&metadata.token0),
                        token1_symbol: get_token_symbol(&metadata.token1),
                        protocol: metadata.protocol,
                        fee_tier: metadata.fee_tier,
                    };
                    
                    info!(
                        "✅ Enriched swap: {}/{} ({}/{})",
                        enriched.token0_symbol,
                        enriched.token1_symbol,
                        enriched.token0_decimals,
                        enriched.token1_decimals
                    );
                    
                    // Send to enriched relay
                    if enriched_tx.send(enriched).await.is_err() {
                        break;
                    }
                }
                Err(e) => {
                    eprintln!("❌ Failed to enrich: {}", e);
                }
            }
        }
        
        info!("📊 Enrichment service stopped");
    });
    
    // Step 4: Spawn consumer task (simulates strategy)
    let consumer_task = tokio::spawn(async move {
        info!("🎯 Strategy consumer started");
        
        while let Some(enriched) = enriched_rx.recv().await {
            // Validate enriched data
            validate_enriched_swap(&enriched);
            
            // Calculate human-readable amounts
            let amount0_in = u256_to_u128(&enriched.raw.amount0_in);
            let amount1_out = u256_to_u128(&enriched.raw.amount1_out);
            
            let human_amount0 = format_amount(amount0_in, enriched.token0_decimals);
            let human_amount1 = format_amount(amount1_out, enriched.token1_decimals);
            
            info!(
                "📈 Strategy processed swap: {} {} → {} {}",
                human_amount0, enriched.token0_symbol,
                human_amount1, enriched.token1_symbol
            );
        }
        
        info!("🎯 Strategy consumer stopped");
    });
    
    // Step 5: Simulate raw swap events
    let test_swaps = vec![
        create_test_swap(
            "0x6e7a5FAFcec6BB1e78bAE2A1F0B612012BF14827", // WMATIC/USDC
            1_000_000_000_000_000_000, // 1 WMATIC
            0,
            0,
            1_500_000, // 1.5 USDC
        ),
        create_test_swap(
            "0x45dDa9cb7c25131DF268515131f647d726f50608", // WETH/USDC
            0,
            2_000_000, // 2 USDC
            1_000_000_000_000_000, // 0.001 WETH
            0,
        ),
    ];
    
    for swap in test_swaps {
        info!("📤 Sending raw swap event");
        raw_tx.send(swap).await.unwrap();
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
    }
    
    // Give time for processing
    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
    
    // Step 6: Shutdown
    drop(raw_tx);
    enrichment_task.await.unwrap();
    drop(enriched_tx);
    consumer_task.await.unwrap();
    
    // Step 7: Verify metrics
    let metrics = metadata_adapter.get_metrics().await;
    println!("\n📊 Final Metrics:");
    println!("   Cache hits: {}", metrics.cache_hits);
    println!("   Cache misses: {}", metrics.cache_misses);
    println!("   RPC discoveries: {}", metrics.rpc_discoveries);
    println!("   RPC failures: {}", metrics.rpc_failures);
    
    // Save cache for inspection
    metadata_adapter.save_cache().await.unwrap();
    
    info!("✅ E2E pipeline test completed successfully!");
}

// Helper functions

fn create_test_swap(
    pool_address: &str,
    amount0_in: u128,
    amount1_in: u128,
    amount0_out: u128,
    amount1_out: u128,
) -> RawSwapEvent {
    let pool_bytes = hex::decode(&pool_address[2..]).unwrap();
    let mut pool_addr = [0u8; 20];
    pool_addr.copy_from_slice(&pool_bytes);
    
    RawSwapEvent {
        pool_address: pool_addr,
        sender: [0x99; 20],
        amount0_in: u128_to_u256_bytes(amount0_in),
        amount1_in: u128_to_u256_bytes(amount1_in),
        amount0_out: u128_to_u256_bytes(amount0_out),
        amount1_out: u128_to_u256_bytes(amount1_out),
        block_number: 50_000_000,
        log_index: 42,
        transaction_hash: [0xaa; 32],
    }
}

fn u128_to_u256_bytes(value: u128) -> [u8; 32] {
    let mut bytes = [0u8; 32];
    bytes[16..].copy_from_slice(&value.to_be_bytes());
    bytes
}

fn u256_to_u128(bytes: &[u8; 32]) -> u128 {
    // Take the lower 128 bits (last 16 bytes)
    u128::from_be_bytes(bytes[16..].try_into().unwrap())
}

fn format_amount(amount: u128, decimals: u8) -> String {
    let divisor = 10u128.pow(decimals as u32);
    let whole = amount / divisor;
    let fraction = amount % divisor;
    
    if fraction == 0 {
        format!("{}", whole)
    } else {
        let fraction_str = format!("{:0width$}", fraction, width = decimals as usize);
        let trimmed = fraction_str.trim_end_matches('0');
        format!("{}.{}", whole, trimmed)
    }
}

fn get_token_symbol(address: &[u8; 20]) -> String {
    // Known token symbols for demo
    match hex::encode(address).as_str() {
        "0d500b1d8e8ef31e21c99d1db9a6444d3adf1270" => "WMATIC".to_string(),
        "2791bca1f2de4661ed88a30c99a7a9449aa84174" => "USDC".to_string(),
        "7ceb23fd6bc0add59e62ac25578270cff1b9f619" => "WETH".to_string(),
        "c2132d05d31c914a87c6611c10748aeb04b58e8f" => "USDT".to_string(),
        "8f3cf7ad23cd3cadbd9735aff958023239c6a063" => "DAI".to_string(),
        _ => "UNKNOWN".to_string(),
    }
}

fn validate_enriched_swap(swap: &EnrichedSwap) {
    // Validate that enrichment added required metadata
    assert_ne!(swap.token0, [0; 20], "Token0 should be discovered");
    assert_ne!(swap.token1, [0; 20], "Token1 should be discovered");
    assert!(swap.token0_decimals <= 18, "Invalid token0 decimals");
    assert!(swap.token1_decimals <= 18, "Invalid token1 decimals");
    assert!(!swap.protocol.is_empty(), "Protocol should be detected");
    
    // Validate amounts are reasonable
    let amount0_in = u256_to_u128(&swap.raw.amount0_in);
    let amount1_out = u256_to_u128(&swap.raw.amount1_out);
    
    if amount0_in > 0 {
        assert_eq!(u256_to_u128(&swap.raw.amount1_in), 0, "Should be single-sided swap");
    }
    if amount1_out > 0 {
        assert_eq!(u256_to_u128(&swap.raw.amount0_out), 0, "Should be single-sided swap");
    }
}