# Torq CDD Standards & Architecture

## 🎯 Compiler-Driven Development Philosophy
**Let the compiler guide you. Use types to encode invariants. Make invalid states unrepresentable.**

Every feature MUST leverage Rust's type system for correctness guarantees, with performance benchmarks validating real-world behavior. Compiler checks are the primary quality gate - tests validate performance and real-world behavior.

## 📐 CDD Architecture Pyramid

```
         /\
        /E2E\       5% - End-to-End Benchmarks (5-10 tests)
       /______\      Performance validation, critical paths
      /        \
     /Integration\  25% - Integration Benchmarks (50+ tests)  
    /______________\  Component performance, real dependencies
   /                \
  /  Compiler Checks \ 70% - Type Safety & Compile-time validation
 /____________________\ Zero-cost abstractions, type-driven design
```

## Layer 1: Compiler-Driven Design (Foundation)

### Purpose
Use Rust's type system to encode business invariants and make invalid states unrepresentable. The compiler becomes your primary quality gate.

### Characteristics
- **Speed**: Zero runtime cost validation
- **Safety**: Impossible states rejected at compile time
- **Precision**: Exact error location through types
- **Coverage**: 100% of encoded invariants checked

### Implementation
```rust
// Type-driven design patterns for AlphaPulse

// 1. Use newtypes to prevent confusion
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct WeiAmount(pub u128);  // 18 decimals for WETH

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct UsdcAmount(pub u64); // 6 decimals for USDC

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct UsdCents(pub i64);   // 8 decimals for USD prices

// 2. Make invalid states unrepresentable
pub struct ArbitrageOpportunity {
    pub profit_after_gas: NonZeroU64,  // Cannot be zero or negative
    pub buy_venue: ValidatedVenue,     // Must be real venue
    pub sell_venue: ValidatedVenue,    // Must be real venue
    pub expiry: FutureTimestamp,       // Must be in the future
}

// 3. Use phantom types for Protocol V2 domains
pub struct TLVMessage<Domain> {
    header: MessageHeader,
    payload: Vec<u8>,
    _domain: PhantomData<Domain>,
}

pub struct MarketDataDomain;
pub struct SignalDomain;
pub struct ExecutionDomain;

impl TLVMessage<MarketDataDomain> {
    pub fn new_trade(trade: TradeTLV) -> Self {
        // Compiler ensures correct domain usage
        Self::build_with_type(TLVType::Trade, trade)
    }
}

// 4. Zero-copy parsing with compile-time bounds checking
use zerocopy::{AsBytes, FromBytes, Unaligned};

#[repr(C)]
#[derive(AsBytes, FromBytes, Unaligned)]
pub struct TradeTLV {
    pub price: u64,      // Always 8-decimal fixed-point for USD
    pub quantity: u64,   // Native token precision
    pub timestamp: u64,  // Nanoseconds
}

// 5. Builder pattern with compile-time validation
pub struct TLVMessageBuilder<State> {
    header: MessageHeader,
    extensions: Vec<u8>,
    _state: PhantomData<State>,
}

pub struct WithDomain;
pub struct WithSequence;
pub struct Ready;

impl TLVMessageBuilder<WithDomain> {
    pub fn with_sequence(mut self, seq: u32) -> TLVMessageBuilder<WithSequence> {
        self.header.sequence = seq;
        TLVMessageBuilder { header: self.header, extensions: self.extensions, _state: PhantomData }
    }
}

impl TLVMessageBuilder<Ready> {
    pub fn build(self) -> TLVMessage<MarketDataDomain> {
        // Only Ready state can build - compile-time safety
        TLVMessage::from_parts(self.header, self.extensions)
    }
}
```

### Required Compiler Checks
- All financial calculations use typed amounts (WeiAmount, UsdCents)
- All state transitions use phantom types (prevent invalid states)
- All parsing uses zerocopy traits (zero-cost bounds checking)
- All TLV messages use domain types (prevent cross-domain confusion)
- All error conditions represented in Result types (no panics)

## Layer 2: Performance Benchmarks

