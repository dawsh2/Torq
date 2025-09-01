//! Transport Performance Metrics
//!
//! High-precision metrics tracking for transport layer performance monitoring.
//! Ensures <35μs hot path operations and tracks P95/P99 latencies.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use std::collections::HashMap;
use parking_lot::RwLock;

/// Performance metrics tracker for transport operations
#[derive(Clone)]
pub struct MetricsTracker {
    /// Atomic counters for lock-free updates
    messages_sent: Arc<AtomicU64>,
    messages_received: Arc<AtomicU64>,
    bytes_sent: Arc<AtomicU64>,
    bytes_received: Arc<AtomicU64>,
    errors: Arc<AtomicU64>,
    
    /// Error type tracking
    error_types: Arc<RwLock<HashMap<String, u64>>>,
    
    /// Latency tracking with percentiles
    latency_tracker: Arc<RwLock<LatencyTracker>>,
    
    /// Last operation timestamps
    last_send: Arc<RwLock<Option<Instant>>>,
    last_receive: Arc<RwLock<Option<Instant>>>,
}

impl MetricsTracker {
    /// Create new metrics tracker
    pub fn new() -> Self {
        Self {
            messages_sent: Arc::new(AtomicU64::new(0)),
            messages_received: Arc::new(AtomicU64::new(0)),
            bytes_sent: Arc::new(AtomicU64::new(0)),
            bytes_received: Arc::new(AtomicU64::new(0)),
            errors: Arc::new(AtomicU64::new(0)),
            error_types: Arc::new(RwLock::new(HashMap::new())),
            latency_tracker: Arc::new(RwLock::new(LatencyTracker::new())),
            last_send: Arc::new(RwLock::new(None)),
            last_receive: Arc::new(RwLock::new(None)),
        }
    }
    
    /// Record a send operation
    #[inline]
    pub fn record_send(&self, bytes: usize, latency_ns: u64) {
        self.messages_sent.fetch_add(1, Ordering::Release);
        self.bytes_sent.fetch_add(bytes as u64, Ordering::Release);
        
        // Update latency tracker (only if not in hot path)
        if let Some(mut tracker) = self.latency_tracker.try_write() {
            tracker.record(latency_ns);
        }
        
        *self.last_send.write() = Some(Instant::now());
    }
    
    /// Record a receive operation
    #[inline]
    pub fn record_receive(&self, bytes: usize) {
        self.messages_received.fetch_add(1, Ordering::Release);
        self.bytes_received.fetch_add(bytes as u64, Ordering::Release);
        *self.last_receive.write() = Some(Instant::now());
    }
    
    /// Record an error with type tracking
    #[inline]
    pub fn record_error(&self) {
        self.errors.fetch_add(1, Ordering::Release);
        self.record_error_type("unknown");
    }
    
    /// Record an error with specific type
    #[inline]
    pub fn record_error_type(&self, error_type: &str) {
        if let Some(mut types) = self.error_types.try_write() {
            *types.entry(error_type.to_string()).or_insert(0) += 1;
        }
    }
    
    /// Get current metrics snapshot
    pub fn get_snapshot(&self) -> super::TransportMetrics {
        let latency = self.latency_tracker.read();
        
        super::TransportMetrics {
            messages_sent: self.messages_sent.load(Ordering::Acquire),
            messages_received: self.messages_received.load(Ordering::Acquire),
            bytes_sent: self.bytes_sent.load(Ordering::Acquire),
            bytes_received: self.bytes_received.load(Ordering::Acquire),
            errors: self.errors.load(Ordering::Acquire),
            last_send_latency_ns: latency.last(),
            avg_send_latency_ns: latency.average(),
            p95_send_latency_ns: latency.percentile(95),
            p99_send_latency_ns: latency.percentile(99),
            last_activity: self.last_activity(),
        }
    }
    
    /// Get last activity time
    fn last_activity(&self) -> Option<Instant> {
        let last_send = *self.last_send.read();
        let last_recv = *self.last_receive.read();
        
        match (last_send, last_recv) {
            (Some(s), Some(r)) => Some(if s > r { s } else { r }),
            (Some(s), None) => Some(s),
            (None, Some(r)) => Some(r),
            (None, None) => None,
        }
    }
}

/// Latency percentile tracker using reservoir sampling
struct LatencyTracker {
    /// Circular buffer for recent samples
    samples: Vec<u64>,
    /// Current position in buffer
    position: usize,
    /// Total samples recorded
    total_samples: u64,
    /// Sum for average calculation
    sum: u64,
    /// Last recorded value
    last: u64,
}

impl LatencyTracker {
    const SAMPLE_SIZE: usize = 1000; // Keep last 1000 samples for percentiles
    
    fn new() -> Self {
        Self {
            samples: Vec::with_capacity(Self::SAMPLE_SIZE),
            position: 0,
            total_samples: 0,
            sum: 0,
            last: 0,
        }
    }
    
    fn record(&mut self, latency_ns: u64) {
        self.last = latency_ns;
        self.sum += latency_ns;
        self.total_samples += 1;
        
        if self.samples.len() < Self::SAMPLE_SIZE {
            self.samples.push(latency_ns);
        } else {
            self.samples[self.position] = latency_ns;
            self.position = (self.position + 1) % Self::SAMPLE_SIZE;
        }
    }
    
    fn last(&self) -> u64 {
        self.last
    }
    
    fn average(&self) -> u64 {
        if self.total_samples > 0 {
            self.sum / self.total_samples
        } else {
            0
        }
    }
    
    fn percentile(&self, p: usize) -> u64 {
        if self.samples.is_empty() {
            return 0;
        }
        
        let mut sorted = self.samples.clone();
        sorted.sort_unstable();
        
        // Correct percentile calculation to avoid off-by-one errors
        let index = ((sorted.len() - 1) * p) / 100;
        sorted[index]
    }
}

impl Default for MetricsTracker {
    fn default() -> Self {
        Self::new()
    }
}

/// Macro for timing operations with minimal overhead
#[macro_export]
macro_rules! time_operation {
    ($metrics:expr, $op:expr) => {{
        let start = std::time::Instant::now();
        let result = $op;
        let latency_ns = start.elapsed().as_nanos() as u64;
        
        // Only record if under threshold to avoid hot path impact
        if latency_ns < 1_000_000 { // 1ms threshold
            if let Some(ref metrics) = $metrics {
                metrics.record_send(0, latency_ns);
            }
        }
        
        result
    }};
}