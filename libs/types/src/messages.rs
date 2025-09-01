//! Domain-specific message types for the Protocol V2 system
//!
//! Contains the message enums and structures for different trading domains:
//! - Market Data: Swaps, quotes, order book updates
//! - Signals: Arbitrage opportunities, momentum signals, risk alerts  
//! - Execution: Order requests, cancellations, execution reports
//!
//! These types handle TLV serialization/deserialization and maintain
//! proper precision for financial calculations.

// Note: TLVMessageBuilder imports removed to avoid circular dependencies
// Messages use simple bincode serialization for now

use anyhow::Result;
use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::marker::PhantomData;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::warn;

/// Core trait for all messages in the system
pub trait Message: Send + Sync + 'static {
    /// Convert to TLV for cross-process communication
    fn to_tlv(&self) -> Result<Vec<u8>>;
    
    /// Reconstruct from TLV bytes
    fn from_tlv(bytes: &[u8]) -> Result<Self>
    where 
        Self: Sized;
    
    /// Type ID for runtime checking and downcasting
    fn message_type_id(&self) -> TypeId {
        TypeId::of::<Self>()
    }
    
    /// Convert to Any for local passing
    fn as_any(self: Arc<Self>) -> Arc<dyn Any + Send + Sync> where Self: Sized {
        self as Arc<dyn Any + Send + Sync>
    }
    
    /// Estimated message size for metrics
    fn estimated_size(&self) -> usize where Self: Sized {
        std::mem::size_of::<Self>()
    }
}

/// Market data domain messages - Internal representation (with Arc)
#[derive(Debug, Clone)]
pub enum MarketMessageInternal {
    /// Pool swap event from DEX
    Swap(Arc<PoolSwapEvent>),
    /// Price quote update
    Quote(Arc<QuoteUpdate>),
    /// Order book update
    OrderBook(Arc<OrderBookUpdate>),
    /// Volume snapshot
    VolumeSnapshot(Arc<VolumeData>),
}

/// Market data domain messages - Wire representation (without Arc)
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum MarketMessageWire {
    /// Pool swap event from DEX
    Swap(PoolSwapEvent),
    /// Price quote update
    Quote(QuoteUpdate),
    /// Order book update
    OrderBook(OrderBookUpdate),
    /// Volume snapshot
    VolumeSnapshot(VolumeData),
}

/// Signal generation domain messages - Internal representation (with Arc)
#[derive(Debug, Clone)]
pub enum SignalMessageInternal {
    /// Arbitrage opportunity signal
    Arbitrage(Arc<ArbitrageSignal>),
    /// Momentum trading signal
    Momentum(Arc<MomentumSignal>),
    /// Liquidation opportunity signal
    Liquidation(Arc<LiquidationSignal>),
    /// Risk threshold breach
    RiskAlert(Arc<RiskAlert>),
}

/// Signal generation domain messages - Wire representation (without Arc)
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum SignalMessageWire {
    /// Arbitrage opportunity signal
    Arbitrage(ArbitrageSignal),
    /// Momentum trading signal
    Momentum(MomentumSignal),
    /// Liquidation opportunity signal
    Liquidation(LiquidationSignal),
    /// Risk threshold breach
    RiskAlert(RiskAlert),
}

/// Execution domain messages - Internal representation (with Arc)
#[derive(Debug, Clone)]
pub enum ExecutionMessageInternal {
    /// Submit new order
    SubmitOrder(Arc<OrderRequest>),
    /// Cancel existing order
    CancelOrder(Arc<CancelRequest>),
    /// Execution result report
    ExecutionReport(Arc<ExecutionResult>),
    /// Position update
    PositionUpdate(Arc<PositionUpdate>),
}

/// Execution domain messages - Wire representation (without Arc)
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum ExecutionMessageWire {
    /// Submit new order
    SubmitOrder(OrderRequest),
    /// Cancel existing order
    CancelOrder(CancelRequest),
    /// Execution result report
    ExecutionReport(ExecutionResult),
    /// Position update
    PositionUpdate(PositionUpdate),
}

/// Type aliases for backward compatibility - default to Internal for service use
pub type MarketMessage = MarketMessageInternal;
pub type SignalMessage = SignalMessageInternal;
pub type ExecutionMessage = ExecutionMessageInternal;

