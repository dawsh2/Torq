//! State Management Traits
//!
//! Core traits for implementing stateful components with sequence tracking.

use thiserror::Error;

/// Error types for state management operations
#[derive(Debug, Error)]
pub enum StateError {
    #[error("Sequence gap detected: expected {expected}, actual {actual}")]
    SequenceGap { expected: u64, actual: u64 },
    
    #[error("State validation failed: {reason}")]
    ValidationFailed { reason: String },
    
    #[error("State operation failed: {reason}")]
    OperationFailed { reason: String },
}

/// Core trait for stateful components that can apply events
pub trait Stateful {
    /// Event type this component can handle
    type Event;
    
    /// Error type for failed operations  
    type Error: std::error::Error + Send + Sync + 'static;
    
    /// Apply an event to update the state
    fn apply_event(&mut self, event: Self::Event) -> Result<(), Self::Error>;
    
    /// Create a snapshot of the current state
    fn snapshot(&self) -> Vec<u8>;
    
    /// Restore state from a snapshot
    fn restore(&mut self, snapshot: &[u8]) -> Result<(), Self::Error>;
}

/// Enhanced stateful trait that includes sequence number tracking
pub trait SequencedStateful: Stateful {
    /// Apply an event with sequence number validation
    fn apply_sequenced(&mut self, sequence: u64, event: Self::Event) -> Result<(), Self::Error>;
    
    /// Get the last processed sequence number
    fn last_sequence(&self) -> u64;
}

/// Sequence tracking for gap detection
pub struct SequenceTracker {
    next_expected: u64,
    last_processed: u64,
}

impl SequenceTracker {
    /// Create a new sequence tracker starting from sequence 1
    pub fn new() -> Self {
        Self { 
            next_expected: 1,
            last_processed: 0,
        }
    }
    
    /// Get the next expected sequence number
    pub fn next_expected(&self) -> u64 {
        self.next_expected
    }
    
    /// Get the last successfully processed sequence number
    pub fn last_sequence(&self) -> u64 {
        self.last_processed
    }
    
    /// Set the last processed sequence number (used during restore)
    pub fn set_last_sequence(&mut self, sequence: u64) {
        self.last_processed = sequence;
        self.next_expected = sequence + 1;
    }
    
    /// Update the expected sequence (call after successful processing)
    pub fn advance(&mut self) {
        self.last_processed = self.next_expected;
        self.next_expected += 1;
    }
    
    /// Track a sequence number and detect gaps
    pub fn track(&mut self, sequence: u64) -> Result<(), StateError> {
        if sequence == self.next_expected {
            self.advance();
            Ok(())
        } else {
            Err(StateError::SequenceGap { 
                expected: self.next_expected, 
                actual: sequence 
            })
        }
    }
    
    /// Check if sequence is valid and advance if so (alias for track)
    pub fn validate_and_advance(&mut self, sequence: u64) -> Result<(), StateError> {
        self.track(sequence)
    }
}

impl Default for SequenceTracker {
    fn default() -> Self {
        Self::new()
    }
}