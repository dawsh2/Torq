//! Dashboard constants and well-known values
//!
//! These constants provide default values and lookups for the dashboard display.

use once_cell::sync::Lazy;
use std::collections::HashMap;

/// Zero address constant for unknown/uninitialized values
pub const ZERO_ADDRESS: &str = "0x0000000000000000000000000000000000000000";

/// Default values for incomplete arbitrage opportunities
pub mod defaults {
    use super::ZERO_ADDRESS;
    
    /// Default pair name when actual pair is unknown
    pub const UNKNOWN_PAIR: &str = "UNKNOWN-PAIR";
    
    /// Default token addresses when not available
    pub const DEFAULT_TOKEN_ADDRESS: &str = ZERO_ADDRESS;
    
    /// Default DEX names for display
    pub const DEFAULT_BUY_DEX: &str = "QuickSwap";
    pub const DEFAULT_SELL_DEX: &str = "SushiSwap";
    
    /// Default router addresses
    pub const DEFAULT_ROUTER_ADDRESS: &str = ZERO_ADDRESS;
    
    // Note: USD costs should come from actual signal data or price feeds
    // These defaults are removed to enforce using real values
}

/// Token symbol lookup for common Polygon tokens
/// Maps first 8 bytes of address to symbol for efficient lookup
pub static TOKEN_SYMBOL_LOOKUP: Lazy<HashMap<u64, &'static str>> = Lazy::new(|| {
    let mut m = HashMap::new();
    m.insert(0x2791bca1f2de4661u64, "USDC");   // USDC on Polygon
    m.insert(0x0d500b1d8e8ef31eu64, "WMATIC"); // WMATIC on Polygon  
    m.insert(0x7ceb23fd6bc0add5u64, "WETH");   // WETH on Polygon
    m.insert(0x8f3cf7ad23cd3cadu64, "DAI");    // DAI on Polygon
    m.insert(0x1bfd67037b42cf73u64, "WBTC");   // WBTC on Polygon
    m.insert(0xc2132d05d31c914au64, "USDT");   // USDT on Polygon
    m.insert(0x831753dd7087cac6u64, "AAVE");   // AAVE on Polygon
    m.insert(0xd6df932a45c0f255u64, "LINK");   // LINK on Polygon
    m.insert(0x53e0bca35ec356bdu64, "UNI");    // UNI on Polygon
    m.insert(0xb33eaad8d922b108u64, "SUSHI");  // SUSHI on Polygon (first 8 bytes)
    m
});

/// DEX venue name mapping for display
pub static DEX_DISPLAY_NAMES: Lazy<HashMap<&'static str, &'static str>> = Lazy::new(|| {
    let mut m = HashMap::new();
    m.insert("quickswap", "QuickSwap");
    m.insert("quickswapv3", "QuickSwap V3");
    m.insert("sushiswap", "SushiSwap");
    m.insert("uniswapv3", "Uniswap V3");
    m.insert("balancer", "Balancer");
    m.insert("curve", "Curve");
    m
});

/// Get token symbol from address fragment (first 8 bytes)
pub fn get_token_symbol(address_fragment: u64) -> &'static str {
    TOKEN_SYMBOL_LOOKUP.get(&address_fragment).copied().unwrap_or("UNKNOWN")
}

/// Get display name for DEX venue
pub fn get_dex_display_name(venue: &str) -> &'static str {
    let venue_lower = venue.to_lowercase();
    DEX_DISPLAY_NAMES.get(venue_lower.as_str()).copied().unwrap_or("Unknown")
}