// Bidirectional conversion implementations for MarketMessage

impl From<&MarketMessageInternal> for MarketMessageWire {
    fn from(internal: &MarketMessageInternal) -> Self {
        match internal {
            MarketMessageInternal::Swap(arc) => MarketMessageWire::Swap(arc.as_ref().clone()),
            MarketMessageInternal::Quote(arc) => MarketMessageWire::Quote(arc.as_ref().clone()),
            MarketMessageInternal::OrderBook(arc) => MarketMessageWire::OrderBook(arc.as_ref().clone()),
            MarketMessageInternal::VolumeSnapshot(arc) => MarketMessageWire::VolumeSnapshot(arc.as_ref().clone()),
        }
    }
}

impl From<MarketMessageWire> for MarketMessageInternal {
    fn from(wire: MarketMessageWire) -> Self {
        match wire {
            MarketMessageWire::Swap(event) => MarketMessageInternal::Swap(Arc::new(event)),
            MarketMessageWire::Quote(update) => MarketMessageInternal::Quote(Arc::new(update)),
            MarketMessageWire::OrderBook(book) => MarketMessageInternal::OrderBook(Arc::new(book)),
            MarketMessageWire::VolumeSnapshot(volume) => MarketMessageInternal::VolumeSnapshot(Arc::new(volume)),
        }
    }
}

// Bidirectional conversion implementations for SignalMessage

impl From<&SignalMessageInternal> for SignalMessageWire {
    fn from(internal: &SignalMessageInternal) -> Self {
        match internal {
            SignalMessageInternal::Arbitrage(arc) => SignalMessageWire::Arbitrage(arc.as_ref().clone()),
            SignalMessageInternal::Momentum(arc) => SignalMessageWire::Momentum(arc.as_ref().clone()),
            SignalMessageInternal::Liquidation(arc) => SignalMessageWire::Liquidation(arc.as_ref().clone()),
            SignalMessageInternal::RiskAlert(arc) => SignalMessageWire::RiskAlert(arc.as_ref().clone()),
        }
    }
}

impl From<SignalMessageWire> for SignalMessageInternal {
    fn from(wire: SignalMessageWire) -> Self {
        match wire {
            SignalMessageWire::Arbitrage(signal) => SignalMessageInternal::Arbitrage(Arc::new(signal)),
            SignalMessageWire::Momentum(signal) => SignalMessageInternal::Momentum(Arc::new(signal)),
            SignalMessageWire::Liquidation(signal) => SignalMessageInternal::Liquidation(Arc::new(signal)),
            SignalMessageWire::RiskAlert(alert) => SignalMessageInternal::RiskAlert(Arc::new(alert)),
        }
    }
}

// Bidirectional conversion implementations for ExecutionMessage

impl From<&ExecutionMessageInternal> for ExecutionMessageWire {
    fn from(internal: &ExecutionMessageInternal) -> Self {
        match internal {
            ExecutionMessageInternal::SubmitOrder(arc) => ExecutionMessageWire::SubmitOrder(arc.as_ref().clone()),
            ExecutionMessageInternal::CancelOrder(arc) => ExecutionMessageWire::CancelOrder(arc.as_ref().clone()),
            ExecutionMessageInternal::ExecutionReport(arc) => ExecutionMessageWire::ExecutionReport(arc.as_ref().clone()),
            ExecutionMessageInternal::PositionUpdate(arc) => ExecutionMessageWire::PositionUpdate(arc.as_ref().clone()),
        }
    }
}

impl From<ExecutionMessageWire> for ExecutionMessageInternal {
    fn from(wire: ExecutionMessageWire) -> Self {
        match wire {
            ExecutionMessageWire::SubmitOrder(request) => ExecutionMessageInternal::SubmitOrder(Arc::new(request)),
            ExecutionMessageWire::CancelOrder(request) => ExecutionMessageInternal::CancelOrder(Arc::new(request)),
            ExecutionMessageWire::ExecutionReport(report) => ExecutionMessageInternal::ExecutionReport(Arc::new(report)),
            ExecutionMessageWire::PositionUpdate(update) => ExecutionMessageInternal::PositionUpdate(Arc::new(update)),
        }
    }
}

/// Type-safe receiver for message channels
pub struct TypedReceiver<M: Message> {
    rx: mpsc::Receiver<Arc<dyn Any + Send + Sync>>,
    _phantom: PhantomData<M>,
}

