# Conduit — Architecture & Open-Source Reference Repositories

This directory contains architectural specifications, feature mappings, and implementation blueprints extracted from premier open-source Minecraft and system tooling. Conduit synthesizes these technologies into a unified, zero-duplication, high-performance Minecraft runtime and dedicated server orchestrator.

---

## Reference Matrix & Conduit Integration Roadmap

| # | Reference Repository | Technology Stack | Core Architecture Pattern | Conduit Feature Mapping |
|---|----------------------|------------------|---------------------------|-------------------------|
| 1 | [Refract_MC](https://github.com/RefractMC/Refract_MC) | Tauri + Rust + Vue/TS | Event-driven IPC, process supervisors | Tauri v2 background daemon & IPC bridge |
| 2 | [LightyLauncherLib](https://github.com/Lighty-Launcher/LightyLauncherLib) | Rust / Async | Rule evaluation, library resolution | Multi-loader resolver (NeoForge/Fabric/Vanilla) |
| 3 | [PrismLauncher](https://github.com/PrismLauncher/PrismLauncher) | C++ / Qt / Components | Component-based instance metadata | Library hierarchy & grouped runtime realms |
| 4 | [XMCL (X Minecraft Launcher)](https://github.com/Voxelum/x-minecraft-launcher) | TypeScript / Electron / Rust | Content hashing, CAS, Modrinth API | CAS zero-duplication cache & mod manager |
| 5 | [Skin Anvil](https://github.com/spencerfrost/skin-anvil) | WebGL / Canvas / TS | Dual-layer 64x64 pixel manipulation | 2D/3D skin painting & texture editor |
| 6 | [Skinview3D](https://github.com/bs-community/skinview3d) | Three.js / WebGL | Skeletal rigging, animation cycles | 3D interactive player avatar & skin preview |
| 7 | [Cubiomes](https://github.com/Cubitect/cubiomes) | C / High Performance | Biome & structure noise generators | Native seed reverse-engineering & biome map |
| 8 | [Minemap](https://github.com/hube12/Minemap) | Java / JavaFX | Fast multi-threaded biome sampling | Interactive seed map visualizer & coordinate finder |
| 9 | [MinedMap](https://github.com/neocturne/MinedMap) | Rust | Parallel Region (.mca) rendering | Real-time map renderer for live server worlds |
| 10 | [MCA Selector](https://github.com/Querz/mcaselector) | Java | Chunk filter expressions, NBT masking | Integrated chunk pruner & disk reclaimer |
| 11 | [FastNBT](https://github.com/owengage/fastnbt) | Rust | Zero-copy NBT parser / serializer | Fast playerdata, level.dat & chunk processing |
| 12 | [Azalea](https://github.com/azalea-rs/azalea) | Rust / Tokio | Pure Rust Minecraft protocol client | Zero-overhead local server probe & ping engine |

---

## Detailed Specifications

### 1. Refract (`RefractMC/Refract_MC`)
- **Focus**: High-speed, secure desktop client using Tauri and Rust.
- **Conduit Implementation**: Asynchronous process management for headless dedicated server and Minecraft client JVMs with non-blocking stream pipes (`tokio::io::BufReader`).

### 2. LightyLauncherLib (`Lighty-Launcher/LightyLauncherLib`)
- **Focus**: Clean Rust implementation of Mojang version manifests, asset downloads, library path construction, and loader chain discovery.
- **Conduit Implementation**: Fast loader installation for NeoForge, Fabric, Quilt, and Paper with automated asset index verification.

### 3. Prism Launcher (`PrismLauncher/PrismLauncher`)
- **Focus**: Mature instance architecture where each runtime and version isolates its configuration, with clear parent-child component trees.
- **Conduit Implementation**: Library view where **Realms are grouped strictly under their parent Runtime & Loader**, ensuring saves are correctly isolated and displayed.

### 4. XMCL (`Voxelum/x-minecraft-launcher`)
- **Focus**: Content-addressable storage (CAS), Modrinth/CurseForge indexing, and mod dependency resolution.
- **Conduit Implementation**: NTFS hard links and directory junctions for zero-duplication mod sharing between singleplayer instances and dedicated servers without copying disk space.

### 5. Skin Anvil & Skinview3D
- **Focus**: Real-time 3D Minecraft player preview (Alex/Steve models) and 64x64 layer editing.
- **Conduit Implementation**: Interactive 3D player canvas in the Player section with Microsoft profile sync.

### 6. MCA Selector & FastNBT
- **Focus**: Reading Anvil chunk files (`.mca`), parsing compound NBT, and selectively pruning untouched or uninhabited chunks.
- **Conduit Implementation**: The built-in World Pruner tab in Realm Dashboard to prune unneeded chunks and reduce world backups from gigabytes down to megabytes.

### 7. MinedMap & Minemap
- **Focus**: Biome and block-topography world generation and exploration.
- **Conduit Implementation**: Seed mapping and realm territory visualization.

### 8. Azalea
- **Focus**: Asynchronous Tokio-based Minecraft network protocol.
- **Conduit Implementation**: Direct TCP ping and status monitoring on port 25565 without spawning external ping utilities.
