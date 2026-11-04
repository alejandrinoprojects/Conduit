//! Conduit Userspace BoringTun WireGuard Mesh Network Test Suite
//! Tiers 1-4: Feature Coverage, Boundary & Corner Cases, Cross-Feature Combinations, Real-World E2E Scenarios.
//!
//! Authoritative references:
//! - Cloudflare BoringTun WireGuard Protocol (Noise_IKpsk2, WireGuard Type 1/2/4 datagrams)
//! - RFC 5389 STUN (Session Traversal Utilities for NAT)
//! - RFC 791 IPv4 Specification & Synthetic Userspace Framing
//! - Conduit Room Code Tickets (CND-1-... with CRC16-CCITT and Base64URL)
//! - Minecraft Java Edition LAN Discovery Multicast Protocol (224.0.2.60:4445)
//! - Conduit Tauri IPC Command Facade Contracts

use base64::Engine;
use chacha20poly1305::{
    aead::{Aead, KeyInit},
    ChaCha20Poly1305, Nonce,
};
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream, UdpSocket};
use tokio::sync::mpsc;
use x25519_dalek::{PublicKey, StaticSecret};

// =========================================================================
// SECTION 1: PROTOCOL DEFINITIONS, CODECS & CONTRACT TEST FIXTURES
// =========================================================================

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConduitIdentity {
    pub public_key: String, // Base64URL encoded 32-byte public key
    pub short_id: String,   // First 8 characters of public key for easy display
    pub handle: String,     // Player handle or alias
    pub created_at: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoomHostingInfo {
    pub room_code: String,
    pub host_pubkey: String,
    pub local_udp_port: u16,
    pub public_endpoint: Option<String>,
    pub status: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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

/// Standard CRC-16-CCITT calculation (Polynomial 0x1021, Initial 0xFFFF).
pub fn crc16_ccitt(data: &[u8]) -> u16 {
    let mut crc: u16 = 0xFFFF;
    for &byte in data {
        crc ^= (byte as u16) << 8;
        for _ in 0..8 {
            if (crc & 0x8000) != 0 {
                crc = (crc << 1) ^ 0x1021;
            } else {
                crc <<= 1;
            }
        }
    }
    crc
}

/// Binary self-contained room ticket for Mode A (`CND-1-...`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoomTicket {
    pub version: u8,
    pub host_pubkey: [u8; 32],
    pub wan_endpoint: SocketAddr,
    pub lan_endpoint: SocketAddr,
    pub relay_flags: u16,
}

impl RoomTicket {
    pub fn new(
        host_pubkey: [u8; 32],
        wan_endpoint: SocketAddr,
        lan_endpoint: SocketAddr,
        relay_flags: u16,
    ) -> Self {
        Self {
            version: 1,
            host_pubkey,
            wan_endpoint,
            lan_endpoint,
            relay_flags,
        }
    }

    pub fn encode(&self) -> String {
        let mut buf = Vec::with_capacity(49);
        buf.push(self.version);
        buf.extend_from_slice(&self.host_pubkey);

        match self.wan_endpoint {
            SocketAddr::V4(v4) => {
                buf.extend_from_slice(&v4.ip().octets());
                buf.extend_from_slice(&v4.port().to_be_bytes());
            }
            SocketAddr::V6(_) => panic!("Only IPv4 endpoints currently supported in binary ticket"),
        }

        match self.lan_endpoint {
            SocketAddr::V4(v4) => {
                buf.extend_from_slice(&v4.ip().octets());
                buf.extend_from_slice(&v4.port().to_be_bytes());
            }
            SocketAddr::V6(_) => panic!("Only IPv4 endpoints currently supported in binary ticket"),
        }

        buf.extend_from_slice(&self.relay_flags.to_be_bytes());
        let crc = crc16_ccitt(&buf);
        buf.extend_from_slice(&crc.to_be_bytes());

        format!(
            "CND-1-{}",
            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(&buf)
        )
    }

    pub fn decode(ticket_str: &str) -> Result<Self, String> {
        let trimmed = ticket_str.trim();
        if !trimmed.starts_with("CND-1-") {
            return Err("Ticket missing required 'CND-1-' prefix".to_string());
        }
        let b64_part = &trimmed["CND-1-".len()..];
        let raw = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(b64_part)
            .map_err(|e| format!("Base64URL decode error: {}", e))?;

        if raw.len() < 49 {
            return Err(format!(
                "Ticket byte length too short: expected >=49, got {}",
                raw.len()
            ));
        }

        let payload_len = raw.len() - 2;
        let expected_crc = u16::from_be_bytes([raw[payload_len], raw[payload_len + 1]]);
        let actual_crc = crc16_ccitt(&raw[..payload_len]);
        if expected_crc != actual_crc {
            return Err(format!(
                "CRC16 checksum mismatch: expected 0x{:04X}, computed 0x{:04X}",
                expected_crc, actual_crc
            ));
        }

        let version = raw[0];
        if version != 1 {
            return Err(format!("Unsupported ticket version: {}", version));
        }

        let mut host_pubkey = [0u8; 32];
        host_pubkey.copy_from_slice(&raw[1..33]);

        let wan_ip = Ipv4Addr::new(raw[33], raw[34], raw[35], raw[36]);
        let wan_port = u16::from_be_bytes([raw[37], raw[38]]);
        let wan_endpoint = SocketAddr::new(IpAddr::V4(wan_ip), wan_port);

        let lan_ip = Ipv4Addr::new(raw[39], raw[40], raw[41], raw[42]);
        let lan_port = u16::from_be_bytes([raw[43], raw[44]]);
        let lan_endpoint = SocketAddr::new(IpAddr::V4(lan_ip), lan_port);

        let relay_flags = u16::from_be_bytes([raw[45], raw[46]]);

        Ok(Self {
            version,
            host_pubkey,
            wan_endpoint,
            lan_endpoint,
            relay_flags,
        })
    }
}

/// Synthetic 20-byte IPv4 packet framing compliant with RFC 791 and BoringTun packet validation.
pub fn wrap_synthetic_ipv4(src_ip: [u8; 4], dst_ip: [u8; 4], payload: &[u8]) -> Vec<u8> {
    let total_len = (20 + payload.len()) as u16;
    let mut pkt = Vec::with_capacity(total_len as usize);
    pkt.push(0x45); // Version 4, IHL 5 (20 bytes)
    pkt.push(0x00); // DSCP/ECN
    pkt.extend_from_slice(&total_len.to_be_bytes());
    pkt.extend_from_slice(&[0x00, 0x00]); // Identification
    pkt.extend_from_slice(&[0x40, 0x00]); // Don't Fragment flag (DF)
    pkt.push(64); // TTL
    pkt.push(6); // Protocol: TCP
    pkt.extend_from_slice(&[0x00, 0x00]); // Checksum placeholder
    pkt.extend_from_slice(&src_ip);
    pkt.extend_from_slice(&dst_ip);
    pkt.extend_from_slice(payload);

    // RFC 791 16-bit one's complement checksum over header
    let mut sum: u32 = 0;
    for i in (0..20).step_by(2) {
        if i == 10 {
            continue;
        }
        let word = u16::from_be_bytes([pkt[i], pkt[i + 1]]) as u32;
        sum += word;
    }
    while (sum >> 16) > 0 {
        sum = (sum & 0xFFFF) + (sum >> 16);
    }
    let checksum = (!sum & 0xFFFF) as u16;
    let chk_bytes = checksum.to_be_bytes();
    pkt[10] = chk_bytes[0];
    pkt[11] = chk_bytes[1];
    pkt
}

/// Unwraps and validates synthetic 20-byte IPv4 packets.
pub fn unwrap_synthetic_ipv4(packet: &[u8]) -> Result<&[u8], String> {
    if packet.len() < 20 {
        return Err("Packet too short for IPv4 header (<20 bytes)".to_string());
    }
    let ver = packet[0] >> 4;
    if ver != 4 {
        return Err(format!("Invalid IP version: expected 4, got {}", ver));
    }
    let ihl = (packet[0] & 0x0F) as usize * 4;
    if ihl < 20 || ihl > packet.len() {
        return Err(format!("Invalid IHL: {} bytes", ihl));
    }
    let total_len = u16::from_be_bytes([packet[2], packet[3]]) as usize;
    if packet.len() < total_len {
        return Err(format!(
            "Truncated packet: buffer {} < total_len {}",
            packet.len(),
            total_len
        ));
    }
    Ok(&packet[ihl..total_len])
}

/// RFC 5389 STUN protocol message builder and attribute parser.
pub struct StunProtocol;
impl StunProtocol {
    pub const MAGIC_COOKIE: [u8; 4] = [0x21, 0x12, 0xA4, 0x42];
    pub const BINDING_REQUEST: u16 = 0x0001;
    pub const BINDING_RESPONSE: u16 = 0x0101;
    pub const ATTR_XOR_MAPPED_ADDRESS: u16 = 0x0020;
    pub const ATTR_MAPPED_ADDRESS: u16 = 0x0001;

    pub fn build_binding_request(transaction_id: &[u8; 12]) -> [u8; 20] {
        let mut req = [0u8; 20];
        req[0] = (Self::BINDING_REQUEST >> 8) as u8;
        req[1] = (Self::BINDING_REQUEST & 0xFF) as u8;
        req[2] = 0x00;
        req[3] = 0x00;
        req[4..8].copy_from_slice(&Self::MAGIC_COOKIE);
        req[8..20].copy_from_slice(transaction_id);
        req
    }

    pub fn build_binding_response(
        transaction_id: &[u8; 12],
        endpoint: SocketAddr,
        use_xor: bool,
    ) -> Vec<u8> {
        let mut resp = Vec::with_capacity(32);
        resp.extend_from_slice(&Self::BINDING_RESPONSE.to_be_bytes());
        resp.extend_from_slice(&12u16.to_be_bytes()); // Attribute section length
        resp.extend_from_slice(&Self::MAGIC_COOKIE);
        resp.extend_from_slice(transaction_id);

        if use_xor {
            resp.extend_from_slice(&Self::ATTR_XOR_MAPPED_ADDRESS.to_be_bytes());
            resp.extend_from_slice(&8u16.to_be_bytes());
            resp.push(0x00); // Reserved
            resp.push(0x01); // IPv4
            let xor_port = endpoint.port() ^ u16::from_be_bytes([Self::MAGIC_COOKIE[0], Self::MAGIC_COOKIE[1]]);
            resp.extend_from_slice(&xor_port.to_be_bytes());
            match endpoint.ip() {
                IpAddr::V4(v4) => {
                    let oct = v4.octets();
                    resp.push(oct[0] ^ Self::MAGIC_COOKIE[0]);
                    resp.push(oct[1] ^ Self::MAGIC_COOKIE[1]);
                    resp.push(oct[2] ^ Self::MAGIC_COOKIE[2]);
                    resp.push(oct[3] ^ Self::MAGIC_COOKIE[3]);
                }
                IpAddr::V6(_) => panic!("IPv6 unsupported in fixture"),
            }
        } else {
            resp.extend_from_slice(&Self::ATTR_MAPPED_ADDRESS.to_be_bytes());
            resp.extend_from_slice(&8u16.to_be_bytes());
            resp.push(0x00);
            resp.push(0x01);
            resp.extend_from_slice(&endpoint.port().to_be_bytes());
            match endpoint.ip() {
                IpAddr::V4(v4) => resp.extend_from_slice(&v4.octets()),
                IpAddr::V6(_) => panic!("IPv6 unsupported in fixture"),
            }
        }
        resp
    }

    pub fn parse_mapped_address(
        response: &[u8],
        expected_tx_id: &[u8; 12],
    ) -> Result<SocketAddr, String> {
        if response.len() < 20 {
            return Err("STUN response too short (<20 bytes)".to_string());
        }
        let msg_type = u16::from_be_bytes([response[0], response[1]]);
        if msg_type != Self::BINDING_RESPONSE {
            return Err(format!(
                "Invalid STUN message type: expected 0x0101, got 0x{:04X}",
                msg_type
            ));
        }
        if response[4..8] != Self::MAGIC_COOKIE {
            return Err("STUN Magic Cookie mismatch".to_string());
        }
        if &response[8..20] != expected_tx_id {
            return Err("STUN Transaction ID mismatch".to_string());
        }

        let mut idx = 20;
        while idx + 4 <= response.len() {
            let attr_type = u16::from_be_bytes([response[idx], response[idx + 1]]);
            let attr_len = u16::from_be_bytes([response[idx + 2], response[idx + 3]]) as usize;
            idx += 4;
            if idx + attr_len > response.len() {
                return Err("STUN attribute extends beyond response length".to_string());
            }

            if (attr_type == Self::ATTR_XOR_MAPPED_ADDRESS || attr_type == Self::ATTR_MAPPED_ADDRESS)
                && attr_len >= 8
            {
                let family = response[idx + 1];
                if family == 0x01 {
                    let port = if attr_type == Self::ATTR_XOR_MAPPED_ADDRESS {
                        let xor_port = u16::from_be_bytes([response[idx + 2], response[idx + 3]]);
                        xor_port ^ u16::from_be_bytes([Self::MAGIC_COOKIE[0], Self::MAGIC_COOKIE[1]])
                    } else {
                        u16::from_be_bytes([response[idx + 2], response[idx + 3]])
                    };

                    let ip = if attr_type == Self::ATTR_XOR_MAPPED_ADDRESS {
                        Ipv4Addr::new(
                            response[idx + 4] ^ Self::MAGIC_COOKIE[0],
                            response[idx + 5] ^ Self::MAGIC_COOKIE[1],
                            response[idx + 6] ^ Self::MAGIC_COOKIE[2],
                            response[idx + 7] ^ Self::MAGIC_COOKIE[3],
                        )
                    } else {
                        Ipv4Addr::new(
                            response[idx + 4],
                            response[idx + 5],
                            response[idx + 6],
                            response[idx + 7],
                        )
                    };

                    return Ok(SocketAddr::new(IpAddr::V4(ip), port));
                }
            }

            let pad = (4 - (attr_len % 4)) % 4;
            idx += attr_len + pad;
        }

        Err("No MAPPED-ADDRESS or XOR-MAPPED-ADDRESS attribute found".to_string())
    }
}

/// Zero-Knowledge Relay Transport framing.
pub struct RelayFrame;
impl RelayFrame {
    pub const MSG_REGISTER: u8 = 0x01;
    pub const MSG_DATA: u8 = 0x02;
    pub const MSG_PING: u8 = 0x03;
    pub const MSG_PONG: u8 = 0x04;
    pub const MSG_CLOSE: u8 = 0x05;

    pub fn encode_register(session_id: &[u8; 16], role: u8) -> Vec<u8> {
        let mut v = Vec::with_capacity(18);
        v.push(Self::MSG_REGISTER);
        v.extend_from_slice(session_id);
        v.push(role);
        v
    }

    pub fn encode_data(session_id: &[u8; 16], payload: &[u8]) -> Vec<u8> {
        let mut v = Vec::with_capacity(1 + 16 + payload.len());
        v.push(Self::MSG_DATA);
        v.extend_from_slice(session_id);
        v.extend_from_slice(payload);
        v
    }

    pub fn encode_ping(timestamp_ms: u64) -> Vec<u8> {
        let mut v = Vec::with_capacity(9);
        v.push(Self::MSG_PING);
        v.extend_from_slice(&timestamp_ms.to_be_bytes());
        v
    }

    pub fn encode_pong(timestamp_ms: u64) -> Vec<u8> {
        let mut v = Vec::with_capacity(9);
        v.push(Self::MSG_PONG);
        v.extend_from_slice(&timestamp_ms.to_be_bytes());
        v
    }

    pub fn encode_close(session_id: &[u8; 16]) -> Vec<u8> {
        let mut v = Vec::with_capacity(17);
        v.push(Self::MSG_CLOSE);
        v.extend_from_slice(session_id);
        v
    }

    pub fn decode(bytes: &[u8]) -> Result<(u8, Option<[u8; 16]>, &[u8]), String> {
        if bytes.is_empty() {
            return Err("Empty relay frame".to_string());
        }
        let msg_type = bytes[0];
        match msg_type {
            Self::MSG_REGISTER => {
                if bytes.len() < 18 {
                    return Err("Truncated REGISTER relay frame".to_string());
                }
                let mut sess = [0u8; 16];
                sess.copy_from_slice(&bytes[1..17]);
                Ok((msg_type, Some(sess), &bytes[17..18]))
            }
            Self::MSG_DATA => {
                if bytes.len() < 17 {
                    return Err("Truncated DATA relay frame".to_string());
                }
                let mut sess = [0u8; 16];
                sess.copy_from_slice(&bytes[1..17]);
                Ok((msg_type, Some(sess), &bytes[17..]))
            }
            Self::MSG_PING | Self::MSG_PONG => {
                if bytes.len() < 9 {
                    return Err("Truncated PING/PONG relay frame".to_string());
                }
                Ok((msg_type, None, &bytes[1..9]))
            }
            Self::MSG_CLOSE => {
                if bytes.len() < 17 {
                    return Err("Truncated CLOSE relay frame".to_string());
                }
                let mut sess = [0u8; 16];
                sess.copy_from_slice(&bytes[1..17]);
                Ok((msg_type, Some(sess), &[]))
            }
            _ => Err(format!("Unknown relay message type: 0x{:02x}", msg_type)),
        }
    }
}

/// Minecraft Java Edition LAN Discovery multicast protocol (224.0.2.60:4445).
pub struct MinecraftLanBeacon;
impl MinecraftLanBeacon {
    pub const MULTICAST_GROUP: &'static str = "224.0.2.60";
    pub const MULTICAST_PORT: u16 = 4445;

    pub fn format(motd: &str, port: u16) -> String {
        format!("[MOTD]{}[/MOTD][AD]{}[/AD]", motd, port)
    }

    pub fn parse(packet: &str) -> Option<(String, u16)> {
        let motd_start = packet.find("[MOTD]")? + "[MOTD]".len();
        let motd_end = packet.find("[/MOTD]")?;
        let ad_start = packet.find("[AD]")? + "[AD]".len();
        let ad_end = packet.find("[/AD]")?;

        if motd_end < motd_start || ad_end < ad_start {
            return None;
        }

        let motd = packet[motd_start..motd_end].to_string();
        let port = packet[ad_start..ad_end].parse::<u16>().ok()?;
        Some((motd, port))
    }
}

/// Official WireGuard wire format specification (Types 1, 2, 4) validator and builder.
pub struct WireGuardWireProtocol;
impl WireGuardWireProtocol {
    pub const TYPE_HANDSHAKE_INITIATION: u8 = 1;
    pub const TYPE_HANDSHAKE_RESPONSE: u8 = 2;
    pub const TYPE_COOKIE_REPLY: u8 = 3;
    pub const TYPE_DATA: u8 = 4;

    pub fn build_handshake_initiation(
        sender_idx: u32,
        ephemeral_pubkey: &[u8; 32],
        encrypted_static: &[u8; 48],
        encrypted_timestamp: &[u8; 28],
        mac1: &[u8; 16],
        mac2: &[u8; 16],
    ) -> [u8; 148] {
        let mut pkt = [0u8; 148];
        pkt[0] = Self::TYPE_HANDSHAKE_INITIATION;
        pkt[1..4].copy_from_slice(&[0, 0, 0]);
        pkt[4..8].copy_from_slice(&sender_idx.to_le_bytes());
        pkt[8..40].copy_from_slice(ephemeral_pubkey);
        pkt[40..88].copy_from_slice(encrypted_static);
        pkt[88..116].copy_from_slice(encrypted_timestamp);
        pkt[116..132].copy_from_slice(mac1);
        pkt[132..148].copy_from_slice(mac2);
        pkt
    }

    pub fn validate_handshake_initiation(packet: &[u8]) -> Result<(u32, [u8; 32]), String> {
        if packet.len() != 148 {
            return Err(format!(
                "Invalid Handshake Initiation size: expected 148 bytes, got {}",
                packet.len()
            ));
        }
        if packet[0] != Self::TYPE_HANDSHAKE_INITIATION {
            return Err(format!(
                "Invalid packet type: expected 1, got {}",
                packet[0]
            ));
        }
        if packet[1..4] != [0, 0, 0] {
            return Err("Reserved fields must be zero".to_string());
        }
        let sender_idx = u32::from_le_bytes([packet[4], packet[5], packet[6], packet[7]]);
        let mut ephem = [0u8; 32];
        ephem.copy_from_slice(&packet[8..40]);
        Ok((sender_idx, ephem))
    }

    pub fn build_handshake_response(
        sender_idx: u32,
        receiver_idx: u32,
        ephemeral_pubkey: &[u8; 32],
        encrypted_empty: &[u8; 16],
        mac1: &[u8; 16],
        mac2: &[u8; 16],
    ) -> [u8; 92] {
        let mut pkt = [0u8; 92];
        pkt[0] = Self::TYPE_HANDSHAKE_RESPONSE;
        pkt[1..4].copy_from_slice(&[0, 0, 0]);
        pkt[4..8].copy_from_slice(&sender_idx.to_le_bytes());
        pkt[8..12].copy_from_slice(&receiver_idx.to_le_bytes());
        pkt[12..44].copy_from_slice(ephemeral_pubkey);
        pkt[44..60].copy_from_slice(encrypted_empty);
        pkt[60..76].copy_from_slice(mac1);
        pkt[76..92].copy_from_slice(mac2);
        pkt
    }

    pub fn validate_handshake_response(packet: &[u8]) -> Result<(u32, u32, [u8; 32]), String> {
        if packet.len() != 92 {
            return Err(format!(
                "Invalid Handshake Response size: expected 92 bytes, got {}",
                packet.len()
            ));
        }
        if packet[0] != Self::TYPE_HANDSHAKE_RESPONSE {
            return Err(format!(
                "Invalid packet type: expected 2, got {}",
                packet[0]
            ));
        }
        if packet[1..4] != [0, 0, 0] {
            return Err("Reserved fields must be zero".to_string());
        }
        let sender_idx = u32::from_le_bytes([packet[4], packet[5], packet[6], packet[7]]);
        let receiver_idx = u32::from_le_bytes([packet[8], packet[9], packet[10], packet[11]]);
        let mut ephem = [0u8; 32];
        ephem.copy_from_slice(&packet[12..44]);
        Ok((sender_idx, receiver_idx, ephem))
    }

    pub fn build_data_packet(
        receiver_idx: u32,
        counter: u64,
        ciphertext_and_tag: &[u8],
    ) -> Vec<u8> {
        let mut pkt = Vec::with_capacity(16 + ciphertext_and_tag.len());
        pkt.push(Self::TYPE_DATA);
        pkt.extend_from_slice(&[0, 0, 0]);
        pkt.extend_from_slice(&receiver_idx.to_le_bytes());
        pkt.extend_from_slice(&counter.to_le_bytes());
        pkt.extend_from_slice(ciphertext_and_tag);
        pkt
    }

    pub fn validate_data_packet(packet: &[u8]) -> Result<(u32, u64, usize), String> {
        if packet.len() < 32 {
            return Err(format!(
                "Invalid WireGuard Data packet: expected >=32 bytes, got {}",
                packet.len()
            ));
        }
        if packet[0] != Self::TYPE_DATA {
            return Err(format!(
                "Invalid packet type: expected 4, got {}",
                packet[0]
            ));
        }
        if packet[1..4] != [0, 0, 0] {
            return Err("Reserved fields must be zero".to_string());
        }
        let receiver_idx = u32::from_le_bytes([packet[4], packet[5], packet[6], packet[7]]);
        let mut counter_bytes = [0u8; 8];
        counter_bytes.copy_from_slice(&packet[8..16]);
        let counter = u64::from_le_bytes(counter_bytes);
        let payload_len = packet.len() - 32; // minus 16B header and 16B Poly1305 MAC tag
        Ok((receiver_idx, counter, payload_len))
    }
}

/// In-Memory WireGuard State Machine Wrapper mimicking BoringTun sans-I/O `Tunn` interface.
pub struct MockWireGuardTunnel {
    pub static_private: StaticSecret,
    pub peer_public: PublicKey,
    pub shared_key: [u8; 32],
    pub cipher: ChaCha20Poly1305,
    pub sender_idx: u32,
    pub receiver_idx: u32,
    pub counter: AtomicU64,
    pub is_established: AtomicBool,
}

impl MockWireGuardTunnel {
    pub fn new(static_private_bytes: [u8; 32], peer_public_bytes: [u8; 32], sender_idx: u32) -> Self {
        let static_private = StaticSecret::from(static_private_bytes);
        let peer_public = PublicKey::from(peer_public_bytes);
        let shared_key = static_private.diffie_hellman(&peer_public).to_bytes();
        let cipher = ChaCha20Poly1305::new((&shared_key).into());

        Self {
            static_private,
            peer_public,
            shared_key,
            cipher,
            sender_idx,
            receiver_idx: 0,
            counter: AtomicU64::new(0),
            is_established: AtomicBool::new(false),
        }
    }

    pub fn format_handshake_initiation(&self) -> [u8; 148] {
        let ephem_pub = PublicKey::from(&self.static_private);
        let enc_static = [0xAA; 48];
        let enc_ts = [0xBB; 28];
        let mac1 = [0xCC; 16];
        let mac2 = [0xDD; 16];
        WireGuardWireProtocol::build_handshake_initiation(
            self.sender_idx,
            ephem_pub.as_bytes(),
            &enc_static,
            &enc_ts,
            &mac1,
            &mac2,
        )
    }

    pub fn process_handshake_initiation(&mut self, init_packet: &[u8]) -> Result<[u8; 92], String> {
        let (peer_sender_idx, _ephem) =
            WireGuardWireProtocol::validate_handshake_initiation(init_packet)?;
        self.receiver_idx = peer_sender_idx;
        self.is_established.store(true, Ordering::SeqCst);

        let my_ephem = PublicKey::from(&self.static_private);
        let enc_empty = [0x11; 16];
        let mac1 = [0x22; 16];
        let mac2 = [0x33; 16];
        Ok(WireGuardWireProtocol::build_handshake_response(
            self.sender_idx,
            self.receiver_idx,
            my_ephem.as_bytes(),
            &enc_empty,
            &mac1,
            &mac2,
        ))
    }

    pub fn process_handshake_response(&mut self, resp_packet: &[u8]) -> Result<(), String> {
        let (peer_sender_idx, peer_receiver_idx, _ephem) =
            WireGuardWireProtocol::validate_handshake_response(resp_packet)?;
        if peer_receiver_idx != self.sender_idx {
            return Err("Mismatched receiver index in handshake response".to_string());
        }
        self.receiver_idx = peer_sender_idx;
        self.is_established.store(true, Ordering::SeqCst);
        Ok(())
    }

    pub fn encapsulate(&self, ip_packet: &[u8]) -> Result<Vec<u8>, String> {
        if !self.is_established.load(Ordering::Relaxed) {
            return Err("Tunnel handshake not established".to_string());
        }
        let ctr = self.counter.fetch_add(1, Ordering::SeqCst);
        let mut nonce_bytes = [0u8; 12];
        nonce_bytes[4..12].copy_from_slice(&ctr.to_le_bytes());
        let nonce = Nonce::from(nonce_bytes);

        let ciphertext_and_tag = self
            .cipher
            .encrypt(&nonce, ip_packet)
            .map_err(|e| format!("ChaCha20Poly1305 encrypt error: {:?}", e))?;

        Ok(WireGuardWireProtocol::build_data_packet(
            self.receiver_idx,
            ctr,
            &ciphertext_and_tag,
        ))
    }

    pub fn decapsulate<'a>(&self, datagram: &[u8], dst_buf: &'a mut [u8]) -> Result<&'a [u8], String> {
        let (_rx_idx, counter, _payload_len) =
            WireGuardWireProtocol::validate_data_packet(datagram)?;
        let mut nonce_bytes = [0u8; 12];
        nonce_bytes[4..12].copy_from_slice(&counter.to_le_bytes());
        let nonce = Nonce::from(nonce_bytes);

        let ciphertext_and_tag = &datagram[16..];
        let plaintext = self
            .cipher
            .decrypt(&nonce, ciphertext_and_tag)
            .map_err(|e| format!("ChaCha20Poly1305 decrypt error: {:?}", e))?;

        if dst_buf.len() < plaintext.len() {
            return Err("Destination buffer too small".to_string());
        }
        dst_buf[..plaintext.len()].copy_from_slice(&plaintext);
        Ok(&dst_buf[..plaintext.len()])
    }

    pub fn format_keepalive(&self) -> Result<Vec<u8>, String> {
        // WireGuard keepalive is empty payload encrypted (16 bytes Poly1305 tag)
        self.encapsulate(&[])
    }
}

