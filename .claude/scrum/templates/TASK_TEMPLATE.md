---
task_id: TASK-XXX
status: TODO  ← CHANGE TO "IN_PROGRESS" WHEN STARTING, THEN "COMPLETE" WHEN FINISHED!
priority: CRITICAL
estimated_hours: 3
assigned_branch: fix/specific-issue-name
assignee: TBD
created: YYYY-MM-DD
completed: null
# Dependencies: task IDs that must be COMPLETE before this can start
depends_on: []
  # Example: [S010-T002, S007-T001]
# Blocks: task IDs that cannot start until this is COMPLETE
blocks: []
  # Example: [S014-T001, S014-T002]
# Scope: files/directories this task modifies (for conflict detection)
scope: []
  # Example: ["relays/src/common/*.rs", "libs/types/src/protocol/"]
---

# TASK-XXX: [Clear Task Description]

**🚨 CRITICAL**: Update status to COMPLETE when finished!

## 🔴 CRITICAL INSTRUCTIONS

### 0. 📋 MARK AS IN-PROGRESS IMMEDIATELY
**⚠️ FIRST ACTION: Change status when you start work!**
```yaml
# Edit the YAML frontmatter above:
status: TODO → status: IN_PROGRESS

# This makes the kanban board show you're working on it!
```

### 1. Git Worktree Setup (REQUIRED)
```bash
# NEVER use git checkout - it changes all sessions!
# ALWAYS use git worktree for isolated development:
git worktree add -b fix/specific-issue-name ../task-xxx-worktree
cd ../task-xxx-worktree

# Verify you're in the correct worktree:
git branch --show-current  # Should show: fix/specific-issue-name
pwd  # Should show: ../task-xxx-worktree

# NEVER work directly in main repository!
```

### 2. 🦀 COMPILER-DRIVEN DEVELOPMENT MANDATORY
**⚠️ AGENTS: You MUST use types to encode business invariants BEFORE implementation!**
```bash
# WORKFLOW: TYPES → IMPLEMENT → BENCHMARK
# 1. Design types that make invalid states unrepresentable
# 2. Implement using zero-cost abstractions
# 3. Benchmark with real data to validate performance
# 4. Repeat for next feature

# DO NOT write implementation without type safety first!
```

## Status
**Status**: TODO (⚠️ CHANGE TO IN_PROGRESS WHEN YOU START!)
**Priority**: CRITICAL
**Worktree**: `../task-xxx-worktree` (Branch: `fix/specific-issue-name`)
**Estimated**: 3 hours

## Problem Statement
[Clear description of what problem this solves or what feature this adds]

## Acceptance Criteria
- [ ] [Specific measurable outcome]
- [ ] [Another specific outcome]
- [ ] Types designed to prevent invalid states
- [ ] All compiler checks pass (cargo check + clippy)
- [ ] Performance benchmarks pass (>1M msg/s for critical paths)
- [ ] Real exchange data validation passes
- [ ] Zero unsafe code blocks added

## Technical Approach
### Files to Modify
- `path/to/file1.rs` - [What changes]
- `path/to/file2.rs` - [What changes]

### Implementation Steps - CDD REQUIRED ⚠️
**🚨 CRITICAL: Follow Compiler-Driven Development - Design Types FIRST!**

1. **TYPES**: Design domain types that encode business invariants
   - Use newtypes to prevent primitive confusion
   - Use phantom types for compile-time domain separation
   - Use NonZero* types to encode positive value requirements
   - Use Result types to handle all error cases
2. **IMPLEMENT**: Use zero-cost abstractions for performance
   - Leverage zerocopy traits for parsing
   - Use const generics for compile-time configuration
   - Implement using typed APIs only
3. **BENCHMARK**: Validate performance with real exchange data
   - Measure throughput with criterion benchmarks
   - Validate memory usage patterns
   - Test with sustained load scenarios
4. **REPEAT**: Extend types and implementation iteratively

**Example CDD Workflow:**
```bash
# Step 1: Design types first
echo "Design types in src/types.rs that prevent invalid states"
cargo check  # Should PASS (types compile)

# Step 2: Implement using types
echo "Implement logic using only typed APIs"
cargo clippy  # Should PASS (no warnings)

# Step 3: Benchmark with real data
echo "Validate performance with real exchange data"
cargo bench  # Should meet >1M msg/s targets
```

## CDD Requirements - Rust Type Safety

### 🏗️ Type Safety AND Performance Validation Required

Following Compiler-Driven Development practices, implement **both** validation types:

#### 1. Type Safety Validation (REQUIRED) - Compile-time Correctness
**Location**: In type definitions and API design
**Access**: Compiler enforces all invariants
**Purpose**: Make invalid states unrepresentable, prevent entire error classes

```rust
// Add to: src/[module].rs - Design types that prevent errors

// 1. Use newtypes to prevent confusion
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct ValidatedSize(NonZeroU32); // Cannot be zero!

#[derive(Debug, Clone, Copy)]
pub struct UsdPrice(u64); // Always 8-decimal fixed-point

// 2. Make invalid states unrepresentable
pub struct ValidatedData {
    size: ValidatedSize,      // Compiler guarantees > 0
    price: UsdPrice,          // Compiler guarantees proper format
    timestamp: SystemTime,    // Cannot be in past (if needed)
}

impl ValidatedData {
    // Constructor enforces all invariants
    pub fn new(size: u32, price_cents: u64) -> Result<Self, ValidationError> {
        let size = ValidatedSize(NonZeroU32::new(size)
            .ok_or(ValidationError::ZeroSize)?);
        let price = UsdPrice(price_cents);
        
        Ok(Self {
            size,
            price,
            timestamp: SystemTime::now(),
        })
    }
    
    // All operations are safe - types guarantee validity
    pub fn calculate_total(&self) -> UsdPrice {
        UsdPrice(self.price.0.saturating_mul(self.size.0.get() as u64))
    }
}

// 3. API cannot be misused - types prevent all error cases
pub fn process_validated_data(data: ValidatedData) -> ProcessedData {
    // No need for runtime validation - types guarantee correctness!
    ProcessedData {
        total: data.calculate_total(), // Cannot overflow or be negative
        size: data.size.0.get(),       // Cannot be zero
        processed_at: SystemTime::now(),
    }
}

// 4. Performance validation with criterion (not unit tests)
#[cfg(test)]
mod validation {
    use super::*;
    
    #[test]
    fn validate_precision_with_real_data() {
        // Use real exchange data - NO MOCKS!
        let real_price_data = load_real_coinbase_prices();
        
        for price_cents in real_price_data {
            let data = ValidatedData::new(1, price_cents).unwrap();
            let processed = process_validated_data(data);
            
            // Type safety guarantees these properties
            assert_eq!(processed.total.0, price_cents); // Guaranteed by types
        }
    }
}
```

#### 2. Performance Benchmarks (REQUIRED for all components) - Zero-Cost Validation
**Location**: In `benches/` directory (criterion benchmarks)
**Access**: Real exchange data and production scenarios
**Purpose**: Validate zero-cost abstractions maintain >1M msg/s performance

```rust
// Create: benches/performance_[feature].rs
use criterion::{criterion_group, criterion_main, Criterion};
use my_crate::{ValidatedData, process_validated_data};

fn bench_typed_processing_performance(c: &mut Criterion) {
    // Use real exchange data - NO MOCKS!
    let real_market_data = load_real_coinbase_trades();
    
    c.bench_function("process_validated_data", |b| {
        b.iter(|| {
            for trade_data in &real_market_data {
                let validated = ValidatedData::new(
                    trade_data.quantity,
                    trade_data.price_cents
                ).unwrap();
                
                let processed = process_validated_data(validated);
                criterion::black_box(processed);
            }
        });
    });
    
    // Validate performance target
    let samples = c.measure_performance(1000);
    assert!(samples.mean_throughput > 1_000_000.0); // >1M ops/sec
}

fn bench_zero_cost_abstractions(c: &mut Criterion) {
    let mut group = c.benchmark_group("zero_cost_validation");
    
    // Compare typed vs untyped performance
    group.bench_function("typed_approach", |b| {
        b.iter(|| {
            let data = ValidatedData::new(100, 4500_00000000).unwrap();
            criterion::black_box(process_validated_data(data));
        });
    });
    
    group.bench_function("primitive_approach", |b| {
        b.iter(|| {
            let result = (100u32 as u64) * 4500_00000000u64;
            criterion::black_box(result);
        });
    });
    
    // Types should have zero cost - performance must be identical
    group.finish();
}

criterion_group!(benches, bench_typed_processing_performance, bench_zero_cost_abstractions);
criterion_main!(benches);
```

#### 3. Real Data Validation (if critical system paths)
```rust
// Add to: tests/e2e/[feature]_validation.rs
#[tokio::test]
async fn validate_[feature]_with_real_data() {
    // Connect to real exchanges - NO MOCKS!
    let system = TypedTradingSystem::start_validation_mode().await;
    
    // Use typed configuration prevents misconfiguration
    let config = FeatureConfig {
        min_threshold: ValidatedSize(NonZeroU32::new(100).unwrap()),
        timeout: Duration::from_secs(30),
    };
    
    let result = system.execute_feature_typed(config).await;
    
    // Type safety guarantees result validity
    match result {
        Ok(output) => assert!(output.is_valid_by_construction()),
        Err(SystemError::NoDataAvailable) => {
            // Acceptable - market conditions vary
        },
        Err(other) => panic!("Unexpected error: {:?}", other),
    }
}
```

