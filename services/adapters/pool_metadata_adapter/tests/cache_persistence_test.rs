//! Cache Persistence Tests
//!
//! Ensures pool metadata cache correctly persists to disk and survives restarts

use pool_metadata_adapter::{PoolMetadataAdapter, PoolMetadataConfig, PoolInfo};
use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

#[tokio::test]
async fn test_cache_persistence_across_restarts() {
    println!("🧪 Testing cache persistence across restarts");
    
    // Create temporary directory for test cache
    let temp_dir = TempDir::new().unwrap();
    let cache_dir = temp_dir.path().to_path_buf();
    
    // Test pool data
    let test_pool = PoolInfo {
        pool_address: [0x01; 20],
        token0: [0x02; 20],
        token1: [0x03; 20],
        token0_decimals: 18,
        token1_decimals: 6,
        protocol: "UniswapV2".to_string(),
        fee_tier: 30,
        discovered_at: 1700000000000000000,
    };
    
    let pool_address = test_pool.pool_address;
    
    // Phase 1: Create adapter and insert pool
    {
        let config = PoolMetadataConfig {
            cache_dir: cache_dir.clone(),
            enable_disk_cache: true,
            ..Default::default()
        };
        
        let adapter = PoolMetadataAdapter::new(config).unwrap();
        
        // Manually insert test pool (simulating discovery)
        adapter.insert_pool(test_pool.clone()).await.unwrap();
        
        // Verify it's in memory
        let retrieved = adapter.get_from_cache(&pool_address).await;
        assert!(retrieved.is_some(), "Pool should be in memory cache");
        
        // Force save to disk
        adapter.save_cache().await.unwrap();
        
        println!("✅ Phase 1: Pool saved to cache");
    }
    
    // Phase 2: Create new adapter instance and verify persistence
    {
        let config = PoolMetadataConfig {
            cache_dir: cache_dir.clone(),
            enable_disk_cache: true,
            ..Default::default()
        };
        
        let adapter = PoolMetadataAdapter::new(config).unwrap();
        
        // Should load from disk automatically
        let retrieved = adapter.get_from_cache(&pool_address).await;
        assert!(retrieved.is_some(), "Pool should be loaded from disk");
        
        let pool_info = retrieved.unwrap();
        assert_eq!(pool_info.token0, test_pool.token0);
        assert_eq!(pool_info.token1, test_pool.token1);
        assert_eq!(pool_info.token0_decimals, test_pool.token0_decimals);
        assert_eq!(pool_info.token1_decimals, test_pool.token1_decimals);
        assert_eq!(pool_info.protocol, test_pool.protocol);
        
        println!("✅ Phase 2: Pool successfully loaded from disk");
    }
    
    // Verify cache file exists
    let cache_file = cache_dir.join("pool_metadata.json");
    assert!(cache_file.exists(), "Cache file should exist");
    
    let file_contents = fs::read_to_string(&cache_file).unwrap();
    assert!(file_contents.contains("\"protocol\":\"UniswapV2\""));
    assert!(file_contents.contains("\"token0_decimals\":18"));
    assert!(file_contents.contains("\"token1_decimals\":6"));
    
    println!("✅ Cache file validated");
    println!("✅ Cache persistence test passed!");
}

#[tokio::test]
async fn test_concurrent_cache_access() {
    println!("🧪 Testing concurrent cache access");
    
    let config = PoolMetadataConfig {
        cache_dir: PathBuf::from("./test_concurrent_cache"),
        enable_disk_cache: false, // Test memory cache concurrency
        ..Default::default()
    };
    
    let adapter = Arc::new(PoolMetadataAdapter::new(config).unwrap());
    
    // Create multiple tasks accessing cache concurrently
    let mut handles = vec![];
    
    for i in 0..10 {
        let adapter_clone = adapter.clone();
        let handle = tokio::spawn(async move {
            // Each task works with different pool
            let mut pool_address = [0u8; 20];
            pool_address[0] = i;
            
            let pool_info = PoolInfo {
                pool_address,
                token0: [i + 1; 20],
                token1: [i + 2; 20],
                token0_decimals: 18,
                token1_decimals: 6,
                protocol: format!("Protocol{}", i),
                fee_tier: 30 * (i as u32 + 1),
                discovered_at: 1700000000000000000 + i as u64,
            };
            
            // Insert pool
            adapter_clone.insert_pool(pool_info.clone()).await.unwrap();
            
            // Read it back multiple times
            for _ in 0..5 {
                let retrieved = adapter_clone.get_from_cache(&pool_address).await;
                assert!(retrieved.is_some());
                assert_eq!(retrieved.unwrap().protocol, format!("Protocol{}", i));
            }
            
            println!("  Task {} completed", i);
        });
        
        handles.push(handle);
    }
    
    // Wait for all tasks
    for handle in handles {
        handle.await.unwrap();
    }
    
    // Verify all pools are in cache
    for i in 0..10 {
        let mut pool_address = [0u8; 20];
        pool_address[0] = i;
        
        let retrieved = adapter.get_from_cache(&pool_address).await;
        assert!(retrieved.is_some(), "Pool {} should be in cache", i);
    }
    
    println!("✅ Concurrent cache access test passed!");
}

#[tokio::test]
async fn test_cache_size_limits() {
    println!("🧪 Testing cache size and memory usage");
    
    let config = PoolMetadataConfig {
        cache_dir: PathBuf::from("./test_cache_size"),
        enable_disk_cache: false,
        ..Default::default()
    };
    
    let adapter = PoolMetadataAdapter::new(config).unwrap();
    
    // Insert many pools to test memory usage
    let pool_count = 1000;
    
    for i in 0..pool_count {
        let mut pool_address = [0u8; 20];
        pool_address[0] = (i >> 8) as u8;
        pool_address[1] = (i & 0xFF) as u8;
        
        let pool_info = PoolInfo {
            pool_address,
            token0: [(i + 1) as u8; 20],
            token1: [(i + 2) as u8; 20],
            token0_decimals: 18,
            token1_decimals: 6,
            protocol: "UniswapV2".to_string(),
            fee_tier: 30,
            discovered_at: 1700000000000000000,
        };
        
        adapter.insert_pool(pool_info).await.unwrap();
    }
    
    let cache_size = adapter.cache_size().await;
    assert_eq!(cache_size, pool_count, "All pools should be cached");
    
    println!("✅ Cached {} pools successfully", pool_count);
    
    // Test retrieval performance with large cache
    let start = std::time::Instant::now();
    
    for i in 0..100 {
        let mut pool_address = [0u8; 20];
        pool_address[0] = (i >> 8) as u8;
        pool_address[1] = (i & 0xFF) as u8;
        
        let _ = adapter.get_from_cache(&pool_address).await;
    }
    
    let elapsed = start.elapsed();
    println!("  100 cache lookups took {:?}", elapsed);
    assert!(elapsed.as_millis() < 10, "Cache lookups should be fast");
    
    println!("✅ Cache size and performance test passed!");
}

use std::sync::Arc;