### Purpose
Validate that type-safe abstractions maintain zero-cost performance characteristics with real exchange data.

### Characteristics
- **Speed**: Benchmark harness execution
- **Dependencies**: Real exchange data streams
- **Scope**: Critical performance paths
- **Coverage**: >1M msg/s performance targets

### Implementation
```rust
// Location: benches/ directory in crate
// File: benches/performance_[feature].rs

use criterion::{criterion_group, criterion_main, Criterion, BenchmarkId};

fn bench_tlv_parsing_performance(c: &mut Criterion) {
    let mut group = c.benchmark_group("TLV Message Processing");
    
    // Real exchange data for benchmarking
    let real_trade_data = load_real_coinbase_trades(); // NO MOCKS!
    
    for trade_count in [100, 1000, 10000] {
        group.bench_with_input(
            BenchmarkId::new("parse_trades", trade_count),
            &trade_count,
            |b, &count| {
                b.iter(|| {
                    for trade in &real_trade_data[..count] {
                        // Type safety at zero cost
                        let message: TLVMessage<MarketDataDomain> = 
                            TLVMessage::from_bytes(trade).unwrap();
                        criterion::black_box(message.parse_trade_tlv());
                    }
                });
            },
        );
    }
    
    // Performance requirement: >1M msg/s
    group.finish();
}

fn bench_precision_arithmetic(c: &mut Criterion) {
    c.bench_function("arbitrage_calculation_with_types", |b| {
        let price_a = UsdCents::from_dollars(3000);
        let price_b = UsdCents::from_dollars(3050);
        let quantity = WeiAmount::from_ether(1);
        
        b.iter(|| {
            // Compiler ensures type safety, benchmark ensures performance
            let profit = calculate_arbitrage_profit_typed(
                criterion::black_box(price_a),
                criterion::black_box(price_b),
                criterion::black_box(quantity)
            );
            criterion::black_box(profit);
        });
    });
}

criterion_group!(benches, bench_tlv_parsing_performance, bench_precision_arithmetic);
criterion_main!(benches);
```

### Required Performance Benchmarks
- TLV message parsing performance (>1M msg/s target)
- Financial calculation precision preservation
- Zero-cost abstraction validation
- Real exchange data processing throughput
- Memory allocation patterns under load

## Layer 3: End-to-End Validation

### Purpose
Validate complete trading workflows with real exchange data to ensure type-safe abstractions work in production scenarios.

### Characteristics  
- **Speed**: <30s per validation
- **Scope**: Complete pipeline with real data
- **Data**: Live exchange feeds (NO MOCKS)
- **Count**: Critical paths only (5-10 total)

### Implementation
```rust
// Location: tests/e2e/ in root
// File: tests/e2e/arbitrage_validation.rs

#[tokio::test]
async fn validate_arbitrage_pipeline_with_real_data() {
    // Connect to REAL exchange feeds (NO MOCKS)
    let system = ProductionSystem::start_validation_mode().await;
    
    // Use typed configuration - compiler prevents invalid setups
    let config = ArbitrageConfig {
        min_profit: UsdCents::from_dollars(50),    // Type-safe minimum
        max_gas: WeiAmount::from_gwei(100),       // Type-safe gas limit
        exchanges: vec![
            ExchangeConfig::Coinbase(CoinbaseAuth::test_credentials()),
            ExchangeConfig::Polygon(PolygonRpc::mainnet()),
        ],
    };
    
    // Start monitoring - types prevent misconfiguration
    let monitor = system.start_arbitrage_monitoring(config).await?;
    
    // Wait for real market opportunity (30s timeout)
    let opportunity = monitor.await_opportunity(Duration::from_secs(30)).await;
    
    // Validate with typed results
    match opportunity {
        Some(opp) => {
            // Compiler ensures these fields exist and have correct types
            assert!(opp.profit_after_gas.get() > 0);  // NonZeroU64 guarantees profitability
            assert!(opp.expiry > SystemTime::now());   // FutureTimestamp guarantees validity
        },
        None => {
            // No opportunity found - acceptable in validation
            println!("No arbitrage opportunities in 30s window - market efficient");
        }
    }
    
    // Performance validation: system must maintain >1M msg/s
    let metrics = monitor.get_performance_metrics();
    assert!(metrics.messages_per_second > 1_000_000);
}
```

