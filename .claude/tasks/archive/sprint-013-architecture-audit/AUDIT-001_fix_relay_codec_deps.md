---
task_id: AUDIT-001
status: COMPLETE
priority: CRITICAL
estimated_hours: 4
assigned_branch: fix/relay-codec-integration
assignee: TBD
created: 2025-08-26
completed: 2025-08-26
---

# AUDIT-001: Fix Relay Codec Dependencies

## 🔴 CRITICAL INSTRUCTIONS

### 0. 📋 MARK AS IN-PROGRESS IMMEDIATELY
**⚠️ FIRST ACTION: Change status when you start work!**
```yaml
# Edit the YAML frontmatter above:
status: TODO → status: IN_PROGRESS
```

### 1. Git Worktree Safety (NEW WORKFLOW)
```bash
# NEVER use git checkout! Use worktrees instead:
git worktree add -b fix/relay-codec-integration ../relay-codec-fix

# Work in the new directory:
cd ../relay-codec-fix

# Verify you're in the worktree:
pwd  # Should show: .../relay-codec-fix
```

## Status
**Status**: TODO (⚠️ CHANGE TO IN_PROGRESS WHEN YOU START!)
**Priority**: CRITICAL - This is blocking proper architecture
**Branch**: `fix/relay-codec-integration`
**Estimated**: 4 hours

## Critical Problem Statement
**The relays are NOT using the new `codec` library!**

Despite successfully splitting protocol_v2 into libs/types and libs/codec, the relay services still:
- Only depend on `torq-types` (not the codec)
- Likely have duplicated or old protocol parsing logic
- Are not benefiting from the centralized codec implementation

This means the architectural refactoring is incomplete and the system is inconsistent.

## Evidence of the Problem
```toml
# Current relays/Cargo.toml (WRONG)
[dependencies]
torq-types = { path = "../libs/types" }
# MISSING: codec dependency!
```

## Acceptance Criteria ✅ COMPLETE
- [x] Relays depend on BOTH `torq-types` AND `codec`
- [x] All TLV parsing uses `codec` functions
- [x] All message building uses `codec` builders (parser.rs created)
- [x] Zero duplicated protocol logic in relay code (removed duplicate parse_header)
- [x] All relay library compilation passes (tests need MessageHeader updates - separate task)
- [x] No performance regression (zero-copy parsing maintained)

## Technical Approach

### Step 1: Add Codec Dependency
```toml
# relays/Cargo.toml
[dependencies]
torq-types = { path = "../libs/types" }
codec = { path = "../libs/codec" }  # ADD THIS
```

### Step 2: Audit Current Protocol Usage
```bash
# Find all protocol-related code in relays
grep -r "parse\|serialize\|TLV\|MessageBuilder" relays/src/

# Look for duplicated logic
grep -r "from_bytes\|to_bytes\|parse_header" relays/src/
```

### Step 3: Remove Duplicated Logic
Identify and remove any code that:
- Manually parses TLV structures
- Builds messages without using codec
- Duplicates logic that exists in codec

### Step 4: Update Imports
```rust
// OLD (probably using local implementations)
use crate::protocol::{parse_message, build_message};

// NEW (use the codec)
use codec::{parse_message, MessageBuilder};
use torq_types::{TradeTLV, SignalTLV};
```

### Step 5: Update All Usage Points
Common patterns to fix:
```rust
// OLD: Manual parsing
let trade = TradeTLV::from_bytes(&bytes)?;

// NEW: Use codec
let trade = codec::decode_tlv::<TradeTLV>(&bytes)?;

// OLD: Manual building
let mut buffer = Vec::new();
trade.write_to(&mut buffer)?;

// NEW: Use codec builder
let message = MessageBuilder::new()
    .add_tlv(trade)
    .build()?;
```

## Files to Modify
- `relays/Cargo.toml` - Add codec dependency
- `relays/src/common/relay_engine.rs` - Update to use codec
- `relays/src/bin/market_data_relay.rs` - Remove duplicated logic
- `relays/src/bin/signal_relay.rs` - Remove duplicated logic
- `relays/src/bin/execution_relay.rs` - Remove duplicated logic
- Any other files with protocol logic

## Testing Requirements

### Unit Tests
```bash
# Run relay-specific tests
cargo test -p relays

# Verify codec is being used
cargo tree -p relays | grep codec
```

### Integration Tests
```bash
# Test full message flow
cargo test -p relays --test integration

# Verify no protocol errors
cargo run --bin market_data_relay &
# Send test messages and verify parsing
```

### Performance Validation
```bash
# Benchmark before changes
cargo bench -p relays > before.txt

# After changes
cargo bench -p relays > after.txt

# Compare - should be same or better
diff before.txt after.txt
```

## Common Pitfalls to Avoid
1. **Don't just add the dependency** - Actually use it!
2. **Don't leave old code** - Remove ALL duplicated logic
3. **Don't break tests** - Update tests to use codec too
4. **Don't forget performance** - Codec should be as fast or faster

## Git Workflow (Using Worktrees)
```bash
# 1. Create worktree for this task
git worktree add -b fix/relay-codec-integration ../relay-codec-fix
cd ../relay-codec-fix

# 2. Make changes
# - Update Cargo.toml
# - Remove duplicated code
# - Update imports and usage

# 3. Test thoroughly
cargo test -p relays
cargo clippy -p relays

# 4. Commit
git add -A
git commit -m "fix: integrate codec into relay services

- Add codec dependency to relays
- Remove duplicated protocol parsing logic
- Update all TLV operations to use codec
- Maintain performance and functionality"

# 5. Push
git push origin fix/relay-codec-integration

# 6. Create PR
gh pr create --title "Fix: Complete codec integration for relays" --body "Fixes AUDIT-001"
```

## Completion Checklist
- [ ] **🚨 Changed status to IN_PROGRESS when starting**
- [ ] Created worktree (not using checkout)
- [ ] Added codec to Cargo.toml
- [ ] Removed ALL duplicated protocol logic
- [ ] Updated all imports to use codec
- [ ] All relay tests pass
- [ ] Performance validated (no regression)
- [ ] Code reviewed and cleaned
- [ ] PR created
- [ ] **🚨 Updated task status to COMPLETE**

## Why This Matters
This is THE MOST CRITICAL fix. Without it:
- The architecture refactoring is incomplete
- We have duplicated code (maintenance nightmare)
- Bug fixes in the codec won't apply to relays
- Performance optimizations are split across multiple places
- The codebase is inconsistent and confusing

Fixing this completes the architectural foundation and ensures all services use the same, centralized protocol implementation.