impl<M: Message> TypedReceiver<M> {
    /// Create new typed receiver from channel
    pub fn new(rx: mpsc::Receiver<Arc<dyn Any + Send + Sync>>) -> Self {
        Self {
            rx,
            _phantom: PhantomData,
        }
    }
    
    /// Receive next message of expected type
    pub async fn recv(&mut self) -> Option<Arc<M>> {
        while let Some(any_msg) = self.rx.recv().await {
            // Try to downcast to expected type
            if let Ok(typed) = any_msg.downcast::<M>() {
                return Some(typed);
            } else {
                // Log unexpected message type and continue waiting
                warn!(
                    expected_type = std::any::type_name::<M>(),
                    "Received unexpected message type in TypedReceiver"
                );
            }
        }
        None
    }
    
    /// Try to receive without blocking
    pub fn try_recv(&mut self) -> Result<Arc<M>, tokio::sync::mpsc::error::TryRecvError> {
        match self.rx.try_recv() {
            Ok(any_msg) => {
                if let Ok(typed) = any_msg.downcast::<M>() {
                    Ok(typed)
                } else {
                    warn!(
                        expected_type = std::any::type_name::<M>(),
                        "Received unexpected message type in TypedReceiver"
                    );
                    Err(tokio::sync::mpsc::error::TryRecvError::Empty)
                }
            },
            Err(e) => Err(e),
        }
    }
}

/// Message handler trait for efficient dispatch
pub trait MessageHandler: Send + Sync {
    type Message: Message;
    
    /// Handle incoming message
    async fn handle(&mut self, msg: Self::Message) -> Result<()>;
}

// Individual message types

/// Pool swap event from DEX monitoring
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PoolSwapEvent {
    pub pool_address: [u8; 20],
    pub token0_in: u128,
    pub token1_out: u128,
    pub timestamp_ns: u64,
    pub tx_hash: [u8; 32],
    pub gas_used: u64,
}

/// Price quote update from exchange
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct QuoteUpdate {
    pub instrument_id: u64,
    pub bid_price: i64,  // 8-decimal fixed point
    pub ask_price: i64,  // 8-decimal fixed point
    pub bid_size: u64,
    pub ask_size: u64,
    pub timestamp_ns: u64,
}

