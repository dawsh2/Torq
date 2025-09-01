//! Configuration for Pool Metadata Adapter

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoolMetadataConfig {
    /// Primary RPC endpoint
    pub primary_rpc: String,
    
    /// Fallback RPC endpoints
    pub fallback_rpcs: Vec<String>,
    
    /// Chain ID (137 for Polygon)
    pub chain_id: u32,
    
    /// Cache directory path
    pub cache_dir: PathBuf,
    
    /// Maximum concurrent RPC requests
    pub max_concurrent_discoveries: usize,
    
    /// RPC timeout in milliseconds
    pub rpc_timeout_ms: u64,
    
    /// Maximum retries for failed RPC calls
    pub max_retries: u32,
    
    /// Rate limit (requests per second)
    pub rate_limit_per_sec: u32,
    
    /// Enable persistent disk cache
    pub enable_disk_cache: bool,
}

impl Default for PoolMetadataConfig {
    fn default() -> Self {
        Self {
            primary_rpc: "https://polygon-rpc.com".to_string(),
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
        }
    }
}