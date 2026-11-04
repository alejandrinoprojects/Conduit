# 3. Prism Launcher — Mature Instance Management
**Repository:** [PrismLauncher/PrismLauncher](https://github.com/PrismLauncher/PrismLauncher)
**Tech Stack:** C++, Qt, JSON Components

## Core Concepts & Architecture
Prism Launcher is the gold standard for Minecraft instance isolation, component overriding, and mod lifecycle management.

### Key Architectural Patterns
1. **Component-Based Instance Model:**
   - Instances are defined by a component list (`mmc-pack.json` or `instance.cfg`).
   - Every version, loader, and patch is an independent component with override flags.
2. **Library-First Hierarchy:**
   - Worlds, screenshots, and logs are children of their parent runtime.
   - Saves are NEVER flattened into a single global pool; they remain isolated within their instance.
3. **Java Auto-Detection:**
   - Probes registry (`HKLM\SOFTWARE\JavaSoft`) and environment variables to catalog available JDKs with major versions (JDK 8, 17, 21).

## Conduit Implementation Roadmap
- [x] Strict hierarchy: Library -> Runtimes -> Grouped Realms / Saves
- [x] Java installation cataloging (Java 21 detection for 1.20.5+)
- [x] Instance-level mod and server settings isolation
