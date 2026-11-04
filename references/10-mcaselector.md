# 10. MCA Selector — World Trimming and Chunk Selection
**Repository:** [Querz/mcaselector](https://github.com/Querz/mcaselector)
**Tech Stack:** Java, JavaFX, Custom NBT parser

## Core Concepts & Architecture
MCA Selector enables selecting, deleting, exporting, and modifying chunks in Minecraft worlds based on criteria like InhabitedTime, chunk status, or coordinates.

### Key Architectural Patterns
1. **Header Parsing:**
   - Reads the 8KB header of each `.mca` file (4KB chunk locations + 4KB timestamps) without decompressing chunk payloads.
2. **Selective Deletion & Re-packing:**
   - Sets chunk offset to 0 and recalculates sector allocations to instantly prune worlds.

## Conduit Implementation Roadmap
- [x] Built-in World Pruner tab in Conduit Realm Dashboard (`get_realm_prune_info` and `execute_realm_prune`)
- [x] Selective dimension chunk deletion (Overworld, Nether, End)