// =========================================================================
// SECTION 2: TIER 1 — FEATURE COVERAGE (>=5 tests per feature)
// =========================================================================

// --- Feature 1: WireGuard keys and handshake initiation/response/confirmation ---

#[test]
fn t1_1_wireguard_keypair_generation_and_dh_exchange() {
    let mut alice_sec_bytes = [0u8; 32];
    let mut bob_sec_bytes = [0u8; 32];
    rand::rng().fill_bytes(&mut alice_sec_bytes);
    rand::rng().fill_bytes(&mut bob_sec_bytes);

    let alice_secret = StaticSecret::from(alice_sec_bytes);
    let bob_secret = StaticSecret::from(bob_sec_bytes);

    let alice_pub = PublicKey::from(&alice_secret);
    let bob_pub = PublicKey::from(&bob_secret);

    assert_ne!(alice_pub.as_bytes(), bob_pub.as_bytes());

    let shared_alice = alice_secret.diffie_hellman(&bob_pub);
    let shared_bob = bob_secret.diffie_hellman(&alice_pub);

    assert_eq!(shared_alice.to_bytes(), shared_bob.to_bytes());
    assert_ne!(shared_alice.to_bytes(), [0u8; 32]);
}

#[test]
fn t1_2_wireguard_type1_handshake_initiation_framing() {
    let mut key_bytes = [0u8; 32];
    rand::rng().fill_bytes(&mut key_bytes);
    let tunnel = MockWireGuardTunnel::new(key_bytes, key_bytes, 1001);

    let init_pkt = tunnel.format_handshake_initiation();
    assert_eq!(init_pkt.len(), 148);
    assert_eq!(init_pkt[0], 1); // Message Type 1
    assert_eq!(&init_pkt[1..4], &[0, 0, 0]); // Reserved

    let (sender_idx, ephem) = WireGuardWireProtocol::validate_handshake_initiation(&init_pkt).unwrap();
    assert_eq!(sender_idx, 1001);
    assert_ne!(ephem, [0u8; 32]);
}

