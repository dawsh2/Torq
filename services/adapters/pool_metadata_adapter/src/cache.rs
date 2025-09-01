//! Pool metadata caching with persistent storage
//!
//! This module provides efficient in-memory caching with disk persistence
//! for pool metadata. Once discovered via RPC, pool metadata is cached
//! forever since it's immutable.

use anyhow::{Context, Result};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use tracing::{debug, info, warn};

/// Pool metadata information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoolInfo {
    /// Pool contract address (20 bytes)
    pub pool_address: [u8; 20],
    
    /// Token 0 address (20 bytes)
    pub token0: [u8; 20],
    
    /// Token 1 address (20 bytes)  
    pub token1: [u8; 20],
    
    /// Token 0 decimals (e.g., 18 for WETH, 6 for USDC)
    pub token0_decimals: u8,
    
    /// Token 1 decimals
    pub token1_decimals: u8,
    
    /// DEX protocol (UniswapV2, UniswapV3, etc.)
    pub protocol: String,
    
    /// Fee tier (for V3 pools)
    pub fee_tier: u32,
    
    /// Timestamp when discovered (for cache management)
    pub discovered_at: u64,
}

/// Thread-safe pool metadata cache
pub struct PoolCache {
    /// In-memory cache using DashMap for concurrent access
    cache: Arc<DashMap<[u8; 20], PoolInfo>>,
    
    /// Path to persistent cache file
    cache_file: PathBuf,
    
    /// Whether disk persistence is enabled
    persist_to_disk: bool,
}

impl PoolCache {
    /// Create new pool cache
    pub fn new(cache_dir: PathBuf, persist_to_disk: bool) -> Result<Self> {
        // Ensure cache directory exists
        if persist_to_disk {
            fs::create_dir_all(&cache_dir)
                .context("Failed to create cache directory")?;
        }
        
        let cache_file = cache_dir.join("pool_metadata.json");
        let cache = Arc::new(DashMap::new());
        
        let mut pool_cache = Self {
            cache,
            cache_file,
            persist_to_disk,
        };
        
        // Load existing cache from disk
        if persist_to_disk {
            pool_cache.load_from_disk()?;
        }
        
        Ok(pool_cache)
    }
    
    /// Get pool info from cache
    pub fn get(&self, pool_address: &[u8; 20]) -> Option<PoolInfo> {
        self.cache.get(pool_address).map(|entry| entry.clone())
    }
    
    /// Insert pool info into cache
    pub fn insert(&self, pool_info: PoolInfo) -> Result<()> {
        let pool_address = pool_info.pool_address;
        self.cache.insert(pool_address, pool_info);
        
        // Persist to disk if enabled
        if self.persist_to_disk {
            self.save_to_disk()?;
        }
        
        Ok(())
    }
    
    /// Check if pool exists in cache
    pub fn contains(&self, pool_address: &[u8; 20]) -> bool {
        self.cache.contains_key(pool_address)
    }
    
    /// Get total number of cached pools
    pub fn len(&self) -> usize {
        self.cache.len()
    }
    
    /// Load cache from disk
    fn load_from_disk(&mut self) -> Result<()> {
        if !self.cache_file.exists() {
            info!("No existing cache file found at {:?}", self.cache_file);
            return Ok(());
        }
        
        let data = fs::read_to_string(&self.cache_file)
            .context("Failed to read cache file")?;
            
        let pools: Vec<PoolInfo> = serde_json::from_str(&data)
            .context("Failed to parse cache file")?;
            
        for pool in pools {
            let address = pool.pool_address;
            self.cache.insert(address, pool);
        }
        
        info!("Loaded {} pools from disk cache", self.cache.len());
        Ok(())
    }
    
    /// Save cache to disk
    fn save_to_disk(&self) -> Result<()> {
        let pools: Vec<PoolInfo> = self.cache
            .iter()
            .map(|entry| entry.value().clone())
            .collect();
            
        let data = serde_json::to_string_pretty(&pools)
            .context("Failed to serialize cache")?;
            
        fs::write(&self.cache_file, data)
            .context("Failed to write cache file")?;
            
        debug!("Saved {} pools to disk cache", pools.len());
        Ok(())
    }
    
    /// Force a snapshot to disk (for graceful shutdown)
    pub fn force_snapshot(&self) -> Result<()> {
        if self.persist_to_disk {
            self.save_to_disk()?;
        }
        Ok(())
    }
}