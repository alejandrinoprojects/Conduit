use crate::launcher::resolve_java_path;
use crate::mods::sync_server_mods;
use crate::state::{AppState, ClientStatus, ServerStatus};
use regex::Regex;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

// -----------------------------------------------------------------------------
// Playit.gg CLI Provisioner
// -----------------------------------------------------------------------------

async fn ensure_playit_binary(app: &AppHandle) -> Result<PathBuf, String> {
    let app_data = app.path().app_data_dir().unwrap_or_else(|_| PathBuf::from("."));
    let bin_dir = app_data.join("bin");
    fs::create_dir_all(&bin_dir).map_err(|e| format!("Failed creating bin directory: {e}"))?;

    let playit_path = bin_dir.join("playit-cli.exe");
    if playit_path.exists() {
        return Ok(playit_path);
    }

    let download_url = "https://github.com/playit-cloud/playit-agent/releases/latest/download/playit-windows-x86_64.exe";
    let client = reqwest::Client::new();
    let resp = client
        .get(download_url)
        .send()
        .await
        .map_err(|e| format!("Failed downloading playit-cli: {e}"))?;

    let bytes = resp
        .bytes()
        .await
        .map_err(|e| format!("Failed reading playit-cli bytes: {e}"))?;

    let mut file = File::create(&playit_path)
        .map_err(|e| format!("Failed creating playit-cli file: {e}"))?;
    file.write_all(&bytes)
        .map_err(|e| format!("Failed writing playit-cli binary: {e}"))?;

    Ok(playit_path)
}

// -----------------------------------------------------------------------------
// 1. Headless Server & Tunnel Supervisor
// -----------------------------------------------------------------------------

