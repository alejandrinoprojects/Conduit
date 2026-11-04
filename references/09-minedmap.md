# 9. MinedMap — Existing-World Mapping
**Repository:** [neocturne/MinedMap](https://github.com/neocturne/MinedMap)
**Tech Stack:** Rust, Rayon, Fast Chunk Decoding

## Core Concepts & Architecture
MinedMap generates ultra-fast top-down maps of real Minecraft worlds by decoding region files (`.mca`) in parallel with Rust.

### Key Architectural Patterns
1. **Parallel Region Scanning:**
   - Uses Rayon to process multiple `.mca` files concurrently across all CPU cores.
2. **Biome & Depth Shading:**
   - Shaded relief based on block height and water depth.
3. **Web Tile Output:**
   - Outputs Leaflet-compatible tile pyramids for browser viewing.

## Conduit Implementation Roadmap
- [ ] Direct `.mca` region scanning for live map rendering in Conduit Realm Dashboard
