//! Polygon Pool Metadata Service
//!
//! Enriches Polygon DEX events with pool metadata (token decimals, symbols, etc).
//! Connects directly to Polygon Adapter to receive raw events, enriches them
//! using Polygon RPC, and forwards to Market Data Relay.
//!
//! Architecture:
//! Polygon Adapter → THIS SERVICE → Market Data Relay → Strategies
//!     (raw)          (enriches)      (enriched)

use anyhow::{Context, Result};
use clap::Parser;
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

use pool_metadata_adapter::{PoolMetadataAdapter, PoolMetadataConfig};
use types::protocol::tlv::market_data::PoolSwapTLV;
use codec::{parse_header_without_checksum, serialize_with_header};
use config::{ServiceConfig as TorqConfig, load_config};

/// Command-line arguments
#[derive(Parser, Debug)]
#[command(name = "polygon_pool_metadata")]
#[command(about = "Polygon Pool Metadata enrichment service")]
struct Args {
    /// Configuration file path
    #[arg(short, long, default_value = "config/services.toml")]
    config: PathBuf,
    
    /// Environment (development, staging, production)
    #[arg(short, long)]
    environment: Option<String>,
    
    /// Service name in configuration
    #[arg(short, long, default_value = "polygon_pool_metadata")]
    service: String,
}

/// Service configuration
#[derive(Debug, Clone)]
struct ServiceConfig {
    /// Socket to listen on for raw events from Polygon Adapter
    input_socket: PathBuf,
    
    /// Socket to connect to for publishing enriched events
    output_socket: PathBuf,
    
    /// Polygon RPC configuration
    polygon_rpc: String,
    
    /// Polygon RPC fallback URLs
    rpc_fallback: Vec<String>,
    
    /// Cache directory
    cache_dir: PathBuf,
    
    /// Rate limit per second
    rate_limit_per_sec: u32,
    
    /// Max retries for RPC calls
    max_retries: u32,
    
    /// Retry interval in seconds
    retry_interval_sec: u64,
}

impl ServiceConfig {
    /// Create from Torq configuration
    fn from_torq_config(config: &TorqConfig, service_name: &str) -> Result<Self> {
        let service = config.get_service(service_name)
            .ok_or_else(|| anyhow::anyhow!("Service '{}' not found in config", service_name))?;
        
        // Parse socket paths
        let input_socket = service.input
            .as_ref()
            .and_then(|s| s.strip_prefix("unix://"))
            .map(PathBuf::from)
            .ok_or_else(|| anyhow::anyhow!("Missing or invalid input socket"))?;
            
        let output_socket = service.output
            .as_ref()
            .and_then(|s| s.strip_prefix("unix://"))
            .map(PathBuf::from)
            .ok_or_else(|| anyhow::anyhow!("Missing or invalid output socket"))?;
        
        Ok(Self {
            input_socket,
            output_socket,
            polygon_rpc: service.rpc_primary
                .clone()
                .unwrap_or_else(|| "https://polygon-rpc.com".to_string()),
            rpc_fallback: service.rpc_fallback.clone().unwrap_or_default(),
            cache_dir: service.cache_dir
                .clone()
                .unwrap_or_else(|| PathBuf::from("./data/polygon_pool_cache")),
            rate_limit_per_sec: service.rate_limit_per_sec.unwrap_or(5),
            max_retries: service.max_retries.unwrap_or(3),
            retry_interval_sec: service.retry_interval_sec.unwrap_or(30),
        })
    }
}

/// Polygon Pool Metadata Service
#[derive(Clone)]
struct PolygonPoolMetadata {
    config: ServiceConfig,
    adapter: Arc<PoolMetadataAdapter>,
    metrics: Arc<RwLock<Metrics>>,
    retry_queue: Arc<RwLock<HashSet<[u8; 20]>>>,
}

#[derive(Debug, Default, Clone)]
struct Metrics {
    events_received: u64,
    events_enriched: u64,
    events_dropped: u64,
    events_with_defaults: u64,
    cache_hits: u64,
    rpc_discoveries: u64,
}

impl PolygonPoolMetadata {
    async fn new(config: ServiceConfig) -> Result<Self> {
        // Create Polygon-specific pool metadata adapter
        let adapter_config = PoolMetadataConfig {
            cache_dir: config.cache_dir.clone(),
            primary_rpc: config.polygon_rpc.clone(),
            chain_id: 137, // Polygon chain ID
            fallback_rpcs: config.rpc_fallback.clone(),
            rate_limit_per_sec: config.rate_limit_per_sec,
            max_retries: config.max_retries,
            ..Default::default()
        };
        
        let adapter = Arc::new(
            PoolMetadataAdapter::new(adapter_config)
                .context("Failed to create Polygon pool metadata adapter")?
        );
        
        Ok(Self {
            config,
            adapter,
            metrics: Arc::new(RwLock::new(Metrics::default())),
            retry_queue: Arc::new(RwLock::new(HashSet::new())),
        })
    }
    
