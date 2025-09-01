# Task Template with CDD Requirements

---
task_id: [CATEGORY]-[NUMBER]
status: TODO
priority: [CRITICAL|HIGH|MEDIUM|LOW]
estimated_hours: [1-8]
assigned_branch: [branch-name]
worktree_path: ../task-worktree
dependencies: [list dependencies if any]
---

## Git Worktree Setup (REQUIRED)
```bash
# NEVER use git checkout - it changes all sessions!
# Create isolated worktree for this task:
git worktree add -b [branch-name] ../task-worktree
cd ../task-worktree

# Verify setup:
git branch --show-current  # Should show: [branch-name]
pwd  # Should show: ../task-worktree
```

## Task Description
[Clear, concise description of what needs to be done]

## Definition of Done - Compiler-Driven Development
- [ ] **Type Safety**: Domain types prevent invalid states at compile time
- [ ] **Compiler Checks**: `cargo check` + `cargo clippy` pass with zero warnings
- [ ] **Performance Benchmarks**: Zero-cost abstractions validated with real data (>1M ops/s)
- [ ] **Real Data Integration**: System works with live exchange feeds (NO MOCKS)
- [ ] **Property Validation**: Mathematical invariants enforced by types
- [ ] **Fuzz Safety**: Parser safety validated with malformed input
- [ ] **E2E Validation**: Complete workflow validated with real exchange data
- [ ] **Precision Preservation**: Financial calculations maintain exact precision
- [ ] All compiler checks pass: `cargo check --workspace`
- [ ] Protocol V2 validation passes: `cargo check --package protocol_v2`
- [ ] Performance targets met: >1M msg/s parsing, >1M arbitrage detections/s
- [ ] Documentation updated with type-driven examples
- [ ] Breaking changes documented and justified
- [ ] Zero-cost abstraction performance verified

## CDD Strategy

### Layer 1: Type-Driven Design (Required)
```rust
// Design types that prevent entire error classes

// 1. Use newtypes for domain separation
#[derive(Debug, Clone, Copy)]
pub struct WethAmount(NonZeroU128);  // Cannot be zero

#[derive(Debug, Clone, Copy)]
pub struct UsdPrice(NonZeroU64);     // Cannot be zero

// 2. Encode business rules in types
pub struct ValidatedArbitrageParams {
    buy_price: UsdPrice,
    sell_price: UsdPrice,
    quantity: WethAmount,
    max_gas: WeiAmount,
}

impl ValidatedArbitrageParams {
    pub fn new(
        buy_cents: u64,
        sell_cents: u64, 
        wei_amount: u128,
        max_gas_wei: u128,
    ) -> Result<Self, ValidationError> {
        // Validation happens once at construction
        let buy_price = UsdPrice(NonZeroU64::new(buy_cents)
            .ok_or(ValidationError::ZeroPrice)?);
        let sell_price = UsdPrice(NonZeroU64::new(sell_cents)
            .ok_or(ValidationError::ZeroPrice)?);
        let quantity = WethAmount(NonZeroU128::new(wei_amount)
            .ok_or(ValidationError::ZeroQuantity)?);
        let max_gas = WeiAmount(max_gas_wei);
        
        // Business rule: sell price must be higher than buy price
        if sell_price.0.get() <= buy_price.0.get() {
            return Err(ValidationError::NoProfitOpportunity);
        }
        
        Ok(Self { buy_price, sell_price, quantity, max_gas })
    }
    
    // This function cannot fail - types guarantee valid inputs
    pub fn calculate_profit_guaranteed(&self) -> NonZeroU64 {
        let gross = self.sell_price.0.get() - self.buy_price.0.get();
        let net = gross.saturating_mul(self.quantity.0.get() as u64);
        
        // Safe: constructor guarantees sell > buy, so net > 0
        unsafe { NonZeroU64::new_unchecked(net) }
    }
}

// 3. Validation tests focus on edge cases, not business logic
#[cfg(test)]
mod validation {
    use super::*;
    
    #[test]
    fn validate_with_real_coinbase_data() {
        // Use real market data - NO MOCKS!
        let real_quotes = load_real_coinbase_quotes();
        
        for quote in real_quotes {
            let params = ValidatedArbitrageParams::new(
                quote.bid_cents,
                quote.ask_cents,
                WeiAmount::from_ether(1).0,
                WeiAmount::from_gwei(50).0,
            );
            
            // Test type safety with real data
            if let Ok(valid_params) = params {
                let profit = valid_params.calculate_profit_guaranteed();
                assert!(profit.get() > 0); // Always true by construction
            }
        }
    }
}
```