### Required E2E Validations
- Profitable arbitrage detection with real data
- Market efficiency validation (no opportunities)
- Performance under sustained >1M msg/s load
- Precision preservation through complete pipeline
- Graceful degradation during exchange disconnects

## Specialized Patterns for Financial Systems

### Type-Safe Financial Calculations
Encode financial invariants in the type system to prevent errors at compile time.

```rust
// Use Rust's type system to prevent financial errors

// 1. Phantom types prevent price/quantity confusion
#[derive(Debug, Clone, Copy)]
pub struct Price<Asset, Currency>(pub u64, PhantomData<(Asset, Currency)>);
#[derive(Debug, Clone, Copy)]
pub struct Quantity<Asset>(pub u64, PhantomData<Asset>);

pub struct WETH;
pub struct USDC;
pub struct USD;

// Compiler prevents: price + quantity (type error!)
type WethPrice = Price<WETH, USD>;     // $WETH per token
type WethQuantity = Quantity<WETH>;    // WETH amount

// 2. Enforce business rules through types
pub struct ProfitableOpportunity {
    pub profit: NonZeroU64,              // Cannot be zero/negative
    pub buy_price: Price<WETH, USD>,     // Clear directionality
    pub sell_price: Price<WETH, USD>,    // Compiler prevents reversal
    pub quantity: Quantity<WETH>,        // Amount to trade
}

impl ProfitableOpportunity {
    pub fn new(
        buy_price: Price<WETH, USD>,
        sell_price: Price<WETH, USD>,
        quantity: Quantity<WETH>,
        gas_cost: UsdCents,
    ) -> Option<Self> {
        // Business logic encoded in constructor
        let gross_profit = (sell_price.0 - buy_price.0) * quantity.0;
        let net_profit = gross_profit.saturating_sub(gas_cost.0 as u64);
        
        NonZeroU64::new(net_profit).map(|profit| Self {
            profit,
            buy_price,
            sell_price,
            quantity,
        })
    }
}

// 3. Property-based validation still useful for edge cases
use proptest::prelude::*;

proptest! {
    #[test]
    fn typed_profit_calculation_never_overflows(
        buy_price in 1u64..1_000_000_000u64,
        sell_price in 1u64..1_000_000_000u64,
        quantity in 1u64..1_000_000u64,
    ) {
        let buy = Price::<WETH, USD>(buy_price, PhantomData);
        let sell = Price::<WETH, USD>(sell_price, PhantomData);
        let qty = Quantity::<WETH>(quantity, PhantomData);
        let gas = UsdCents(100_000_000); // $1.00
        
        // Type system prevents invalid operations
        let opportunity = ProfitableOpportunity::new(buy, sell, qty, gas);
        
        // If profitable, profit must be positive (NonZeroU64 guarantees this)
        if let Some(opp) = opportunity {
            prop_assert!(opp.profit.get() > 0);  // Always true by construction
        }
    }
}
```

### Fuzz Testing for Type Safety
Validate that type-safe parsing handles malformed input gracefully without panics.

```rust
// fuzz/fuzz_targets/typed_tlv_parser.rs
#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Type-safe parsing must never panic, even with garbage
    // Result types handle all error cases gracefully
    match TLVMessage::<MarketDataDomain>::from_bytes(data) {
        Ok(msg) => {
            // If parsing succeeds, types guarantee validity
            let _ = msg.extract_trade_tlv(); // Cannot panic
        },
        Err(_) => {
            // Expected for malformed input - no panic
        }
    }
});
```

Run with:
```bash
cargo fuzz run typed_tlv_parser -- -max_len=1024
```

### Market Replay Validation
Validate type-safe system behavior with real historical market data.

