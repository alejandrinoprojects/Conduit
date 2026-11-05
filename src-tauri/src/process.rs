use crate::launcher::resolve_java_path;
use crate::mods::sync_server_mods;
use crate::state::{AppState, ClientStatus, ServerStatus};
use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use tauri::{AppHandle, Emitter, State};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

// -----------------------------------------------------------------------------
// 1. Headless Server & Tunnel Supervisor
// -----------------------------------------------------------------------------

#[tauri::command]
pub async fn start_server(
    app: AppHandle,
    state: State<'_, AppState>,
    instance_dir: String,
    ram_gb: u32,
    world_name: Option<String>,
) -> Result<(), String> {
    start_server_internal(&app, (*state).clone(), instance_dir, ram_gb, world_name).await
}

pub async fn start_server_internal(
    app: &AppHandle,
    state: AppState,
    instance_dir: String,
    ram_gb: u32,
    world_name: Option<String>,
) -> Result<(), String> {
    let mut instance_path = PathBuf::from(&instance_dir);
    if instance_path.is_relative() {
        if let Ok(exe_dir) = std::env::current_exe() {
            if let Some(parent) = exe_dir.parent() {
                let candidate = parent.join(&instance_dir);
                if candidate.exists() {
                    instance_path = candidate;
                }
            }
        }
    }

    // Maintain zero duplication NTFS directory junctions (libraries, config) upfront
    let _ = crate::mods::ensure_zero_duplication_junctions(&instance_path);

    // Determine effective server directory: check if the realm is an existing dedicated server
    let mut server_dir = instance_path.join("server");
    let client_dir = instance_path.join(".minecraft");

    if let Some(ref chosen_world) = world_name {
        let trimmed = chosen_world.trim();
        if !trimmed.is_empty() {
            if let Ok(localappdata) = std::env::var("LOCALAPPDATA") {
                let sq_candidate = PathBuf::from(&localappdata).join("SquidServers").join(trimmed);
                if sq_candidate.exists() && (sq_candidate.join("run.bat").exists() || sq_candidate.join("server.properties").exists()) {
                    server_dir = sq_candidate;
                }
            }
        }
    }

    let is_instance_server = server_dir == instance_path.join("server");
    let props_path = server_dir.join("server.properties");
    let _ = fs::create_dir_all(&server_dir);

    if is_instance_server {
        if let Some(ref chosen_world) = world_name {
            let trimmed = chosen_world.trim();
            if !trimmed.is_empty() {
                let mut resolved_world_path: Option<PathBuf> = None;
                let c1 = client_dir.join("saves").join(trimmed);
                let c2 = instance_path.join("saves").join(trimmed);
                let c3 = PathBuf::from(trimmed);
                if c1.exists() {
                    resolved_world_path = Some(c1);
                } else if c2.exists() {
                    resolved_world_path = Some(c2);
                } else if c3.exists() && c3.is_dir() {
                    resolved_world_path = Some(c3);
                } else {
                    // Search Modrinth App profiles
                    if let Ok(appdata) = std::env::var("APPDATA") {
                        let profs = PathBuf::from(appdata).join("ModrinthApp").join("profiles");
                        if profs.exists() {
                            if let Ok(entries) = fs::read_dir(profs) {
                                for e in entries.flatten() {
                                    let cand = e.path().join("saves").join(trimmed);
                                    if cand.exists() {
                                        resolved_world_path = Some(cand);
                                        break;
                                    }
                                }
                            }
                        }
                    }
                    // Search SquidServers
                    if resolved_world_path.is_none() {
                        if let Ok(localappdata) = std::env::var("LOCALAPPDATA") {
                            let cand = PathBuf::from(&localappdata).join("SquidServers").join(trimmed).join("world");
                            if cand.exists() {
                                resolved_world_path = Some(cand);
                            } else {
                                // Search any subfolder with level.dat
                                let base = PathBuf::from(&localappdata).join("SquidServers").join(trimmed);
                                if let Ok(subentries) = fs::read_dir(base) {
                                    for sub in subentries.flatten() {
                                        if sub.path().is_dir() && sub.path().join("level.dat").exists() {
                                            resolved_world_path = Some(sub.path());
                                            break;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                if let Some(target_world) = resolved_world_path {
                    let server_world = server_dir.join("world");
                    if server_world.exists() {
                        if crate::mods::is_junction_or_symlink(&server_world) {
                            let _ = crate::mods::remove_junction(&server_world);
                            let _ = crate::mods::create_directory_junction(&server_world, &target_world);
                        } else {
                            let backup = server_dir.join(format!("world_backup_{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs()));
                            let _ = fs::rename(&server_world, backup);
                            let _ = crate::mods::create_directory_junction(&server_world, &target_world);
                        }
                    } else {
                        let _ = crate::mods::create_directory_junction(&server_world, &target_world);
                    }
                }
            }
        }

        // Ensure server.properties sets level-name=world so it transparently loads the active realm junction
        // and online-mode=true for Mojang account verification
        if props_path.exists() {
            if let Ok(content) = fs::read_to_string(&props_path) {
                let mut lines: Vec<String> = Vec::new();
                let mut found_level = false;
                let mut found_online = false;
                for line in content.lines() {
                    if line.starts_with("level-name=") {
                        lines.push("level-name=world".to_string());
                        found_level = true;
                    } else if line.starts_with("online-mode=") {
                        lines.push("online-mode=true".to_string());
                        found_online = true;
                    } else {
                        lines.push(line.to_string());
                    }
                }
                if !found_level {
                    lines.push("level-name=world".to_string());
                }
                if !found_online {
                    lines.push("online-mode=true".to_string());
                }
                let _ = fs::write(&props_path, lines.join("\n"));
            }
        } else {
            let _ = fs::write(
                &props_path,
                "level-name=world\nonline-mode=true\nsimulation-distance=6\nview-distance=10\nsync-chunk-writes=false\nserver-port=25565\nenable-rcon=false\n"
            );
        }

        // Maintain zero duplication NTFS directory junctions (libraries, config)
        let _ = crate::mods::ensure_zero_duplication_junctions(&instance_path);

        // Sync mods so server/mods/ is up to date with zero-overhead hard links and client-only mods purged
        sync_server_mods(&instance_path)?;
    }

    // Ensure the current user profile is registered in server/ops.json as operator / admin (level 4)
    let auth = {
        let guard = state.lock().await;
        guard.auth_profile.clone()
    };
    let ops_path = server_dir.join("ops.json");
    let username = auth.as_ref().map(|p| p.username.clone()).unwrap_or_else(|| "DeanDylan2378".to_string());
    let uuid = auth.as_ref().map(|p| p.uuid.clone()).unwrap_or_else(|| "00000000-0000-0000-0000-000000000000".to_string());
    let op_entry = serde_json::json!([
        {
            "uuid": uuid,
            "name": username,
            "level": 4,
            "bypassesPlayerLimit": true
        }
    ]);
    let _ = fs::write(&ops_path, serde_json::to_string_pretty(&op_entry).unwrap_or_default());

    // Clean up any stale session.lock files in server worlds before starting
    let server_world_lock = instance_path.join("server").join("world").join("session.lock");
    if server_world_lock.exists() {
        let _ = fs::remove_file(server_world_lock);
    }

    // Terminate any zombie java processes holding the server port 25565
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        let _ = std::process::Command::new("powershell")
            .args(["-NoProfile", "-Command", "Get-NetTCPConnection -LocalPort 25565 -ErrorAction SilentlyContinue | ForEach-Object { Stop-Process -Id $_.OwningProcess -Force -ErrorAction SilentlyContinue }"])
            .creation_flags(0x08000000)
            .status();
    }

    // 2. Set server status to Starting
    {
        let mut guard = state.lock().await;
        if guard.server_status != ServerStatus::Stopped {
            return Err("Server is already running or starting".into());
        }
        guard.server_status = ServerStatus::Starting;
    }
    let _ = app.emit("server-status", ServerStatus::Starting);

    // 3. Initialize Built-in Conduit Network Tunnel (Zero-config P2P encrypted tunnel)
    // + Automated Reverse Tunnel (Enables vanilla Minecraft friends to connect without Conduit or port forwarding)
    let world_name_copy = world_name.clone().unwrap_or_else(|| "World".to_string());
    let app_clone = app.clone();
    let state_clone = state.inner.clone();
    tokio::spawn(async move {
        // A. Conduit WireGuard P2P Host Tunnel
        match crate::network::conduit_host_realm(world_name_copy, Some(25565)).await {
            Ok(info) => {
                let _ = app_clone.emit("conduit-room-code", info.room_code);
            }
            Err(e) => {
                eprintln!("[Conduit Network] Failed to initialize realm tunnel: {}", e);
            }
        }

        // B. Zero-Config Public TCP Reverse Tunnel (Vanilla Minecraft Friend Support)
        let mut ssh_cmd = tokio::process::Command::new("ssh");
        ssh_cmd.args([
            "-o", "StrictHostKeyChecking=no",
            "-o", "ServerAliveInterval=30",
            "-o", "ServerAliveCountMax=3",
            "-R", "0:localhost:25565",
            "serveo.net",
        ]);
        ssh_cmd.stdout(Stdio::piped());
        ssh_cmd.stderr(Stdio::piped());

        #[cfg(target_os = "windows")]
        {
            ssh_cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
        }

        if let Ok(mut child) = ssh_cmd.spawn() {
            if let Some(stdout) = child.stdout.take() {
                let app_inner = app_clone.clone();
                let state_inner = state_clone.clone();
                tokio::spawn(async move {
                    let mut reader = BufReader::new(stdout).lines();
                    while let Ok(Some(line)) = reader.next_line().await {
                        // Look for: Forwarding TCP connections from serveousercontent.com:XXXXX
                        if let Some(idx) = line.find("Forwarding TCP connections from ") {
                            let addr = line[idx + "Forwarding TCP connections from ".len()..].trim();
                            if !addr.is_empty() {
                                {
                                    let mut guard = state_inner.lock().await;
                                    guard.tunnel_url = Some(addr.to_string());
                                }
                                let _ = app_inner.emit("tunnel-status", addr.to_string());
                                let _ = app_inner.emit("public-endpoint", addr.to_string());
                                break;
                            }
                        }
                    }
                });
            }

            let mut guard = state_clone.lock().await;
            guard.tunnel_child = Some(child);
        }
    });

    // 4. Locate server executable / args file (NeoForge, Fabric, Paper, Vanilla)
    let libraries_dir = server_dir.join("libraries");
    let mut args_file_arg = None;

    // Ensure libraries junction exists if client libraries are available
    if !libraries_dir.exists() || !libraries_dir.join("net").exists() {
        let client_libs = client_dir.join("libraries");
        if client_libs.exists() {
            let _ = crate::mods::create_directory_junction(&libraries_dir, &client_libs);
        }
    }

    // A. Check run.bat for explicit win_args.txt path
    let run_bat_path = server_dir.join("run.bat");
    if run_bat_path.exists() {
        if let Ok(content) = fs::read_to_string(&run_bat_path) {
            for token in content.split_whitespace() {
                if token.starts_with("@libraries/") || token.starts_with("@libraries\\") {
                    args_file_arg = Some(token.replace('\\', "/"));
                    break;
                }
            }
        }
    }

    // B. Search libraries/net/neoforged/neoforge for highest win_args.txt
    if args_file_arg.is_none() && libraries_dir.exists() {
        let neoforge_libs_dir = libraries_dir.join("net").join("neoforged").join("neoforge");
        if neoforge_libs_dir.exists() {
            if let Ok(entries) = fs::read_dir(&neoforge_libs_dir) {
                let mut ver_dirs: Vec<PathBuf> = entries.flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect();
                ver_dirs.sort();
                for entry in ver_dirs.into_iter().rev() {
                    let win_args = entry.join("win_args.txt");
                    let unix_args = entry.join("unix_args.txt");
                    if win_args.exists() {
                        if let Ok(rel) = win_args.strip_prefix(&server_dir) {
                            args_file_arg = Some(format!("@{}", rel.to_string_lossy().replace('\\', "/")));
                            break;
                        }
                    } else if unix_args.exists() {
                        if let Ok(rel) = unix_args.strip_prefix(&server_dir) {
                            args_file_arg = Some(format!("@{}", rel.to_string_lossy().replace('\\', "/")));
                            break;
                        }
                    }
                }
            }
        }
    }

    let java_bin = resolve_java_path(&app);

    // 5. Assemble server JVM command with Aikar's G1GC flags
    let mut server_cmd = tokio::process::Command::new(&java_bin);
    server_cmd.current_dir(&server_dir);
    #[cfg(target_os = "windows")]
    {
        server_cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
    }

    // Memory & GC: Dynamic heap scaling
    server_cmd.arg("-Xms512M");
    server_cmd.arg(format!("-Xmx{}G", ram_gb));
    server_cmd.args([
        "-XX:+UseG1GC",
        "-XX:+ParallelRefProcEnabled",
        "-XX:+UseStringDeduplication",
        "-XX:MaxGCPauseMillis=50",
        "-XX:+UnlockExperimentalVMOptions",
        "-XX:+DisableExplicitGC",
        "-XX:G1ReservePercent=15",
        "-XX:G1HeapRegionSize=8m",
        "-XX:InitiatingHeapOccupancyPercent=45",
        "-Dio.netty.allocator.type=pooled",
        "-Dio.netty.tryReflectionSetAccessible=true",
        "-Dio.netty.eventLoopThreads=4",
        "-Djava.net.preferIPv4Stack=true",
    ]);

    // Check loader type:
    if let Some(arg_file) = args_file_arg {
        server_cmd.arg(arg_file);
    } else {
        let server_jar = server_dir.join("server.jar");
        let fabric_server_jar = server_dir.join("fabric-server-launch.jar");
        let quilt_server_jar = server_dir.join("quilt-server-launch.jar");
        let paper_jar = server_dir.join("paper.jar");
        let purpur_jar = server_dir.join("purpur.jar");

        if fabric_server_jar.exists() {
            server_cmd.arg("-jar").arg(fabric_server_jar);
        } else if quilt_server_jar.exists() {
            server_cmd.arg("-jar").arg(quilt_server_jar);
        } else if paper_jar.exists() {
            server_cmd.arg("-jar").arg(paper_jar);
        } else if purpur_jar.exists() {
            server_cmd.arg("-jar").arg(purpur_jar);
        } else if server_jar.exists() {
            server_cmd.arg("-jar").arg(server_jar);
        } else {
            return Err(format!("Server binary or loader not found in {:?}. Please install server software first.", server_dir));
        }
    }
    server_cmd.arg("--nogui");

    server_cmd.stdin(Stdio::piped());
    server_cmd.stdout(Stdio::piped());
    server_cmd.stderr(Stdio::piped());

    #[cfg(target_os = "windows")]
    {
        server_cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
    }

    let mut child = server_cmd
        .spawn()
        .map_err(|e| format!("Failed to spawn NeoForge server: {e}"))?;

    // Hard Constraint 3: Immediately take stdin, stdout, stderr
    let stdin = child.stdin.take();
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();

    // Store stdin and child in AppState
    {
        let mut guard = state.lock().await;
        guard.server_stdin = stdin;
        guard.server_child = Some(child);
    }

    // Aggregated IPC log stream buffer (Spec 3: Throttling 32ms batching)
    if let Some(out) = stdout {
        let app_clone = app.clone();
        let state_clone = state.inner.clone();
        tokio::spawn(async move {
            let reader = BufReader::new(out);
            let mut lines = reader.lines();
            let mut batch: Vec<String> = Vec::with_capacity(64);
            let mut ticker = tokio::time::interval(tokio::time::Duration::from_millis(32));

            loop {
                tokio::select! {
                    res = lines.next_line() => {
                        match res {
                            Ok(Some(line)) => {
                                if line.contains("Done (") {
                                    {
                                        let mut guard = state_clone.lock().await;
                                        guard.server_status = ServerStatus::Online;
                                    }
                                    let _ = app_clone.emit("server-status", ServerStatus::Online);
                                }
                                // Detect e4mc public domain allocation from server log output
                                if let Some(idx) = line.find("Domain assigned: ") {
                                    let domain = line[idx + "Domain assigned: ".len()..].trim();
                                    if !domain.is_empty() {
                                        {
                                            let mut guard = state_clone.lock().await;
                                            guard.tunnel_url = Some(domain.to_string());
                                        }
                                        let _ = app_clone.emit("tunnel-status", domain.to_string());
                                    }
                                }
                                batch.push(line);
                                if batch.len() >= 100 {
                                    let _ = app_clone.emit("server-log-batch", &batch);
                                    batch.clear();
                                }
                            }
                            Ok(None) => {
                                if !batch.is_empty() {
                                    let _ = app_clone.emit("server-log-batch", &batch);
                                    batch.clear();
                                }
                                break;
                            }
                            Err(_) => break,
                        }
                    }
                    _ = ticker.tick() => {
                        if !batch.is_empty() {
                            let _ = app_clone.emit("server-log-batch", &batch);
                            batch.clear();
                        }
                    }
                }
            }
        });
    }

    // Read stderr lines with batch aggregation
    if let Some(err) = stderr {
        let app_clone = app.clone();
        tokio::spawn(async move {
            let reader = BufReader::new(err);
            let mut lines = reader.lines();
            let mut batch: Vec<String> = Vec::with_capacity(32);
            let mut ticker = tokio::time::interval(tokio::time::Duration::from_millis(40));

            loop {
                tokio::select! {
                    res = lines.next_line() => {
                        match res {
                            Ok(Some(line)) => {
                                batch.push(format!("[STDERR] {}", line));
                                if batch.len() >= 50 {
                                    let _ = app_clone.emit("server-log-batch", &batch);
                                    batch.clear();
                                }
                            }
                            Ok(None) => {
                                if !batch.is_empty() {
                                    let _ = app_clone.emit("server-log-batch", &batch);
                                    batch.clear();
                                }
                                break;
                            }
                            Err(_) => break,
                        }
                    }
                    _ = ticker.tick() => {
                        if !batch.is_empty() {
                            let _ = app_clone.emit("server-log-batch", &batch);
                            batch.clear();
                        }
                    }
                }
            }
        });
    }

    // Monitor server process exit to ensure status resets to Stopped
    {
        let app_clone = app.clone();
        let state_clone = state.inner.clone();
        tokio::spawn(async move {
            let mut poll_interval = tokio::time::interval(tokio::time::Duration::from_millis(500));
            loop {
                poll_interval.tick().await;
                let mut guard = state_clone.lock().await;
                if let Some(ref mut child) = guard.server_child {
                    match child.try_wait() {
                        Ok(Some(status)) => {
                            guard.server_status = ServerStatus::Stopped;
                            guard.server_stdin = None;
                            guard.server_child = None;
                            let _ = app_clone.emit("server-log", format!("[Conduit] Server process exited with status: {}", status));
                            let _ = app_clone.emit("server-status", ServerStatus::Stopped);
                            break;
                        }
                        Ok(None) => {}
                        Err(e) => {
                            guard.server_status = ServerStatus::Stopped;
                            guard.server_stdin = None;
                            guard.server_child = None;
                            let _ = app_clone.emit("server-log", format!("[Conduit] Error checking server process: {}", e));
                            let _ = app_clone.emit("server-status", ServerStatus::Stopped);
                            break;
                        }
                    }
                } else {
                    break;
                }
            }
        });
    }

    Ok(())
}

// -----------------------------------------------------------------------------
// 2. Stop Server Command
// -----------------------------------------------------------------------------

#[tauri::command]
pub async fn stop_server(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    stop_server_internal(&app, (*state).clone()).await
}

pub async fn stop_server_internal(app: &AppHandle, state: AppState) -> Result<(), String> {
    // 1. Take server_stdin from state and write "stop\n"
    let stdin_opt = {
        let mut guard = state.lock().await;
        guard.server_stdin.take()
    };

    if let Some(mut stdin) = stdin_opt {
        let _ = stdin.write_all(b"stop\n").await;
        let _ = stdin.flush().await;
    }

    // 2. Take server_child from state and wait up to 15s
    let server_child = {
        let mut guard = state.lock().await;
        guard.server_child.take()
    };

    if let Some(mut child) = server_child {
        let wait_result =
            tokio::time::timeout(tokio::time::Duration::from_secs(15), child.wait()).await;
        if wait_result.is_err() {
            let _ = child.kill().await;
        }
    }

    // 3. Stop Built-in Conduit Network Tunnel
    let _ = crate::network::conduit_stop_hosting().await;

    let tunnel_child = {
        let mut guard = state.lock().await;
        guard.tunnel_child.take()
    };

    if let Some(mut child) = tunnel_child {
        let _ = child.kill().await;
    }

    // 4. Update status and emit
    {
        let mut guard = state.lock().await;
        guard.server_status = ServerStatus::Stopped;
        guard.tunnel_url = None;
        guard.tunnel_claim_url = None;
    }

    let _ = app.emit("server-status", ServerStatus::Stopped);
    Ok(())
}

// -----------------------------------------------------------------------------
// 3. Client Launcher & Local Direct-Join
// -----------------------------------------------------------------------------

#[tauri::command]
pub async fn launch_client(
    app: AppHandle,
    state: State<'_, AppState>,
    instance_dir: String,
    ram_gb: u32,
    mode: String,
) -> Result<(), String> {
    let mut instance_path = PathBuf::from(&instance_dir);
    if instance_path.is_relative() {
        if let Ok(exe_dir) = std::env::current_exe() {
            if let Some(parent) = exe_dir.parent() {
                let candidate = parent.join(&instance_dir);
                if candidate.exists() {
                    instance_path = candidate;
                }
            }
        }
    }
    let mc_dir = instance_path.join(".minecraft");

    // Fetch auth profile
    let profile = {
        let guard = state.lock().await;
        guard.auth_profile.clone()
    }
    .unwrap_or(crate::state::MinecraftProfile {
        uuid: "00000000-0000-0000-0000-000000000000".into(),
        username: "Player".into(),
        access_token: "dummy_token".into(),
    });

    let java_bin = resolve_java_path(&app);
    let libraries_dir = mc_dir.join("libraries");

    // Dynamically locate the latest NeoForge version installed
    let mut neoforge_ver_json = None;
    let mut installed_neoforge_ver = "21.1.265".to_string();
    let mut fml_ver = "4.0.45".to_string();
    let mut neoform_ver = "20240808.144430".to_string();

    let versions_parent = mc_dir.join("versions");
    if versions_parent.exists() {
        if let Ok(entries) = fs::read_dir(&versions_parent) {
            let mut detected_versions: Vec<PathBuf> = entries
                .flatten()
                .map(|e| e.path())
                .filter(|p| {
                    p.is_dir()
                        && p.file_name()
                            .and_then(|n| n.to_str())
                            .map(|n| n.starts_with("neoforge-"))
                            .unwrap_or(false)
                })
                .collect();
            detected_versions.sort();
            if let Some(latest_ver_dir) = detected_versions.last() {
                let dir_name = latest_ver_dir.file_name().unwrap().to_string_lossy();
                let json_path = latest_ver_dir.join(format!("{}.json", dir_name));
                if json_path.exists() {
                    installed_neoforge_ver = dir_name.trim_start_matches("neoforge-").to_string();
                    neoforge_ver_json = Some(json_path);
                }
            }
        }
    }

    let v1211_ver_json = mc_dir.join("versions").join("1.21.1").join("1.21.1.json");

    let mut classpath_entries = Vec::new();
    let mut added_paths = std::collections::HashSet::new();

    let parse_libs = |json_path: &Path, list: &mut Vec<PathBuf>, set: &mut std::collections::HashSet<PathBuf>, base_dir: &Path| {
        if let Ok(bytes) = fs::read(json_path) {
            if let Ok(val) = serde_json::from_slice::<serde_json::Value>(&bytes) {
                if let Some(libs) = val.get("libraries").and_then(|l| l.as_array()) {
                    for lib in libs {
                        // Check rules for OS compatibility (e.g. only windows natives on Windows)
                        let mut allowed = true;
                        if let Some(rules) = lib.get("rules").and_then(|r| r.as_array()) {
                            allowed = false;
                            for r in rules {
                                if r.get("action").and_then(|a| a.as_str()) == Some("allow") {
                                    if let Some(os) = r.get("os") {
                                        if os.get("name").and_then(|n| n.as_str()) == Some("windows") {
                                            allowed = true;
                                        }
                                    } else {
                                        allowed = true;
                                    }
                                }
                            }
                        }

                        if allowed {
                            if let Some(path_str) = lib.get("downloads").and_then(|d| d.get("artifact")).and_then(|a| a.get("path")).and_then(|p| p.as_str()) {
                                let p = base_dir.join("libraries").join(path_str);
                                if p.exists() && set.insert(p.clone()) {
                                    list.push(p);
                                }
                            }
                        }
                    }
                }
            }
        }
    };

    if let Some(ref nf_json) = neoforge_ver_json {
        // Read fml arguments dynamically if available
        if let Ok(bytes) = fs::read(nf_json) {
            if let Ok(val) = serde_json::from_slice::<serde_json::Value>(&bytes) {
                if let Some(args_arr) = val.get("arguments").and_then(|a| a.get("game")).and_then(|g| g.as_array()) {
                    for i in 0..args_arr.len() {
                        if args_arr[i].as_str() == Some("--fml.fmlVersion") && i + 1 < args_arr.len() {
                            if let Some(v) = args_arr[i + 1].as_str() {
                                fml_ver = v.to_string();
                            }
                        }
                        if args_arr[i].as_str() == Some("--fml.neoFormVersion") && i + 1 < args_arr.len() {
                            if let Some(v) = args_arr[i + 1].as_str() {
                                neoform_ver = v.to_string();
                            }
                        }
                    }
                }
            }
        }
        parse_libs(nf_json, &mut classpath_entries, &mut added_paths, &mc_dir);
    }
    parse_libs(&v1211_ver_json, &mut classpath_entries, &mut added_paths, &mc_dir);

    // Fallback if version json is not found yet: collect directory
    if classpath_entries.is_empty() && libraries_dir.exists() {
        collect_jars(&libraries_dir, &mut classpath_entries);
    }

    // Add 1.21.1 client.jar to classpath
    let client_jar = mc_dir.join("versions").join("1.21.1").join("1.21.1.jar");
    if client_jar.exists() && added_paths.insert(client_jar.clone()) {
        classpath_entries.push(client_jar);
    }

    let classpath_separator = ";"; // Windows classpath separator
    let full_classpath = classpath_entries
        .into_iter()
        .map(|p| p.to_string_lossy().to_string())
        .collect::<Vec<_>>()
        .join(classpath_separator);

    let mut client_cmd = tokio::process::Command::new(&java_bin);
    client_cmd.current_dir(&mc_dir);
    #[cfg(target_os = "windows")]
    {
        client_cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
    }

    // Memory & GC: Dynamic client heap scaling (starts low, scales on demand)
    client_cmd.arg("-Xms1G");
    client_cmd.arg(format!("-Xmx{}G", ram_gb));
    client_cmd.args([
        "-XX:+UseG1GC",
        "-XX:+ParallelRefProcEnabled",
        "-XX:+UseStringDeduplication", // Spec: Compact string storage
        "-XX:MaxGCPauseMillis=20", // Strict 20ms pause target prevents frame stutter & protects 1% lows
        "-XX:+UnlockExperimentalVMOptions",
        "-XX:+DisableExplicitGC",
        "-XX:G1ReservePercent=15",
        "-Dio.netty.allocator.type=pooled",
        "-Dio.netty.tryReflectionSetAccessible=true",
        "-Djava.net.preferIPv4Stack=true",
    ]);

    // NeoForge 1.21.1 exact bootstrap module path (only the 8 bootstrap jars!)
    let module_path = [
        libraries_dir.join("cpw").join("mods").join("bootstraplauncher").join("2.0.2").join("bootstraplauncher-2.0.2.jar"),
        libraries_dir.join("cpw").join("mods").join("securejarhandler").join("3.0.8").join("securejarhandler-3.0.8.jar"),
        libraries_dir.join("org").join("ow2").join("asm").join("asm-commons").join("9.10.1").join("asm-commons-9.10.1.jar"),
        libraries_dir.join("org").join("ow2").join("asm").join("asm-util").join("9.10.1").join("asm-util-9.10.1.jar"),
        libraries_dir.join("org").join("ow2").join("asm").join("asm-analysis").join("9.10.1").join("asm-analysis-9.10.1.jar"),
        libraries_dir.join("org").join("ow2").join("asm").join("asm-tree").join("9.10.1").join("asm-tree-9.10.1.jar"),
        libraries_dir.join("org").join("ow2").join("asm").join("asm").join("9.10.1").join("asm-9.10.1.jar"),
        libraries_dir.join("net").join("neoforged").join("JarJarFileSystems").join("0.4.1").join("JarJarFileSystems-0.4.1.jar"),
    ]
    .into_iter()
    .filter(|p| p.exists())
    .map(|p| p.to_string_lossy().to_string())
    .collect::<Vec<_>>()
    .join(";");

    client_cmd.args([
        "-Djava.net.preferIPv6Addresses=system",
        "-DignoreList=client-extra,1.21.1.jar",
        &format!("-DlibraryDirectory={}", libraries_dir.to_string_lossy().replace('\\', "/")),
        "-p",
        &module_path,
        "--add-modules",
        "ALL-MODULE-PATH",
        "--add-opens",
        "java.base/java.util.jar=cpw.mods.securejarhandler",
        "--add-opens",
        "java.base/java.lang.invoke=cpw.mods.securejarhandler",
        "--add-exports",
        "java.base/sun.security.util=cpw.mods.securejarhandler",
        "--add-exports",
        "jdk.naming.dns/com.sun.jndi.dns=java.naming",
    ]);

    // Classpath & Main
    if !full_classpath.is_empty() {
        client_cmd.arg("-cp").arg(full_classpath);
    }

    client_cmd.arg("cpw.mods.bootstraplauncher.BootstrapLauncher");

    // NeoForge game arguments dynamically resolved
    client_cmd.args([
        "--fml.neoForgeVersion",
        &installed_neoforge_ver,
        "--fml.fmlVersion",
        &fml_ver,
        "--fml.mcVersion",
        "1.21.1",
        "--fml.neoFormVersion",
        &neoform_ver,
        "--launchTarget",
        "forgeclient",
    ]);

    // Game arguments
    client_cmd.args([
        "--username",
        &profile.username,
        "--version",
        "1.21.1-neoforge",
        "--gameDir",
        &mc_dir.to_string_lossy(),
        "--assetsDir",
        &mc_dir.join("assets").to_string_lossy(),
        "--assetIndex",
        "17",
        "--uuid",
        &profile.uuid,
        "--accessToken",
        &profile.access_token,
        "--userType",
        "msa",
        "--versionType",
        "release",
        "--width",
        "1280",
        "--height",
        "720",
    ]);

    // Direct Join Server Mode or Direct Singleplayer World Mode
    if mode == "join_local_server" {
        client_cmd.args(["--quickPlayMultiplayer", "127.0.0.1:25565"]);
    } else if let Some(world) = mode.strip_prefix("singleplayer:") {
        if !world.trim().is_empty() {
            client_cmd.args(["--quickPlaySingleplayer", world.trim()]);
        }
    }

    // Log client output to file so it never blocks or fails silently
    let client_log_path = mc_dir.join("conduit_client.log");
    if let Ok(file) = File::create(&client_log_path) {
        let std_file = file.try_clone().unwrap_or(file);
        client_cmd.stdout(Stdio::from(std_file));
    } else {
        client_cmd.stdout(Stdio::null());
    }

    if let Ok(file) = File::options().append(true).open(&client_log_path) {
        client_cmd.stderr(Stdio::from(file));
    } else {
        client_cmd.stderr(Stdio::null());
    }

    let mut client_child = client_cmd
        .spawn()
        .map_err(|e| format!("Failed to spawn Minecraft client: {e}"))?;

    // Update client status
    {
        let mut guard = state.lock().await;
        guard.client_status = ClientStatus::Playing;
    }
    let _ = app.emit("client-status", ClientStatus::Playing);

    // Detached tokio task to wait for client exit
    let app_clone = app.clone();
    let state_clone = state.inner().clone();
    tokio::spawn(async move {
        let _ = client_child.wait().await;
        {
            let mut guard = state_clone.lock().await;
            guard.client_status = ClientStatus::Stopped;
        }
        let _ = app_clone.emit("client-status", ClientStatus::Stopped);
    });

    Ok(())
}

fn collect_jars(dir: &Path, list: &mut Vec<PathBuf>) {
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_dir() {
                collect_jars(&p, list);
            } else if p.extension().and_then(|e| e.to_str()) == Some("jar") {
                list.push(p);
            }
        }
    }
}

#[tauri::command]
pub async fn join_local_server(
    app: AppHandle,
    state: State<'_, AppState>,
    instance_dir: String,
    ram_gb: u32,
) -> Result<(), String> {
    launch_client(app, state, instance_dir, ram_gb, "join_local_server".into()).await
}

#[tauri::command]
pub async fn open_instance_folder(_app: AppHandle, instance_dir: String) -> Result<(), String> {
    let mut path = PathBuf::from(&instance_dir);
    if path.is_relative() {
        if let Ok(exe_dir) = std::env::current_exe() {
            if let Some(parent) = exe_dir.parent() {
                let candidate = parent.join(&instance_dir);
                if candidate.exists() {
                    path = candidate;
                }
            }
        }
    }
    
    // Default to the server mods directory when opening mods
    let server_mods = path.join("server").join("mods");
    let target = if server_mods.exists() {
        server_mods
    } else {
        path
    };

    let _ = fs::create_dir_all(&target);
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer")
            .arg(target)
            .spawn()
            .map_err(|e| format!("Failed to open folder: {e}"))?;
    }
    Ok(())
}

#[tauri::command]
pub async fn send_server_command(
    state: State<'_, AppState>,
    command: String,
) -> Result<(), String> {
    let mut stdin_opt = {
        let mut guard = state.lock().await;
        guard.server_stdin.take()
    };

    if let Some(ref mut stdin) = stdin_opt {
        let cmd_str = format!("{}\n", command.trim());
        let res = stdin.write_all(cmd_str.as_bytes()).await;
        let _ = stdin.flush().await;

        // Restore stdin back into state
        let mut guard = state.lock().await;
        guard.server_stdin = stdin_opt;

        res.map_err(|e| format!("Failed writing command to server stdin: {e}"))
    } else {
        Err("Server stdin is not available".into())
    }
}