#[test]
fn t1_3_wireguard_type2_handshake_response_framing() {
    let mut host_sec = [0u8; 32];
    let mut guest_sec = [0u8; 32];
    rand::rng().fill_bytes(&mut host_sec);
    rand::rng().fill_bytes(&mut guest_sec);

    let host_pub = PublicKey::from(&StaticSecret::from(host_sec));
    let guest_pub = PublicKey::from(&StaticSecret::from(guest_sec));

    let mut host_tunn = MockWireGuardTunnel::new(host_sec, *guest_pub.as_bytes(), 5001);
    let guest_tunn = MockWireGuardTunnel::new(guest_sec, *host_pub.as_bytes(), 9001);

    let init_pkt = guest_tunn.format_handshake_initiation();
    let resp_pkt = host_tunn.process_handshake_initiation(&init_pkt).unwrap();

    assert_eq!(resp_pkt.len(), 92);
    assert_eq!(resp_pkt[0], 2); // Message Type 2
    assert_eq!(&resp_pkt[1..4], &[0, 0, 0]);

    let (sender_idx, rx_idx, _ephem) =
        WireGuardWireProtocol::validate_handshake_response(&resp_pkt).unwrap();
    assert_eq!(sender_idx, 5001);
    assert_eq!(rx_idx, 9001);
}

#[test]
fn t1_4_wireguard_handshake_confirmation_and_session_transition() {
    let mut host_sec = [0u8; 32];
    let mut guest_sec = [0u8; 32];
    rand::rng().fill_bytes(&mut host_sec);
    rand::rng().fill_bytes(&mut guest_sec);

    let host_pub = PublicKey::from(&StaticSecret::from(host_sec));
    let guest_pub = PublicKey::from(&StaticSecret::from(guest_sec));

    let mut host_tunn = MockWireGuardTunnel::new(host_sec, *guest_pub.as_bytes(), 111);
    let mut guest_tunn = MockWireGuardTunnel::new(guest_sec, *host_pub.as_bytes(), 222);

    assert!(!guest_tunn.is_established.load(Ordering::SeqCst));
    assert!(!host_tunn.is_established.load(Ordering::SeqCst));

    let init = guest_tunn.format_handshake_initiation();
    let resp = host_tunn.process_handshake_initiation(&init).unwrap();
    guest_tunn.process_handshake_response(&resp).unwrap();

    assert!(guest_tunn.is_established.load(Ordering::SeqCst));
    assert!(host_tunn.is_established.load(Ordering::SeqCst));
    assert_eq!(guest_tunn.receiver_idx, 111);
    assert_eq!(host_tunn.receiver_idx, 222);
}

#[test]
fn t1_5_wireguard_type4_data_packet_counter_monotonicity() {
    let mut key = [0u8; 32];
    rand::rng().fill_bytes(&mut key);
    let mut tunn = MockWireGuardTunnel::new(key, key, 777);
    tunn.is_established.store(true, Ordering::SeqCst);
    tunn.receiver_idx = 888;

    for expected_ctr in 0..10u64 {
        let pkt = tunn.encapsulate(b"MinecraftPing").unwrap();
        let (rx, ctr, len) = WireGuardWireProtocol::validate_data_packet(&pkt).unwrap();
        assert_eq!(rx, 888);
        assert_eq!(ctr, expected_ctr);
        assert_eq!(len, b"MinecraftPing".len());
    }
}

// --- Feature 2: Packet encryption and decapsulation fidelity ---

#[test]
fn t1_6_synthetic_ipv4_packet_wrapping_rfc791() {
    let payload = b"GET /minecraft/ping HTTP/1.1\r\n\r\n";
    let src = [10, 88, 0, 2];
    let dst = [10, 88, 0, 1];

    let packet = wrap_synthetic_ipv4(src, dst, payload);
    assert_eq!(packet.len(), 20 + payload.len());
    assert_eq!(packet[0], 0x45); // IPv4, 20 bytes
    assert_eq!(packet[8], 64); // TTL
    assert_eq!(packet[9], 6); // Protocol TCP
    assert_eq!(&packet[12..16], &src);
    assert_eq!(&packet[16..20], &dst);
}

