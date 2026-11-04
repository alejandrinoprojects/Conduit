# 11. FastNBT — NBT and Anvil Processing
**Repository:** [owengage/fastnbt](https://github.com/owengage/fastnbt)
**Tech Stack:** Rust, Serde, Zero-Copy Parsing

## Core Concepts & Architecture
FastNBT provides extremely fast Named Binary Tag (NBT) parsing and serialization in Rust, optimized for streaming chunk and level data.

### Key Architectural Patterns
1. **Zero-Copy Byte Slices:**
   - Deserializes directly into `&'a str` and `&'a [u8]` without heap allocations.
2. **Compound Tag Navigation:**
   - Direct binary search for named tags (`LevelName`, `Data`, `GameRules`, `Player`).

## Conduit Implementation Roadmap
- [x] Big-endian binary length NBT extraction for `level.dat` (`LevelName`, `GameType`) in `src-tauri/src/mods.rs`
- [ ] Complete player inventory and playerdata NBT inspection in Player tab