### Layer 2: Performance Benchmarks (Required)
```rust
// In benches/performance_[module].rs
use criterion::{criterion_group, criterion_main, Criterion};

fn bench_typed_component_performance(c: &mut Criterion) {
    // Use real exchange data for realistic benchmarks
    let real_market_data = load_real_polygon_swaps(); // NO MOCKS!
    
    c.bench_function("typed_arbitrage_detection", |b| {
        b.iter(|| {
            for swap_data in &real_market_data {
                // Type safety at zero cost
                let params = ValidatedArbitrageParams::new(
                    swap_data.price_cents,
                    swap_data.price_cents + 5000, // $50 spread
                    WeiAmount::from_ether(1).0,
                    WeiAmount::from_gwei(30).0,
                ).unwrap();
                
                let profit = params.calculate_profit_guaranteed();
                criterion::black_box(profit);
            }
        });
    });
    
    // Performance requirement validation
    let stats = c.get_last_benchmark_stats();
    assert!(stats.mean_throughput > 1_000_000.0); // >1M calculations/sec
}

criterion_group!(benches, bench_typed_component_performance);
criterion_main!(benches);
```

### Layer 3: Real Data Validation (For core pipeline changes)
```rust
// In tests/e2e/validation/[feature]_validation.rs
#[tokio::test]
async fn validate_[feature]_with_live_data() {
    // Connect to real exchanges - NO MOCKS!
    let live_system = TypedTradingSystem::connect_live().await?;
    
    // Use typed configuration - compiler prevents invalid setups
    let config = ArbitrageConfig {
        min_profit: UsdCents::from_dollars(10),  // Type-safe minimum
        max_position: WethAmount::from_ether(5), // Type-safe maximum
        timeout: Duration::from_secs(60),
    };
    
    let monitor = live_system.start_monitoring_typed(config).await?;
    
    // Wait for real market data
    match monitor.await_opportunity().await {
        Some(opportunity) => {
            // Type system guarantees these properties
            assert!(opportunity.profit_after_gas.get() > 0);  // NonZeroU64 guarantee
            assert!(opportunity.expiry > SystemTime::now());   // FutureTimestamp guarantee
            
            // Validate performance under real load
            let metrics = monitor.get_performance_metrics();
            assert!(metrics.detections_per_second > 1_000_000);
        },
        None => {
            // No opportunities in timeframe - acceptable
            println!("No arbitrage opportunities detected - market efficient");
        }
    }
}
```

### Property-Based Validation (For mathematical invariants)
```rust
use proptest::prelude::*;

proptest! {
    #[test]
    fn validate_typed_arbitrage_invariants(
        buy_cents in 1u64..1_000_000_000u64,
        spread_cents in 1u64..1_000_000u64,
        wei_amount in 1u128..1_000_000_000_000_000_000u128,
    ) {
        let sell_cents = buy_cents.saturating_add(spread_cents);
        
        // Type system enforces validity
        if let Ok(params) = ValidatedArbitrageParams::new(
            buy_cents, sell_cents, wei_amount, 1_000_000_000u128
        ) {
            // These properties are guaranteed by types
            let profit = params.calculate_profit_guaranteed();
            prop_assert!(profit.get() > 0);  // NonZeroU64 guarantees this
            
            // Type system prevents parameter confusion
            prop_assert!(params.sell_price.0.get() > params.buy_price.0.get());
        }
    }
}
```

### Fuzz Safety Validation (For parsers and validation)
```rust
// fuzz/fuzz_targets/typed_parser_safety.rs
#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Type-safe parsing must never panic
    match TLVMessage::<MarketDataDomain>::from_bytes(data) {
        Ok(msg) => {
            // If parsing succeeds, all operations are safe
            let _ = msg.extract_trade_tlv();           // Cannot panic
            let _ = msg.validate_checksum();           // Cannot panic
            let _ = msg.get_timestamp_nanos();         // Cannot panic
        },
        Err(_) => {
            // Expected for malformed input - graceful failure
        }
    }
    
    // Test that type constructors handle edge cases
    let _ = ValidatedArbitrageParams::new(
        data.get(0).copied().unwrap_or(0) as u64,
        data.get(1).copied().unwrap_or(0) as u64,
        data.get(2).copied().unwrap_or(0) as u128,
        data.get(3).copied().unwrap_or(0) as u128,
    ); // Must return Result, never panic
});
```

## Implementation Notes

### Critical CDD Guidelines
1. **Types First**: Design domain types before implementation
2. **No Mocks Ever**: Validate with real exchange connections and data
3. **Compiler as QA**: Use type system to prevent entire error classes
4. **Zero-Cost Safety**: Type safety must not impact performance
5. **Real Data Only**: Use actual exchange data for all validation

### Typed Data Management
- Use real exchange data for all validation scenarios
- Capture live market feeds for performance testing
- Never hardcode expected values - use type-safe calculations
- Types prevent edge cases: NonZero types eliminate zero values, checked arithmetic prevents overflow