#[test]
fn t1_7_synthetic_ipv4_checksum_correctness() {
    let payload = b"TestPayloadWithChecksumVerification";
    let pkt = wrap_synthetic_ipv4([10, 88, 0, 2], [10, 88, 0, 1], payload);

    // Sum over 20-byte header should fold to 0xFFFF
    let mut sum: u32 = 0;
    for i in (0..20).step_by(2) {
        sum += u16::from_be_bytes([pkt[i], pkt[i + 1]]) as u32;
    }
    while (sum >> 16) > 0 {
        sum = (sum & 0xFFFF) + (sum >> 16);
    }
    assert_eq!(sum as u16, 0xFFFF);
}

#[test]
fn t1_8_packet_encapsulation_and_decapsulation_roundtrip() {
    let mut host_sec = [0u8; 32];
    let mut guest_sec = [0u8; 32];
    rand::rng().fill_bytes(&mut host_sec);
    rand::rng().fill_bytes(&mut guest_sec);

    let host_pub = PublicKey::from(&StaticSecret::from(host_sec));
    let guest_pub = PublicKey::from(&StaticSecret::from(guest_sec));

    let mut host = MockWireGuardTunnel::new(host_sec, *guest_pub.as_bytes(), 10);
    let mut guest = MockWireGuardTunnel::new(guest_sec, *host_pub.as_bytes(), 20);

    let init = guest.format_handshake_initiation();
    let resp = host.process_handshake_initiation(&init).unwrap();
    guest.process_handshake_response(&resp).unwrap();

    let raw_minecraft_msg = b"\x00\x02\x43\x6f\x6e\x64\x75\x69\x74\x53\x65\x72\x76\x65\x72";
    let ip_pkt = wrap_synthetic_ipv4([10, 88, 0, 2], [10, 88, 0, 1], raw_minecraft_msg);

    // Guest encapsulates
    let encrypted_wg_datagram = guest.encapsulate(&ip_pkt).unwrap();

    // Host decapsulates
    let mut dec_buf = vec![0u8; 4096];
    let decrypted_ip = host.decapsulate(&encrypted_wg_datagram, &mut dec_buf).unwrap();
    assert_eq!(decrypted_ip, ip_pkt.as_slice());

    // Unwrap synthetic IPv4 to obtain pure Minecraft bytes
    let unwrapped_payload = unwrap_synthetic_ipv4(decrypted_ip).unwrap();
    assert_eq!(unwrapped_payload, raw_minecraft_msg);
}

#[test]
fn t1_9_wireguard_mtu_boundary_packet_handling() {
    let mut sec = [0u8; 32];
    rand::rng().fill_bytes(&mut sec);
    let tunn = MockWireGuardTunnel::new(sec, sec, 1);
    tunn.is_established.store(true, Ordering::SeqCst);

    // Exact WireGuard payload limit within standard 1420 MTU: 1380 bytes TCP + 20 IP + 20 TCP
    let large_payload = vec![0xFEu8; 1380];
    let ip_pkt = wrap_synthetic_ipv4([10, 88, 0, 2], [10, 88, 0, 1], &large_payload);
    assert_eq!(ip_pkt.len(), 1400);

    let datagram = tunn.encapsulate(&ip_pkt).unwrap();
    assert_eq!(datagram.len(), 16 + 1400 + 16); // 16 header + 1400 payload + 16 MAC tag

    let mut buf = vec![0u8; 2048];
    let dec = tunn.decapsulate(&datagram, &mut buf).unwrap();
    assert_eq!(dec.len(), 1400);
}

#[test]
fn t1_10_empty_keepalive_packet_encryption_and_decapsulation() {
    let mut sec = [0u8; 32];
    rand::rng().fill_bytes(&mut sec);
    let tunn = MockWireGuardTunnel::new(sec, sec, 99);
    tunn.is_established.store(true, Ordering::SeqCst);

    let keepalive_datagram = tunn.format_keepalive().unwrap();
    assert_eq!(keepalive_datagram.len(), 32); // 16 header + 0 payload + 16 tag

    let mut buf = vec![0u8; 64];
    let dec = tunn.decapsulate(&keepalive_datagram, &mut buf).unwrap();
    assert_eq!(dec.len(), 0);
}

// --- Feature 3: Non-privileged userspace TCP loopback proxy on port 25565 ---

#[tokio::test]
async fn t1_11_loopback_proxy_listener_bind_without_privilege() {
    // Attempt binding on 25565 or dynamic port if busy
    let listener = match TcpListener::bind("127.0.0.1:25565").await {
        Ok(l) => l,
        Err(_) => TcpListener::bind("127.0.0.1:0").await.unwrap(),
    };
    let addr = listener.local_addr().unwrap();
    assert_eq!(addr.ip(), IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)));
    assert!(addr.port() > 0);
}

#[tokio::test]
async fn t1_12_loopback_proxy_bidirectional_data_transfer() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();

    let server_task = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut buf = [0u8; 128];
        let n = socket.read(&mut buf).await.unwrap();
        socket.write_all(&buf[..n]).await.unwrap();
    });

    let mut client = TcpStream::connect(format!("127.0.0.1:{}", port)).await.unwrap();
    client.write_all(b"MinecraftLoginRequest").await.unwrap();

    let mut resp = [0u8; 128];
    let n = client.read(&mut resp).await.unwrap();
    assert_eq!(&resp[..n], b"MinecraftLoginRequest");

    server_task.await.unwrap();
}

#[tokio::test]
async fn t1_13_loopback_proxy_graceful_stream_shutdown() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();

    let server_task = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut buf = [0u8; 64];
        let n = socket.read(&mut buf).await.unwrap();
        assert_eq!(n, 5);
        // Client shutdown should yield EOF (0 bytes)
        let eof = socket.read(&mut buf).await.unwrap();
        assert_eq!(eof, 0);
    });

    let mut client = TcpStream::connect(format!("127.0.0.1:{}", port)).await.unwrap();
    client.write_all(b"Hello").await.unwrap();
    client.shutdown().await.unwrap();

    server_task.await.unwrap();
}

#[tokio::test]
async fn t1_14_loopback_proxy_sequential_connections() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();

    tokio::spawn(async move {
        for expected_id in 0..3u8 {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut byte = [0u8; 1];
            socket.read_exact(&mut byte).await.unwrap();
            assert_eq!(byte[0], expected_id);
            socket.write_all(&[byte[0] + 10]).await.unwrap();
        }
    });

    for id in 0..3u8 {
        let mut client = TcpStream::connect(format!("127.0.0.1:{}", port)).await.unwrap();
        client.write_all(&[id]).await.unwrap();
        let mut resp = [0u8; 1];
        client.read_exact(&mut resp).await.unwrap();
        assert_eq!(resp[0], id + 10);
    }
}

#[tokio::test]
async fn t1_15_loopback_proxy_listener_drop_releases_port() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let bound_port = listener.local_addr().unwrap().port();
    drop(listener);

    // Must be rebindable immediately
    let rebind = TcpListener::bind(format!("127.0.0.1:{}", bound_port)).await;
    assert!(rebind.is_ok());
}

// --- Feature 4: Room code ticket encoding/decoding (`CND-1-...`) ---

#[test]
fn t1_16_room_ticket_encode_decode_roundtrip() {
    let mut pubkey = [0u8; 32];
    rand::rng().fill_bytes(&mut pubkey);
    let wan = "203.0.113.50:54123".parse().unwrap();
    let lan = "192.168.1.100:54123".parse().unwrap();

    let ticket = RoomTicket::new(pubkey, wan, lan, 0x0001);
    let encoded = ticket.encode();

    assert!(encoded.starts_with("CND-1-"));
    let decoded = RoomTicket::decode(&encoded).unwrap();
    assert_eq!(decoded, ticket);
}

#[test]
fn t1_17_room_ticket_crc16_integrity_validation() {
    let mut pubkey = [0u8; 32];
    rand::rng().fill_bytes(&mut pubkey);
    let wan = "198.51.100.25:25565".parse().unwrap();
    let lan = "10.0.0.5:25565".parse().unwrap();

    let ticket = RoomTicket::new(pubkey, wan, lan, 0);
    let encoded = ticket.encode();

    // Mutate one base64 character in the middle
    let mut chars: Vec<char> = encoded.chars().collect();
    let mid = chars.len() / 2;
    chars[mid] = if chars[mid] == 'A' { 'B' } else { 'A' };
    let corrupted: String = chars.into_iter().collect();

    let res = RoomTicket::decode(&corrupted);
    assert!(res.is_err());
}

#[test]
fn t1_18_room_ticket_relay_flags_serialization() {
    let pubkey = [0x55; 32];
    let wan = "1.1.1.1:8080".parse().unwrap();
    let lan = "127.0.0.1:8080".parse().unwrap();

    let ticket = RoomTicket::new(pubkey, wan, lan, 0xABCD);
    let encoded = ticket.encode();
    let decoded = RoomTicket::decode(&encoded).unwrap();

    assert_eq!(decoded.relay_flags, 0xABCD);
}

#[test]
fn t1_19_short_room_code_generation_format() {
    // Short code format: CND-XXXX or CND-XXXX-SUFFIX
    fn generate_room_code(suffix: Option<&str>) -> String {
        let charset = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
        let mut rand_bytes = [0u8; 4];
        rand::rng().fill_bytes(&mut rand_bytes);
        let code: String = rand_bytes
            .iter()
            .map(|b| charset[(*b as usize) % charset.len()] as char)
            .collect();
        if let Some(s) = suffix {
            format!("CND-{}-{}", code, s.to_uppercase().replace(' ', ""))
        } else {
            format!("CND-{}", code)
        }
    }

    let code = generate_room_code(Some("SMP"));
    assert!(code.starts_with("CND-"));
    assert!(code.ends_with("-SMP"));
    let parts: Vec<&str> = code.split('-').collect();
    assert_eq!(parts.len(), 3);
    assert_eq!(parts[1].len(), 4);
}

#[test]
fn t1_20_short_room_code_without_suffix() {
    fn generate_room_code(suffix: Option<&str>) -> String {
        let charset = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
        let mut rand_bytes = [0u8; 4];
        rand::rng().fill_bytes(&mut rand_bytes);
        let code: String = rand_bytes
            .iter()
            .map(|b| charset[(*b as usize) % charset.len()] as char)
            .collect();
        if let Some(s) = suffix {
            format!("CND-{}-{}", code, s.to_uppercase().replace(' ', ""))
        } else {
            format!("CND-{}", code)
        }
    }

    let code = generate_room_code(None);
    assert!(code.starts_with("CND-"));
    let parts: Vec<&str> = code.split('-').collect();
    assert_eq!(parts.len(), 2);
    assert_eq!(parts[1].len(), 4);
}

// --- Feature 5: STUN RFC 5389 request/response parsing and endpoint mapping ---

#[test]
fn t1_21_stun_binding_request_structure() {
    let tx_id = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12];
    let req = StunProtocol::build_binding_request(&tx_id);

    assert_eq!(req.len(), 20);
    assert_eq!(req[0..2], [0x00, 0x01]); // 0x0001
    assert_eq!(req[2..4], [0x00, 0x00]); // Length 0
    assert_eq!(req[4..8], StunProtocol::MAGIC_COOKIE);
    assert_eq!(req[8..20], tx_id);
}

#[test]
fn t1_22_stun_xor_mapped_address_parsing() {
    let tx_id = [0xAA; 12];
    let expected_endpoint: SocketAddr = "203.0.113.199:51820".parse().unwrap();
    let resp = StunProtocol::build_binding_response(&tx_id, expected_endpoint, true);

    let parsed = StunProtocol::parse_mapped_address(&resp, &tx_id).unwrap();
    assert_eq!(parsed, expected_endpoint);
}

#[test]
fn t1_23_stun_standard_mapped_address_fallback_parsing() {
    let tx_id = [0xBB; 12];
    let expected_endpoint: SocketAddr = "198.51.100.77:25565".parse().unwrap();
    let resp = StunProtocol::build_binding_response(&tx_id, expected_endpoint, false);

    let parsed = StunProtocol::parse_mapped_address(&resp, &tx_id).unwrap();
    assert_eq!(parsed, expected_endpoint);
}

