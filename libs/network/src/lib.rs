//! Network Infrastructure
//! 
//! This crate provides the reorganized networking infrastructure with clear module boundaries
//! and consolidated functionality. The previous scattered implementations have been unified.

pub mod error;
pub mod message;

// New unified modules
pub mod transports;
pub mod routing;
// actors moved to services/messaging/actors
pub mod discovery; 
pub mod protocol;
pub mod recovery;

// Performance and monitoring modules
pub mod performance;
pub mod time;

// Re-export commonly used types
pub use error::{NetworkError, Result, TransportError};
pub use message::{NetworkMessage, ByteMessage, NetworkEnvelope, NetworkPriority, Priority};

// Re-export from new unified modules
pub use transports::{Transport, TransportFactory, TransportConfig, TransportType, TransportInfo};
pub use routing::{Router, RouterFactory, RoutingStrategy, RoutingDecision};
// Actor exports removed - actors moved to services/messaging/actors
pub use discovery::{ServiceDiscovery, ServiceDiscoveryFactory, ServiceLocation};
pub use protocol::{ProtocolProcessor, ProtocolConfig};

// Re-export time functions for external use
pub use time::{
    CachedClock, fast_timestamp_ns, current_timestamp_ns, precise_timestamp_ns,
    init_timestamp_system, parse_external_timestamp_safe, parse_external_unix_timestamp_safe,
    safe_duration_to_ns, safe_duration_to_ns_checked, safe_system_timestamp_ns, 
    safe_system_timestamp_ns_checked, timestamp_accuracy_info, timestamp_system_stats,
    TimestampError
};


// Utility functions
use std::sync::atomic::{AtomicU64, Ordering};
static MESSAGE_ID_COUNTER: AtomicU64 = AtomicU64::new(1);

/// Generate a unique message ID
pub fn generate_message_id() -> u64 {
    MESSAGE_ID_COUNTER.fetch_add(1, Ordering::SeqCst)
}

/// Topology version constant - re-exported from discovery
pub use discovery::TOPOLOGY_VERSION;

// Constants for configuration
pub const DEFAULT_TCP_BUFFER_SIZE: usize = 64 * 1024; // 64KB
pub const DEFAULT_UDP_BUFFER_SIZE: usize = 64 * 1024; // 64KB
pub const DEFAULT_CONNECTION_POOL_SIZE: usize = 10;
pub const DEFAULT_CONNECTION_TIMEOUT_SECS: u64 = 30;
pub const DEFAULT_HEARTBEAT_INTERVAL_SECS: u64 = 60;
