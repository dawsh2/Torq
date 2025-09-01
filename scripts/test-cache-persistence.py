#!/usr/bin/env python3
"""
Test cache persistence by manually adding pools to the cache file
"""

import struct
import time
from pathlib import Path

def add_test_pool():
    """Add a test pool to see if it persists"""
    cache_file = Path("data/pool_cache/chain_137_pool_cache.tlv")
    
    # Read existing cache
    with open(cache_file, "rb") as f:
        data = f.read()
    
    # Parse header (first 20 bytes)
    magic = data[0:4]
    version = data[4]
    reserved = data[5:8]
    pool_count = struct.unpack("<I", data[8:12])[0]
    chain_id = struct.unpack("<I", data[12:16])[0]
    timestamp = struct.unpack("<Q", data[16:24])[0]
    
    print(f"Current cache: {pool_count} pools, chain {chain_id}")
    
    # Calculate size of existing pools (each pool record is 83 bytes)
    # 20 bytes pool_address + 20 bytes token0 + 20 bytes token1 + 
    # 1 byte token0_decimals + 1 byte token1_decimals + 1 byte pool_type +
    # 4 bytes fee_tier + 8 bytes discovered_at + 8 bytes last_seen = 83 bytes
    existing_size = 24 + (pool_count * 83)
    
    # Create new pool record for one of the failing discoveries
    # Using pool 0xdc9232e2df177d7a12fdff6ecbab114e2231198d from logs
    new_pool = b""
    new_pool += bytes.fromhex("dc9232e2df177d7a12fdff6ecbab114e2231198d")  # pool address
    new_pool += bytes.fromhex("0d500B1d8E8eF31E21C99d1Db9A6444d3ADf1270")  # WMATIC
    new_pool += bytes.fromhex("2791Bca1f2de4661ED88A30C99A7a9449Aa84174")  # USDC
    new_pool += struct.pack("<BBB", 18, 6, 1)  # decimals and pool type (V2)
    new_pool += struct.pack("<I", 30)  # fee tier
    new_now = int(time.time() * 1e9)
    new_pool += struct.pack("<Q", new_now)  # discovered_at
    new_pool += struct.pack("<Q", new_now)  # last_seen
    
    # Write updated cache
    with open(cache_file, "wb") as f:
        # Updated header with incremented pool count
        f.write(magic)
        f.write(struct.pack("<B", version))
        f.write(reserved)
        f.write(struct.pack("<I", pool_count + 1))
        f.write(struct.pack("<I", chain_id))
        f.write(struct.pack("<Q", int(time.time() * 1e9)))
        
        # Write existing pools
        f.write(data[24:existing_size])
        
        # Write new pool
        f.write(new_pool)
    
    print(f"✅ Added test pool, cache now has {pool_count + 1} pools")
    print(f"   Pool: 0xdc9232e2df177d7a12fdff6ecbab114e2231198d")
    print(f"   Tokens: WMATIC/USDC")

if __name__ == "__main__":
    add_test_pool()