/// Order book update
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct OrderBookUpdate {
    pub instrument_id: u64,
    pub bids: Vec<PriceLevel>,
    pub asks: Vec<PriceLevel>,
    pub timestamp_ns: u64,
    pub sequence: u64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PriceLevel {
    pub price: i64,  // 8-decimal fixed point
    pub size: u64,
}

/// Volume data snapshot
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct VolumeData {
    pub instrument_id: u64,
    pub volume_24h: u64,
    pub volume_1h: u64,
    pub timestamp_ns: u64,
}

/// Arbitrage opportunity signal
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ArbitrageSignal {
    pub opportunity_id: u64,
    pub venue_a: u8,
    pub venue_b: u8,
    pub instrument_id: u64,
    pub price_difference: i64,  // 8-decimal fixed point
    pub potential_profit_usd: i64,  // 8-decimal fixed point
    pub confidence_score: u8,  // 0-100
    pub timestamp_ns: u64,
}

/// Momentum trading signal
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct MomentumSignal {
    pub signal_id: u64,
    pub instrument_id: u64,
    pub direction: TradeDirection,
    pub strength: u8,  // 0-100
    pub duration_estimate_seconds: u32,
    pub timestamp_ns: u64,
}

/// Liquidation opportunity
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct LiquidationSignal {
    pub position_id: u64,
    pub instrument_id: u64,
    pub liquidation_price: i64,  // 8-decimal fixed point
    pub position_size: u64,
    pub timestamp_ns: u64,
}

/// Risk alert
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct RiskAlert {
    pub alert_id: u64,
    pub alert_type: RiskAlertType,
    pub severity: AlertSeverity,
    pub description: String,
    pub timestamp_ns: u64,
}

/// Order execution request
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct OrderRequest {
    pub order_id: u64,
    pub instrument_id: u64,
    pub side: TradeSide,
    pub order_type: OrderType,
    pub quantity: u64,
    pub price: Option<i64>,  // 8-decimal fixed point, None for market orders
    pub timestamp_ns: u64,
}

/// Cancel order request
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CancelRequest {
    pub order_id: u64,
    pub timestamp_ns: u64,
}

/// Execution result
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ExecutionResult {
    pub execution_id: u64,
    pub order_id: u64,
    pub status: ExecutionStatus,
    pub filled_quantity: u64,
    pub average_price: i64,  // 8-decimal fixed point
    pub fees: i64,  // 8-decimal fixed point
    pub timestamp_ns: u64,
}

/// Position update
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PositionUpdate {
    pub position_id: u64,
    pub instrument_id: u64,
    pub quantity: i64,  // Signed for long/short
    pub average_price: i64,  // 8-decimal fixed point
    pub unrealized_pnl: i64,  // 8-decimal fixed point
    pub timestamp_ns: u64,
}

// Enums for message fields

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum TradeDirection {
    Buy,
    Sell,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum TradeSide {
    Buy,
    Sell,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum OrderType {
    Market,
    Limit,
    StopLoss,
    TakeProfit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ExecutionStatus {
    Pending,
    PartiallyFilled,
    Filled,
    Cancelled,
    Rejected,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum RiskAlertType {
    PositionLimit,
    VolumeLimit,
    DrawdownLimit,
    VolatilitySpike,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum AlertSeverity {
    Info,
    Warning,
    Critical,
    Emergency,
}

// Message trait implementations

impl Message for MarketMessageInternal {
    fn to_tlv(&self) -> Result<Vec<u8>> {
        // Convert to Wire representation for serialization
        let wire: MarketMessageWire = self.into();
        match &wire {
            MarketMessageWire::Swap(event) => event.to_tlv(),
            MarketMessageWire::Quote(quote) => quote.to_tlv(),
            MarketMessageWire::OrderBook(book) => book.to_tlv(),
            MarketMessageWire::VolumeSnapshot(volume) => volume.to_tlv(),
        }
    }
    
    fn from_tlv(_bytes: &[u8]) -> Result<Self>
    where 
        Self: Sized 
    {
        // TODO: Implement TLV parsing - requires TLV header analysis
        anyhow::bail!("TLV parsing implementation not yet complete")
    }
    
    fn estimated_size(&self) -> usize {
        match self {
            MarketMessageInternal::Swap(event) => std::mem::size_of_val(event.as_ref()),
            MarketMessageInternal::Quote(quote) => std::mem::size_of_val(quote.as_ref()),
            MarketMessageInternal::OrderBook(book) => {
                std::mem::size_of_val(book.as_ref()) + 
                book.bids.len() * std::mem::size_of::<PriceLevel>() +
                book.asks.len() * std::mem::size_of::<PriceLevel>()
            },
            MarketMessageInternal::VolumeSnapshot(volume) => std::mem::size_of_val(volume.as_ref()),
        }
    }
}

impl Message for SignalMessageInternal {
    fn to_tlv(&self) -> Result<Vec<u8>> {
        // Convert to Wire representation for serialization
        let wire: SignalMessageWire = self.into();
        match &wire {
            SignalMessageWire::Arbitrage(signal) => signal.to_tlv(),
            SignalMessageWire::Momentum(signal) => signal.to_tlv(),
            SignalMessageWire::Liquidation(signal) => signal.to_tlv(),
            SignalMessageWire::RiskAlert(alert) => alert.to_tlv(),
        }
    }
    
    fn from_tlv(_bytes: &[u8]) -> Result<Self>
    where 
        Self: Sized 
    {
        // TODO: Implement TLV parsing
        anyhow::bail!("TLV parsing implementation not yet complete")
    }
}

impl Message for ExecutionMessageInternal {
    fn to_tlv(&self) -> Result<Vec<u8>> {
        // Convert to Wire representation for serialization
        let wire: ExecutionMessageWire = self.into();
        match &wire {
            ExecutionMessageWire::SubmitOrder(request) => request.to_tlv(),
            ExecutionMessageWire::CancelOrder(request) => request.to_tlv(),
            ExecutionMessageWire::ExecutionReport(report) => report.to_tlv(),
            ExecutionMessageWire::PositionUpdate(update) => update.to_tlv(),
        }
    }
    
    fn from_tlv(_bytes: &[u8]) -> Result<Self>
    where 
        Self: Sized 
    {
        // TODO: Implement TLV parsing
        anyhow::bail!("TLV parsing implementation not yet complete")
    }
}

// Individual message TLV implementations using Protocol V2

macro_rules! impl_message_tlv {
    ($type:ty, $tlv_type:expr, $domain:expr) => {
        impl Message for $type {
            fn to_tlv(&self) -> Result<Vec<u8>> {
                // Use bincode for now - can be upgraded to Protocol V2 later
                // This creates a minimal TLV-like structure
                let payload = bincode::serialize(self)?;
                let mut message = Vec::new();
                
                // Simple TLV header: [type:2][length:2][payload...]
                message.extend_from_slice(&($tlv_type as u16).to_le_bytes());
                message.extend_from_slice(&(payload.len() as u16).to_le_bytes());
                message.extend_from_slice(&payload);
                
                Ok(message)
            }
            
            fn from_tlv(bytes: &[u8]) -> Result<Self>
            where 
                Self: Sized 
            {
                if bytes.len() < 4 {
                    anyhow::bail!("Message too short for TLV header");
                }
                
                let payload = &bytes[4..];
                Ok(bincode::deserialize(payload)?)
            }
        }
    };
}

// Market Data domain messages (TLV types 1-19)
// CRITICAL: TLV type numbers MUST match central registry in libs/types/src/protocol/tlv/types.rs
impl_message_tlv!(PoolSwapEvent, 11, 1);    // Market Data domain - matches PoolSwap = 11
impl_message_tlv!(QuoteUpdate, 17, 1);      // Market Data domain - matches QuoteUpdate = 17
impl_message_tlv!(OrderBookUpdate, 3, 1);   // Market Data domain - matches OrderBook = 3
impl_message_tlv!(VolumeData, 9, 1);        // Market Data domain - matches VolumeUpdate = 9

// Signal domain messages (TLV types 20-39) 
// CRITICAL: Signal domain TLV numbers MUST match registry
impl_message_tlv!(ArbitrageSignal, 32, 2);  // Signal domain - matches ArbitrageSignal = 32
impl_message_tlv!(MomentumSignal, 21, 2);   // Signal domain - matches AssetCorrelation = 21
impl_message_tlv!(LiquidationSignal, 33, 2); // Signal domain - unique type 33
impl_message_tlv!(RiskAlert, 34, 2);        // Signal domain - unique type 34

// Execution domain messages (TLV types 40-79)
// CRITICAL: Execution domain TLV numbers MUST match registry  
impl_message_tlv!(OrderRequest, 40, 3);     // Execution domain - matches OrderRequest = 40
impl_message_tlv!(CancelRequest, 43, 3);    // Execution domain - matches OrderCancel = 43
impl_message_tlv!(ExecutionResult, 42, 3);  // Execution domain - matches Fill = 42
impl_message_tlv!(PositionUpdate, 61, 3);   // Execution domain - matches PositionUpdate = 61

/// Message registry for debugging and monitoring
#[derive(Debug, Default)]
pub struct MessageRegistry {
    types: HashMap<TypeId, &'static str>,
    counts: HashMap<TypeId, AtomicU64>,
}

impl MessageRegistry {
    pub fn new() -> Self {
        let mut registry = Self::default();
        
        // Register known message types
        registry.register::<MarketMessage>("MarketMessage");
        registry.register::<SignalMessage>("SignalMessage");
        registry.register::<ExecutionMessage>("ExecutionMessage");
        registry.register::<PoolSwapEvent>("PoolSwapEvent");
        registry.register::<QuoteUpdate>("QuoteUpdate");
        registry.register::<ArbitrageSignal>("ArbitrageSignal");
        
        registry
    }
    
    /// Register a message type
    pub fn register<M: Message>(&mut self, name: &'static str) {
        let type_id = TypeId::of::<M>();
        self.types.insert(type_id, name);
        self.counts.insert(type_id, AtomicU64::new(0));
    }
    
    /// Record message processed
    pub fn record_message<M: Message>(&self) {
        if let Some(counter) = self.counts.get(&TypeId::of::<M>()) {
            counter.fetch_add(1, Ordering::Relaxed);
        }
    }
    
    /// Get message statistics
    pub fn get_stats(&self) -> MessageStats {
        let mut message_counts = HashMap::new();
        
        for (type_id, counter) in &self.counts {
            if let Some(&name) = self.types.get(type_id) {
                let count = counter.load(Ordering::Relaxed);
                message_counts.insert(name.to_string(), count);
            }
        }
        
        MessageStats { message_counts }
    }
}

/// Message statistics
#[derive(Debug, Clone)]
pub struct MessageStats {
    pub message_counts: HashMap<String, u64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_enum_sizes() {
        // All Internal message enums should be 16 bytes (8 byte Arc + 8 byte discriminant)
        assert_eq!(std::mem::size_of::<MarketMessageInternal>(), 16);
        assert_eq!(std::mem::size_of::<SignalMessageInternal>(), 16);
        assert_eq!(std::mem::size_of::<ExecutionMessageInternal>(), 16);
        
        // Type aliases should resolve to Internal types
        assert_eq!(std::mem::size_of::<MarketMessage>(), 16);
        assert_eq!(std::mem::size_of::<SignalMessage>(), 16);
        assert_eq!(std::mem::size_of::<ExecutionMessage>(), 16);
    }

    #[test]
    fn test_message_registry() {
        let mut registry = MessageRegistry::new();
        registry.register::<PoolSwapEvent>("PoolSwap");
        
        // Record some messages
        registry.record_message::<PoolSwapEvent>();
        registry.record_message::<PoolSwapEvent>();
        
        let stats = registry.get_stats();
        assert_eq!(stats.message_counts["PoolSwap"], 2);
    }

    #[test]
    fn test_arc_sharing() {
        let event = Arc::new(PoolSwapEvent {
            pool_address: [1; 20],
            token0_in: 1000u128,
            token1_out: 2000u128,
            timestamp_ns: 12345,
            tx_hash: [2; 32],
            gas_used: 21000,
        });
        
        let msg1 = MarketMessage::Swap(Arc::clone(&event));
        let msg2 = MarketMessage::Swap(Arc::clone(&event));
        
        // Both messages point to same allocation
        assert_eq!(Arc::strong_count(&event), 3);
    }

    #[test]
    fn test_message_type_ids() {
        let swap = PoolSwapEvent {
            pool_address: [1; 20],
            token0_in: 1000u128,
            token1_out: 2000u128,
            timestamp_ns: 12345,
            tx_hash: [2; 32],
            gas_used: 21000,
        };
        
        let quote = QuoteUpdate {
            instrument_id: 123,
            bid_price: 4500000000000,  // $45,000.00
            ask_price: 4500100000000,  // $45,001.00
            bid_size: 1000,
            ask_size: 1000,
            timestamp_ns: 12345,
        };
        
        assert_ne!(swap.message_type_id(), quote.message_type_id());
    }

    #[tokio::test]
    async fn test_typed_receiver() {
        let (tx, rx) = mpsc::channel(10);
        let mut typed_rx = TypedReceiver::<PoolSwapEvent>::new(rx);
        
        // Send correct type
        let swap = Arc::new(PoolSwapEvent {
            pool_address: [1; 20],
            token0_in: 1000u128,
            token1_out: 2000u128,
            timestamp_ns: 12345,
            tx_hash: [2; 32],
            gas_used: 21000,
        });
        
        tx.send(swap.clone() as Arc<dyn Any + Send + Sync>).await.unwrap();
        
        // Receive and verify
        let received = typed_rx.recv().await.unwrap();
        assert_eq!(*received, *swap);
    }

    #[test]
    fn test_internal_wire_conversion() {
        let event = Arc::new(PoolSwapEvent {
            pool_address: [1; 20],
            token0_in: 1000u128,
            token1_out: 2000u128,
            timestamp_ns: 12345,
            tx_hash: [2; 32],
            gas_used: 21000,
        });
        
        let internal = MarketMessageInternal::Swap(Arc::clone(&event));
        
        // Convert to wire representation
        let wire: MarketMessageWire = (&internal).into();
        
        // Convert back to internal
        let internal_back: MarketMessageInternal = wire.into();
        
        // Verify the data is preserved (though Arc is new)
        match (&internal, &internal_back) {
            (MarketMessageInternal::Swap(orig), MarketMessageInternal::Swap(back)) => {
                assert_eq!(**orig, **back);
            },
            _ => panic!("Message variants don't match"),
        }
    }
}