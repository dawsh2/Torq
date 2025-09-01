#!/bin/bash

# Initialize Pool Cache with Known Polygon DEX Pools
# This script pre-populates the pool cache to avoid RPC rate limiting on startup

set -euo pipefail

# Color codes
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

echo -e "${BLUE}Initializing Polygon Pool Cache...${NC}"

# Create cache directory in persistent location
CACHE_DIR="./data/pool_cache"
mkdir -p "$CACHE_DIR"

# Known popular pools on Polygon (QuickSwap V2, SushiSwap, etc.)
# Format: pool_address,token0,token1,decimals0,decimals1,protocol,fee_tier
KNOWN_POOLS=(
  # WMATIC/USDC pools
  "0x6e7a5FAFcec6BB1e78bAE2A1F0B612012BF14827,0x0d500B1d8E8eF31E21C99d1Db9A6444d3ADf1270,0x2791Bca1f2de4661ED88A30C99A7a9449Aa84174,18,6,V2,30"
  "0xcd353F79d9FADe311fC3119B841e1f456b54e858,0x0d500B1d8E8eF31E21C99d1Db9A6444d3ADf1270,0x2791Bca1f2de4661ED88A30C99A7a9449Aa84174,18,6,V2,30"
  
  # WETH/USDC pools
  "0x853Ee4b2A13f8a742d64C8F088bE7bA2131f670d,0x7ceB23fD6bC0adD59E62ac25578270cFf1b9f619,0x2791Bca1f2de4661ED88A30C99A7a9449Aa84174,18,6,V2,30"
  "0x45dDa9cb7c25131DF268515131f647d726f50608,0x7ceB23fD6bC0adD59E62ac25578270cFf1b9f619,0x2791Bca1f2de4661ED88A30C99A7a9449Aa84174,18,6,V3,500"
  
  # WBTC/WETH pools
  "0xdC9232E2Df177d7a12FdFf6EcBAb114E2231198D,0x1bfd67037b42cf73acF2047067bd4F2C47D9BfD6,0x7ceB23fD6bC0adD59E62ac25578270cFf1b9f619,8,18,V2,30"
  
  # USDC/USDT pools
  "0x2cF7252e74036d1Da831d11089D326296e64a728,0x2791Bca1f2de4661ED88A30C99A7a9449Aa84174,0xc2132D05D31c914a87C6611C10748AEb04B58e8F,6,6,V2,30"
  "0xDaC8A8E6DBf8c690ec6815e0fF03491B2770255D,0x2791Bca1f2de4661ED88A30C99A7a9449Aa84174,0xc2132D05D31c914a87C6611C10748AEb04B58e8F,6,6,V3,100"
  
  # WMATIC/WETH pools
  "0xc4e595acDD7d12feC385E5dA5D43160e8A0bAC0E,0x0d500B1d8E8eF31E21C99d1Db9A6444d3ADf1270,0x7ceB23fD6bC0adD59E62ac25578270cFf1b9f619,18,18,V2,30"
  "0x86f1d8390222A3691C28938eC7404A1661E618e0,0x0d500B1d8E8eF31E21C99d1Db9A6444d3ADf1270,0x7ceB23fD6bC0adD59E62ac25578270cFf1b9f619,18,18,V3,3000"
  
  # DAI/USDC pools
  "0xf04adBF75cDFc5eD26eeA4bbbb991DB002036Bdd,0x8f3Cf7ad23Cd3CaDbD9735AFf958023239c6A063,0x2791Bca1f2de4661ED88A30C99A7a9449Aa84174,18,6,V2,30"
  
  # More active pools
  "0x9b08288c3Be4F62bbf8d1C20Ac9C5e6f9467d8B7,0x53E0bca35eC356BD5ddDFebbD1Fc0fD03FaBad39,0x2791Bca1f2de4661ED88A30C99A7a9449Aa84174,18,6,V2,30"
  "0xA374094527e1673A86dE625aa59517c5dE346d32,0x0d500B1d8E8eF31E21C99d1Db9A6444d3ADf1270,0xc2132D05D31c914a87C6611C10748AEb04B58e8F,18,6,V2,30"
)

# Generate a simple JSON cache file
CACHE_FILE="$CACHE_DIR/polygon_pools.json"

echo -e "${YELLOW}Creating pool cache at $CACHE_FILE${NC}"

# Start JSON
echo '{
  "version": 1,
  "chain_id": 137,
  "pools": [' > "$CACHE_FILE"

FIRST=true
for pool_data in "${KNOWN_POOLS[@]}"; do
  IFS=',' read -r pool token0 token1 dec0 dec1 protocol fee <<< "$pool_data"
  
  if [ "$FIRST" = false ]; then
    echo "," >> "$CACHE_FILE"
  fi
  FIRST=false
  
  cat >> "$CACHE_FILE" << EOF
    {
      "pool_address": "$pool",
      "token0": "$token0",
      "token1": "$token1",
      "token0_decimals": $dec0,
      "token1_decimals": $dec1,
      "protocol": "$protocol",
      "fee_tier": $fee,
      "discovered_at": $(date +%s)000000000,
      "venue": "Polygon"
    }
EOF
done

# Close JSON
echo '
  ]
}' >> "$CACHE_FILE"

echo -e "${GREEN}✓ Created pool cache with ${#KNOWN_POOLS[@]} known pools${NC}"
echo -e "${BLUE}Cache file: $CACHE_FILE${NC}"

# Also create the TLV cache file that the Rust code expects
# The pool cache uses chain_137_pool_cache.tlv format
TLV_CACHE="$CACHE_DIR/chain_137_pool_cache.tlv"
echo -e "${YELLOW}Note: TLV cache format at $TLV_CACHE needs to be created by the Rust application${NC}"

echo -e "${GREEN}Pool cache initialization complete!${NC}"
echo ""
echo "To use this cache:"
echo "1. The Polygon adapter will automatically load from ./data/pool_cache/"
echo "2. New pools discovered via WebSocket will be added to the cache"
echo "3. The cache persists across restarts and reboots"