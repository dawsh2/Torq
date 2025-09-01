#!/bin/bash

# Script to run flash_arbitrage with pre-loaded pool cache
# This ensures arbitrage detection works by having multiple pools available

set -e

echo "🚀 Starting Flash Arbitrage with pre-loaded pools..."

# First, ensure we have the pool cache data
POOL_CACHE_FILE="./data/pool_cache/polygon_pools.json"

if [ ! -f "$POOL_CACHE_FILE" ]; then
    echo "❌ Pool cache not found at $POOL_CACHE_FILE"
    echo "Run: ./scripts/init-pool-cache.sh first"
    exit 1
fi

# Count pools in cache
POOL_COUNT=$(jq '.pools | length' "$POOL_CACHE_FILE")
echo "📦 Found $POOL_COUNT pools in cache"

# Show token pairs that have multiple pools (good for arbitrage)
echo "🔍 Token pairs with multiple pools (arbitrage opportunities):"
jq -r '.pools | group_by(.token0 + "-" + .token1) | map(select(length > 1)) | .[] | "  - " + .[0].token0 + "/" + .[0].token1 + " (" + (length | tostring) + " pools)"' "$POOL_CACHE_FILE"

# Set environment to enable pool cache loading
export RUST_LOG=info,flash_arbitrage=debug,flash_arbitrage::detector=debug
export LOAD_POOL_CACHE=true
export POOL_CACHE_PATH="./data/pool_cache/polygon_pools.json"

echo ""
echo "📊 Starting flash_arbitrage with debug logging..."
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

# Run flash_arbitrage with the environment variables set
./target/release/flash_arbitrage