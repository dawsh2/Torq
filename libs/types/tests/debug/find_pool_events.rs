//! Find pools with actual Mint/Burn events

use web3::types::{FilterBuilder, H256, U64};

#[tokio::test]
async fn find_pool_events() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    println!("🔍 Searching for pools with liquidity events...\n");
    
    let transport = web3::transports::Http::new("https://polygon-rpc.com")?;
    let web3 = web3::Web3::new(transport);
    
    let latest_block = web3.eth().block_number().await?;
    println!("Latest block: {}\n", latest_block);
    
    // Popular Uniswap V3 pools on Polygon
    let pools = vec![
        ("0x45dDa9cb7c25131DF268515131f647d726f50608", "USDC/WETH 0.05%"),
        ("0xA374094527e1673A86dE625aa59517c5dE346d32", "WMATIC/USDC 0.05%"),
        ("0x0e44cEb592AcFC5D3F09D996302eB4C499ff8c10", "USDC/USDT 0.01%"),
        ("0x3F5228d0e7D75467366be7De2c31D0d098bA2C23", "USDC/USDT 0.05%"),
        ("0x88f3C15523544835fF6c738DDb30995339AD57d6", "WMATIC/WETH 0.3%"),
        ("0x167384319B41F7094e62f7506409Eb38079AbfF8", "WMATIC/WETH 0.05%"),
        ("0x50eaEDB835021E4A108B7290636d62E9765cc6d7", "WBTC/WETH 0.05%"),
    ];
    
    let mint_sig: H256 = "0x7a53080ba414158be7ec69b987b5fb7d07dee101bff6d8c7e4c92a5fa38b3b9a".parse()?;
    let burn_sig: H256 = "0x0c396cd989a39f4459b5fa1aed6a9a8dcdbc45908acfd67e028cd568da98982c".parse()?;
    
    // Look back a moderate amount
    let from_block = latest_block.saturating_sub(10.into());
    
    println!("Checking last 10 blocks for liquidity events...\n");
    
    for (pool_addr, pool_name) in pools {
        let pool_address = pool_addr.parse()?;
        
        // Query Mint events
        let mint_filter = FilterBuilder::default()
            .address(vec![pool_address])
            .topics(Some(vec![mint_sig]), None, None, None)
            .from_block(web3::types::BlockNumber::Number(from_block))
            .to_block(web3::types::BlockNumber::Latest)
            .build();
        
        let mint_logs = match web3.eth().logs(mint_filter).await {
            Ok(logs) => logs,
            Err(_) => {
                println!("Rate limited on {}. Trying smaller range...", pool_name);
                // Try smaller range
                let small_from = latest_block.saturating_sub(100.into());
                let mint_filter = FilterBuilder::default()
                    .address(vec![pool_address])
                    .topics(Some(vec![mint_sig]), None, None, None)
                    .from_block(web3::types::BlockNumber::Number(small_from))
                    .to_block(web3::types::BlockNumber::Latest)
                    .build();
                
                web3.eth().logs(mint_filter).await.unwrap_or_default()
            }
        };
        
        // Query Burn events
        let burn_filter = FilterBuilder::default()
            .address(vec![pool_address])
            .topics(Some(vec![burn_sig]), None, None, None)
            .from_block(web3::types::BlockNumber::Number(from_block))
            .to_block(web3::types::BlockNumber::Latest)
            .build();
        
        let burn_logs = match web3.eth().logs(burn_filter).await {
            Ok(logs) => logs,
            Err(_) => Vec::new(),
        };
        
        if !mint_logs.is_empty() || !burn_logs.is_empty() {
            println!("✅ {}: {} mints, {} burns", pool_name, mint_logs.len(), burn_logs.len());
            
            // Analyze first mint if exists
            if let Some(log) = mint_logs.first() {
                println!("  First Mint:");
                println!("    Block: {:?}", log.block_number);
                println!("    TX: {:?}", log.transaction_hash);
                println!("    Topics: {}", log.topics.len());
                println!("    Data length: {} bytes", log.data.0.len());
                
                // Parse the Mint event data
                if log.data.0.len() >= 160 {
                    // Mint event structure:
                    // event Mint(
                    //   address sender,
                    //   address indexed owner,
                    //   int24 indexed tickLower,
                    //   int24 indexed tickUpper,
                    //   uint128 liquidity,
                    //   uint256 amount0,
                    //   uint256 amount1
                    // )
                    
                    println!("\n    Parsing Mint data:");
                    
                    // Note: tickLower and tickUpper are INDEXED, so they're in topics!
                    if log.topics.len() >= 4 {
                        // topics[0] = event signature
                        // topics[1] = owner (indexed)
                        // topics[2] = tickLower (indexed)
                        // topics[3] = tickUpper (indexed)
                        
                        let tick_lower = i32::from_be_bytes(log.topics[2].0[28..32].try_into().unwrap());
                        let tick_upper = i32::from_be_bytes(log.topics[3].0[28..32].try_into().unwrap());
                        println!("    Tick range: [{}, {}]", tick_lower, tick_upper);
                    }
                    
                    // Data contains: sender, liquidity, amount0, amount1
                    let mut offset = 0;
                    
                    // sender (address) - 32 bytes
                    let sender = &log.data.0[offset+12..offset+32];
                    println!("    Sender: 0x{}", hex::encode(sender));
                    offset += 32;
                    
                    // liquidity (uint128) - 32 bytes
                    let liquidity = u128::from_be_bytes(log.data.0[offset+16..offset+32].try_into().unwrap());
                    println!("    Liquidity: {}", liquidity);
                    offset += 32;
                    
                    // amount0 (uint256) - 32 bytes  
                    let amount0 = u128::from_be_bytes(log.data.0[offset+16..offset+32].try_into().unwrap());
                    offset += 32;
                    
                    // amount1 (uint256) - 32 bytes
                    let amount1 = u128::from_be_bytes(log.data.0[offset+16..offset+32].try_into().unwrap());
                    
                    // Guess token decimals based on amounts
                    println!("    Amount0: {} (raw)", amount0);
                    println!("    Amount1: {} (raw)", amount1);
                    
                    // Common patterns:
                    // USDC/WETH: USDC=6 decimals, WETH=18
                    // WMATIC/USDC: WMATIC=18, USDC=6
                    // USDC/USDT: both 6 decimals
                    
                    if pool_name.contains("USDC") && pool_name.contains("WETH") {
                        println!("    Amount0 (USDC): {:.6}", amount0 as f64 / 1e6);
                        println!("    Amount1 (WETH): {:.18}", amount1 as f64 / 1e18);
                    } else if pool_name.contains("WMATIC") && pool_name.contains("USDC") {
                        println!("    Amount0 (WMATIC): {:.18}", amount0 as f64 / 1e18);
                        println!("    Amount1 (USDC): {:.6}", amount1 as f64 / 1e6);
                    } else if pool_name.contains("USDC") && pool_name.contains("USDT") {
                        println!("    Amount0 (USDC): {:.6}", amount0 as f64 / 1e6);
                        println!("    Amount1 (USDT): {:.6}", amount1 as f64 / 1e6);
                    }
                }
            }
            
            // Analyze first burn if exists
            if let Some(log) = burn_logs.first() {
                println!("\n  First Burn:");
                println!("    Block: {:?}", log.block_number);
                println!("    TX: {:?}", log.transaction_hash);
                println!("    Topics: {}", log.topics.len());
                println!("    Data length: {} bytes", log.data.0.len());
            }
            
            println!();
        } else {
            println!("❌ {}: No liquidity events found", pool_name);
        }
    }
    
    Ok(())
}