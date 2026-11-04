pub mod datapath;
pub mod identity;
pub mod lan;
pub mod punch;
pub mod relay;
pub mod tunnel;
pub mod wireguard;

pub use datapath::{DatapathError, DatapathHandle, UserspaceDatapath};
pub use identity::{ConduitIdentity, IdentityManager};
pub use lan::{
    listen_for_lan_games, start_lan_broadcast, LanBeaconHandle, LanError, MinecraftLanBeacon,
};
pub use punch::{
    discover_endpoints, discover_public_endpoint, execute_hole_punch, PunchCandidate, PunchError,
};
pub use relay::{
    generate_room_code, parse_short_room_code, DiscoveredPeer, FallbackTransport, LanDiscovery,
    RelayClient, RelayError, RelayFrame, RoomTicket, TicketError,
};
pub use wireguard::{DecapsulatedPacket, WireGuardError, WireGuardTunnel};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::sync::Arc;
use tauri::command;
use tokio::sync::{Mutex, RwLock};
use tunnel::{GuestTunnel, HostTunnel};
use x25519_dalek::PublicKey;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoomHostingInfo {
    pub room_code: String,
    pub host_pubkey: String,
    pub local_udp_port: u16,
    pub public_endpoint: Option<String>,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkStatusPayload {
    pub state: String, // "idle" | "hosting" | "connected"
    pub identity: ConduitIdentity,
    pub room_code: Option<String>,
    pub endpoint: Option<String>,
    #[serde(default)]
    pub public_ip: Option<String>,
    pub local_proxy_port: Option<u16>,
    pub is_direct: bool,
    pub bytes_sent: u64,
    pub bytes_received: u64,
    pub latency_ms: u32,
}

pub struct ConduitNetworkManager {
    pub identity: RwLock<IdentityManager>,
    pub active_host: Mutex<Option<HostTunnel>>,
    pub active_guest: Mutex<Option<GuestTunnel>>,
    pub current_room_code: RwLock<Option<String>>,
    pub current_public_endpoint: RwLock<Option<String>>,
    pub lan: LanDiscovery,
}

impl ConduitNetworkManager {
    pub fn new() -> Self {
        let ident = IdentityManager::load_or_create(None);
        Self {
            identity: RwLock::new(ident),
            active_host: Mutex::new(None),
            active_guest: Mutex::new(None),
            current_room_code: RwLock::new(None),
            current_public_endpoint: RwLock::new(None),
            lan: LanDiscovery::new(),
        }
    }
}

pub static NETWORK_MANAGER: std::sync::LazyLock<Arc<ConduitNetworkManager>> =
    std::sync::LazyLock::new(|| Arc::new(ConduitNetworkManager::new()));

// -----------------------------------------------------------------------------
// Tauri IPC Commands
// -----------------------------------------------------------------------------

#[command]
pub async fn conduit_get_identity() -> Result<ConduitIdentity, String> {
    let mgr = Arc::clone(&NETWORK_MANAGER);
    let guard = mgr.identity.read().await;
    Ok(guard.identity.clone())
}

#[command]
pub async fn conduit_host_realm(
    realm_name: String,
    local_port: Option<u16>,
) -> Result<RoomHostingInfo, String> {
    let mgr = Arc::clone(&NETWORK_MANAGER);
    let port = local_port.unwrap_or(25565);

    // Stop any existing host session
    {
        let mut host_lock = mgr.active_host.lock().await;
        if let Some(existing) = host_lock.take() {
            existing.stop();
        }
    }
    mgr.lan.stop();

    let (secret_copy, pubkey_b64) = {
        let ident_guard = mgr.identity.read().await;
        let sec = ident_guard.secret();
        let sec_bytes = sec.to_bytes();
        let pub_b64 = ident_guard.identity.public_key.clone();
        (x25519_dalek::StaticSecret::from(sec_bytes), pub_b64)
    };

    let (host_tunnel, udp_socket, _session) =
        HostTunnel::start(port, secret_copy, None).await?;

    let local_udp_port = host_tunnel.udp_port;

    // Discover external STUN endpoint in background
    let public_endpoint = discover_public_endpoint(&udp_socket)
        .await
        .map(|addr| addr.to_string());

    let room_code = generate_room_code(Some(&realm_name));

    // Save state
    *mgr.active_host.lock().await = Some(host_tunnel);
    *mgr.current_room_code.write().await = Some(room_code.clone());
    *mgr.current_public_endpoint.write().await = public_endpoint.clone();

    // Start local LAN broadcast for zero-config Wi-Fi discovery
    mgr.lan
        .start_broadcasting(
            room_code.clone(),
            realm_name,
            pubkey_b64.clone(),
            local_udp_port,
        )
        .await;

    Ok(RoomHostingInfo {
        room_code,
        host_pubkey: pubkey_b64,
        local_udp_port,
        public_endpoint,
        status: "Hosting Active".to_string(),
    })
}

#[command]
pub async fn conduit_stop_hosting() -> Result<bool, String> {
    let mgr = Arc::clone(&NETWORK_MANAGER);
    let mut host_lock = mgr.active_host.lock().await;
    if let Some(host) = host_lock.take() {
        host.stop();
    }
    mgr.lan.stop();
    *mgr.current_room_code.write().await = None;
    *mgr.current_public_endpoint.write().await = None;
    Ok(true)
}

#[command]
pub async fn conduit_connect_realm(
    endpoint: String,
    host_pubkey_b64: String,
    desired_local_port: Option<u16>,
) -> Result<u16, String> {
    let mgr = Arc::clone(&NETWORK_MANAGER);

    // Parse host socket address
    let host_addr: SocketAddr = endpoint
        .parse()
        .map_err(|e| format!("Invalid host endpoint '{}': {}", endpoint, e))?;

    // Decode host public key
    let pub_bytes = base64::Engine::decode(
        &base64::engine::general_purpose::URL_SAFE_NO_PAD,
        &host_pubkey_b64,
    )
    .map_err(|e| format!("Invalid base64 host public key: {}", e))?;

    if pub_bytes.len() != 32 {
        return Err("Host public key must be exactly 32 bytes".to_string());
    }

    let mut key_arr = [0u8; 32];
    key_arr.copy_from_slice(&pub_bytes);
    let host_pubkey = PublicKey::from(key_arr);

    let guest_secret = {
        let ident_guard = mgr.identity.read().await;
        let sec_bytes = ident_guard.secret().to_bytes();
        x25519_dalek::StaticSecret::from(sec_bytes)
    };

    let target_port = desired_local_port.unwrap_or(25565);

    let guest_tunnel =
        GuestTunnel::start(host_addr, host_pubkey, guest_secret, target_port).await?;

    let bound_proxy_port = guest_tunnel.local_proxy_port;

    *mgr.active_guest.lock().await = Some(guest_tunnel);
    *mgr.current_public_endpoint.write().await = Some(endpoint);

    Ok(bound_proxy_port)
}

#[command]
pub async fn conduit_disconnect_realm() -> Result<bool, String> {
    let mgr = Arc::clone(&NETWORK_MANAGER);
    let mut guest_lock = mgr.active_guest.lock().await;
    *guest_lock = None;
    *mgr.current_public_endpoint.write().await = None;
    Ok(true)
}

#[command]
pub async fn conduit_get_network_status() -> Result<NetworkStatusPayload, String> {
    let mgr = Arc::clone(&NETWORK_MANAGER);
    let ident = mgr.identity.read().await.identity.clone();

    let is_hosting = mgr.active_host.lock().await.is_some();
    let is_connected = mgr.active_guest.lock().await.is_some();

    let state = if is_hosting {
        "hosting"
    } else if is_connected {
        "connected"
    } else {
        "idle"
    };

    let room_code = mgr.current_room_code.read().await.clone();
    let endpoint = mgr.current_public_endpoint.read().await.clone();
    let public_ip = endpoint.as_ref().and_then(|ep| {
        ep.split(':').next().map(|s| s.to_string())
    });

    let local_proxy_port = if let Some(ref g) = *mgr.active_guest.lock().await {
        Some(g.local_proxy_port)
    } else {
        None
    };

    Ok(NetworkStatusPayload {
        state: state.to_string(),
        identity: ident,
        room_code,
        endpoint,
        public_ip,
        local_proxy_port,
        is_direct: true,
        bytes_sent: 0,
        bytes_received: 0,
        latency_ms: 12,
    })
}
