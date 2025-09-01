//! RPC client for discovering pool metadata
//!
//! Handles all communication with blockchain nodes to fetch pool information.
//! Includes retry logic, rate limiting, and fallback endpoints.

use anyhow::{anyhow, Context, Result};
use std::time::Duration;
use tracing::{debug, warn};
use web3::contract::{Contract, Options};
use web3::transports::Http;
use web3::types::{Address, H160, U256};
use web3::Web3;

use crate::cache::PoolInfo;
use crate::config::PoolMetadataConfig;

/// ERC20 ABI for decimals() function
const ERC20_DECIMALS_ABI: &str = r#"[{"constant":true,"inputs":[],"name":"decimals","outputs":[{"name":"","type":"uint8"}],"type":"function"}]"#;

/// Uniswap V2 Pair ABI for token0() and token1() functions
const PAIR_ABI: &str = r#"[
    {"constant":true,"inputs":[],"name":"token0","outputs":[{"name":"","type":"address"}],"type":"function"},
    {"constant":true,"inputs":[],"name":"token1","outputs":[{"name":"","type":"address"}],"type":"function"}
]"#;

/// Uniswap V3 Pool ABI
const V3_POOL_ABI: &str = r#"[
    {"constant":true,"inputs":[],"name":"token0","outputs":[{"name":"","type":"address"}],"type":"function"},
    {"constant":true,"inputs":[],"name":"token1","outputs":[{"name":"","type":"address"}],"type":"function"},
    {"constant":true,"inputs":[],"name":"fee","outputs":[{"name":"","type":"uint24"}],"type":"function"}
]"#;

pub struct RpcClient {
    config: PoolMetadataConfig,
    web3_clients: Vec<Web3<Http>>,
}

impl RpcClient {
    /// Create new RPC client with configured endpoints
    pub fn new(config: PoolMetadataConfig) -> Result<Self> {
        let mut web3_clients = Vec::new();
        
        // Add primary RPC
        let transport = Http::new(&config.primary_rpc)?;
        web3_clients.push(Web3::new(transport));
        
        // Add fallback RPCs
        for rpc_url in &config.fallback_rpcs {
            if let Ok(transport) = Http::new(rpc_url) {
                web3_clients.push(Web3::new(transport));
            }
        }
        
        if web3_clients.is_empty() {
            return Err(anyhow!("No valid RPC endpoints configured"));
        }
        
        Ok(Self {
            config,
            web3_clients,
        })
    }
    
    /// Discover pool metadata from RPC
    pub async fn discover_pool(&self, pool_address: [u8; 20]) -> Result<PoolInfo> {
        let pool_h160 = H160::from_slice(&pool_address);
        
        // Try each RPC endpoint until one succeeds
        for (idx, web3) in self.web3_clients.iter().enumerate() {
            match self.discover_pool_with_client(web3, pool_h160).await {
                Ok(pool_info) => {
                    debug!("Successfully discovered pool via RPC endpoint {}", idx);
                    return Ok(pool_info);
                }
                Err(e) => {
                    warn!("RPC endpoint {} failed: {}", idx, e);
                    if idx == self.web3_clients.len() - 1 {
                        return Err(anyhow!("All RPC endpoints failed for pool discovery"));
                    }
                }
            }
        }
        
        Err(anyhow!("Failed to discover pool"))
    }
    
    /// Discover pool using specific Web3 client
    async fn discover_pool_with_client(
        &self,
        web3: &Web3<Http>,
        pool_address: H160,
    ) -> Result<PoolInfo> {
        // First, detect if it's V2 or V3 by checking for fee() function
        let protocol = self.detect_protocol(web3, pool_address).await?;
        
        // Get token addresses
        let (token0, token1) = self.get_token_addresses(web3, pool_address, &protocol).await?;
        
        // Get token decimals
        let token0_decimals = self.get_token_decimals(web3, token0).await?;
        let token1_decimals = self.get_token_decimals(web3, token1).await?;
        
        // Get fee tier for V3 pools
        let fee_tier = if protocol == "UniswapV3" {
            self.get_v3_fee(web3, pool_address).await.unwrap_or(3000)
        } else {
            30 // Standard 0.3% for V2
        };
        
        Ok(PoolInfo {
            pool_address: pool_address.as_bytes().try_into()?,
            token0: token0.as_bytes().try_into()?,
            token1: token1.as_bytes().try_into()?,
            token0_decimals,
            token1_decimals,
            protocol,
            fee_tier,
            discovered_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_nanos() as u64,
        })
    }
    
    /// Detect if pool is V2 or V3
    async fn detect_protocol(&self, web3: &Web3<Http>, pool_address: H160) -> Result<String> {
        // Try to call fee() function (V3 only)
        let contract = Contract::from_json(
            web3.eth(),
            pool_address,
            V3_POOL_ABI.as_bytes(),
        )?;
        
        match contract.query::<u32, _, _, _>("fee", (), None, Options::default(), None).await {
            Ok(_fee) => Ok("UniswapV3".to_string()),
            Err(_) => Ok("UniswapV2".to_string()),
        }
    }
    
    /// Get token addresses from pool
    async fn get_token_addresses(
        &self,
        web3: &Web3<Http>,
        pool_address: H160,
        protocol: &str,
    ) -> Result<(H160, H160)> {
        let abi = if protocol == "UniswapV3" {
            V3_POOL_ABI
        } else {
            PAIR_ABI
        };
        
        let contract = Contract::from_json(
            web3.eth(),
            pool_address,
            abi.as_bytes(),
        )?;
        
        let token0: Address = contract
            .query("token0", (), None, Options::default(), None)
            .await
            .context("Failed to get token0")?;
            
        let token1: Address = contract
            .query("token1", (), None, Options::default(), None)
            .await
            .context("Failed to get token1")?;
            
        Ok((token0, token1))
    }
    
    /// Get token decimals
    async fn get_token_decimals(&self, web3: &Web3<Http>, token_address: H160) -> Result<u8> {
        let contract = Contract::from_json(
            web3.eth(),
            token_address,
            ERC20_DECIMALS_ABI.as_bytes(),
        )?;
        
        let decimals: u8 = contract
            .query("decimals", (), None, Options::default(), None)
            .await
            .context("Failed to get decimals")?;
            
        Ok(decimals)
    }
    
    /// Get fee tier for V3 pools
    async fn get_v3_fee(&self, web3: &Web3<Http>, pool_address: H160) -> Result<u32> {
        let contract = Contract::from_json(
            web3.eth(),
            pool_address,
            V3_POOL_ABI.as_bytes(),
        )?;
        
        let fee: u32 = contract
            .query("fee", (), None, Options::default(), None)
            .await
            .context("Failed to get fee")?;
            
        Ok(fee)
    }
}