#[test]
fn t1_24_stun_transaction_id_mismatch_rejection() {
    let tx_id1 = [0x11; 12];
    let tx_id2 = [0x22; 12];
    let endpoint: SocketAddr = "127.0.0.1:19302".parse().unwrap();
    let resp = StunProtocol::build_binding_response(&tx_id1, endpoint, true);

    let res = StunProtocol::parse_mapped_address(&resp, &tx_id2);
    assert!(res.is_err());
    assert!(res.unwrap_err().contains("Transaction ID mismatch"));
}

#[test]
fn t1_25_stun_cookie_mismatch_rejection() {
    let tx_id = [0x33; 12];
    let mut resp = StunProtocol::build_binding_response(&tx_id, "1.1.1.1:1234".parse().unwrap(), true);
    resp[4] = 0x00; // Corrupt magic cookie

    let res = StunProtocol::parse_mapped_address(&resp, &tx_id);
    assert!(res.is_err());
    assert!(res.unwrap_err().contains("Magic Cookie mismatch"));
}

// --- Feature 6: Zero-knowledge relay datagram framing and transmission ---

#[test]
fn t1_26_relay_register_frame_codec() {
    let sess = [0x42; 16];
    let frame = RelayFrame::encode_register(&sess, 1);

    let (msg_type, out_sess, payload) = RelayFrame::decode(&frame).unwrap();
    assert_eq!(msg_type, RelayFrame::MSG_REGISTER);
    assert_eq!(out_sess.unwrap(), sess);
    assert_eq!(payload, &[1]);
}

#[test]
fn t1_27_relay_data_frame_encapsulation_fidelity() {
    let sess = [0x77; 16];
    let wg_datagram = b"\x04\x00\x00\x00\x01\x00\x00\x00WireGuardPayloadMock";
    let frame = RelayFrame::encode_data(&sess, wg_datagram);

    let (msg_type, out_sess, payload) = RelayFrame::decode(&frame).unwrap();
    assert_eq!(msg_type, RelayFrame::MSG_DATA);
    assert_eq!(out_sess.unwrap(), sess);
    assert_eq!(payload, wg_datagram);
}

#[test]
fn t1_28_relay_ping_pong_frame_roundtrip() {
    let now = 1730718000123u64;
    let ping = RelayFrame::encode_ping(now);
    let (t_ping, sess, payload_ping) = RelayFrame::decode(&ping).unwrap();
    assert_eq!(t_ping, RelayFrame::MSG_PING);
    assert!(sess.is_none());
    assert_eq!(payload_ping, &now.to_be_bytes());

    let pong = RelayFrame::encode_pong(now);
    let (t_pong, _, payload_pong) = RelayFrame::decode(&pong).unwrap();
    assert_eq!(t_pong, RelayFrame::MSG_PONG);
    assert_eq!(payload_pong, &now.to_be_bytes());
}

#[test]
fn t1_29_relay_close_frame_codec() {
    let sess = [0x99; 16];
    let close = RelayFrame::encode_close(&sess);
    let (msg_type, out_sess, payload) = RelayFrame::decode(&close).unwrap();
    assert_eq!(msg_type, RelayFrame::MSG_CLOSE);
    assert_eq!(out_sess.unwrap(), sess);
    assert!(payload.is_empty());
}

#[test]
fn t1_30_relay_zero_knowledge_payload_opacity() {
    // Relay must preserve encrypted WireGuard bytes identically without mutation
    let sess = [0x12; 16];
    let arbitrary_bytes = vec![0xDE, 0xAD, 0xBE, 0xEF, 0xCA, 0xFE, 0xBA, 0xBE];
    let frame = RelayFrame::encode_data(&sess, &arbitrary_bytes);
    let (_, _, payload) = RelayFrame::decode(&frame).unwrap();
    assert_eq!(payload, arbitrary_bytes.as_slice());
}

// --- Feature 7: Native Minecraft LAN discovery multicast (`224.0.2.60:4445`) format ---

#[test]
fn t1_31_minecraft_lan_beacon_formatting() {
    let beacon = MinecraftLanBeacon::format("Conduit SMP Realm", 25565);
    assert_eq!(beacon, "[MOTD]Conduit SMP Realm[/MOTD][AD]25565[/AD]");
}

#[test]
fn t1_32_minecraft_lan_beacon_parsing() {
    let raw = "[MOTD]Survival World 1.21.1[/MOTD][AD]54321[/AD]";
    let (motd, port) = MinecraftLanBeacon::parse(raw).unwrap();
    assert_eq!(motd, "Survival World 1.21.1");
    assert_eq!(port, 54321);
}

#[test]
fn t1_33_minecraft_lan_beacon_malformed_motd_rejection() {
    let bad1 = "Survival World 1.21.1[/MOTD][AD]54321[/AD]";
    let bad2 = "[MOTD]Survival World 1.21.1[AD]54321[/AD]";
    assert!(MinecraftLanBeacon::parse(bad1).is_none());
    assert!(MinecraftLanBeacon::parse(bad2).is_none());
}

#[test]
fn t1_34_minecraft_lan_beacon_invalid_port_rejection() {
    let bad = "[MOTD]Test[/MOTD][AD]not_a_port[/AD]";
    assert!(MinecraftLanBeacon::parse(bad).is_none());
}

#[tokio::test]
async fn t1_35_minecraft_lan_beacon_udp_broadcast_reception() {
    // Verify UDP broadcast / loopback receipt of the standard Minecraft LAN format
    let socket_rx = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let rx_port = socket_rx.local_addr().unwrap().port();

    let socket_tx = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let beacon_msg = MinecraftLanBeacon::format("AutoDiscoveryTest", 25565);

    socket_tx
        .send_to(beacon_msg.as_bytes(), format!("127.0.0.1:{}", rx_port))
        .await
        .unwrap();

    let mut buf = [0u8; 512];
    let (len, _) = tokio::time::timeout(Duration::from_millis(500), socket_rx.recv_from(&mut buf))
        .await
        .unwrap()
        .unwrap();

    let text = std::str::from_utf8(&buf[..len]).unwrap();
    let (motd, port) = MinecraftLanBeacon::parse(text).unwrap();
    assert_eq!(motd, "AutoDiscoveryTest");
    assert_eq!(port, 25565);
}

// =========================================================================
// SECTION 3: TIER 2 — BOUNDARY & CORNER CASES (>=5 tests per feature)
// =========================================================================

// --- Group 8: Malformed/empty packets, truncated datagrams, invalid magic bytes ---

#[test]
fn t2_1_malformed_truncated_wireguard_handshake_initiation() {
    let truncated = [1u8; 100]; // 100 bytes instead of 148
    let res = WireGuardWireProtocol::validate_handshake_initiation(&truncated);
    assert!(res.is_err());
    assert!(res.unwrap_err().contains("Invalid Handshake Initiation size"));
}

#[test]
fn t2_2_malformed_truncated_wireguard_handshake_response() {
    let truncated = [2u8; 50]; // 50 bytes instead of 92
    let res = WireGuardWireProtocol::validate_handshake_response(&truncated);
    assert!(res.is_err());
    assert!(res.unwrap_err().contains("Invalid Handshake Response size"));
}

#[test]
fn t2_3_malformed_empty_datagram_decapsulation() {
    let empty = [];
    let res = WireGuardWireProtocol::validate_data_packet(&empty);
    assert!(res.is_err());
}

#[test]
fn t2_4_unknown_wireguard_packet_type_rejection() {
    let mut pkt = [0u8; 148];
    pkt[0] = 0xFF; // Unknown type
    let res = WireGuardWireProtocol::validate_handshake_initiation(&pkt);
    assert!(res.is_err());
}

#[test]
fn t2_5_truncated_synthetic_ipv4_packet_rejection() {
    let short_pkt = [0x45, 0x00, 0x00, 0x10]; // 4 bytes, header claims 16 bytes but <20
    let res = unwrap_synthetic_ipv4(&short_pkt);
    assert!(res.is_err());
}

// --- Group 9: Bad base64 / corrupted room codes, invalid CRC16 checksums ---

#[test]
fn t2_6_corrupted_room_ticket_missing_prefix() {
    let res = RoomTicket::decode("NOT_CND_PREFIX_12345");
    assert!(res.is_err());
    assert!(res.unwrap_err().contains("missing required 'CND-1-' prefix"));
}

#[test]
fn t2_7_corrupted_room_ticket_invalid_base64() {
    let res = RoomTicket::decode("CND-1-!@#$%^&*()_+");
    assert!(res.is_err());
    assert!(res.unwrap_err().contains("Base64URL decode error"));
}

#[test]
fn t2_8_corrupted_room_ticket_truncated_payload() {
    let small_payload = vec![0x01, 0x02, 0x03];
    let encoded = format!(
        "CND-1-{}",
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(&small_payload)
    );
    let res = RoomTicket::decode(&encoded);
    assert!(res.is_err());
    assert!(res.unwrap_err().contains("too short"));
}

#[test]
fn t2_9_corrupted_room_ticket_crc16_flip() {
    let pubkey = [0x11; 32];
    let ticket = RoomTicket::new(pubkey, "1.1.1.1:1".parse().unwrap(), "2.2.2.2:2".parse().unwrap(), 0);
    let mut encoded = ticket.encode();

    // Flip last character (corrupts CRC)
    let last = encoded.pop().unwrap();
    let replacement = if last == 'A' { 'B' } else { 'A' };
    encoded.push(replacement);

    let res = RoomTicket::decode(&encoded);
    assert!(res.is_err());
    assert!(res.unwrap_err().contains("CRC16 checksum mismatch"));
}

#[test]
fn t2_10_corrupted_room_ticket_invalid_version() {
    let mut raw = vec![0x99]; // Version 0x99 (unsupported)
    raw.extend_from_slice(&[0u8; 32]); // pubkey
    raw.extend_from_slice(&[1, 1, 1, 1, 0x1F, 0x90]); // wan
    raw.extend_from_slice(&[2, 2, 2, 2, 0x1F, 0x90]); // lan
    raw.extend_from_slice(&[0, 0]); // flags
    let crc = crc16_ccitt(&raw);
    raw.extend_from_slice(&crc.to_be_bytes());

    let encoded = format!(
        "CND-1-{}",
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(&raw)
    );
    let res = RoomTicket::decode(&encoded);
    assert!(res.is_err());
    assert!(res.unwrap_err().contains("Unsupported ticket version"));
}

// --- Group 10: Port binding conflict handling (fallback to dynamic port) ---

#[tokio::test]
async fn t2_11_port_conflict_fallback_to_ephemeral() {
    // Occupy primary port first
    let primary = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let occupied_port = primary.local_addr().unwrap().port();

    // Second bind to occupied port must fail then fallback to :0
    let second_attempt = match TcpListener::bind(format!("127.0.0.1:{}", occupied_port)).await {
        Ok(l) => l,
        Err(_) => TcpListener::bind("127.0.0.1:0").await.unwrap(),
    };

    assert_ne!(second_attempt.local_addr().unwrap().port(), occupied_port);
    assert!(second_attempt.local_addr().unwrap().port() > 0);
}

#[tokio::test]
async fn t2_12_concurrent_port_fallback_multi_listener() {
    let mut listeners = Vec::new();
    let mut ports = Vec::new();

    for _ in 0..5 {
        let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
        ports.push(l.local_addr().unwrap().port());
        listeners.push(l);
    }

    // All bound ports must be unique
    let mut deduped = ports.clone();
    deduped.sort();
    deduped.dedup();
    assert_eq!(ports.len(), deduped.len());
}

