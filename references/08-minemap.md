# 8. Minemap — Seed Mapper Implementation
**Repository:** [hube12/Minemap](https://github.com/hube12/Minemap)
**Tech Stack:** Java, JavaFX, Multi-threaded Biome Sampling

## Core Concepts & Architecture
Minemap provides a graphical interface built on top of Cubiomes for zooming, panning, and searching Minecraft seeds.

### Key Architectural Patterns
1. **Multi-Threaded Tile Renderer:**
   - Divides the viewport into 64x64 pixel tiles processed by a work-stealing thread pool.
2. **Interactive Coordinate Finder:**
   - Real-time cursor coordinates with biome names, structure icons, and slime chunk overlay.

## Conduit Implementation Roadmap
- [ ] Interactive Canvas Seed Map in the Realm Dashboard