### CDD Validation Hierarchy Summary

| Validation Type | Location | Access | Purpose | Example |
|-----------------|----------|--------|---------|---------|
| **Type Safety** | `src/types.rs` | Compile-time | Prevent invalid states | `NonZeroU64` prevents negative profit |
| **Performance Benchmarks** | `benches/performance.rs` | Real data | Validate zero-cost abstractions | `>1M msg/s with types` |
| **Real Data Validation** | `tests/e2e/` | Live exchanges | Validate production scenarios | Coinbase → Polygon arbitrage detection |

## CDD Validation Execution
```bash
# Primary quality gate: compiler validation
cargo check --package package_name           # Type safety
cargo clippy --package package_name          # Advanced lints

# Performance validation
cargo bench --package package_name           # Benchmark performance
cargo bench -- --baseline previous_version   # Check regressions

# Real data validation
cargo test --package package_name --release  # With real exchanges
ENABLE_REAL_DATA_TESTS=true cargo test --test validation_[feature]

# Verify zero-cost abstractions
cargo build --release && objdump -d target/release/[binary] | grep -A10 "hot_path_function"
```

## Git Workflow
```bash
# 1. Create worktree (already done in step 1)
git worktree add -b fix/specific-issue-name ../task-xxx-worktree
cd ../task-xxx-worktree

# 2. Make changes and commit
git add -A
git commit -m "fix: clear description of change"

# 3. Push to origin
git push origin fix/specific-issue-name

# 4. Create PR
gh pr create --title "Fix: Clear description" --body "Closes TASK-XXX"

# 5. Clean up worktree after PR merge
cd ../backend_v2  # Return to main repository
git worktree remove ../task-xxx-worktree
git branch -D fix/specific-issue-name  # Delete local branch if desired
```

## ✅ Before Marking Complete
- [ ] All acceptance criteria met
- [ ] Code committed in worktree
- [ ] All compiler checks pass (cargo check + clippy)
- [ ] Performance benchmarks pass (>1M msg/s targets)
- [ ] **UPDATE: Change `status: TODO` to `status: COMPLETE` in YAML frontmatter above**
- [ ] Run: `../../../scrum/task-manager.sh sprint-XXX` to verify status

## Completion Checklist
- [ ] **🚨 STEP 0: Changed status to IN_PROGRESS when starting** ← AGENTS MUST DO THIS!
- [ ] Working in correct worktree (not main repository)
- [ ] **🚨 CDD FOLLOWED: Types designed BEFORE implementation**
- [ ] All compiler checks pass (check + clippy + build --release)
- [ ] Performance benchmarks meet targets
- [ ] All acceptance criteria met
- [ ] Code reviewed locally
- [ ] Zero-cost abstractions verified
- [ ] PR created
- [ ] **🚨 STEP FINAL: Updated task status to COMPLETE** ← AGENTS MUST DO THIS!

## 📋 Sprint Task Workflow
1. Pick task from TODO status
2. **IMMEDIATELY**: Change status: TODO → IN_PROGRESS
3. Do the work
4. **BEFORE COMMITTING**: Change status: IN_PROGRESS → COMPLETE
5. Verify with: `task-manager.sh sprint-XXX`

## ⚠️ IMPORTANT: Status Updates Required
**When you START this task, you MUST:**
1. **IMMEDIATELY** change `status: TODO` to `status: IN_PROGRESS` in the YAML frontmatter above
2. This makes the kanban board show you're working on it

**When you FINISH this task, you MUST:**
1. Change `status: IN_PROGRESS` to `status: COMPLETE` in the YAML frontmatter above
2. This is NOT optional - the task-manager.sh depends on accurate status
3. If you forget, the task will show as incomplete forever
4. Update immediately after PR is merged, not before

**Status Flow: TODO → IN_PROGRESS → COMPLETE**

## Task Completion Protocol
- [ ] Technical work completed
- [ ] Code committed in worktree
- [ ] All compiler checks passing (primary quality gate)
- [ ] Performance benchmarks meeting targets
- [ ] **CRITICAL**: Update YAML status to COMPLETE
- [ ] Verify status with task manager script

## Notes
[Space for implementation notes, blockers, or discoveries]