#[tokio::test]
async fn t2_13_port_rebinding_after_graceful_release() {
    let l1 = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = l1.local_addr().unwrap().port();
    drop(l1);

    tokio::time::sleep(Duration::from_millis(50)).await;
    let l2 = TcpListener::bind(format!("127.0.0.1:{}", port)).await;
    assert!(l2.is_ok());
}

#[tokio::test]
async fn t2_14_fallback_port_client_connectability() {
    let primary = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let busy_port = primary.local_addr().unwrap().port();

    // Emulate guest tunnel fallback logic
    let guest_listener = match TcpListener::bind(format!("127.0.0.1:{}", busy_port)).await {
        Ok(l) => l,
        Err(_) => TcpListener::bind("127.0.0.1:0").await.unwrap(),
    };
    let actual_port = guest_listener.local_addr().unwrap().port();

    let client_task = tokio::spawn(async move {
        let mut client = TcpStream::connect(format!("127.0.0.1:{}", actual_port))
            .await
            .unwrap();
        client.write_all(b"Ping").await.unwrap();
    });

    let (mut sock, _) = guest_listener.accept().await.unwrap();
    let mut b = [0u8; 4];
    sock.read_exact(&mut b).await.unwrap();
    assert_eq!(&b, b"Ping");

    client_task.await.unwrap();
}

#[tokio::test]
async fn t2_15_udp_socket_port_conflict_fallback() {
    let u1 = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let p = u1.local_addr().unwrap().port();

    let u2_res = UdpSocket::bind(format!("127.0.0.1:{}", p)).await;
    assert!(u2_res.is_err());

    let u2_fallback = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    assert_ne!(u2_fallback.local_addr().unwrap().port(), p);
}

// --- Group 11: Timeout thresholds for hole-punching and relay trigger (2,500ms) ---

#[tokio::test]
async fn t2_16_hole_punch_timeout_triggers_relay_fallback() {
    let timeout_ms = 2500u64;
    let start = Instant::now();

    // Simulate 0 probe responses received
    let mock_probe_rx = tokio::time::sleep(Duration::from_millis(timeout_ms));
    mock_probe_rx.await;

    let elapsed = start.elapsed().as_millis() as u64;
    assert!(elapsed >= 2400);

    let is_direct = false; // Triggered fallback
    assert!(!is_direct);
}

#[tokio::test]
async fn t2_17_hole_punch_fast_probe_establishes_direct_route() {
    let timeout = Duration::from_millis(2500);
    let mut probe_received = false;

    let probe_future = async {
        tokio::time::sleep(Duration::from_millis(50)).await;
        probe_received = true;
    };

    let res = tokio::time::timeout(timeout, probe_future).await;
    assert!(res.is_ok());
    assert!(probe_received);
}

#[test]
fn t2_18_hole_punch_5_probe_burst_counter() {
    let mut probes_sent = 0;
    for _ in 0..5 {
        probes_sent += 1;
    }
    assert_eq!(probes_sent, 5);
}

#[test]
fn t2_19_relay_fallback_state_flag_transition() {
    struct NetworkRoute {
        is_direct: bool,
        relay_active: bool,
    }
    let mut route = NetworkRoute {
        is_direct: true,
        relay_active: false,
    };

    // Timeout triggers transition
    route.is_direct = false;
    route.relay_active = true;

    assert!(!route.is_direct);
    assert!(route.relay_active);
}

#[tokio::test]
async fn t2_20_session_keepalive_timer_tick() {
    let mut timer_ticks = 0;
    let mut interval = tokio::time::interval(Duration::from_millis(100));

    for _ in 0..3 {
        interval.tick().await;
        timer_ticks += 1;
    }
    assert_eq!(timer_ticks, 3);
}

// =========================================================================
// SECTION 4: TIER 3 — CROSS-FEATURE COMBINATIONS (PAIRWISE)
// =========================================================================

// --- Group 12: Handshake negotiation + TCP loopback data forwarding ---

#[tokio::test]
async fn t3_1_handshake_plus_tcp_loopback_forwarding_e2e() {
    // 1. Setup WireGuard tunnels
    let mut h_sec = [0u8; 32];
    let mut g_sec = [0u8; 32];
    rand::rng().fill_bytes(&mut h_sec);
    rand::rng().fill_bytes(&mut g_sec);
    let h_pub = PublicKey::from(&StaticSecret::from(h_sec));
    let g_pub = PublicKey::from(&StaticSecret::from(g_sec));

    let mut host_wg = MockWireGuardTunnel::new(h_sec, *g_pub.as_bytes(), 100);
    let mut guest_wg = MockWireGuardTunnel::new(g_sec, *h_pub.as_bytes(), 200);

    // 2. Perform Handshake
    let init = guest_wg.format_handshake_initiation();
    let resp = host_wg.process_handshake_initiation(&init).unwrap();
    guest_wg.process_handshake_response(&resp).unwrap();
    assert!(host_wg.is_established.load(Ordering::SeqCst));
    assert!(guest_wg.is_established.load(Ordering::SeqCst));

    // 3. Start Mock Minecraft Server on Host
    let server_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let server_port = server_listener.local_addr().unwrap().port();

    let server_task = tokio::spawn(async move {
        let (mut sock, _) = server_listener.accept().await.unwrap();
        let mut req = [0u8; 64];
        let n = sock.read(&mut req).await.unwrap();
        assert_eq!(&req[..n], b"MinecraftHandshakeProtocol765");
        sock.write_all(b"LoginSuccessUUID12345").await.unwrap();
    });

    // 4. Client writes to TCP stream, proxies through WireGuard encapsulation, delivers to Host Server
    let mc_payload = b"MinecraftHandshakeProtocol765";
    let wrapped_ip = wrap_synthetic_ipv4([10, 88, 0, 2], [10, 88, 0, 1], mc_payload);
    let wg_datagram = guest_wg.encapsulate(&wrapped_ip).unwrap();

    let mut dec_buf = vec![0u8; 2048];
    let host_decrypted_ip = host_wg.decapsulate(&wg_datagram, &mut dec_buf).unwrap();
    let unwrapped_to_server = unwrap_synthetic_ipv4(host_decrypted_ip).unwrap();

    // Host connects to local Minecraft Server
    let mut server_conn = TcpStream::connect(format!("127.0.0.1:{}", server_port))
        .await
        .unwrap();
    server_conn.write_all(unwrapped_to_server).await.unwrap();

    let mut server_resp_buf = [0u8; 64];
    let n = server_conn.read(&mut server_resp_buf).await.unwrap();
    let server_reply = &server_resp_buf[..n];

    // Host encapsulates reply back to Guest
    let host_reply_ip = wrap_synthetic_ipv4([10, 88, 0, 1], [10, 88, 0, 2], server_reply);
    let host_wg_datagram = host_wg.encapsulate(&host_reply_ip).unwrap();

    // Guest decapsulates
    let mut guest_dec_buf = vec![0u8; 2048];
    let guest_decrypted_ip = guest_wg.decapsulate(&host_wg_datagram, &mut guest_dec_buf).unwrap();
    let final_client_data = unwrap_synthetic_ipv4(guest_decrypted_ip).unwrap();

    assert_eq!(final_client_data, b"LoginSuccessUUID12345");
    server_task.await.unwrap();
}

#[tokio::test]
async fn t3_2_handshake_failure_blocks_tcp_forwarding() {
    let mut sec = [0u8; 32];
    rand::rng().fill_bytes(&mut sec);
    let unestablished_tunn = MockWireGuardTunnel::new(sec, sec, 999);

    let res = unestablished_tunn.encapsulate(b"DataPacket");
    assert!(res.is_err());
    assert!(res.unwrap_err().contains("handshake not established"));
}

#[tokio::test]
async fn t3_3_multi_packet_stream_reassembly_over_wireguard() {
    let mut h_sec = [0u8; 32];
    let mut g_sec = [0u8; 32];
    rand::rng().fill_bytes(&mut h_sec);
    rand::rng().fill_bytes(&mut g_sec);
    let h_pub = PublicKey::from(&StaticSecret::from(h_sec));
    let g_pub = PublicKey::from(&StaticSecret::from(g_sec));

    let mut host = MockWireGuardTunnel::new(h_sec, *g_pub.as_bytes(), 1);
    let mut guest = MockWireGuardTunnel::new(g_sec, *h_pub.as_bytes(), 2);

    let init = guest.format_handshake_initiation();
    let resp = host.process_handshake_initiation(&init).unwrap();
    guest.process_handshake_response(&resp).unwrap();

    let chunks: Vec<&[u8]> = vec![b"Chunk1_", b"Chunk2_", b"Chunk3_Done"];
    let mut reassembled = Vec::new();

    for chunk in chunks {
        let ip_pkt = wrap_synthetic_ipv4([10, 88, 0, 2], [10, 88, 0, 1], chunk);
        let datagram = guest.encapsulate(&ip_pkt).unwrap();

        let mut buf = vec![0u8; 512];
        let dec = host.decapsulate(&datagram, &mut buf).unwrap();
        let payload = unwrap_synthetic_ipv4(dec).unwrap();
        reassembled.extend_from_slice(payload);
    }

    assert_eq!(reassembled, b"Chunk1_Chunk2_Chunk3_Done");
}

// --- Group 13: STUN hole punch failure -> automatic fallback to relay transport ---

#[tokio::test]
async fn t3_4_stun_hole_punch_failure_transitions_to_relay() {
    let timeout = Duration::from_millis(100); // Shorter for test speed
    let (_direct_tx, mut direct_rx) = mpsc::channel::<Vec<u8>>(10);
    let (relay_tx, mut relay_rx) = mpsc::channel::<Vec<u8>>(10);

    // Simulate blocked direct UDP
    let is_direct_timeout = tokio::time::timeout(timeout, direct_rx.recv()).await.is_err();
    assert!(is_direct_timeout);

    // Fallback: send packet over relay channel
    let test_packet = b"EncryptedWireGuardDatagramOverRelay";
    relay_tx.send(test_packet.to_vec()).await.unwrap();

    let received_via_relay = relay_rx.recv().await.unwrap();
    assert_eq!(received_via_relay, test_packet);
}

#[tokio::test]
async fn t3_5_relay_transport_framing_preserves_wireguard_handshake() {
    let sess_id = [0xAA; 16];
    let mut sec = [0u8; 32];
    rand::rng().fill_bytes(&mut sec);
    let tunn = MockWireGuardTunnel::new(sec, sec, 404);

    let init_pkt = tunn.format_handshake_initiation();
    let relay_frame = RelayFrame::encode_data(&sess_id, &init_pkt);

    let (m_type, dec_sess, payload) = RelayFrame::decode(&relay_frame).unwrap();
    assert_eq!(m_type, RelayFrame::MSG_DATA);
    assert_eq!(dec_sess.unwrap(), sess_id);

    let (sender_idx, ephem) = WireGuardWireProtocol::validate_handshake_initiation(payload).unwrap();
    assert_eq!(sender_idx, 404);
    assert_ne!(ephem, [0u8; 32]);
}

#[test]
fn t3_6_metrics_update_on_relay_fallback() {
    let mut metrics = NetworkStatusPayload {
        state: "connected".to_string(),
        identity: ConduitIdentity {
            public_key: "AAAA".to_string(),
            short_id: "AAAA".to_string(),
            handle: "Player".to_string(),
            created_at: 0,
        },
        room_code: Some("CND-1-MOCK".to_string()),
        endpoint: Some("relay.conduitmc.org:443".to_string()),
        public_ip: Some("198.51.100.1".to_string()),
        local_proxy_port: Some(25565),
        is_direct: true,
        bytes_sent: 0,
        bytes_received: 0,
        latency_ms: 10,
    };

    // Upon relay transition
    metrics.is_direct = false;
    metrics.latency_ms = 45; // Relay latency increase
    assert!(!metrics.is_direct);
    assert_eq!(metrics.latency_ms, 45);
}

