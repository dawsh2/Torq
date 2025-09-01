#!/usr/bin/env python3
"""
Convert JSON pool data to TLV cache format
This bypasses RPC discovery which is rate limited
"""

import json
import struct
import time
from pathlib import Path

def main():
    # Read JSON pools
    json_path = Path("./data/pool_cache/polygon_pools.json")
    if not json_path.exists():
        print(f"❌ {json_path} not found")
        return
        
    with open(json_path) as f:
        data = json.load(f)
    
    pools = data["pools"]
    print(f"📊 Found {len(pools)} pools in JSON")
    
    # Create TLV cache file
    cache_dir = Path("./data/pool_cache")
    cache_dir.mkdir(exist_ok=True)
    cache_file = cache_dir / "polygon_137.cache"
    
    with open(cache_file, "wb") as f:
        # Write header
        f.write(b"POOL")  # Magic
        f.write(struct.pack("<B", 1))  # Version
        f.write(struct.pack("<BBB", 0, 0, 0))  # Reserved
        f.write(struct.pack("<I", len(pools)))  # Pool count
        f.write(struct.pack("<I", 137))  # Chain ID (Polygon)
        f.write(struct.pack("<Q", int(time.time() * 1e9)))  # Timestamp
        
        # Write pool records
        for pool in pools:
            # Pool address (20 bytes)
            pool_addr = bytes.fromhex(pool["pool_address"][2:])
            f.write(pool_addr)
            
            # Token addresses (20 bytes each)
            token0 = bytes.fromhex(pool["token0"][2:])
            token1 = bytes.fromhex(pool["token1"][2:])
            f.write(token0)
            f.write(token1)
            
            # Decimals and pool type
            f.write(struct.pack("<B", pool.get("token0_decimals", 18)))
            f.write(struct.pack("<B", pool.get("token1_decimals", 18)))
            
            # Pool type: 1=V2, 2=V3
            pool_type = 2 if pool.get("protocol") == "V3" else 1
            f.write(struct.pack("<B", pool_type))
            
            # Fee tier (4 bytes)
            f.write(struct.pack("<I", pool.get("fee_tier", 30)))
            
            # Timestamps (8 bytes each)
            now_ns = int(time.time() * 1e9)
            f.write(struct.pack("<Q", now_ns))  # discovered_at
            f.write(struct.pack("<Q", now_ns))  # last_seen
    
    print(f"✅ Wrote {len(pools)} pools to {cache_file}")
    print("🎯 Pool cache is now ready for use!")
    
    # Show some pool pairs for verification
    print("\n📋 Sample pool pairs (for arbitrage detection):")
    pairs = {}
    for pool in pools[:6]:
        pair = f"{pool['token0'][:10]}/{pool['token1'][:10]}"
        if pair not in pairs:
            pairs[pair] = []
        pairs[pair].append(pool['pool_address'][:10])
    
    for pair, addrs in pairs.items():
        if len(addrs) > 1:
            print(f"  {pair}: {len(addrs)} pools (arbitrage possible!)")
        else:
            print(f"  {pair}: 1 pool")

if __name__ == "__main__":
    main()