    /// Main service loop
    async fn run(&self) -> Result<()> {
        info!("🚀 Starting Polygon Pool Metadata Service");
        info!("📥 Input: {:?}", self.config.input_socket);
        info!("📤 Output: {:?}", self.config.output_socket);
        info!("⛓️ Chain: Polygon (137)");
        info!("🔗 RPC: {}", self.config.polygon_rpc);
        
        // Create listener socket for raw events
        if self.config.input_socket.exists() {
            std::fs::remove_file(&self.config.input_socket)?;
        }
        
        if let Some(parent) = self.config.input_socket.parent() {
            std::fs::create_dir_all(parent)?;
        }
        
        let listener = UnixListener::bind(&self.config.input_socket)
            .context("Failed to bind input socket")?;
        
        info!("✅ Listening for raw events on {:?}", self.config.input_socket);
        
        // Accept connection from Polygon Adapter
        loop {
            match listener.accept().await {
                Ok((stream, _)) => {
                    info!("✅ Polygon Adapter connected");
                    
                    // Process events from this connection
                    if let Err(e) = self.process_connection(stream).await {
                        error!("Connection processing error: {}", e);
                    }
                    
                    info!("Connection closed, waiting for reconnection...");
                }
                Err(e) => {
                    error!("Failed to accept connection: {}", e);
                }
            }
            
            // Wait before accepting new connection
            tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
        }
    }
    
    /// Process events from a connection
    async fn process_connection(&self, mut input: UnixStream) -> Result<()> {
        // Connect to output (Market Data Relay)
        let mut output = UnixStream::connect(&self.config.output_socket)
            .await
            .context("Failed to connect to Market Data Relay")?;
        
        info!("✅ Connected to Market Data Relay");
        
        let mut buffer = vec![0u8; 65536];
        
        loop {
            // Read TLV header
            let header_size = input.read(&mut buffer[..8]).await?;
            
            if header_size == 0 {
                info!("Input connection closed");
                break;
            }
            
            if header_size < 8 {
                warn!("Incomplete header: {} bytes", header_size);
                continue;
            }
            
            // Parse header
            let (msg_type, payload_size) = parse_header_without_checksum(&buffer[..8])?;
            
            // Read payload
            if payload_size > buffer.len() {
                buffer.resize(payload_size, 0);
            }
            
            input.read_exact(&mut buffer[..payload_size]).await?;
            
            // Update metrics
            {
                let mut metrics = self.metrics.write().await;
                metrics.events_received += 1;
            }
            
            // Process based on message type
            // Assuming type 11 is PoolSwapTLV
            if msg_type == 11 {
                self.process_swap_event(&buffer[..payload_size], &mut output).await?;
            } else {
                // Forward other messages unchanged
                let full_message = serialize_with_header(msg_type, &buffer[..payload_size])?;
                output.write_all(&full_message).await?;
            }
        }
        
        Ok(())
    }
    
    /// Process and enrich a swap event
    async fn process_swap_event(
        &self,
        payload: &[u8],
        output: &mut UnixStream,
    ) -> Result<()> {
        // Deserialize PoolSwapTLV
        let mut swap = PoolSwapTLV::from_bytes(payload)
            .context("Failed to deserialize PoolSwapTLV")?;
        
        // Extract pool address (first 20 bytes)
        let pool_address: [u8; 20] = swap.pool_address[..20].try_into()?;
        
        // Try to enrich with metadata
        match self.adapter.get_or_discover_pool(pool_address).await {
            Ok(pool_info) => {
                // Successfully enriched - populate decimals
                swap.amount_in_decimals = pool_info.token0_decimals;
                swap.amount_out_decimals = pool_info.token1_decimals;
                
                debug!(
                    "✅ Enriched pool 0x{}: decimals {}/{}",
                    hex::encode(&pool_address[..4]),
                    pool_info.token0_decimals,
                    pool_info.token1_decimals
                );
                
                // Update metrics
                {
                    let mut metrics = self.metrics.write().await;
                    metrics.events_enriched += 1;
                    
                    // Check if this was from cache
                    let adapter_metrics = self.adapter.get_metrics().await;
                    if adapter_metrics.cache_hits > metrics.cache_hits {
                        metrics.cache_hits = adapter_metrics.cache_hits;
                    }
                    if adapter_metrics.rpc_discoveries > metrics.rpc_discoveries {
                        metrics.rpc_discoveries = adapter_metrics.rpc_discoveries;
                    }
                }
                
                // Serialize and forward enriched event
                let enriched_bytes = serialize_with_header(11, swap.as_bytes())?;
                output.write_all(&enriched_bytes).await?;
                
                debug!("📤 Forwarded enriched event ({} bytes)", enriched_bytes.len());
            }
            Err(e) => {
                // Failed to enrich - DROP strategy
                warn!(
                    "❌ Failed to enrich pool 0x{}, dropping event: {}",
                    hex::encode(&pool_address[..4]),
                    e
                );
                
                // Add to retry queue for background discovery
                {
                    let mut queue = self.retry_queue.write().await;
                    if queue.insert(pool_address) {
                        info!(
                            "📋 Added pool 0x{} to retry queue (size: {})",
                            hex::encode(&pool_address[..4]),
                            queue.len()
                        );
                    }
                }
                
                // Update metrics
                {
                    let mut metrics = self.metrics.write().await;
                    metrics.events_dropped += 1;
                }
                
                // Event is dropped - not forwarded to output
            }
        }
        
        Ok(())
    }
    