```rust
#[tokio::test]
async fn validate_market_crash_type_safety() {
    // Load real market crash data
    let replay_data: Vec<MarketEvent> = load_real_market_data("2024-01-15-flash-crash.json");
    let mut system = TypedTradingSystem::new();
    
    for event in replay_data {
        // Type safety ensures no invalid state transitions
        match system.process_typed_event(event).await {
            Ok(state) => {
                // Type system guarantees health invariants
                assert!(state.is_valid()); // Cannot fail due to types
            },
            Err(SystemError::ExchangeDisconnection) => {
                // Expected during market stress - graceful degradation
                continue;
            },
            Err(other) => {
                panic!("Unexpected error during replay: {:?}", other);
            }
        }
    }
    
    // Performance validation: type safety shouldn't hurt throughput
    let metrics = system.get_performance_metrics();
    assert!(metrics.events_per_second > 100_000); // Type-safe system performance
}
```

## Type-Safe Data Management

### Use Real Exchange Data with Type Safety
```rust
// ❌ WRONG - Hardcoded and untyped
#[test]
fn test_bad() {
    let profit = 150.0; // Hardcoded AND f64!
    assert_eq!(calculate(), profit);
}

// ✅ CORRECT - Real data with typed calculations
#[test] 
fn test_with_real_coinbase_data() {
    let real_market_data = load_real_coinbase_quotes(); // NO MOCKS!
    let buy_price: Price<WETH, USD> = real_market_data.best_bid.into();
    let sell_price: Price<WETH, USD> = real_market_data.best_ask.into();
    let quantity = Quantity::<WETH>(WeiAmount::from_ether(1).0, PhantomData);
    
    // Type-safe calculation with real data
    let opportunity = ProfitableOpportunity::new(buy_price, sell_price, quantity, UsdCents(0));
    
    // Type system guarantees: if Some(opp), then opp.profit > 0
    if let Some(opp) = opportunity {
        assert!(opp.profit.get() > 0); // Always true by NonZeroU64
    }
}
```

### Use Typed Data Builders
```rust
pub struct TypedDataBuilder {
    price: Price<WETH, USD>,
    quantity: Quantity<WETH>,
    timestamp: SystemTime,
}

impl TypedDataBuilder {
    pub fn profitable_scenario() -> ArbitrageScenario {
        Self {
            price: Price(UsdCents::from_dollars(3000).0, PhantomData),
            quantity: Quantity(WeiAmount::from_ether(1).0, PhantomData),
            timestamp: SystemTime::now(),
        }.build()
    }
    
    pub fn from_real_exchange_data(exchange: ExchangeId) -> Self {
        // Always use real data - NO MOCKS
        let live_data = fetch_live_quotes(exchange);
        Self {
            price: live_data.weth_usd_price,
            quantity: live_data.available_liquidity,
            timestamp: live_data.timestamp,
        }
    }
}
```

## Compiler-Driven Validation Requirements

### Minimum Compiler Validation by Component
- Protocol parsing: 100% (zerocopy traits enforce bounds)
- Financial calculations: 100% (typed amounts prevent confusion)
- Business logic: 100% (Result types handle all error cases)
- State management: 100% (phantom types prevent invalid transitions)
- Type safety: 100% (impossible states unrepresentable)
- Performance: >1M msg/s (benchmark validation required)

### How to Validate
```bash
# Primary quality gate: compiler checks
cargo check --workspace                    # Basic compilation
cargo clippy --workspace                   # Advanced lint checks
cargo clippy -- -D warnings               # Treat warnings as errors

# Performance validation (replaces coverage metrics)
cargo bench --workspace                    # Performance benchmarks
cargo run --bin performance_validator      # AlphaPulse specific targets

# Type safety validation
cargo build --release                      # Optimized builds reveal issues
```

## CI/CD Requirements

### Pre-commit Hooks
```bash
#!/bin/bash
# .git/hooks/pre-commit

# Primary quality gate: compiler validation
cargo check --workspace
cargo clippy --workspace -- -D warnings

# Performance validation
cargo bench --workspace -- --sample-size 10  # Quick benchmark
```

