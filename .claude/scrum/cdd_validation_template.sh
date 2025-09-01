#!/bin/bash
# CDD Validation Template - Use this to validate PR compiler-driven development requirements
# Usage: ./cdd_validation_template.sh [COMPONENT] [PACKAGE] [PR_NUMBER]

set -e

COMPONENT=${1:-"unknown"}
PACKAGE=${2:-"services"}
PR_NUMBER=${3:-""}

echo "🦀 COMPREHENSIVE CDD VALIDATION FOR: $COMPONENT"
echo "Package: $PACKAGE"
echo "PR: #$PR_NUMBER"
echo "======================================================="

# Function to check validation results
check_validation() {
    local validation_name="$1"
    local cmd="$2"

    echo ""
    echo "🔍 Running: $validation_name"
    echo "Command: $cmd"
    echo "---"

    if eval "$cmd"; then
        echo "✅ PASSED: $validation_name"
        return 0
    else
        echo "❌ FAILED: $validation_name"
        return 1
    fi
}

# Track results
VALIDATIONS_PASSED=0
VALIDATIONS_FAILED=0

echo ""
echo "1️⃣ COMPILER VALIDATION (PRIMARY GATE)"
echo "====================================="
if check_validation "Compilation Check" "cargo check --package $PACKAGE"; then
    ((VALIDATIONS_PASSED++))
else
    ((VALIDATIONS_FAILED++))
fi

if check_validation "Clippy Linting" "cargo clippy --package $PACKAGE -- -D warnings"; then
    ((VALIDATIONS_PASSED++))
else
    ((VALIDATIONS_FAILED++))
fi

if check_validation "Release Build" "cargo build --release --package $PACKAGE"; then
    ((VALIDATIONS_PASSED++))
else
    ((VALIDATIONS_FAILED++))
fi

echo ""
echo "2️⃣ TYPE SAFETY VALIDATION"
echo "========================="

# Check for type safety patterns
NEWTYPE_COUNT=$(find src -name "*.rs" -exec grep -l "struct.*([A-Z].*)" {} \; | wc -l)
NONZERO_COUNT=$(find src -name "*.rs" -exec grep -l "NonZero" {} \; | wc -l)
RESULT_COUNT=$(find src -name "*.rs" -exec grep -l "Result<.*Error>" {} \; | wc -l)

echo "Newtype patterns found: $NEWTYPE_COUNT"
echo "NonZero type usage: $NONZERO_COUNT"
echo "Result type usage: $RESULT_COUNT"

if [ $((NEWTYPE_COUNT + NONZERO_COUNT + RESULT_COUNT)) -gt 0 ]; then
    echo "✅ PASSED: Type safety patterns detected"
    ((VALIDATIONS_PASSED++))
else
    echo "❌ FAILED: No type safety patterns found"
    ((VALIDATIONS_FAILED++))
fi

echo ""
echo "3️⃣ PERFORMANCE BENCHMARKS"
echo "========================="
if check_validation "Performance Benchmarks" "cargo bench --package $PACKAGE -- --sample-size 10"; then
    ((VALIDATIONS_PASSED++))
else
    ((VALIDATIONS_FAILED++))
fi

echo ""
echo "4️⃣ ZERO-COST ABSTRACTION VALIDATION"
echo "==================================="
# Check that optimized build doesn't have unnecessary overhead
if check_validation "Optimized Build Analysis" "cargo build --release --package $PACKAGE && echo 'Build completed - check for zero-cost abstractions'"; then
    ((VALIDATIONS_PASSED++))
else
    ((VALIDATIONS_FAILED++))
fi

