//! Native Minecraft LAN Discovery Multicast Protocol (224.0.2.60:4445) Emulation.
//!
//! Provides:
//! - Standard Minecraft LAN beacon formatting: `[MOTD]{realm_name}[/MOTD][AD]{local_port}[/AD]`
//! - Strict parsing and validation of Minecraft LAN discovery datagrams
//! - Multicast and loopback beacon transmission (`224.0.2.60:4445`, `127.0.0.1:4445`, `255.255.255.255:4445`)
//! - RAII `LanBeaconHandle` ensuring background broadcast tasks are cleanly terminated on drop
//! - Local network game listener collecting discovered peers

use crate::network::relay::DiscoveredPeer;
use std::collections::HashSet;
use std::net::{Ipv4Addr, SocketAddr};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::net::UdpSocket;
use tokio::task::JoinHandle;

#[derive(Debug)]
pub enum LanError {
    Io(std::io::Error),
    InvalidPacket(String),
    Timeout,
}

impl std::fmt::Display for LanError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "LAN discovery I/O error: {}", e),
            Self::InvalidPacket(s) => write!(f, "Invalid LAN beacon packet: {}", s),
            Self::Timeout => write!(f, "LAN discovery listener timed out"),
        }
    }
}

impl std::error::Error for LanError {}

impl From<std::io::Error> for LanError {
    fn from(err: std::io::Error) -> Self {
        Self::Io(err)
    }
}

/// Official Minecraft Java Edition LAN Discovery multicast protocol codecs and constants.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MinecraftLanBeacon;

impl MinecraftLanBeacon {
    pub const MULTICAST_GROUP: &'static str = "224.0.2.60";
    pub const MULTICAST_PORT: u16 = 4445;

    /// Formats a standard Minecraft LAN discovery beacon string.
    pub fn format(motd: &str, port: u16) -> String {
        format!("[MOTD]{}[/MOTD][AD]{}[/AD]", motd, port)
    }

    /// Parses a Minecraft LAN discovery beacon string into `(MOTD, port)`.
    pub fn parse(packet: &str) -> Option<(String, u16)> {
        let motd_tag_open = "[MOTD]";
        let motd_tag_close = "[/MOTD]";
        let ad_tag_open = "[AD]";
        let ad_tag_close = "[/AD]";

        let motd_open_idx = packet.find(motd_tag_open)?;
        let motd_start = motd_open_idx + motd_tag_open.len();
        let motd_close_idx = packet.find(motd_tag_close)?;

        if motd_close_idx < motd_start {
            return None;
        }

        let ad_open_idx = packet.find(ad_tag_open)?;
        let ad_start = ad_open_idx + ad_tag_open.len();
        let ad_close_idx = packet.find(ad_tag_close)?;

        if ad_close_idx < ad_start {
            return None;
        }

        // Tags must not overlap illegally
        if ad_open_idx < motd_close_idx {
            return None;
        }

        let motd = packet[motd_start..motd_close_idx].to_string();
        let port = packet[ad_start..ad_close_idx].trim().parse::<u16>().ok()?;

        if port == 0 {
            return None;
        }

        Some((motd, port))
    }
}

/// RAII handle managing active LAN beacon broadcast loop. Cleanly shuts down on drop.
pub struct LanBeaconHandle {
    is_running: Arc<AtomicBool>,
    task: Option<JoinHandle<()>>,
}

impl LanBeaconHandle {
    pub fn new(is_running: Arc<AtomicBool>, task: JoinHandle<()>) -> Self {
        Self {
            is_running,
            task: Some(task),
        }
    }

    pub fn stop(&mut self) {
        self.is_running.store(false, Ordering::SeqCst);
        if let Some(task) = self.task.take() {
            task.abort();
        }
    }

    pub fn is_active(&self) -> bool {
        self.is_running.load(Ordering::Relaxed)
    }
}

impl Drop for LanBeaconHandle {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Starts periodic Minecraft LAN multicast beacon broadcasting.
pub fn start_lan_broadcast(realm_name: String, local_port: u16) -> LanBeaconHandle {
    let is_running = Arc::new(AtomicBool::new(true));
    let running_clone = Arc::clone(&is_running);

    let beacon_text = MinecraftLanBeacon::format(&realm_name, local_port);
    let beacon_payload = beacon_text.into_bytes();

    let task = tokio::spawn(async move {
        let socket = match UdpSocket::bind("0.0.0.0:0").await {
            Ok(s) => s,
            Err(_) => return,
        };
        let _ = socket.set_broadcast(true);

        let targets: [SocketAddr; 3] = [
            format!("{}:{}", MinecraftLanBeacon::MULTICAST_GROUP, MinecraftLanBeacon::MULTICAST_PORT)
                .parse()
                .unwrap(),
            format!("127.0.0.1:{}", MinecraftLanBeacon::MULTICAST_PORT)
                .parse()
                .unwrap(),
            format!("255.255.255.255:{}", MinecraftLanBeacon::MULTICAST_PORT)
                .parse()
                .unwrap(),
        ];

        while running_clone.load(Ordering::Relaxed) {
            for target in &targets {
                let _ = socket.send_to(&beacon_payload, target).await;
            }
            tokio::time::sleep(Duration::from_millis(1500)).await;
        }
    });

    LanBeaconHandle::new(is_running, task)
}

/// Listens for active Minecraft LAN games across the local network for the specified timeout.
pub async fn listen_for_lan_games(timeout: Duration) -> Result<Vec<DiscoveredPeer>, LanError> {
    // Attempt binding standard multicast port 4445; fallback to ephemeral if occupied
    let socket = match UdpSocket::bind(format!("0.0.0.0:{}", MinecraftLanBeacon::MULTICAST_PORT)).await {
        Ok(s) => s,
        Err(_) => UdpSocket::bind("0.0.0.0:0").await?,
    };

    let _ = socket.join_multicast_v4(
        Ipv4Addr::new(224, 0, 2, 60),
        Ipv4Addr::new(0, 0, 0, 0),
    );

    let mut discovered = Vec::new();
    let mut seen_keys = HashSet::new();

    let deadline = Instant::now() + timeout;
    let mut buf = [0u8; 1024];

    while Instant::now() < deadline {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            break;
        }

        let recv_res = tokio::time::timeout(remaining, socket.recv_from(&mut buf)).await;
        match recv_res {
            Ok(Ok((len, peer_addr))) => {
                if let Ok(msg_str) = std::str::from_utf8(&buf[..len]) {
                    if let Some((motd, port)) = MinecraftLanBeacon::parse(msg_str) {
                        let key = (motd.clone(), port);
                        if !seen_keys.contains(&key) {
                            seen_keys.insert(key);

                            let now_ms = SystemTime::now()
                                .duration_since(UNIX_EPOCH)
                                .unwrap_or_default()
                                .as_millis() as u64;

                            discovered.push(DiscoveredPeer {
                                room_code: "LAN".to_string(),
                                realm_name: motd,
                                host_pubkey: "".to_string(),
                                endpoint: format!("{}:{}", peer_addr.ip(), port),
                                last_seen_ms: now_ms,
                            });
                        }
                    }
                }
            }
            Ok(Err(_)) => break,
            Err(_) => break, // Timeout reached
        }
    }

    Ok(discovered)
}