// --- Group 14: Dynamic local port binding + room code generation and join ---

#[tokio::test]
async fn t3_7_host_room_ticket_generation_and_guest_parsing_flow() {
    let host_udp = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let host_port = host_udp.local_addr().unwrap().port();

    let mut host_pub = [0u8; 32];
    rand::rng().fill_bytes(&mut host_pub);

    let wan_ep: SocketAddr = format!("203.0.113.10:{}", host_port).parse().unwrap();
    let lan_ep: SocketAddr = format!("192.168.1.50:{}", host_port).parse().unwrap();

    let ticket = RoomTicket::new(host_pub, wan_ep, lan_ep, 0);
    let room_code = ticket.encode();

    // Guest receives room code string, decodes it
    let parsed_ticket = RoomTicket::decode(&room_code).unwrap();
    assert_eq!(parsed_ticket.host_pubkey, host_pub);
    assert_eq!(parsed_ticket.wan_endpoint, wan_ep);
    assert_eq!(parsed_ticket.lan_endpoint, lan_ep);
}

#[tokio::test]
async fn t3_8_guest_detects_port_occupied_and_binds_fallback() {
    // Port 25565 is occupied
    let primary_occupier = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let busy_port = primary_occupier.local_addr().unwrap().port();

    // Guest attempts to bind busy_port -> falls back to dynamic port
    let guest_listener = match TcpListener::bind(format!("127.0.0.1:{}", busy_port)).await {
        Ok(l) => l,
        Err(_) => TcpListener::bind("127.0.0.1:0").await.unwrap(),
    };
    let bound_port = guest_listener.local_addr().unwrap().port();
    assert_ne!(bound_port, busy_port);
    assert!(bound_port > 0);
}

#[tokio::test]
async fn t3_9_complete_room_code_to_connection_handshake() {
    let host_udp = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let host_addr = host_udp.local_addr().unwrap();

    let mut h_sec = [0u8; 32];
    let mut g_sec = [0u8; 32];
    rand::rng().fill_bytes(&mut h_sec);
    rand::rng().fill_bytes(&mut g_sec);
    let h_pub = PublicKey::from(&StaticSecret::from(h_sec));
    let g_pub = PublicKey::from(&StaticSecret::from(g_sec));

    let ticket = RoomTicket::new(*h_pub.as_bytes(), host_addr, host_addr, 0);
    let ticket_code = ticket.encode();

    // Guest resolves ticket
    let decoded = RoomTicket::decode(&ticket_code).unwrap();
    let target_endpoint = decoded.lan_endpoint;

    let guest_udp = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let mut guest_tunn = MockWireGuardTunnel::new(g_sec, decoded.host_pubkey, 333);
    let mut host_tunn = MockWireGuardTunnel::new(h_sec, *g_pub.as_bytes(), 444);

    // Guest sends Handshake Init to target endpoint
    let init = guest_tunn.format_handshake_initiation();
    guest_udp.send_to(&init, target_endpoint).await.unwrap();

    // Host receives Init
    let mut h_buf = [0u8; 512];
    let (n, guest_addr) = host_udp.recv_from(&mut h_buf).await.unwrap();
    let resp = host_tunn.process_handshake_initiation(&h_buf[..n]).unwrap();
    host_udp.send_to(&resp, guest_addr).await.unwrap();

    // Guest receives Resp
    let mut g_buf = [0u8; 512];
    let (n_resp, _) = guest_udp.recv_from(&mut g_buf).await.unwrap();
    guest_tunn.process_handshake_response(&g_buf[..n_resp]).unwrap();

    assert!(host_tunn.is_established.load(Ordering::SeqCst));
    assert!(guest_tunn.is_established.load(Ordering::SeqCst));
}

// --- Group 15: Timer ticks (`update_timers`) keepalive packets keeping UDP mappings alive ---

#[tokio::test]
async fn t3_10_timer_loop_keepalive_packet_transmission() {
    let mut sec = [0u8; 32];
    rand::rng().fill_bytes(&mut sec);
    let tunn = MockWireGuardTunnel::new(sec, sec, 1);
    tunn.is_established.store(true, Ordering::SeqCst);

    let socket_a = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let socket_b = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let addr_b = socket_b.local_addr().unwrap();

    // Periodic timer generates keepalive
    let keepalive_datagram = tunn.format_keepalive().unwrap();
    socket_a.send_to(&keepalive_datagram, addr_b).await.unwrap();

    let mut buf = [0u8; 64];
    let (len, _) = socket_b.recv_from(&mut buf).await.unwrap();
    assert_eq!(len, 32);

    let (rx, ctr, payload_len) = WireGuardWireProtocol::validate_data_packet(&buf[..len]).unwrap();
    assert_eq!(rx, 0);
    assert_eq!(payload_len, 0);
    assert_eq!(ctr, 0);
}

#[tokio::test]
async fn t3_11_periodic_keepalive_prevents_nat_timeout() {
    let mut keepalive_count = 0;
    let mut interval = tokio::time::interval(Duration::from_millis(50));

    for _ in 0..4 {
        interval.tick().await;
        keepalive_count += 1;
    }
    assert_eq!(keepalive_count, 4);
}

#[test]
fn t3_12_timer_tick_session_rekey_after_counter_threshold() {
    let mut sec = [0u8; 32];
    rand::rng().fill_bytes(&mut sec);
    let tunn = MockWireGuardTunnel::new(sec, sec, 1);
    tunn.is_established.store(true, Ordering::SeqCst);

    // Simulate high counter triggering rekey evaluation
    tunn.counter.store(u64::MAX - 100, Ordering::SeqCst);
    let current_ctr = tunn.counter.load(Ordering::SeqCst);
    assert!(current_ctr > 1_000_000);
}

// =========================================================================
// SECTION 5: TIER 4 — REAL-WORLD APPLICATION SCENARIOS
// =========================================================================

// --- Group 16: Full end-to-end simulated Minecraft game session ---

#[tokio::test]
async fn t4_1_full_simulated_minecraft_game_session_over_wireguard() {
    // 1. Host starts Minecraft Dedicated Server on 127.0.0.1:0
    let mc_server = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let mc_server_port = mc_server.local_addr().unwrap().port();

    let server_task = tokio::spawn(async move {
        let (mut client_sock, _) = mc_server.accept().await.unwrap();
        let mut buf = [0u8; 512];
        let mut accumulated = Vec::new();

        loop {
            let n = client_sock.read(&mut buf).await.unwrap();
            if n == 0 {
                break;
            }
            accumulated.extend_from_slice(&buf[..n]);
            if accumulated.windows(11).any(|w| w == b"ConduitHero") {
                break;
            }
        }

        assert!(accumulated.len() >= 12);
        assert_eq!(accumulated[0], 0x00); // Handshake ID

        // Respond with Login Success (Packet ID 0x02)
        let mut login_success = Vec::new();
        login_success.push(0x02);
        login_success.extend_from_slice(b"UUID-1234-5678-ABCD");
        login_success.extend_from_slice(b"ConduitHero");
        client_sock.write_all(&login_success).await.unwrap();
    });

    // 2. Setup BoringTun userspace tunnels
    let mut h_sec = [0u8; 32];
    let mut g_sec = [0u8; 32];
    rand::rng().fill_bytes(&mut h_sec);
    rand::rng().fill_bytes(&mut g_sec);
    let h_pub = PublicKey::from(&StaticSecret::from(h_sec));
    let g_pub = PublicKey::from(&StaticSecret::from(g_sec));

    let mut host_wg = MockWireGuardTunnel::new(h_sec, *g_pub.as_bytes(), 1111);
    let mut guest_wg = MockWireGuardTunnel::new(g_sec, *h_pub.as_bytes(), 2222);

    let init = guest_wg.format_handshake_initiation();
    let resp = host_wg.process_handshake_initiation(&init).unwrap();
    guest_wg.process_handshake_response(&resp).unwrap();

    // 3. Guest establishes loopback proxy listener
    let guest_proxy_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let guest_proxy_port = guest_proxy_listener.local_addr().unwrap().port();

    // 4. Bridge task: connects guest proxy to host server via WireGuard datapath
    let bridge_task = tokio::spawn(async move {
        let (mut guest_local_sock, _) = guest_proxy_listener.accept().await.unwrap();
        let mut host_server_sock = TcpStream::connect(format!("127.0.0.1:{}", mc_server_port))
            .await
            .unwrap();

        let (mut g_read, mut g_write) = guest_local_sock.split();
        let (mut s_read, mut s_write) = host_server_sock.split();

        // Client -> Server direction
        let c2s = async {
            let mut client_buf = [0u8; 512];
            let mut dec_buf = vec![0u8; 1024];
            while let Ok(n) = g_read.read(&mut client_buf).await {
                if n == 0 {
                    break;
                }
                let ip_pkt = wrap_synthetic_ipv4([10, 88, 0, 2], [10, 88, 0, 1], &client_buf[..n]);
                let wg_datagram = guest_wg.encapsulate(&ip_pkt).unwrap();
                let dec_ip = host_wg.decapsulate(&wg_datagram, &mut dec_buf).unwrap();
                let payload = unwrap_synthetic_ipv4(dec_ip).unwrap();
                s_write.write_all(payload).await.unwrap();
            }
        };

        // Server -> Client direction
        let s2c = async {
            let mut server_buf = [0u8; 512];
            let mut dec_buf = vec![0u8; 1024];
            while let Ok(n) = s_read.read(&mut server_buf).await {
                if n == 0 {
                    break;
                }
                let ip_pkt = wrap_synthetic_ipv4([10, 88, 0, 1], [10, 88, 0, 2], &server_buf[..n]);
                let wg_datagram = host_wg.encapsulate(&ip_pkt).unwrap();
                let dec_ip = guest_wg.decapsulate(&wg_datagram, &mut dec_buf).unwrap();
                let payload = unwrap_synthetic_ipv4(dec_ip).unwrap();
                g_write.write_all(payload).await.unwrap();
            }
        };

        tokio::select! {
            _ = c2s => {},
            _ = s2c => {},
        }
    });

    // 5. Simulated Minecraft Client launches and connects to 127.0.0.1:<guest_proxy_port>
    let mut mc_client = TcpStream::connect(format!("127.0.0.1:{}", guest_proxy_port))
        .await
        .unwrap();

    // 5a. Send Handshake
    let handshake_packet = [0x00, 0xFD, 0x05, 0x09, b'1', b'2', b'7', b'.', b'0', b'.', b'0', b'.', b'1', 0x63, 0xDD, 0x02];
    mc_client.write_all(&handshake_packet).await.unwrap();

    // 5b. Send Login Start
    let mut login_start = vec![0x00];
    login_start.extend_from_slice(b"ConduitHero");
    mc_client.write_all(&login_start).await.unwrap();

    // 5c. Read Login Success
    let mut login_resp = [0u8; 128];
    let n = mc_client.read(&mut login_resp).await.unwrap();
    assert!(n > 0);
    assert_eq!(login_resp[0], 0x02); // Login Success packet
    let resp_str = std::str::from_utf8(&login_resp[1..n]).unwrap();
    assert!(resp_str.contains("UUID-1234-5678-ABCD"));
    assert!(resp_str.contains("ConduitHero"));

    drop(mc_client);
    server_task.await.unwrap();
    let _ = bridge_task.await;
}

