# Project Rules: Hybrid NeoForge 1.21.1 Launcher + Server Orchestrator
## Stack & Target
- Framework: Tauri v2 (`tauri = "2"`), Vue 3 (`<script setup lang="ts">`), Tailwind CSS.
- Minecraft Target: 1.21.1 with NeoForge (Java 21 64-bit).
- Tunneling: `playit-cli` sidecar targeting `127.0.0.1:25565`.
## Hard Engineering Constraints
1. **NO PLACEHOLDERS:** Never write `// TODO`, `// implement later`, or stubbed functions. Write 100% complete logic.
2. **Tauri v2 Event Syntax:** Always `use tauri::{AppHandle, Emitter, Manager, State};` and call `app.emit("event-name", payload)`. Do NOT use Tauri v1 `app.emit_all()`.
3. **Async Process Ownership (CRITICAL):**
  - Never hold a `MutexGuard` across an `.await` point when waiting on a child process.
  - When spawning the NeoForge server via `tokio::process::Command`, immediately `.take()` `stdin`, `stdout`, and `stderr` from the `Child` struct.
  - Store `ChildStdin` in `AppState` so `stop_server` can write `b"stop\n"` without deadlocking the background task waiting on `child.wait()`.
4. **Windows File Linking:** Always use `std::fs::hard_link` (with `std::fs::copy` fallback) when linking `.minecraft/mods/*.jar` into `server/mods/*.jar`. Never use `symlink` on Windows as it fails without Administrator privileges.
5. **Compile Before Advancing:** After creating or editing any `.rs` file, you MUST run `cargo check` inside `src-tauri/` and fix all compiler errors before moving to the next task.
