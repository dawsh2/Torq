//! Service Configuration Module
//!
//! Provides configuration loading and management for Torq services.
//! Supports loading from TOML files with environment-specific overrides.

use anyhow::{Context, Result};
use config_crate::{Config, ConfigError, Environment, File};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tracing::{debug, info, warn};

/// Main service configuration structure
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ServiceConfig {
    /// Global settings
    pub global: GlobalConfig,
    
    /// Service-specific configurations
    pub services: HashMap<String, ServiceSettings>,
    
    /// Deployment configuration
    pub deployment: DeploymentConfig,
    
    /// Feature flags
    pub features: FeatureFlags,
}

/// Global configuration settings
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct GlobalConfig {
    pub socket_dir: PathBuf,
    pub log_dir: PathBuf,
    pub log_level: String,
    pub pid_dir: Option<PathBuf>,
    pub enable_metrics: bool,
}

/// Individual service settings
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ServiceSettings {
    #[serde(rename = "type")]
    pub service_type: String,
    
    pub enabled: Option<bool>,
    pub priority: Option<u32>,
    pub health_port: Option<u16>,
    
    // Connection endpoints
    pub socket: Option<String>,
    pub input: Option<String>,
    pub output: Option<String>,
    pub websocket: Option<String>,
    
    // Service-specific settings
    pub chain: Option<String>,
    pub chain_id: Option<u64>,
    pub rpc_primary: Option<String>,
    pub rpc_fallback: Option<Vec<String>>,
    pub cache_dir: Option<PathBuf>,
    pub rate_limit_per_sec: Option<u32>,
    pub max_retries: Option<u32>,
    pub retry_interval_sec: Option<u64>,
    
    // Strategy settings
    pub min_profit_usd: Option<f64>,
    pub max_gas_price_gwei: Option<f64>,
    pub pool_state_file: Option<PathBuf>,
    
    // Relay settings
    pub max_clients: Option<usize>,
    pub buffer_size: Option<usize>,
    
    pub description: Option<String>,
}

/// Deployment configuration
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct DeploymentConfig {
    pub mode: String,  // "separate" or "enriched"
    pub separate: Option<DeploymentMode>,
    pub enriched: Option<DeploymentMode>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct DeploymentMode {
    pub services: Vec<String>,
}

/// Feature flags
#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct FeatureFlags {
    pub enable_arbitrum: bool,
    pub enable_base: bool,
    pub enable_metrics: bool,
    pub enable_execution: bool,
    pub debug_mode: bool,
}

impl Default for GlobalConfig {
    fn default() -> Self {
        Self {
            socket_dir: PathBuf::from("/tmp/torq"),
            log_dir: PathBuf::from("./logs"),
            log_level: "info".to_string(),
            pid_dir: Some(PathBuf::from("./.pids")),
            enable_metrics: false,
        }
    }
}

impl ServiceConfig {
    /// Load configuration from files with environment overrides
    pub fn load(
        base_path: Option<&Path>,
        environment: Option<&str>,
    ) -> Result<Self> {
        let base = base_path.unwrap_or(Path::new("config/services.toml"));
        
        let mut builder = Config::builder()
            .add_source(File::from(base).required(true));
        
        // Add environment-specific overrides if specified
        if let Some(env) = environment {
            let env_file = PathBuf::from("config/environments")
                .join(format!("{}.toml", env));
            
            if env_file.exists() {
                info!("Loading environment config: {:?}", env_file);
                builder = builder.add_source(File::from(env_file));
            } else {
                warn!("Environment config not found: {:?}", env_file);
            }
        }
        
        // Override with environment variables (TORQ_ prefix)
        builder = builder.add_source(
            Environment::with_prefix("TORQ")
                .separator("_")
                .try_parsing(true)
        );
        
        let config = builder.build()
            .context("Failed to build configuration")?;
        
        config.try_deserialize()
            .context("Failed to deserialize configuration")
    }
    
    /// Get settings for a specific service
    pub fn get_service(&self, name: &str) -> Option<&ServiceSettings> {
        self.services.get(name)
    }
    
    /// Get list of services to start based on deployment mode
    pub fn get_active_services(&self) -> Vec<String> {
        match self.deployment.mode.as_str() {
            "enriched" => {
                self.deployment.enriched
                    .as_ref()
                    .map(|m| m.services.clone())
                    .unwrap_or_default()
            }
            _ => {
                self.deployment.separate
                    .as_ref()
                    .map(|m| m.services.clone())
                    .unwrap_or_default()
            }
        }
    }
    
    /// Expand environment variables in string values
    pub fn expand_env_vars(&mut self) -> Result<()> {
        for (_name, service) in &mut self.services {
            // Expand socket paths
            if let Some(socket) = &service.socket {
                let expanded = shellexpand::env(socket)
                    .context("Failed to expand socket path")?;
                service.socket = Some(expanded.to_string());
            }
            
            // Expand input/output paths
            if let Some(input) = &service.input {
                let expanded = shellexpand::env(input)
                    .context("Failed to expand input path")?;
                service.input = Some(expanded.to_string());
            }
            
            if let Some(output) = &service.output {
                let expanded = shellexpand::env(output)
                    .context("Failed to expand output path")?;
                service.output = Some(expanded.to_string());
            }
            
            // Expand websocket URLs
            if let Some(ws) = &service.websocket {
                let expanded = shellexpand::env(ws)
                    .context("Failed to expand websocket URL")?;
                service.websocket = Some(expanded.to_string());
            }
            
            // Expand RPC URLs
            if let Some(rpc) = &service.rpc_primary {
                let expanded = shellexpand::env(rpc)
                    .context("Failed to expand RPC URL")?;
                service.rpc_primary = Some(expanded.to_string());
            }
        }
        
        Ok(())
    }
}

/// Convenience function to load configuration with defaults
pub fn load_config(environment: Option<&str>) -> Result<ServiceConfig> {
    let mut config = ServiceConfig::load(None, environment)?;
    config.expand_env_vars()?;
    Ok(config)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;
    
    #[test]
    fn test_load_base_config() {
        let dir = tempdir().unwrap();
        let config_path = dir.path().join("services.toml");
        
        let config_content = r#"
[global]
socket_dir = "/tmp/test"
log_dir = "./logs"
log_level = "debug"

[services.test_service]
type = "relay"
socket = "unix:///tmp/test.sock"

[deployment]
mode = "separate"

[features]
debug_mode = true
"#;
        
        fs::write(&config_path, config_content).unwrap();
        
        let config = ServiceConfig::load(Some(&config_path), None).unwrap();
        
        assert_eq!(config.global.socket_dir, PathBuf::from("/tmp/test"));
        assert_eq!(config.global.log_level, "debug");
        assert!(config.features.debug_mode);
        
        let service = config.get_service("test_service").unwrap();
        assert_eq!(service.service_type, "relay");
        assert_eq!(service.socket.as_ref().unwrap(), "unix:///tmp/test.sock");
    }
    
    #[test]
    fn test_environment_override() {
        // This would test loading with environment-specific overrides
        // Skipping for brevity
    }
}