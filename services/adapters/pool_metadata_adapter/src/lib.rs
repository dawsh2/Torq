//! Pool Metadata Adapter
//! 
//! Provides a clean interface for pool metadata discovery and caching.
//! This adapter handles all RPC calls for pool information, maintaining
//! architectural boundaries where only adapters communicate with external systems.
//!
//! Features:
//! - Async pool metadata discovery via RPC
//! - Persistent caching to avoid repeated RPC calls
//! - Support for multiple DEX protocols (Uniswap V2/V3, Sushiswap, etc.)
//! - Rate limiting and retry logic for RPC calls

pub mod adapter;
pub mod cache;
pub mod config;
pub mod rpc_client;

pub use adapter::PoolMetadataAdapter;
pub use cache::{PoolCache, PoolInfo};
pub use config::PoolMetadataConfig;

#[cfg(test)]
mod tests;