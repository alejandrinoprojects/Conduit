use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicI64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, Manager};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{Mutex, RwLock};

// -----------------------------------------------------------------------------
// 1. Data Structures & Configuration
// -----------------------------------------------------------------------------

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum HostingMode {
    AlwaysOnline,
    SleepWhenEmpty,
    OnDemand,
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum GatewayStatus {
    Disabled,
    Sleeping,
    Starting,
    Online,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct GatewayConfig {
    pub mode: HostingMode,
    pub idle_timeout_minutes: u32,
    pub gateway_port: u16,
    pub backend_port: u16,
    pub motd_sleeping: String,
    pub motd_starting: String,
}

impl Default for GatewayConfig {
    fn default() -> Self {
        Self {
            mode: HostingMode::OnDemand,
            idle_timeout_minutes: 10,
            gateway_port: 25565,
            backend_port: 25566,
            motd_sleeping: "§6§lCONDUIT §8» §7Realm is §e§lSLEEPING §8(0 MB RAM)\n§a▶ Join to wake up & start the server automatically!".to_string(),
            motd_starting: "§6§lCONDUIT §8» §e§lSTARTING SERVER... §7(Initializing JVM)\n§e▶ Ready in a few seconds! Refresh to join.".to_string(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct GatewayStatePayload {
    pub status: GatewayStatus,
    pub mode: HostingMode,
    pub gateway_port: u16,
    pub backend_port: u16,
    pub active_connections: usize,
    pub memory_saved_mb: f64,
}

// -----------------------------------------------------------------------------
// 2. Global Gateway Manager
// -----------------------------------------------------------------------------

pub struct GatewayManager {
    pub config: RwLock<GatewayConfig>,
    pub status: RwLock<GatewayStatus>,
    pub active_connections: AtomicUsize,
    pub last_activity_time: AtomicI64,
    pub shutdown_sender: Mutex<Option<tokio::sync::broadcast::Sender<()>>>,
}

impl GatewayManager {
    pub fn new() -> Self {
        Self {
            config: RwLock::new(GatewayConfig::default()),
            status: RwLock::new(GatewayStatus::Disabled),
            active_connections: AtomicUsize::new(0),
            last_activity_time: AtomicI64::new(current_epoch_sec()),
            shutdown_sender: Mutex::new(None),
        }
    }
}

static GATEWAY_MANAGER: std::sync::LazyLock<Arc<GatewayManager>> =
    std::sync::LazyLock::new(|| Arc::new(GatewayManager::new()));

fn current_epoch_sec() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

// -----------------------------------------------------------------------------
// 3. Minecraft Protocol Encoding & Helpers
// -----------------------------------------------------------------------------

fn read_varint<R: Read>(reader: &mut R) -> std::io::Result<i32> {
    let mut value = 0i32;
    let mut position = 0;
    loop {
        let mut byte = [0u8; 1];
        reader.read_exact(&mut byte)?;
        let b = byte[0];
        value |= ((b & 0x7F) as i32) << position;
        if (b & 0x80) == 0 {
            break;
        }
        position += 7;
        if position >= 32 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "VarInt exceeds maximum 32-bit width",
            ));
        }
    }
    Ok(value)
}

fn write_varint<W: Write>(writer: &mut W, mut value: i32) -> std::io::Result<()> {
    loop {
        if (value & !0x7F) == 0 {
            writer.write_all(&[value as u8])?;
            return Ok(());
        }
        writer.write_all(&[((value & 0x7F) | 0x80) as u8])?;
        value = ((value as u32) >> 7) as i32;
    }
}

fn create_packet(packet_id: i32, payload: &[u8]) -> Vec<u8> {
    let mut id_buf = Vec::new();
    let _ = write_varint(&mut id_buf, packet_id);
    let total_len = (id_buf.len() + payload.len()) as i32;
    let mut packet = Vec::new();
    let _ = write_varint(&mut packet, total_len);
    packet.extend_from_slice(&id_buf);
    packet.extend_from_slice(payload);
    packet
}

fn create_string_payload(s: &str) -> Vec<u8> {
    let bytes = s.as_bytes();
    let mut buf = Vec::new();
    let _ = write_varint(&mut buf, bytes.len() as i32);
    buf.extend_from_slice(bytes);
    buf
}

// -----------------------------------------------------------------------------
// 4. Gateway Listener & Connection Lifecycle
// -----------------------------------------------------------------------------

pub async fn run_gateway_loop(
    app: AppHandle,
    instance_dir: String,
    world_name: Option<String>,
    ram_gb: u32,
) -> Result<(), String> {
    let mgr = GATEWAY_MANAGER.clone();
    let (shutdown_tx, mut shutdown_rx) = tokio::sync::broadcast::channel(1);
    {
        let mut guard = mgr.shutdown_sender.lock().await;
        *guard = Some(shutdown_tx);
    }

    let config = { mgr.config.read().await.clone() };
    let bind_addr = format!("0.0.0.0:{}", config.gateway_port);

    let listener = TcpListener::bind(&bind_addr)
        .await
        .map_err(|e| format!("Failed to bind Conduit Gateway on {bind_addr}: {e}"))?;

    {
        let mut st = mgr.status.write().await;
        *st = match config.mode {
            HostingMode::AlwaysOnline => GatewayStatus::Starting,
            HostingMode::SleepWhenEmpty => GatewayStatus::Starting,
            HostingMode::OnDemand => GatewayStatus::Sleeping,
        };
    }
    emit_gateway_state(&app, &mgr).await;

    let app_log = app.clone();
    let _ = app_log.emit(
        "server-log-batch",
        vec![format!(
            "[CONDUIT GATEWAY] Online on {bind_addr} | Mode: {:?} | Backend: 127.0.0.1:{}",
            config.mode, config.backend_port
        )],
    );

    // If configured mode is AlwaysOnline or SleepWhenEmpty, start the backend JVM immediately
    if config.mode == HostingMode::AlwaysOnline || config.mode == HostingMode::SleepWhenEmpty {
        let app_start = app.clone();
        let inst = instance_dir.clone();
        let wn = world_name.clone();
        tokio::spawn(async move {
            let state = (*app_start.state::<crate::state::AppState>()).clone();
            let _ = crate::process::start_server_internal(&app_start, state, inst, ram_gb, wn).await;
        });
    }

    // Spawn Idle Auto-Sleep Supervisor
    let app_idle = app.clone();
    let mgr_idle = mgr.clone();
    let _inst_idle = instance_dir.clone();
    let mut idle_shutdown = mgr.shutdown_sender.lock().await.as_ref().unwrap().subscribe();

    tokio::spawn(async move {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(10));
        loop {
            tokio::select! {
                _ = idle_shutdown.recv() => break,
                _ = interval.tick() => {
                    let (mode, timeout_mins) = {
                        let c = mgr_idle.config.read().await;
                        (c.mode, c.idle_timeout_minutes)
                    };

                    if mode == HostingMode::AlwaysOnline {
                        continue;
                    }

                    let cur_status = *mgr_idle.status.read().await;
                    if cur_status != GatewayStatus::Online {
                        continue;
                    }

                    let active = mgr_idle.active_connections.load(Ordering::Relaxed);
                    if active == 0 {
                        let last = mgr_idle.last_activity_time.load(Ordering::Relaxed);
                        let now = current_epoch_sec();
                        let idle_sec = now - last;
                        let threshold_sec = (timeout_mins as i64) * 60;

                        if idle_sec >= threshold_sec {
                            let _ = app_idle.emit(
                                "server-log-batch",
                                vec![format!(
                                    "[CONDUIT GATEWAY] Idle threshold reached ({timeout_mins}m with 0 players). Gracefully sleeping server container to save RAM..."
                                )],
                            );

                            // Send save-all and stop to server stdin
                            let state = (*app_idle.state::<crate::state::AppState>()).clone();
                            let _ = crate::process::stop_server_internal(&app_idle, state).await;

                            {
                                let mut st = mgr_idle.status.write().await;
                                *st = GatewayStatus::Sleeping;
                            }
                            emit_gateway_state(&app_idle, &mgr_idle).await;

                            let _ = app_idle.emit(
                                "server-log-batch",
                                vec![format!(
                                    "[CONDUIT GATEWAY] Server is now SLEEPING. Reclaimed ~{:.1} GB RAM. Gateway listening for players.",
                                    ram_gb
                                )],
                            );
                        }
                    } else {
                        mgr_idle.last_activity_time.store(current_epoch_sec(), Ordering::Relaxed);
                    }
                }
            }
        }
    });

    // Main Accept Loop
    loop {
        tokio::select! {
            _ = shutdown_rx.recv() => {
                let _ = app.emit(
                    "server-log-batch",
                    vec!["[CONDUIT GATEWAY] Gateway listener stopped.".to_string()],
                );
                break;
            }
            accept_res = listener.accept() => {
                match accept_res {
                    Ok((socket, _peer_addr)) => {
                        let app_conn = app.clone();
                        let mgr_conn = mgr.clone();
                        let inst_conn = instance_dir.clone();
                        let wn_conn = world_name.clone();

                        tokio::spawn(async move {
                            handle_client_connection(socket, app_conn, mgr_conn, inst_conn, wn_conn, ram_gb).await;
                        });
                    }
                    Err(e) => {
                        eprintln!("[Gateway] Accept error: {e}");
                        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
                    }
                }
            }
        }
    }

    {
        let mut st = mgr.status.write().await;
        *st = GatewayStatus::Disabled;
    }
    emit_gateway_state(&app, &mgr).await;

    Ok(())
}

async fn handle_client_connection(
    mut client_stream: TcpStream,
    app: AppHandle,
    mgr: Arc<GatewayManager>,
    instance_dir: String,
    world_name: Option<String>,
    ram_gb: u32,
) {
    mgr.active_connections.fetch_add(1, Ordering::Relaxed);
    mgr.last_activity_time.store(current_epoch_sec(), Ordering::Relaxed);
    emit_gateway_state(&app, &mgr).await;

    let current_status = *mgr.status.read().await;
    let config = { mgr.config.read().await.clone() };

    // 1. If Server is ONLINE: Direct transparent TCP proxy pass-through to backend port
    if current_status == GatewayStatus::Online {
        let backend_addr = format!("127.0.0.1:{}", config.backend_port);
        match TcpStream::connect(&backend_addr).await {
            Ok(mut backend_stream) => {
                let _ = tokio::io::copy_bidirectional(&mut client_stream, &mut backend_stream).await;
            }
            Err(_) => {
                // Backend might have dropped or not finished binding; fall back to protocol response
                handle_minecraft_handshake(&mut client_stream, current_status, &config, &app, &mgr, &instance_dir, &world_name, ram_gb).await;
            }
        }
    } else {
        // 2. Server is SLEEPING or STARTING: Handle Minecraft Protocol Handshake, SLP, and Wakeup
        handle_minecraft_handshake(&mut client_stream, current_status, &config, &app, &mgr, &instance_dir, &world_name, ram_gb).await;
    }

    mgr.active_connections.fetch_sub(1, Ordering::Relaxed);
    mgr.last_activity_time.store(current_epoch_sec(), Ordering::Relaxed);
    emit_gateway_state(&app, &mgr).await;
}

async fn handle_minecraft_handshake(
    stream: &mut TcpStream,
    status: GatewayStatus,
    config: &GatewayConfig,
    app: &AppHandle,
    mgr: &Arc<GatewayManager>,
    instance_dir: &str,
    world_name: &Option<String>,
    ram_gb: u32,
) {
    let mut buf = vec![0u8; 1024];

    // Read initial handshake packet
    let read_bytes = match tokio::time::timeout(tokio::time::Duration::from_secs(5), stream.read(&mut buf)).await {
        Ok(Ok(n)) if n > 0 => n,
        _ => return,
    };

    let mut cursor = std::io::Cursor::new(&buf[..read_bytes]);
    let _packet_len = match read_varint(&mut cursor) {
        Ok(l) => l,
        Err(_) => return,
    };
    let packet_id = match read_varint(&mut cursor) {
        Ok(id) => id,
        Err(_) => return,
    };

    if packet_id != 0x00 {
        return; // Not a Handshake packet
    }

    let protocol_version = read_varint(&mut cursor).unwrap_or(767);
    let str_len = read_varint(&mut cursor).unwrap_or(0);
    if str_len > 0 && (cursor.position() as usize + str_len as usize) <= read_bytes {
        cursor.set_position(cursor.position() + str_len as u64);
    }
    let _server_port = {
        let mut port_buf = [0u8; 2];
        let _ = std::io::Read::read_exact(&mut cursor, &mut port_buf);
    };
    let next_state = match read_varint(&mut cursor) {
        Ok(st) => st,
        Err(_) => return,
    };

    // -------------------------------------------------------------------------
    // A. Next State = 1: STATUS (Server List Ping / MOTD)
    // -------------------------------------------------------------------------
    if next_state == 1 {
        // Read Status Request (0x00)
        let _ = stream.read(&mut buf).await;

        let motd_text = if status == GatewayStatus::Sleeping {
            &config.motd_sleeping
        } else {
            &config.motd_starting
        };

        let status_json = serde_json::json!({
            "version": {
                "name": if status == GatewayStatus::Sleeping { "Conduit [Sleeping]" } else { "Conduit [Starting]" },
                "protocol": protocol_version
            },
            "players": {
                "max": 20,
                "online": 0,
                "sample": []
            },
            "description": {
                "text": motd_text
            }
        });

        let json_str = status_json.to_string();
        let payload = create_string_payload(&json_str);
        let resp_packet = create_packet(0x00, &payload);
        let _ = stream.write_all(&resp_packet).await;
        let _ = stream.flush().await;

        // Read Ping (0x01) and reply Pong (0x01)
        let mut ping_buf = [0u8; 32];
        if let Ok(n) = stream.read(&mut ping_buf).await {
            if n >= 9 {
                // Forward the 8-byte ping timestamp
                let mut pong_cursor = std::io::Cursor::new(&ping_buf[..n]);
                let _ = read_varint(&mut pong_cursor); // len
                let ping_id = read_varint(&mut pong_cursor).unwrap_or(-1);
                if ping_id == 0x01 {
                    let mut payload = [0u8; 8];
                    if std::io::Read::read_exact(&mut pong_cursor, &mut payload).is_ok() {
                        let pong_packet = create_packet(0x01, &payload);
                        let _ = stream.write_all(&pong_packet).await;
                        let _ = stream.flush().await;
                    }
                }
            }
        }
    }
    // -------------------------------------------------------------------------
    // B. Next State = 2: LOGIN (Player Attempting Connection)
    // -------------------------------------------------------------------------
    else if next_state == 2 {
        // Read Login Start Packet
        let mut player_name = "Player".to_string();
        if let Ok(n) = stream.read(&mut buf).await {
            if n > 2 {
                let mut p_cursor = std::io::Cursor::new(&buf[..n]);
                let _ = read_varint(&mut p_cursor); // len
                let _ = read_varint(&mut p_cursor); // id
                if let Ok(name_len) = read_varint(&mut p_cursor) {
                    if name_len > 0 && name_len <= 32 {
                        let mut name_bytes = vec![0u8; name_len as usize];
                        if std::io::Read::read_exact(&mut p_cursor, &mut name_bytes).is_ok() {
                            player_name = String::from_utf8_lossy(&name_bytes).to_string();
                        }
                    }
                }
            }
        }

        // If Sleeping -> Trigger On-Demand Wakeup!
        if status == GatewayStatus::Sleeping {
            let _ = app.emit(
                "server-log-batch",
                vec![format!(
                    "[CONDUIT GATEWAY] Incoming connection from '{player_name}'. WAKING UP server container from sleep..."
                )],
            );

            {
                let mut st = mgr.status.write().await;
                *st = GatewayStatus::Starting;
            }
            emit_gateway_state(app, mgr).await;

            // Trigger backend server launch
            let app_spawn = app.clone();
            let inst_str = instance_dir.to_string();
            let wn_clone = world_name.clone();
            let mgr_spawn = mgr.clone();

            tokio::spawn(async move {
                let state = (*app_spawn.state::<crate::state::AppState>()).clone();
                let start_res = crate::process::start_server_internal(&app_spawn, state, inst_str, ram_gb, wn_clone).await;
                if start_res.is_ok() {
                    let mut st = mgr_spawn.status.write().await;
                    *st = GatewayStatus::Online;
                    emit_gateway_state(&app_spawn, &mgr_spawn).await;
                    let _ = app_spawn.emit(
                        "server-log-batch",
                        vec!["[CONDUIT GATEWAY] Backend server is now ONLINE and ready for players!".to_string()],
                    );
                } else {
                    let mut st = mgr_spawn.status.write().await;
                    *st = GatewayStatus::Sleeping;
                    emit_gateway_state(&app_spawn, &mgr_spawn).await;
                }
            });

            // Send friendly Minecraft Login Disconnect packet with countdown instructions
            let disconnect_json = serde_json::json!({
                "text": format!(
                    "§6§lCONDUIT ON-DEMAND HOSTING\n\n§e§lWelcome, {player_name}!\n§7The realm was §e§lSLEEPING§7 to conserve memory.\n§a▶ Server JVM container is waking up now!\n\n§fPlease wait §e~15 seconds§f and reconnect.\n§8[Zero-Duplication Engine Active]"
                )
            });

            let payload = create_string_payload(&disconnect_json.to_string());
            let kick_pkt = create_packet(0x00, &payload); // Login Disconnect = Packet 0x00 in Login State
            let _ = stream.write_all(&kick_pkt).await;
            let _ = stream.flush().await;
        } else if status == GatewayStatus::Starting {
            let disconnect_json = serde_json::json!({
                "text": "§6§lCONDUIT ON-DEMAND HOSTING\n\n§e§lServer is starting up right now...\n§7Almost ready! Please refresh and join in a few seconds."
            });
            let payload = create_string_payload(&disconnect_json.to_string());
            let kick_pkt = create_packet(0x00, &payload);
            let _ = stream.write_all(&kick_pkt).await;
            let _ = stream.flush().await;
        }
    }
}

async fn emit_gateway_state(app: &AppHandle, mgr: &Arc<GatewayManager>) {
    let status = *mgr.status.read().await;
    let config = { mgr.config.read().await.clone() };
    let active_connections = mgr.active_connections.load(Ordering::Relaxed);

    let memory_saved_mb = if status == GatewayStatus::Sleeping {
        4096.0 // Approximately 4 GB saved when sleeping
    } else {
        0.0
    };

    let payload = GatewayStatePayload {
        status,
        mode: config.mode,
        gateway_port: config.gateway_port,
        backend_port: config.backend_port,
        active_connections,
        memory_saved_mb,
    };

    let _ = app.emit("gateway-state-changed", payload);
}

// -----------------------------------------------------------------------------
// 5. Tauri IPC Commands
// -----------------------------------------------------------------------------

#[tauri::command]
pub async fn get_gateway_config(instance_dir: String) -> Result<GatewayConfig, String> {
    let cfg_path = PathBuf::from(&instance_dir)
        .join("server")
        .join("gateway_config.json");
    if cfg_path.exists() {
        if let Ok(data) = std::fs::read_to_string(&cfg_path) {
            if let Ok(cfg) = serde_json::from_str::<GatewayConfig>(&data) {
                let mut guard = GATEWAY_MANAGER.config.write().await;
                *guard = cfg.clone();
                return Ok(cfg);
            }
        }
    }

    let guard = GATEWAY_MANAGER.config.read().await;
    Ok(guard.clone())
}

#[tauri::command]
pub async fn save_gateway_config(
    instance_dir: String,
    config: GatewayConfig,
) -> Result<(), String> {
    let srv_dir = PathBuf::from(&instance_dir).join("server");
    let _ = std::fs::create_dir_all(&srv_dir);
    let cfg_path = srv_dir.join("gateway_config.json");

    let json_str = serde_json::to_string_pretty(&config).map_err(|e| e.to_string())?;
    std::fs::write(&cfg_path, json_str).map_err(|e| e.to_string())?;

    let mut guard = GATEWAY_MANAGER.config.write().await;
    *guard = config;
    Ok(())
}

#[tauri::command]
pub async fn start_gateway(
    app: AppHandle,
    instance_dir: String,
    world_name: Option<String>,
    ram_gb: u32,
) -> Result<(), String> {
    tokio::spawn(async move {
        let _ = run_gateway_loop(app, instance_dir, world_name, ram_gb).await;
    });
    Ok(())
}

#[tauri::command]
pub async fn stop_gateway() -> Result<(), String> {
    let mut sender_opt = GATEWAY_MANAGER.shutdown_sender.lock().await;
    if let Some(tx) = sender_opt.take() {
        let _ = tx.send(());
    }
    let mut st = GATEWAY_MANAGER.status.write().await;
    *st = GatewayStatus::Disabled;
    Ok(())
}

#[tauri::command]
pub async fn get_gateway_state() -> Result<GatewayStatePayload, String> {
    let mgr = &GATEWAY_MANAGER;
    let status = *mgr.status.read().await;
    let config = { mgr.config.read().await.clone() };
    let active_connections = mgr.active_connections.load(Ordering::Relaxed);

    let memory_saved_mb = if status == GatewayStatus::Sleeping {
        4096.0
    } else {
        0.0
    };

    Ok(GatewayStatePayload {
        status,
        mode: config.mode,
        gateway_port: config.gateway_port,
        backend_port: config.backend_port,
        active_connections,
        memory_saved_mb,
    })
}