    /// Background task to retry pool discoveries from queue
    async fn retry_discoveries(&self) {
        info!("🔄 Starting background pool discovery retry task");
        
        loop {
            // Wait 30 seconds between retry attempts
            tokio::time::sleep(tokio::time::Duration::from_secs(30)).await;
            
            // Get pools to retry
            let pools_to_retry: Vec<[u8; 20]> = {
                let queue = self.retry_queue.read().await;
                if queue.is_empty() {
                    continue;
                }
                queue.iter().copied().collect()
            };
            
            if !pools_to_retry.is_empty() {
                info!("🔄 Retrying discovery for {} pools", pools_to_retry.len());
                
                for pool_address in pools_to_retry {
                    // Try to discover
                    match self.adapter.get_or_discover_pool(pool_address).await {
                        Ok(pool_info) => {
                            info!(
                                "✅ Successfully discovered pool 0x{} on retry: {}/{}",
                                hex::encode(&pool_address[..4]),
                                pool_info.token0_decimals,
                                pool_info.token1_decimals
                            );
                            
                            // Remove from retry queue
                            let mut queue = self.retry_queue.write().await;
                            queue.remove(&pool_address);
                        }
                        Err(e) => {
                            debug!(
                                "Still unable to discover pool 0x{}: {}",
                                hex::encode(&pool_address[..4]),
                                e
                            );
                            // Keep in queue for next retry
                        }
                    }
                    
                    // Small delay between individual retries to avoid hammering RPC
                    tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
                }
            }
        }
    }
    
    /// Report metrics periodically
    async fn report_metrics(&self) {
        loop {
            tokio::time::sleep(tokio::time::Duration::from_secs(60)).await;
            
            let metrics = self.metrics.read().await;
            info!(
                "📊 Polygon Metrics: received={}, enriched={}, dropped={}, defaults={}, cache_hits={}, rpc={}",
                metrics.events_received,
                metrics.events_enriched,
                metrics.events_dropped,
                metrics.events_with_defaults,
                metrics.cache_hits,
                metrics.rpc_discoveries
            );
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // Parse command-line arguments
    let args = Args::parse();
    
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("polygon_pool_metadata=info".parse()?)
        )
        .init();
    
    info!("🚀 Polygon Pool Metadata Service Starting");
    info!("📁 Config file: {:?}", args.config);
    info!("🌍 Environment: {:?}", args.environment.as_deref().unwrap_or("default"));
    
    // Load Torq configuration
    let torq_config = TorqConfig::load(
        Some(&args.config),
        args.environment.as_deref()
    ).context("Failed to load configuration")?;
    
    // Create service configuration from Torq config
    let config = ServiceConfig::from_torq_config(&torq_config, &args.service)?;
    
    info!("📥 Input socket: {:?}", config.input_socket);
    info!("📤 Output socket: {:?}", config.output_socket);
    info!("🔗 RPC: {}", config.polygon_rpc);
    info!("💾 Cache: {:?}", config.cache_dir);
    
    // Create service
    let service = PolygonPoolMetadata::new(config).await?;
    
    // Spawn metrics reporter
    let metrics_service = service.clone();
    tokio::spawn(async move {
        metrics_service.report_metrics().await;
    });
    
    // Spawn retry discovery task
    let retry_service = service.clone();
    tokio::spawn(async move {
        retry_service.retry_discoveries().await;
    });
    
    // Run service
    service.run().await?;
    
    Ok(())
}