### PR Checks (GitHub Actions)
```yaml
name: Compiler-Driven Validation
on: [pull_request]

jobs:
  validate:
    runs-on: ubuntu-latest
    steps:
      - name: Compiler Validation
        run: |
          cargo check --workspace
          cargo clippy --workspace -- -D warnings
          cargo build --release --workspace
        
      - name: Performance Benchmarks
        run: |
          cargo bench --workspace
          # Validate >1M msg/s target maintained
          
      - name: Type Safety Validation
        run: |
          # Verify zerocopy traits compile without unsafe
          cargo expand --package protocol_v2 | grep -v unsafe
          
      - name: Real Data E2E Validation
        run: cargo test --test arbitrage_validation --release
        env:
          ENABLE_REAL_DATA_TESTS: true
```

### Main Branch Protection
- All compiler checks must pass (check + clippy + build)
- Performance benchmarks must not regress
- At least one review required
- E2E validation with real data must pass

## CDD Anti-Patterns to Avoid

### ❌ Using Primitive Types for Domain Values
```rust
// BAD: No type safety
fn calculate_profit(price_a: f64, price_b: f64, quantity: f64) -> f64 {
    (price_b - price_a) * quantity  // Can mix up parameters!
}
```

### ❌ Using Mocks Instead of Real Data
```rust
// BAD: Mock data hides real-world issues
#[test]
fn test_with_mock() {
    let mock_price = 3000.0; // Fake data!
    assert!(process_price(mock_price).is_ok());
}
```

### ❌ Allowing Invalid States at Runtime
```rust
// BAD: Runtime validation only
pub struct ArbitrageOpp {
    pub profit: i64,  // Could be negative!
}

impl ArbitrageOpp {
    pub fn new(profit: i64) -> Result<Self, Error> {
        if profit <= 0 {
            return Err(Error::NotProfitable); // Runtime check
        }
        Ok(Self { profit })
    }
}
```

### ❌ Ignoring Compiler Warnings
```rust
// BAD: Letting clippy warnings accumulate
#[allow(clippy::all)]  // Never do this!
fn sloppy_function() -> Result<(), Box<dyn Error>> {
    // Sloppy code that compiles but isn't safe
}
```

## CDD Code Organization

### Directory Structure
```
crate/
├── src/
│   ├── lib.rs          # Type-safe APIs and compiler checks
│   └── types.rs        # Domain types with invariants
├── benches/
│   ├── performance/    # Performance benchmarks (primary validation)
│   └── real_data/      # Real exchange data benchmarks
└── tests/
    └── validation/     # Real data validation tests

project_root/
└── tests/
    └── e2e/            # End-to-end real data validation
```

### Naming Conventions
- Type definitions: `[Domain][Entity]` (e.g., `WethPrice`, `ArbitrageOpportunity`)
- Benchmarks: `bench_[operation]_performance`
- Validation: `validate_[workflow]_with_real_data`
- Property tests: `prop_[type_invariant]`
- Fuzz targets: `fuzz_[parser]_safety`

## Debugging Compiler and Performance Issues

### Run specific validations
```bash
cargo check --package specific_package    # Check compilation
cargo clippy --package specific_package   # Check lints
cargo bench bench_specific_name           # Check performance
```

### Show compiler output
```bash
cargo build --verbose                     # Detailed compilation
cargo expand --package protocol_v2        # Show macro expansions
```

### Performance analysis
```bash
RUST_LOG=debug cargo bench                # Benchmark with logging
cargo build --release --verbose           # Check optimization
```

### Type-level debugging
```bash
cargo build --release                     # Optimized build reveals issues
cargo doc --open                          # Generate docs to verify API
```

## Conclusion

Compiler-driven development is not optional at Torq. Every PR must pass all compiler checks and performance benchmarks. The CDD pyramid ensures zero-cost abstractions while providing production-ready performance. Type safety prevents entire classes of bugs at compile time.

**Remember**: A type that could have prevented the hardcoded $150 issue at compile time is worth 1000 lines of runtime validation code.