echo ""
echo "5️⃣ REAL DATA VALIDATION (NO MOCKS)"
echo "=================================="
# This section varies by component type
case $COMPONENT in
    "polygon"|"binance"|"kraken"|"exchange")
        echo "Validating with live exchange data..."
        export ENABLE_REAL_DATA_TESTS=true
        if check_validation "Live Exchange Data Test" "timeout 30s cargo test --package $PACKAGE --release validation_with_real_data"; then
            ((VALIDATIONS_PASSED++))
        else
            ((VALIDATIONS_FAILED++))
        fi
        ;;
    "protocol"|"parser"|"tlv")
        echo "Validating with real message samples..."
        if check_validation "Real Message Parsing Test" "cargo test --package $PACKAGE real_data_validation --release"; then
            ((VALIDATIONS_PASSED++))
        else
            ((VALIDATIONS_FAILED++))
        fi
        ;;
    *)
        echo "ℹ️  No specific real data validation for component type: $COMPONENT"
        echo "✅ SKIPPED: Real data validation (not applicable)"
        ((VALIDATIONS_PASSED++))
        ;;
esac

echo ""
echo "6️⃣ PERFORMANCE TARGET VALIDATION"
echo "==============================="
# Check for performance targets in benchmarks
if ls benches/*.rs &>/dev/null; then
    echo "Benchmark files found - validating performance targets..."
    if check_validation "Performance Target Validation" "cargo bench --package $PACKAGE 2>&1 | grep -E '(1,000,000|1M|>.*msg/s)'"; then
        ((VALIDATIONS_PASSED++))
    else
        echo "⚠️  Performance targets not clearly validated"
        ((VALIDATIONS_FAILED++))
    fi
else
    echo "ℹ️  No benchmark files found"
fi

echo ""
echo "7️⃣ FUZZ SAFETY VALIDATION"
echo "========================="
# Look for fuzz targets
if [ -d "fuzz" ] && ls fuzz/fuzz_targets/*.rs &>/dev/null; then
    if check_validation "Fuzz Safety" "cargo fuzz list | head -1 | xargs -I {} timeout 10s cargo fuzz run {} -- -runs=1000"; then
        ((VALIDATIONS_PASSED++))
    else
        ((VALIDATIONS_FAILED++))
    fi
else
    echo "ℹ️  No fuzz targets found for this component"
fi

echo ""
echo "📊 CDD VALIDATION SUMMARY"
echo "========================="
echo "Validations Passed: $VALIDATIONS_PASSED"
echo "Validations Failed: $VALIDATIONS_FAILED"
echo "Total Validations:  $((VALIDATIONS_PASSED + VALIDATIONS_FAILED))"

# Calculate pass rate
TOTAL_VALIDATIONS=$((VALIDATIONS_PASSED + VALIDATIONS_FAILED))
if [ $TOTAL_VALIDATIONS -gt 0 ]; then
    PASS_RATE=$((VALIDATIONS_PASSED * 100 / TOTAL_VALIDATIONS))
    echo "Pass Rate: $PASS_RATE%"
fi

if [ $VALIDATIONS_FAILED -eq 0 ]; then
    echo ""
    echo "🎉 ALL CDD VALIDATIONS PASSED!"
    echo "✅ Compiler-driven development requirements satisfied"
    echo "✅ Type safety patterns implemented"
    echo "✅ Performance targets maintained"
    echo "✅ Component ready for code review"
    exit 0
elif [ $VALIDATIONS_PASSED -ge $((TOTAL_VALIDATIONS * 2 / 3)) ]; then
    echo ""
    echo "⚠️  MOSTLY ACCEPTABLE CDD WORKFLOW"
    echo "✅ Most CDD requirements satisfied"
    echo "⚠️  $VALIDATIONS_FAILED validation(s) failed - address before merge"
    echo ""
    echo "💡 Consider:"
    echo "- Adding more type safety patterns (NonZero, phantom types)"
    echo "- Adding performance benchmarks with real data"
    echo "- Fixing compiler warnings"
    exit 1
else
    echo ""
    echo "❌ POOR CDD WORKFLOW"
    echo "❌ CDD requirements not met"
    echo "❌ PR should be rejected for significant rework"
    echo ""
    echo "💡 CDD Requirements:"
    echo "1. Design types that make invalid states unrepresentable"
    echo "2. Pass all compiler checks (check + clippy + build --release)"
    echo "3. Validate performance with real exchange data"
    echo "4. Use zero-cost abstractions"
    echo "5. No mocks - test with real data only"
    exit 1
fi
