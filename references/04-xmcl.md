# 4. XMCL (X Minecraft Launcher) — Content & Dependency Management
**Repository:** [Voxelum/x-minecraft-launcher](https://github.com/Voxelum/x-minecraft-launcher)
**Tech Stack:** TypeScript, Electron, Rust Native Addons, Content-Addressable Storage

## Core Concepts & Architecture
XMCL pioneered Content-Addressable Storage (CAS) for Minecraft mods, shaders, and modpacks.

### Key Architectural Patterns
1. **Deduplication Engine:**
   - Mods and assets are stored by their SHA1/SHA256 hash in a central store.
   - Instances access mods through NTFS hard links (`CreateHardLinkW`) or symlinks, consuming 0 extra disk space.
2. **Modrinth & CurseForge Unified API:**
   - Direct integration with Modrinth v2 API for mod search, version resolution, and automatic dependency installation.
3. **Client vs Server Mod Discrimination:**
   - Filters client-only mods (rendering, HUD, camera, sound) from server distributions.

## Conduit Implementation Roadmap
- [x] Zero-Duplication NTFS Engine (Hard links for mods, directory junctions for assets/libraries)
- [x] Client-only mod filter (`is_client_only_mod`) to prevent dedicated server LWJGL crashes
- [x] Native Modrinth project search and download engine
