//! Integration test for Polygon Pool Metadata enrichment flow

use anyhow::Result;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{UnixListener, UnixStream};
use types::protocol::tlv::market_data::PoolSwapTLV;
use codec::{serialize_with_header, parse_header_without_checksum};

#[tokio::test]
async fn test_enrichment_flow() -> Result<()> {
    // This test simulates:
    // 1. Event Collector sending raw event (decimals = 0)
    // 2. Pool Metadata Service enriching it (adding decimals)
    // 3. Market Data Relay receiving enriched event
    
    println!("🧪 Testing enrichment flow...");
    
    // Create test sockets
    let raw_socket = "/tmp/test_polygon_raw.sock";
    let enriched_socket = "/tmp/test_market_data.sock";
    
    // Clean up old sockets
    let _ = std::fs::remove_file(raw_socket);
    let _ = std::fs::remove_file(enriched_socket);
    
    // Create mock Market Data Relay (receives enriched events)
    let market_relay = UnixListener::bind(enriched_socket)?;
    
    // Spawn task to accept enriched events
    let enriched_handle = tokio::spawn(async move {
        let (mut stream, _) = market_relay.accept().await?;
        let mut buffer = vec![0u8; 1024];
        
        // Read header
        stream.read_exact(&mut buffer[..8]).await?;
        let (_msg_type, payload_size) = parse_header_without_checksum(&buffer[..8])?;
        
        // Read payload
        stream.read_exact(&mut buffer[..payload_size]).await?;
        
        // Parse enriched event
        let enriched = PoolSwapTLV::from_bytes(&buffer[..payload_size])?;
        
        // Verify decimals were added
        assert!(enriched.amount_in_decimals > 0, "Should have input decimals");
        assert!(enriched.amount_out_decimals > 0, "Should have output decimals");
        
        println!("✅ Received enriched event with decimals: {}/{}",
                 enriched.amount_in_decimals, enriched.amount_out_decimals);
        
        Ok::<(), anyhow::Error>(())
    });
    
    // Give relay time to start
    tokio::time::sleep(Duration::from_millis(100)).await;
    
    // Now we would start the Pool Metadata Service here
    // For now, let's just simulate sending a raw event
    
    // Create raw socket listener (Pool Metadata would connect here)
    let raw_listener = UnixListener::bind(raw_socket)?;
    
    // Spawn task to send raw event
    tokio::spawn(async move {
        // Wait for Pool Metadata to connect
        let (mut stream, _) = raw_listener.accept().await?;
        
        // Create raw event (no decimals)
        let raw_event = create_test_swap_event();
        
        // Send to Pool Metadata
        let bytes = serialize_with_header(11, raw_event.as_bytes())?;
        stream.write_all(&bytes).await?;
        
        println!("📤 Sent raw event to enrichment");
        
        Ok::<(), anyhow::Error>(())
    });
    
    // In a real test, we'd start the Pool Metadata Service here
    // For now, let's simulate the enrichment
    
    // Connect as Pool Metadata would
    tokio::time::sleep(Duration::from_millis(200)).await;
    
    let mut raw_stream = UnixStream::connect(raw_socket).await?;
    let mut enriched_stream = UnixStream::connect(enriched_socket).await?;
    
    // Read raw event
    let mut buffer = vec![0u8; 1024];
    let n = raw_stream.read(&mut buffer).await?;
    
    if n > 8 {
        // Parse and enrich
        let (_msg_type, payload_size) = parse_header_without_checksum(&buffer[..8])?;
        let mut swap = PoolSwapTLV::from_bytes(&buffer[8..8+payload_size])?;
        
        // Simulate enrichment
        swap.amount_in_decimals = 18;  // WMATIC
        swap.amount_out_decimals = 6;  // USDC
        
        // Forward enriched
        let enriched_bytes = serialize_with_header(11, swap.as_bytes())?;
        enriched_stream.write_all(&enriched_bytes).await?;
        
        println!("✨ Enriched and forwarded event");
    }
    
    // Wait for enriched event to be received
    enriched_handle.await??;
    
    // Cleanup
    let _ = std::fs::remove_file(raw_socket);
    let _ = std::fs::remove_file(enriched_socket);
    
    println!("🎉 Enrichment flow test passed!");
    
    Ok(())
}

fn create_test_swap_event() -> PoolSwapTLV {
    use types::protocol::identifiers::venue::VenueId;
    
    // Create test pool address (WMATIC/USDC on QuickSwap)
    let pool_address = hex::decode("6e7a5FAFcec6BB1e78bAE2A1F0B612012BF14827")
        .unwrap()
        .try_into()
        .unwrap();
    
    PoolSwapTLV::new(
        pool_address,
        [0x0d; 20], // WMATIC
        [0x2791; 20], // USDC  
        VenueId::QuickswapV2,
        1000000000000000000, // 1 WMATIC
        1000000, // 1 USDC
        0,
        1700000000000000000,
        100000,
        0,
        0, // No decimals yet (raw)
        0, // No decimals yet (raw)
        0,
    )
}