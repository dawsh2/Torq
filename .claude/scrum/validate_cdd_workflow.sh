#!/bin/bash
# CDD Workflow Validation Script
# Usage: ./validate_cdd_workflow.sh [BRANCH_NAME] [PR_NUMBER]

set -e

BRANCH=${1:-"HEAD"}
PR_NUMBER=${2:-"unknown"}

echo "🦀 COMPILER-DRIVEN DEVELOPMENT VALIDATION"
echo "Branch: $BRANCH"
echo "PR: #$PR_NUMBER"
echo "============================================"

# Get commit history for the branch (excluding main)
echo ""
echo "📊 Commit History Analysis"
echo "--------------------------"
git log --oneline $BRANCH --not main

echo ""
echo "🔍 CDD Workflow Verification"
echo "-----------------------------"

# Check for CDD commit pattern
COMMITS=($(git log --format="%H" $BRANCH --not main))
COMMIT_COUNT=${#COMMITS[@]}

echo "Total commits: $COMMIT_COUNT"

if [ $COMMIT_COUNT -lt 2 ]; then
    echo "❌ INSUFFICIENT COMMITS: CDD requires at least 2 commits (types → implementation)"
    exit 1
fi

# Reverse array to get chronological order
for ((i=$COMMIT_COUNT-1; i>=0; i--)); do
    COMMIT=${COMMITS[i]}
    MESSAGE=$(git log --format="%s" -n 1 $COMMIT)
    FILES_CHANGED=$(git diff-tree --no-commit-id --name-only -r $COMMIT)

    echo ""
    echo "Commit $((COMMIT_COUNT-i)): $MESSAGE"
    echo "Files: $FILES_CHANGED"

    # Analyze commit for CDD patterns
    if [[ $MESSAGE =~ ^type.*|.*types.*|.*type-safe ]]; then
        echo "✅ TYPE DESIGN: Type safety implementation detected"
    elif [[ $MESSAGE =~ ^feat.*|^implement ]]; then
        echo "✅ IMPLEMENTATION: Feature implementation detected"
    elif [[ $MESSAGE =~ ^perf.*|^bench ]]; then
        echo "✅ PERFORMANCE: Benchmark validation detected"
    elif [[ $MESSAGE =~ ^refactor ]]; then
        echo "✅ REFACTOR: Code improvement while maintaining type safety"
    elif [[ $MESSAGE =~ ^fix ]]; then
        echo "⚠️  FIX: Check if fix maintains type safety guarantees"
    else
        echo "❓ UNCLEAR: Commit message doesn't indicate CDD pattern"
    fi
done

echo ""
echo "🦀 Type Safety & Performance Analysis"
echo "-------------------------------------"

# Check for type definition files
TYPE_FILES_ADDED=$(git diff --name-status main..$BRANCH | grep -E '^A.*(types?|domain)\.rs$' | wc -l)
TYPE_FILES_MODIFIED=$(git diff --name-status main..$BRANCH | grep -E '^M.*(types?|domain)\.rs$' | wc -l)
IMPL_FILES_MODIFIED=$(git diff --name-status main..$BRANCH | grep -E '^[AM].*\.rs$' | grep -v -E "(test|bench)" | wc -l)
BENCH_FILES_ADDED=$(git diff --name-status main..$BRANCH | grep -E '^A.*bench.*\.rs$' | wc -l)
BENCH_FILES_MODIFIED=$(git diff --name-status main..$BRANCH | grep -E '^M.*bench.*\.rs$' | wc -l)

echo "Type definition files added: $TYPE_FILES_ADDED"
echo "Type definition files modified: $TYPE_FILES_MODIFIED"
echo "Implementation files: $IMPL_FILES_MODIFIED"
echo "Benchmark files added: $BENCH_FILES_ADDED"
echo "Benchmark files modified: $BENCH_FILES_MODIFIED"

# For CDD, implementation files are more important than test files
if [ $IMPL_FILES_MODIFIED -eq 0 ]; then
    echo "❌ NO IMPLEMENTATION: Changes appear to be documentation-only"
    exit 1
fi

# Check for type safety patterns in code
TYPE_SAFETY_INDICATORS=0
if git diff main..$BRANCH | grep -q "NonZero"; then
    echo "✅ NonZero types detected - prevents zero/negative values"
    ((TYPE_SAFETY_INDICATORS++))
fi

if git diff main..$BRANCH | grep -q "PhantomData"; then
    echo "✅ Phantom types detected - compile-time domain separation"
    ((TYPE_SAFETY_INDICATORS++))
fi

if git diff main..$BRANCH | grep -q "Result<.*Error>"; then
    echo "✅ Result types detected - explicit error handling"
    ((TYPE_SAFETY_INDICATORS++))
fi

echo ""
echo "🎯 CDD Best Practices Check"
echo "----------------------------"

# Check first commit for type definitions
FIRST_COMMIT=${COMMITS[$COMMIT_COUNT-1]}
if git show $FIRST_COMMIT --name-only | grep -E "(types?|domain)\.rs"; then
    echo "✅ GOOD: First commit includes type definitions"
elif git show $FIRST_COMMIT | grep -E "(struct|enum|type|impl)" | head -1 | grep -q .; then
    echo "✅ GOOD: First commit includes type definitions inline"
else
    echo "⚠️  CONSIDER: First commit should focus on type design"
fi

# Check for proper commit message patterns
TYPE_COMMITS=$(git log --format="%s" $BRANCH --not main | grep -cE "type|Type|types|Types" || true)
IMPL_COMMITS=$(git log --format="%s" $BRANCH --not main | grep -cE "feat|implement|add" || true)
BENCH_COMMITS=$(git log --format="%s" $BRANCH --not main | grep -cE "perf|bench|performance" || true)

echo ""
echo "CDD Phase Distribution:"
echo "- Type design commits: $TYPE_COMMITS"
echo "- Implementation commits: $IMPL_COMMITS"
echo "- Performance validation commits: $BENCH_COMMITS"

# Run compiler checks
echo ""
echo "🚀 Compiler Validation"
echo "----------------------"

# Check if code compiles
if cargo check --workspace --quiet 2>/dev/null; then
    echo "✅ Code compiles successfully"
    COMPILER_SCORE=1
else
    echo "❌ Code does not compile"
    COMPILER_SCORE=0
fi

# Check clippy warnings
CLIPPY_WARNINGS=$(cargo clippy --workspace --quiet 2>&1 | grep -c "warning:" || true)
if [ $CLIPPY_WARNINGS -eq 0 ]; then
    echo "✅ No clippy warnings"
    ((COMPILER_SCORE++))
else
    echo "⚠️  $CLIPPY_WARNINGS clippy warnings found"
fi

# Check for unsafe blocks in critical files
UNSAFE_COUNT=$(git diff main..$BRANCH | grep -c "unsafe" || true)
if [ $UNSAFE_COUNT -eq 0 ]; then
    echo "✅ No unsafe blocks added"
    ((COMPILER_SCORE++))
else
    echo "⚠️  $UNSAFE_COUNT unsafe blocks added - review carefully"
fi

# Validate CDD workflow
CDD_SCORE=0

if [ $TYPE_SAFETY_INDICATORS -gt 0 ]; then
    echo "✅ Type safety patterns detected"
    ((CDD_SCORE++))
else
    echo "❌ No type safety patterns found"
fi

if [ $COMPILER_SCORE -ge 2 ]; then
    echo "✅ Compiler validation passes"
    ((CDD_SCORE++))
else
    echo "❌ Compiler validation fails"
fi

if [ $((TYPE_FILES_ADDED + TYPE_FILES_MODIFIED)) -gt 0 ] || [ $TYPE_COMMITS -gt 0 ]; then
    echo "✅ Type-driven development evidence"
    ((CDD_SCORE++))
else
    echo "⚠️  Limited type-driven development evidence"
fi

echo ""
echo "📊 CDD WORKFLOW SCORE: $CDD_SCORE/3"

if [ $CDD_SCORE -eq 3 ]; then
    echo ""
    echo "🎉 EXCELLENT CDD WORKFLOW!"
    echo "✅ All CDD requirements satisfied"
    echo "✅ Type safety patterns implemented"
    echo "✅ Compiler validation passes"
    echo "✅ Ready for code review"
    exit 0
elif [ $CDD_SCORE -ge 2 ]; then
    echo ""
    echo "⚠️  ACCEPTABLE CDD WORKFLOW"
    echo "✅ Most CDD requirements satisfied"
    echo "⚠️  Consider adding more type safety patterns"
    exit 0
else
    echo ""
    echo "❌ POOR CDD WORKFLOW"
    echo "❌ CDD requirements not met"
    echo "❌ PR should be rejected for rework"
    echo ""
    echo "💡 CDD Requirements:"
    echo "1. Design types that prevent invalid states"
    echo "2. Use compiler checks as primary quality gate"
    echo "3. Implement zero-cost abstractions"
    echo "4. Validate performance with real data"
    echo "5. Use clear commit messages indicating CDD phases"
    exit 1
fi
