# 2. LightyLauncherLib — Minecraft Installation and Launching
**Repository:** [Lighty-Launcher/LightyLauncherLib](https://github.com/Lighty-Launcher/LightyLauncherLib)
**Tech Stack:** Rust, Reqwest, Tokio, Serde

## Core Concepts & Architecture
LightyLauncherLib provides a high-performance library in pure Rust for downloading Mojang version manifests, parsing rule arguments (OS, architecture, feature flags), and constructing dynamic classpath strings.

### Key Architectural Patterns
1. **Rule Evaluation Engine:**
   - Evaluates `${os.name}`, `${arch}`, and Mojang feature rules (`is_demo_user`, `has_custom_resolution`).
2. **Multi-Loader Chains:**
   - Resolves loader inheritance trees: `NeoForge/Fabric -> Vanilla Version -> Assets Index`.
3. **High-Concurrency Asset Downloader:**
   - Utilizes `futures::stream::buffer_unordered` to download Mojang assets in parallel chunks.

## Conduit Implementation Roadmap
- [x] Mojang Version Manifest parsing in Rust (`piston-meta.mojang.com`)
- [x] Multi-loader asset verification (NeoForge, Fabric, Vanilla)
- [ ] Direct tokenized argument builder for custom JVM flags (-XX:+UseG1GC, -Xmx, -Xms)