### Performance Validation
- Use criterion benchmarks with real exchange data
- Assert performance requirements: `assert!(throughput > 1_000_000)`
- Validate zero-cost abstractions: typed performance = primitive performance
- Monitor allocation patterns in hot paths

### Financial Type Safety Specifics
- Use WeiAmount for 18-decimal Ethereum amounts
- Use UsdcAmount for 6-decimal USDC amounts
- Use UsdCents for 8-decimal fixed-point USD prices
- Newtype wrappers prevent precision confusion
- NonZero types guarantee positive amounts
- Phantom types prevent cross-asset calculations

## Common Test Patterns

### Protocol V2 Type Safety Validation
```rust
use protocol_v2::{TLVMessage, TLVMessageBuilder, RelayDomain, MarketDataDomain};

#[test]
fn validate_typed_message_construction() {
    // Type system prevents domain confusion
    let builder = TLVMessageBuilder::<MarketDataDomain>::new()
        .with_sequence(12345)
        .with_source(SourceType::CoinbaseCollector);
    
    // Compiler ensures only valid TLV types for domain
    let message: TLVMessage<MarketDataDomain> = builder
        .add_trade_tlv(trade_data)  // Only valid for MarketDataDomain
        .build();

    // Type safety guarantees these operations are valid
    let header = message.header(); // Cannot fail
    assert_eq!(header.relay_domain, RelayDomain::MarketData as u8);
    
    // Would be compile error: message.add_execution_tlv(exec_data)
}
```

### Type-Safe Arbitrage Validation  
```rust
#[test]
fn validate_typed_arbitrage_calculation() {
    // Load real pool states - NO MOCKS!
    let real_pool_data = load_real_uniswap_pools();
    
    for pool_pair in real_pool_data.pairs() {
        let pool_a_state = PoolState::from_real_data(pool_pair.pool_a);
        let pool_b_state = PoolState::from_real_data(pool_pair.pool_b);
        
        // Type safety: cannot mix up pools or parameters
        if let Some(opportunity) = ArbitrageOpportunity::detect_between_pools(
            pool_a_state,
            pool_b_state,
            WethAmount::from_ether(1)
        ) {
            // Type guarantees: if Some(opp), then profit > 0
            assert!(opportunity.profit_after_gas.get() > 0);  // Always true
            
            // Type guarantees: cannot calculate with wrong pools
            let recalculated = opportunity.recalculate_with_current_gas();
            assert!(recalculated.is_some() || "Gas price changed too much");
        }
    }
}
```

### Typed Relay Communication Validation
```rust
#[tokio::test]
async fn validate_typed_relay_routing() {
    let relay = TypedRelay::<MarketDataDomain>::start().await?;
    let consumer = TypedConsumer::<MarketDataDomain>::connect(&relay).await?;

    // Type system ensures message/domain compatibility
    let trade_message = TLVMessage::<MarketDataDomain>::new_trade(trade_tlv);
    relay.send_typed_message(trade_message).await?;

    let received = consumer.receive_typed_message().await?;
    
    // Type safety: compiler guarantees message is MarketDataDomain
    match received.parse_content() {
        MessageContent::Trade(trade) => {
            assert_eq!(trade.instrument_id, original_trade.instrument_id);
        },
        MessageContent::Signal(_) => {
            panic!("Wrong domain - compiler should prevent this!");
        }
    }
    
    // Would be compile error: relay.send_typed_message(execution_message)
}
```

## CDD Organization
```
[module]/
├── src/
│   ├── lib.rs          # Type-safe APIs
│   └── types.rs        # Domain types with invariants
├── benches/
│   ├── performance/    # Zero-cost abstraction validation
│   └── real_data/      # Performance with live exchange data
├── tests/
│   ├── validation/     # Real data validation tests
│   └── e2e/           # Complete pipeline validation
└── fuzz/
    └── targets/       # Type safety under malformed input
```

## CI/CD Integration
- All compiler checks run on every commit (`cargo check` + `cargo clippy`)
- Performance benchmarks detect regressions (>1M ops/s targets)
- Type safety validation (zero unsafe blocks in critical paths)
- Fuzz testing validates parser safety
- Real data E2E validation on release branches

## Review Checklist
Before marking task as complete:
- [ ] Domain types designed to prevent invalid states
- [ ] All compiler checks pass with zero warnings
- [ ] Performance benchmarks meet >1M ops/s targets
- [ ] Real exchange data validation passes
- [ ] Type safety patterns followed (NonZero, phantom types, etc.)
- [ ] Zero-cost abstractions verified (performance = primitives)
- [ ] Documentation includes type-driven examples
- [ ] Fuzz targets validate parser safety

---

**Remember**: The compiler is your primary QA engineer. Types that prevent the hardcoded $150 issue are worth 1000 runtime tests.