#[tauri::command]
pub async fn start_server(
    app: AppHandle,
    state: State<'_, AppState>,
    instance_dir: String,
    ram_gb: u32,
) -> Result<(), String> {
    let instance_path = PathBuf::from(&instance_dir);

    // 1. First sync mods so server/mods/ is up to date
    sync_server_mods(&instance_path)?;

    // 2. Set server status to Starting
    {
        let mut guard = state.lock().await;
        if guard.server_status != ServerStatus::Stopped {
            return Err("Server is already running or starting".into());
        }
        guard.server_status = ServerStatus::Starting;
    }
    let _ = app.emit("server-status", ServerStatus::Starting);

    // 3. Ensure and spawn Playit Tunnel
    let playit_bin = ensure_playit_binary(&app).await?;
    let mut playit_cmd = tokio::process::Command::new(&playit_bin);
    playit_cmd
        .arg("run")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut playit_child = playit_cmd
        .spawn()
        .map_err(|e| format!("Failed to spawn playit-cli: {e}"))?;

    let playit_stdout = playit_child.stdout.take();

    // Store tunnel child
    {
        let mut guard = state.lock().await;
        guard.tunnel_child = Some(playit_child);
    }

    // Spawn tunnel reader task
    if let Some(stdout) = playit_stdout {
        let app_clone = app.clone();
        let state_clone = state.inner().clone();
        tokio::spawn(async move {
            let reader = BufReader::new(stdout);
            let mut lines = reader.lines();

            let claim_regex = Regex::new(r"https://playit\.gg/claim/([a-zA-Z0-9]+)").unwrap();
            let tunnel_regex =
                Regex::new(r"([a-zA-Z0-9-]+\.(?:gl\.joinmc\.link|ply\.gg)(?::\d+)?)").unwrap();

            while let Ok(Some(line)) = lines.next_line().await {
                if let Some(caps) = claim_regex.captures(&line) {
                    if let Some(m) = caps.get(0) {
                        let claim_url = m.as_str().to_string();
                        {
                            let mut guard = state_clone.lock().await;
                            guard.tunnel_claim_url = Some(claim_url.clone());
                        }
                        let _ = app_clone.emit("tunnel-claim", claim_url);
                    }
                }

                if let Some(caps) = tunnel_regex.captures(&line) {
                    if let Some(m) = caps.get(1) {
                        let tunnel_addr = m.as_str().to_string();
                        {
                            let mut guard = state_clone.lock().await;
                            guard.tunnel_url = Some(tunnel_addr.clone());
                        }
                        let _ = app_clone.emit("tunnel-status", tunnel_addr);
                    }
                }
            }
        });
    }

    // 4. Locate NeoForge server args file
    let server_dir = instance_path.join("server");
    let libraries_dir = server_dir.join("libraries");

    // Search for @libraries/net/neoforged/neoforge/.../win_args.txt
    let mut args_file_arg = None;
    if libraries_dir.exists() {
        let neoforge_libs_dir = libraries_dir.join("net").join("neoforged").join("neoforge");
        if neoforge_libs_dir.exists() {
            if let Ok(entries) = fs::read_dir(&neoforge_libs_dir) {
                for entry in entries.flatten() {
                    let win_args = entry.path().join("win_args.txt");
                    let unix_args = entry.path().join("unix_args.txt");
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

    // Memory & GC
    server_cmd.arg(format!("-Xms{}G", ram_gb));
    server_cmd.arg(format!("-Xmx{}G", ram_gb));
    server_cmd.args([
        "-XX:+UseG1GC",
        "-XX:+ParallelRefProcEnabled",
        "-XX:MaxGCPauseMillis=200",
        "-XX:+UnlockExperimentalVMOptions",
        "-XX:+DisableExplicitGC",
        "-XX:+AlwaysPreTouch",
    ]);

    // NeoForge args
    if let Some(arg_file) = args_file_arg {
        server_cmd.arg(arg_file);
    }
    server_cmd.arg("--nogui");

    server_cmd.stdin(Stdio::piped());
    server_cmd.stdout(Stdio::piped());
    server_cmd.stderr(Stdio::piped());

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

    // Read stdout lines, stream logs and detect "Done ("
    if let Some(out) = stdout {
        let app_clone = app.clone();
        let state_clone = state.inner().clone();
        tokio::spawn(async move {
            let reader = BufReader::new(out);
            let mut lines = reader.lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let _ = app_clone.emit("server-log", &line);

                if line.contains("Done (") {
                    {
                        let mut guard = state_clone.lock().await;
                        guard.server_status = ServerStatus::Online;
                    }
                    let _ = app_clone.emit("server-status", ServerStatus::Online);
                }
            }
        });
    }

    // Read stderr lines
    if let Some(err) = stderr {
        let app_clone = app.clone();
        tokio::spawn(async move {
            let reader = BufReader::new(err);
            let mut lines = reader.lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let _ = app_clone.emit("server-log", format!("[STDERR] {}", line));
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

    // 3. Take tunnel_child from state and kill
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
    let instance_path = PathBuf::from(&instance_dir);
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

    // Build classpath
    let libraries_dir = mc_dir.join("libraries");
    let mut classpath_entries = Vec::new();

    // Collect library jars
    if libraries_dir.exists() {
        collect_jars(&libraries_dir, &mut classpath_entries);
    }

    // Append client jar
    let client_jar = mc_dir.join("versions").join("1.21.1").join("1.21.1.jar");
    if client_jar.exists() {
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

    // Memory & GC
    client_cmd.arg(format!("-Xms{}G", ram_gb));
    client_cmd.arg(format!("-Xmx{}G", ram_gb));
    client_cmd.args([
        "-XX:+UseG1GC",
        "-XX:+ParallelRefProcEnabled",
        "-XX:MaxGCPauseMillis=200",
        "-XX:+UnlockExperimentalVMOptions",
        "-XX:+DisableExplicitGC",
        "-XX:+AlwaysPreTouch",
    ]);

    // Classpath & Main
    if !full_classpath.is_empty() {
        client_cmd.arg("-cp").arg(full_classpath);
    }

    client_cmd.arg("net.minecraft.client.main.Main");

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
    ]);

    // Direct Join Server Mode
    if mode == "join_local_server" {
        client_cmd.args(["--quickPlayMultiplayer", "127.0.0.1:25565"]);
    }

    client_cmd.stdout(Stdio::null());
    client_cmd.stderr(Stdio::null());

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