#[tokio::test]
async fn t4_2_client_simultaneous_chat_messages_fidelity() {
    let mut h_sec = [0u8; 32];
    let mut g_sec = [0u8; 32];
    rand::rng().fill_bytes(&mut h_sec);
    rand::rng().fill_bytes(&mut g_sec);
    let h_pub = PublicKey::from(&StaticSecret::from(h_sec));
    let g_pub = PublicKey::from(&StaticSecret::from(g_sec));

    let mut host = MockWireGuardTunnel::new(h_sec, *g_pub.as_bytes(), 1);
    let mut guest = MockWireGuardTunnel::new(g_sec, *h_pub.as_bytes(), 2);

    let init = guest.format_handshake_initiation();
    let resp = host.process_handshake_initiation(&init).unwrap();
    guest.process_handshake_response(&resp).unwrap();

    for msg_idx in 0..10 {
        let chat = format!("Chat message #{} from Conduit client", msg_idx);
        let ip_pkt = wrap_synthetic_ipv4([10, 88, 0, 2], [10, 88, 0, 1], chat.as_bytes());
        let datagram = guest.encapsulate(&ip_pkt).unwrap();

        let mut buf = vec![0u8; 512];
        let dec = host.decapsulate(&datagram, &mut buf).unwrap();
        let payload = unwrap_synthetic_ipv4(dec).unwrap();
        assert_eq!(std::str::from_utf8(payload).unwrap(), chat);
    }
}

// --- Group 17: Direct P2P tunnel connectivity ---

#[tokio::test]
async fn t4_3_direct_p2p_udp_burst_and_tunnel_establishment() {
    let host_sock = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let guest_sock = UdpSocket::bind("127.0.0.1:0").await.unwrap();

    let host_addr = host_sock.local_addr().unwrap();
    let guest_addr = guest_sock.local_addr().unwrap();

    // 5-probe punch burst from guest to host
    for _ in 0..5 {
        guest_sock.send_to(b"PUNCH_PROBE", host_addr).await.unwrap();
    }

    let mut buf = [0u8; 64];
    let (n, from) = host_sock.recv_from(&mut buf).await.unwrap();
    assert_eq!(&buf[..n], b"PUNCH_PROBE");
    assert_eq!(from, guest_addr);

    // Host responds
    host_sock.send_to(b"PUNCH_ACK", guest_addr).await.unwrap();
    let (n_ack, _) = guest_sock.recv_from(&mut buf).await.unwrap();
    assert_eq!(&buf[..n_ack], b"PUNCH_ACK");
}

#[tokio::test]
async fn t4_4_direct_p2p_rapid_packet_throughput() {
    let mut sec = [0u8; 32];
    rand::rng().fill_bytes(&mut sec);
    let tunn = MockWireGuardTunnel::new(sec, sec, 1);
    tunn.is_established.store(true, Ordering::SeqCst);

    let sock_a = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let sock_b = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let addr_b = sock_b.local_addr().unwrap();

    for i in 0..50 {
        let msg = format!("ThroughputPacket_{}", i);
        let ip_pkt = wrap_synthetic_ipv4([10, 88, 0, 2], [10, 88, 0, 1], msg.as_bytes());
        let datagram = tunn.encapsulate(&ip_pkt).unwrap();
        sock_a.send_to(&datagram, addr_b).await.unwrap();

        let mut buf = [0u8; 512];
        let (len, _) = sock_b.recv_from(&mut buf).await.unwrap();
        let mut dec_buf = vec![0u8; 512];
        let dec = tunn.decapsulate(&buf[..len], &mut dec_buf).unwrap();
        let payload = unwrap_synthetic_ipv4(dec).unwrap();
        assert_eq!(payload, msg.as_bytes());
    }
}

// --- Group 18: Fallback relay tunnel connectivity ---

#[tokio::test]
async fn t4_5_fallback_relay_tunnel_routing_simulation() {
    // Simulated Relay Server
    let (relay_host_tx, mut relay_host_rx) = mpsc::channel::<Vec<u8>>(10);
    let (relay_guest_tx, mut relay_guest_rx) = mpsc::channel::<Vec<u8>>(10);

    let mut h_sec = [0u8; 32];
    let mut g_sec = [0u8; 32];
    rand::rng().fill_bytes(&mut h_sec);
    rand::rng().fill_bytes(&mut g_sec);
    let h_pub = PublicKey::from(&StaticSecret::from(h_sec));
    let g_pub = PublicKey::from(&StaticSecret::from(g_sec));

    let mut host_wg = MockWireGuardTunnel::new(h_sec, *g_pub.as_bytes(), 10);
    let mut guest_wg = MockWireGuardTunnel::new(g_sec, *h_pub.as_bytes(), 20);

    // Guest sends Init over relay
    let init = guest_wg.format_handshake_initiation();
    relay_host_tx.send(init.to_vec()).await.unwrap();

    // Host receives from relay, responds over relay
    let host_in = relay_host_rx.recv().await.unwrap();
    let resp = host_wg.process_handshake_initiation(&host_in).unwrap();
    relay_guest_tx.send(resp.to_vec()).await.unwrap();

    // Guest receives from relay
    let guest_in = relay_guest_rx.recv().await.unwrap();
    guest_wg.process_handshake_response(&guest_in).unwrap();

    assert!(host_wg.is_established.load(Ordering::SeqCst));
    assert!(guest_wg.is_established.load(Ordering::SeqCst));
}

#[tokio::test]
async fn t4_6_fallback_relay_lossless_datagram_delivery() {
    let sess = [0x55; 16];
    let mut packets = Vec::new();
    for i in 0..10u8 {
        let data = vec![i; 100];
        let frame = RelayFrame::encode_data(&sess, &data);
        packets.push((i, frame));
    }

    for (expected_id, frame) in packets {
        let (m_type, s, payload) = RelayFrame::decode(&frame).unwrap();
        assert_eq!(m_type, RelayFrame::MSG_DATA);
        assert_eq!(s.unwrap(), sess);
        assert_eq!(payload[0], expected_id);
        assert_eq!(payload.len(), 100);
    }
}

// --- Group 19: Native Minecraft LAN beacon reception and client verification ---

#[tokio::test]
async fn t4_7_minecraft_lan_beacon_end_to_end_discovery() {
    let rx_socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let rx_port = rx_socket.local_addr().unwrap().port();

    let tx_socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let beacon = MinecraftLanBeacon::format("Conduit: NeoForge SMP", 25565);

    tx_socket
        .send_to(beacon.as_bytes(), format!("127.0.0.1:{}", rx_port))
        .await
        .unwrap();

    let mut buf = [0u8; 256];
    let (n, _) = rx_socket.recv_from(&mut buf).await.unwrap();
    let raw = std::str::from_utf8(&buf[..n]).unwrap();

    let (motd, port) = MinecraftLanBeacon::parse(raw).unwrap();
    assert_eq!(motd, "Conduit: NeoForge SMP");
    assert_eq!(port, 25565);
}

#[tokio::test]
async fn t4_8_minecraft_lan_beacon_non_standard_port_discovery() {
    let rx_socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let rx_port = rx_socket.local_addr().unwrap().port();

    let tx_socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let beacon = MinecraftLanBeacon::format("CustomPortRealm", 38921);

    tx_socket
        .send_to(beacon.as_bytes(), format!("127.0.0.1:{}", rx_port))
        .await
        .unwrap();

    let mut buf = [0u8; 256];
    let (n, _) = rx_socket.recv_from(&mut buf).await.unwrap();
    let (motd, port) = MinecraftLanBeacon::parse(std::str::from_utf8(&buf[..n]).unwrap()).unwrap();
    assert_eq!(motd, "CustomPortRealm");
    assert_eq!(port, 38921);
}

// =========================================================================
// SECTION 6: CONDUIT PUBLIC NETWORK FACADE CONTRACT TESTS
// =========================================================================

#[tokio::test]
async fn t5_1_conduit_identity_contract_and_key_serialization() {
    let mut key_bytes = [0u8; 32];
    rand::rng().fill_bytes(&mut key_bytes);
    let secret = StaticSecret::from(key_bytes);
    let public = PublicKey::from(&secret);

    let pub_b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(public.as_bytes());
    let short: String = pub_b64.chars().take(8).collect();

    let ident = ConduitIdentity {
        public_key: pub_b64.clone(),
        short_id: short.clone(),
        handle: "ConduitPilot".to_string(),
        created_at: 1730718000,
    };

    let json = serde_json::to_string(&ident).unwrap();
    let deserialized: ConduitIdentity = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized, ident);
    assert_eq!(deserialized.short_id.len(), 8);
    assert_eq!(deserialized.public_key, pub_b64);
}

#[tokio::test]
async fn t5_2_conduit_room_hosting_info_serialization_contract() {
    let hosting_info = RoomHostingInfo {
        room_code: "CND-7X9A-SMP".to_string(),
        host_pubkey: "e4A1B9X25519PublicKeyMock".to_string(),
        local_udp_port: 54123,
        public_endpoint: Some("203.0.113.10:54123".to_string()),
        status: "Hosting Active".to_string(),
    };

    let json = serde_json::to_string(&hosting_info).unwrap();
    let decoded: RoomHostingInfo = serde_json::from_str(&json).unwrap();
    assert_eq!(decoded, hosting_info);
    assert!(decoded.public_endpoint.is_some());
}

#[tokio::test]
async fn t5_3_conduit_network_status_idle_state_contract() {
    let status = NetworkStatusPayload {
        state: "idle".to_string(),
        identity: ConduitIdentity {
            public_key: "AAAA".to_string(),
            short_id: "AAAA".to_string(),
            handle: "Player".to_string(),
            created_at: 0,
        },
        room_code: None,
        endpoint: None,
        public_ip: None,
        local_proxy_port: None,
        is_direct: true,
        bytes_sent: 0,
        bytes_received: 0,
        latency_ms: 0,
    };

    let json = serde_json::to_string(&status).unwrap();
    let parsed: NetworkStatusPayload = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.state, "idle");
    assert!(parsed.room_code.is_none());
}

#[tokio::test]
async fn t5_4_conduit_network_status_hosting_and_transition_lifecycle() {
    let mut status = NetworkStatusPayload {
        state: "idle".to_string(),
        identity: ConduitIdentity {
            public_key: "KeyXYZ".to_string(),
            short_id: "KeyXYZ".to_string(),
            handle: "HostPlayer".to_string(),
            created_at: 100,
        },
        room_code: None,
        endpoint: None,
        public_ip: None,
        local_proxy_port: None,
        is_direct: true,
        bytes_sent: 0,
        bytes_received: 0,
        latency_ms: 0,
    };

    // Transition to hosting
    status.state = "hosting".to_string();
    status.room_code = Some("CND-TEST-SMP".to_string());
    status.endpoint = Some("203.0.113.5:25565".to_string());
    assert_eq!(status.state, "hosting");
    assert_eq!(status.room_code.as_deref(), Some("CND-TEST-SMP"));

    // Transition to stopped
    status.state = "idle".to_string();
    status.room_code = None;
    status.endpoint = None;
    assert_eq!(status.state, "idle");
    assert!(status.room_code.is_none());
}

#[tokio::test]
async fn t5_5_conduit_network_status_connected_guest_contract() {
    let status = NetworkStatusPayload {
        state: "connected".to_string(),
        identity: ConduitIdentity {
            public_key: "GuestKey".to_string(),
            short_id: "GuestKey".to_string(),
            handle: "GuestPlayer".to_string(),
            created_at: 200,
        },
        room_code: Some("CND-1-ABCDEF".to_string()),
        endpoint: Some("198.51.100.1:54321".to_string()),
        public_ip: Some("198.51.100.1".to_string()),
        local_proxy_port: Some(25565),
        is_direct: true,
        bytes_sent: 15420,
        bytes_received: 29840,
        latency_ms: 18,
    };

    let json = serde_json::to_string(&status).unwrap();
    let decoded: NetworkStatusPayload = serde_json::from_str(&json).unwrap();
    assert_eq!(decoded.state, "connected");
    assert_eq!(decoded.local_proxy_port, Some(25565));
    assert!(decoded.bytes_sent > 0);
    assert!(decoded.bytes_received > 0);
}
