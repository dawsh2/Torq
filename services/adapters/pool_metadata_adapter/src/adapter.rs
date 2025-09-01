//! Pool Metadata Adapter Implementation
//!
//! This adapter provides pool metadata discovery and caching services.
//! It's designed to be used by strategies that need to enrich raw swap events
//! with pool information (token addresses, decimals, etc.).

use anyhow::{Context, Result};
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::time::{sleep, Duration};
use tracing::{debug, info, warn};
use crate::cache::{PoolCache, PoolInfo};
use crate::config::PoolMetadataConfig;
use crate::rpc_client::RpcClient;

/// Pool Metadata Adapter
/// 
/// Provides a clean interface for discovering and caching pool metadata.
/// This ensures all RPC calls go through the adapter layer, maintaining
/// architectural boundaries.
pub struct PoolMetadataAdapter {
    /// Configuration
    config: PoolMetadataConfig,
    
    /// Pool metadata cache
    cache: Arc<PoolCache>,
    
    /// RPC client for discoveries
    rpc_client: Arc<RpcClient>,
    
    /// Rate limiter
    rate_limiter: Arc<RwLock<RateLimiter>>,
    
    /// Metrics
    metrics: Arc<RwLock<Metrics>>,
}

#[derive(Debug, Default, Clone)]
pub struct Metrics {
    pub cache_hits: u64,
    pub cache_misses: u64,
    pub rpc_discoveries: u64,
    pub rpc_failures: u64,
}

#[derive(Debug)]
struct RateLimiter {
    requests_per_second: u32,
    last_request: std::time::Instant,
}

impl RateLimiter {
    fn new(requests_per_second: u32) -> Self {
        Self {
            requests_per_second,
            last_request: std::time::Instant::now(),
        }
    }
    
    async fn wait_if_needed(&mut self) {
        let min_interval = Duration::from_millis(1000 / self.requests_per_second as u64);
        let elapsed = self.last_request.elapsed();
        
        if elapsed < min_interval {
            sleep(min_interval - elapsed).await;
        }
        
        self.last_request = std::time::Instant::now();
    }
}

impl PoolMetadataAdapter {
    /// Create new pool metadata adapter
    pub fn new(config: PoolMetadataConfig) -> Result<Self> {
        let cache = Arc::new(PoolCache::new(
            config.cache_dir.clone(),
            config.enable_disk_cache,
        )?);
        
        let rpc_client = Arc::new(RpcClient::new(config.clone())?);
        
        let rate_limiter = Arc::new(RwLock::new(
            RateLimiter::new(config.rate_limit_per_sec)
        ));
        
        info!("Pool Metadata Adapter initialized with {} cached pools", cache.len());
        
        Ok(Self {
            config,
            cache,
            rpc_client,
            rate_limiter,
            metrics: Arc::new(RwLock::new(Metrics::default())),
        })
    }
    
    /// Get pool metadata, discovering via RPC if not cached
    pub async fn get_or_discover_pool(&self, pool_address: [u8; 20]) -> Result<PoolInfo> {
        // Check cache first
        if let Some(pool_info) = self.cache.get(&pool_address) {
            let mut metrics = self.metrics.write().await;
            metrics.cache_hits += 1;
            debug!("Cache hit for pool 0x{}", hex::encode(pool_address));
            return Ok(pool_info);
        }
        
        // Cache miss - need to discover via RPC
        let mut metrics = self.metrics.write().await;
        metrics.cache_misses += 1;
        drop(metrics);
        
        info!("Cache miss for pool 0x{}, discovering via RPC", hex::encode(pool_address));
        
        // Apply rate limiting
        self.rate_limiter.write().await.wait_if_needed().await;
        
        // Discover via RPC with retries
        let mut attempts = 0;
        let max_attempts = self.config.max_retries;
        
        loop {
            attempts += 1;
            
            match self.rpc_client.discover_pool(pool_address).await {
                Ok(pool_info) => {
                    // Cache the discovered pool
                    self.cache.insert(pool_info.clone())?;
                    
                    let mut metrics = self.metrics.write().await;
                    metrics.rpc_discoveries += 1;
                    
                    info!(
                        "Successfully discovered pool 0x{}: {}/{} ({} decimals/{} decimals)",
                        hex::encode(pool_address),
                        hex::encode(pool_info.token0),
                        hex::encode(pool_info.token1),
                        pool_info.token0_decimals,
                        pool_info.token1_decimals,
                    );
                    
                    return Ok(pool_info);
                }
                Err(e) => {
                    warn!(
                        "RPC discovery failed for pool 0x{} (attempt {}/{}): {}",
                        hex::encode(pool_address),
                        attempts,
                        max_attempts,
                        e
                    );
                    
                    if attempts >= max_attempts {
                        let mut metrics = self.metrics.write().await;
                        metrics.rpc_failures += 1;
                        return Err(e);
                    }
                    
                    // Exponential backoff
                    let backoff = Duration::from_millis(1000 * 2_u64.pow(attempts));
                    sleep(backoff).await;
                }
            }
        }
    }
    
    /// Get current metrics
    pub async fn get_metrics(&self) -> Metrics {
        self.metrics.read().await.clone()
    }
    
    /// Force save cache to disk
    pub async fn save_cache(&self) -> Result<()> {
        self.cache.force_snapshot()
    }
}

impl PoolMetadataAdapter {
    /// Helper methods for testing
    #[cfg(test)]
    pub async fn insert_pool(&self, pool_info: PoolInfo) -> Result<()> {
        self.cache.insert(pool_info)
    }
    
    #[cfg(test)]
    pub async fn get_from_cache(&self, pool_address: &[u8; 20]) -> Option<PoolInfo> {
        self.cache.get(pool_address)
    }
    
    #[cfg(test)]
    pub async fn cache_size(&self) -> usize {
        self.cache.len()
    }
}