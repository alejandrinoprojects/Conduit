# 1. Refract — Tauri + Rust Minecraft Launcher
**Repository:** [RefractMC/Refract_MC](https://github.com/RefractMC/Refract_MC)
**Tech Stack:** Tauri v2, Rust, Vue / TypeScript, Tokio

## Core Concepts & Architecture
Refract is a modern Minecraft launcher built with Tauri and Rust. It emphasizes native performance, low memory footprint, and tight integration with the host operating system.

### Key Architectural Patterns
1. **Async IPC & Streaming Channels:**
   - Uses Tauri's `Channel` / Event system for bidirectional stdout/stderr streaming from child JVM processes.
   - Eliminates polling by utilizing `tokio::process::Command` with piped standard I/O.
2. **Process Supervision & Lifecycle:**
   - Tracks JVM PID and exit codes.
   - Clean shutdown guarantees (`SIGTERM` / `SIGKILL` escalations on Windows via `taskkill /F /PID`).
3. **Asset & Library Pre-fetching:**
   - Validates SHA1 checksums of Minecraft client and server libraries before executing `java`.

## Conduit Implementation Roadmap
- [x] Tauri v2 Rust backend with `tokio::process::Command`
- [x] Zero-latency streaming console via Tauri event bridge (`server-log` and `client-log`)
- [ ] Auto-reconnect crash supervisor with configurable restart thresholds
