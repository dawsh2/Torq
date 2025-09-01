#!/bin/bash

# Pre-populate pool cache with known Polygon DEX pools
# This avoids RPC rate limiting during discovery

cat << 'EOF' > /tmp/known_polygon_pools.json
{
  "pools": [
    {
      "address": "0x604229c960e5cacf2aaeac8be68ac07ba9df81c3",
      "token0": "0x2791bca1f2de4661ed88a30c99a7a9449aa84174",
      "token1": "0x7ceb23fd6bc0add59e62ac25578270cff1b9f619",
      "token0_symbol": "USDC",
      "token1_symbol": "WETH",
      "protocol": "UniswapV2",
      "fee": 30
    },
    {
      "address": "0xb4e16d0168e52d35cacd2c6185b44281ec28c9dc",
      "token0": "0x2791bca1f2de4661ed88a30c99a7a9449aa84174",
      "token1": "0x8f3cf7ad23cd3cadbd9735aff958023239c6a063",
      "token0_symbol": "USDC",
      "token1_symbol": "DAI",
      "protocol": "UniswapV2",
      "fee": 30
    },
    {
      "address": "0x45dda9cb7c25131df268515131f647d726f50608",
      "token0": "0x2791bca1f2de4661ed88a30c99a7a9449aa84174",
      "token1": "0x7ceb23fd6bc0add59e62ac25578270cff1b9f619",
      "token0_symbol": "USDC",
      "token1_symbol": "WETH",
      "protocol": "UniswapV3",
      "fee": 500
    }
  ]
}
EOF

echo "Created known pools file at /tmp/known_polygon_pools.json"
echo "This can be loaded by the polygon adapter to bypass RPC discovery"