//! Track pool reserve changes for arbitrage detection
//! 
//! Every swap changes the pool reserves, which changes the price.
//! This is what we need for arbitrage!

use web3::types::{FilterBuilder, H160, H256};
use std::collections::HashMap;

#[derive(Debug, Clone)]
struct PoolState {
    reserve0: i128,  // Token0 reserves
    reserve1: i128,  // Token1 reserves
    sqrt_price_x96: u128,
    tick: i32,
    last_block: u64,
}

impl PoolState {
    /// Update reserves based on swap amounts
    fn apply_swap(&mut self, amount0: i128, amount1: i128, sqrt_price: u128, tick: i32, block: u64) {
        // In a swap:
        // Positive amount = tokens going IN to the pool (pool gains)
        // Negative amount = tokens going OUT of the pool (pool loses)
        self.reserve0 += amount0;
        self.reserve1 += amount1;
        self.sqrt_price_x96 = sqrt_price;
        self.tick = tick;
        self.last_block = block;
    }
    
    /// Calculate spot price from reserves (for constant product AMM)
    /// Note: Uniswap V3 uses concentrated liquidity, so this is approximate
    fn spot_price(&self) -> f64 {
        if self.reserve0 > 0 {
            self.reserve1 as f64 / self.reserve0 as f64
        } else {
            // Use sqrt price for more accuracy
            let sqrt_price = self.sqrt_price_x96 as f64 / (2_f64.powi(96));
            sqrt_price * sqrt_price
        }
    }
}

#[tokio::test]
async fn track_pool_reserves() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    println!("📊 Tracking Pool Reserve Changes for Arbitrage\n");
    println!("Every swap updates reserves → price changes → arbitrage opportunities!\n");
    
    let transport = web3::transports::Http::new("https://polygon-rpc.com")?;
    let web3 = web3::Web3::new(transport);
    
    let latest_block = web3.eth().block_number().await?;
    
    // Track multiple pools for cross-pool arbitrage
    let pools = vec![
        ("0xA374094527e1673A86dE625aa59517c5dE346d32", "WMATIC/USDC 0.05%", 18, 6),
        ("0x45dDa9cb7c25131DF268515131f647d726f50608", "USDC/WETH 0.05%", 6, 18),
    ];
    
    let swap_sig: H256 = "0xc42079f94a6350d7e6235f29174924f928cc2ac818eb64fed8004e115fbcca67".parse()?;
    let from_block = latest_block.saturating_sub(20.into());
    
    // Track pool states
    let mut pool_states: HashMap<H160, PoolState> = HashMap::new();
    
    for (pool_addr, pool_name, decimals0, decimals1) in &pools {
        let pool_address: H160 = pool_addr.parse()?;
        
        println!("Analyzing {}", pool_name);
        println!("{}", "-".repeat(50));
        
        // Get swap events
        let filter = FilterBuilder::default()
            .address(vec![pool_address])
            .topics(Some(vec![swap_sig]), None, None, None)
            .from_block(web3::types::BlockNumber::Number(from_block))
            .to_block(web3::types::BlockNumber::Latest)
            .build();
        
        let logs = web3.eth().logs(filter).await?;
        
        if logs.is_empty() {
            println!("No recent swaps\n");
            continue;
        }
        
        println!("Found {} swaps - tracking reserve changes:\n", logs.len());
        
        // Initialize pool state (in production, we'd query current reserves)
        let mut state = PoolState {
            reserve0: 1_000_000_000_000_000_000_000, // Dummy initial reserves
            reserve1: 240_000_000,                    // Based on ~0.24 USDC/MATIC price
            sqrt_price_x96: 0,
            tick: 0,
            last_block: 0,
        };
        
        // Process swaps chronologically
        for (i, log) in logs.iter().enumerate() {
            if log.data.0.len() >= 160 {
                // Parse swap data
                let amount0 = parse_int256(&log.data.0[0..32]);
                let amount1 = parse_int256(&log.data.0[32..64]);
                let sqrt_price = u128::from_be_bytes(log.data.0[80..96].try_into().unwrap());
                let tick = i32::from_be_bytes(log.data.0[156..160].try_into().unwrap());
                let block = log.block_number.unwrap_or_default().as_u64();
                
                // Calculate price impact
                let old_price = state.spot_price();
                state.apply_swap(amount0, amount1, sqrt_price, tick, block);
                let new_price = state.spot_price();
                
                let price_impact = ((new_price - old_price) / old_price * 100.0).abs();
                
                println!("Swap #{} (Block {}):", i + 1, block);
                
                // Show amounts with proper decimals
                let amount0_decimal = amount0 as f64 / 10_f64.powi(*decimals0);
                let amount1_decimal = amount1 as f64 / 10_f64.powi(*decimals1);
                
                if amount0 > 0 {
                    println!("  IN:  {:.6} token0", amount0_decimal);
                    println!("  OUT: {:.6} token1", amount1_decimal.abs());
                } else {
                    println!("  IN:  {:.6} token1", amount1_decimal);
                    println!("  OUT: {:.6} token0", amount0_decimal.abs());
                }
                
                println!("  Price before: {:.6}", old_price);
                println!("  Price after:  {:.6}", new_price);
                println!("  Impact: {:.3}%", price_impact);
                
                // Check for arbitrage opportunity
                if price_impact > 0.5 {
                    println!("  🎯 ARBITRAGE OPPORTUNITY! Large price impact detected");
                }
                
                println!();
            }
        }
        
        pool_states.insert(pool_address, state);
        println!();
    }
    
    // Cross-pool arbitrage check
    if pool_states.len() >= 2 {
        println!("🔄 Cross-Pool Arbitrage Analysis");
        println!("{}", "=".repeat(50));
        
        // In practice, we'd calculate the triangular arbitrage opportunity
        // E.g., WMATIC → USDC (pool1) → WETH (pool2) → WMATIC
        
        let addresses: Vec<H160> = pool_states.keys().cloned().collect();
        if addresses.len() >= 2 {
            let pool1_state = &pool_states[&addresses[0]];
            let pool2_state = &pool_states[&addresses[1]];
            
            println!("Pool 1 price: {:.6}", pool1_state.spot_price());
            println!("Pool 2 price: {:.6}", pool2_state.spot_price());
            
            // Simplified arbitrage check
            let price_diff = (pool1_state.spot_price() - pool2_state.spot_price()).abs();
            if price_diff > 0.001 {
                println!("\n🚨 ARBITRAGE: Price difference of {:.6} between pools!", price_diff);
                println!("Execute trade to capture the spread!");
            }
        }
    }
    
    println!("\n✅ This is the data we need for arbitrage:");
    println!("  1. Every swap updates pool reserves");
    println!("  2. Reserve changes → price changes");
    println!("  3. Price differences → arbitrage opportunities");
    println!("\nWe DON'T need Mint/Burn events for arbitrage!");
    println!("We need SWAP events and resulting reserve updates!");
    
    Ok(())
}

fn parse_int256(bytes: &[u8]) -> i128 {
    if bytes.len() != 32 {
        return 0;
    }
    
    if bytes[0] & 0x80 != 0 {
        // Negative
        if bytes[0..16].iter().all(|&b| b == 0xFF) {
            i128::from_be_bytes(bytes[16..32].try_into().unwrap())
        } else {
            i128::MIN
        }
    } else {
        // Positive
        if bytes[0..16].iter().all(|&b| b == 0x00) {
            i128::from_be_bytes(bytes[16..32].try_into().unwrap())
        } else {
            i128::MAX
        }
    }
}