//! NAT Traversal & Endpoint Discovery via RFC 5389 STUN and Bidirectional UDP Hole Punching.
//!
//! Provides:
//! - Dual-ended STUN discovery against Google public STUN servers
//! - Strict RFC 5389 attribute parsing: XOR-MAPPED-ADDRESS (0x0020) and MAPPED-ADDRESS (0x0001) for both IPv4 and IPv6
//! - Local LAN candidate discovery
//! - Bidirectional 5-probe UDP hole punching bursts with distinct probe headers (`b"CND_PUNCH\x01"`)
//! - Latency/proximity candidate selection (LAN preferred over WAN when both accessible)

use rand::Rng;
use serde::{Deserialize, Serialize};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::time::{Duration, Instant};
use tokio::net::UdpSocket;

pub const STUN_MAGIC_COOKIE: [u8; 4] = [0x21, 0x12, 0xA4, 0x42];
pub const STUN_BINDING_REQUEST: u16 = 0x0001;
pub const STUN_BINDING_RESPONSE: u16 = 0x0101;
pub const STUN_ATTR_MAPPED_ADDRESS: u16 = 0x0001;
pub const STUN_ATTR_XOR_MAPPED_ADDRESS: u16 = 0x0020;

pub const PUNCH_MAGIC: &[u8; 10] = b"CND_PUNCH\x01";
pub const PUNCH_MSG_PROBE: u8 = 0x01;
pub const PUNCH_MSG_ACK: u8 = 0x02;
pub const PUNCH_BURST_COUNT: usize = 5;
pub const DEFAULT_PUNCH_TIMEOUT_MS: u64 = 2500;

#[derive(Debug)]
pub enum PunchError {
    Timeout,
    NoCandidates,
    Io(std::io::Error),
    Stun(String),
    Parse(String),
}

impl std::fmt::Display for PunchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Timeout => write!(f, "UDP hole punching timed out (2,500ms threshold reached)"),
            Self::NoCandidates => write!(f, "No valid punch candidate endpoints provided"),
            Self::Io(e) => write!(f, "I/O error during punch operation: {}", e),
            Self::Stun(s) => write!(f, "STUN protocol error: {}", s),
            Self::Parse(s) => write!(f, "Punch packet parse error: {}", s),
        }
    }
}

impl std::error::Error for PunchError {}

impl From<std::io::Error> for PunchError {
    fn from(err: std::io::Error) -> Self {
        Self::Io(err)
    }
}

/// Discovered network candidate endpoints for P2P connection establishment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PunchCandidate {
    pub lan: Option<SocketAddr>,
    pub wan: Option<SocketAddr>,
}

impl PunchCandidate {
    pub fn new(lan: Option<SocketAddr>, wan: Option<SocketAddr>) -> Self {
        Self { lan, wan }
    }

    pub fn endpoints(&self) -> Vec<SocketAddr> {
        let mut list = Vec::with_capacity(2);
        if let Some(lan) = self.lan {
            list.push(lan);
        }
        if let Some(wan) = self.wan {
            if !list.contains(&wan) {
                list.push(wan);
            }
        }
        list
    }
}

/// Builds an RFC 5389 STUN Binding Request header.
pub fn build_stun_request(transaction_id: &[u8; 12]) -> [u8; 20] {
    let mut req = [0u8; 20];
    req[0] = (STUN_BINDING_REQUEST >> 8) as u8;
    req[1] = (STUN_BINDING_REQUEST & 0xFF) as u8;
    req[2] = 0x00;
    req[3] = 0x00; // Message Length: 0
    req[4..8].copy_from_slice(&STUN_MAGIC_COOKIE);
    req[8..20].copy_from_slice(transaction_id);
    req
}

/// Builds a synthetic RFC 5389 STUN Binding Response for test fixtures and local reflection.
pub fn build_stun_response(
    transaction_id: &[u8; 12],
    endpoint: SocketAddr,
    use_xor: bool,
) -> Vec<u8> {
    let mut resp = Vec::with_capacity(36);
    resp.extend_from_slice(&STUN_BINDING_RESPONSE.to_be_bytes());

    let attr_len: u16 = match endpoint {
        SocketAddr::V4(_) => 8,
        SocketAddr::V6(_) => 20,
    };

    // Header length excludes 20-byte STUN header
    let total_attr_len = 4 + attr_len;
    resp.extend_from_slice(&total_attr_len.to_be_bytes());
    resp.extend_from_slice(&STUN_MAGIC_COOKIE);
    resp.extend_from_slice(transaction_id);

    let attr_type = if use_xor {
        STUN_ATTR_XOR_MAPPED_ADDRESS
    } else {
        STUN_ATTR_MAPPED_ADDRESS
    };
    resp.extend_from_slice(&attr_type.to_be_bytes());
    resp.extend_from_slice(&attr_len.to_be_bytes());

    match endpoint {
        SocketAddr::V4(v4) => {
            resp.push(0x00); // Reserved
            resp.push(0x01); // IPv4
            if use_xor {
                let xor_port = v4.port()
                    ^ u16::from_be_bytes([STUN_MAGIC_COOKIE[0], STUN_MAGIC_COOKIE[1]]);
                resp.extend_from_slice(&xor_port.to_be_bytes());
                let oct = v4.ip().octets();
                resp.push(oct[0] ^ STUN_MAGIC_COOKIE[0]);
                resp.push(oct[1] ^ STUN_MAGIC_COOKIE[1]);
                resp.push(oct[2] ^ STUN_MAGIC_COOKIE[2]);
                resp.push(oct[3] ^ STUN_MAGIC_COOKIE[3]);
            } else {
                resp.extend_from_slice(&v4.port().to_be_bytes());
                resp.extend_from_slice(&v4.ip().octets());
            }
        }
        SocketAddr::V6(v6) => {
            resp.push(0x00); // Reserved
            resp.push(0x02); // IPv6
            if use_xor {
                let xor_port = v6.port()
                    ^ u16::from_be_bytes([STUN_MAGIC_COOKIE[0], STUN_MAGIC_COOKIE[1]]);
                resp.extend_from_slice(&xor_port.to_be_bytes());
                let oct = v6.ip().octets();
                for i in 0..16 {
                    let key_byte = if i < 4 {
                        STUN_MAGIC_COOKIE[i]
                    } else {
                        transaction_id[i - 4]
                    };
                    resp.push(oct[i] ^ key_byte);
                }
            } else {
                resp.extend_from_slice(&v6.port().to_be_bytes());
                resp.extend_from_slice(&v6.ip().octets());
            }
        }
    }

    resp
}

/// Parses an RFC 5389 STUN Binding Response and extracts the mapped public endpoint.
pub fn parse_stun_response(
    response: &[u8],
    expected_tx_id: &[u8; 12],
) -> Result<SocketAddr, PunchError> {
    if response.len() < 20 {
        return Err(PunchError::Parse(
            "STUN response too short (<20 bytes)".to_string(),
        ));
    }

    let msg_type = u16::from_be_bytes([response[0], response[1]]);
    if msg_type != STUN_BINDING_RESPONSE {
        return Err(PunchError::Stun(format!(
            "Invalid STUN message type: expected 0x0101, got 0x{:04X}",
            msg_type
        )));
    }

    if response[4..8] != STUN_MAGIC_COOKIE {
        return Err(PunchError::Stun("STUN Magic Cookie mismatch".to_string()));
    }

    if &response[8..20] != expected_tx_id {
        return Err(PunchError::Stun(
            "STUN Transaction ID mismatch".to_string(),
        ));
    }

    let msg_len = u16::from_be_bytes([response[2], response[3]]) as usize;
    let end = (20 + msg_len).min(response.len());

    let mut idx = 20;
    while idx + 4 <= end {
        let attr_type = u16::from_be_bytes([response[idx], response[idx + 1]]);
        let attr_len = u16::from_be_bytes([response[idx + 2], response[idx + 3]]) as usize;
        idx += 4;

        if idx + attr_len > response.len() {
            return Err(PunchError::Parse(
                "STUN attribute extends beyond response length".to_string(),
            ));
        }

        if attr_type == STUN_ATTR_XOR_MAPPED_ADDRESS || attr_type == STUN_ATTR_MAPPED_ADDRESS {
            if attr_len >= 8 {
                let family = response[idx + 1];
                if family == 0x01 {
                    // IPv4
                    let port = if attr_type == STUN_ATTR_XOR_MAPPED_ADDRESS {
                        let xor_port = u16::from_be_bytes([response[idx + 2], response[idx + 3]]);
                        xor_port
                            ^ u16::from_be_bytes([STUN_MAGIC_COOKIE[0], STUN_MAGIC_COOKIE[1]])
                    } else {
                        u16::from_be_bytes([response[idx + 2], response[idx + 3]])
                    };

                    let ip = if attr_type == STUN_ATTR_XOR_MAPPED_ADDRESS {
                        Ipv4Addr::new(
                            response[idx + 4] ^ STUN_MAGIC_COOKIE[0],
                            response[idx + 5] ^ STUN_MAGIC_COOKIE[1],
                            response[idx + 6] ^ STUN_MAGIC_COOKIE[2],
                            response[idx + 7] ^ STUN_MAGIC_COOKIE[3],
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
                } else if family == 0x02 && attr_len >= 20 {
                    // IPv6
                    let port = if attr_type == STUN_ATTR_XOR_MAPPED_ADDRESS {
                        let xor_port = u16::from_be_bytes([response[idx + 2], response[idx + 3]]);
                        xor_port
                            ^ u16::from_be_bytes([STUN_MAGIC_COOKIE[0], STUN_MAGIC_COOKIE[1]])
                    } else {
                        u16::from_be_bytes([response[idx + 2], response[idx + 3]])
                    };

                    let mut octets = [0u8; 16];
                    if attr_type == STUN_ATTR_XOR_MAPPED_ADDRESS {
                        for i in 0..16 {
                            let key_byte = if i < 4 {
                                STUN_MAGIC_COOKIE[i]
                            } else {
                                expected_tx_id[i - 4]
                            };
                            octets[i] = response[idx + 4 + i] ^ key_byte;
                        }
                    } else {
                        octets.copy_from_slice(&response[idx + 4..idx + 20]);
                    }

                    let ip = Ipv6Addr::from(octets);
                    return Ok(SocketAddr::new(IpAddr::V6(ip), port));
                }
            }
        }

        // Align to 32-bit boundary per RFC 5389 §15
        let pad = (4 - (attr_len % 4)) % 4;
        idx += attr_len + pad;
    }

    Err(PunchError::Stun(
        "No MAPPED-ADDRESS or XOR-MAPPED-ADDRESS attribute found".to_string(),
    ))
}

/// Queries a single STUN server endpoint with an RFC 5389 Binding Request.
async fn query_stun_server(
    socket: &UdpSocket,
    target: SocketAddr,
) -> Result<SocketAddr, PunchError> {
    let mut tx_id = [0u8; 12];
    rand::rng().fill_bytes(&mut tx_id);

    let req = build_stun_request(&tx_id);
    socket.send_to(&req, target).await?;

    let mut buf = [0u8; 512];
    let res = tokio::time::timeout(Duration::from_millis(1500), socket.recv_from(&mut buf)).await;
    let (len, _src) = res.map_err(|_| PunchError::Timeout)??;

    parse_stun_response(&buf[..len], &tx_id)
}

/// Discovers both local LAN and external public WAN endpoints for the given bound UDP socket.
pub async fn discover_endpoints(socket: &UdpSocket) -> Result<PunchCandidate, PunchError> {
    let local_port = socket.local_addr()?.port();

    // 1. LAN candidate discovery:
    // Determine the host's actual local network interface IP via standard routing table lookup.
    let lan_addr = match std::net::UdpSocket::bind("0.0.0.0:0") {
        Ok(dummy) => match dummy.connect("8.8.8.8:80") {
            Ok(_) => match dummy.local_addr() {
                Ok(addr) => Some(SocketAddr::new(addr.ip(), local_port)),
                Err(_) => None,
            },
            Err(_) => None,
        },
        Err(_) => None,
    }
    .or_else(|| match socket.local_addr() {
        Ok(addr) if !addr.ip().is_unspecified() => Some(addr),
        _ => Some(SocketAddr::new(
            IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
            local_port,
        )),
    });

    // 2. WAN candidate discovery via STUN:
    let stun_servers = [
        "stun.l.google.com:19302",
        "stun1.l.google.com:19302",
        "stun2.l.google.com:19302",
    ];

    let mut wan_addr = None;
    for server_str in stun_servers {
        if let Ok(mut addrs) = tokio::net::lookup_host(server_str).await {
            if let Some(target) = addrs.next() {
                if let Ok(mapped) = query_stun_server(socket, target).await {
                    wan_addr = Some(mapped);
                    break;
                }
            }
        }
    }

    // Fallback: If STUN was blocked or timed out, resolve WAN IP via lightweight HTTPS lookup
    if wan_addr.is_none() {
        if let Ok(resp) = reqwest::Client::builder()
            .timeout(Duration::from_millis(1500))
            .build()
        {
            if let Ok(body) = resp.get("https://api.ipify.org").send().await {
                if let Ok(ip_text) = body.text().await {
                    if let Ok(ip) = ip_text.trim().parse::<IpAddr>() {
                        wan_addr = Some(SocketAddr::new(ip, local_port));
                    }
                }
            }
        }
    }

    Ok(PunchCandidate {
        lan: lan_addr,
        wan: wan_addr,
    })
}

/// Backward-compatible helper returning the public WAN endpoint if discovered.
pub async fn discover_public_endpoint(socket: &UdpSocket) -> Option<SocketAddr> {
    discover_endpoints(socket).await.ok().and_then(|c| c.wan)
}

/// Formats a 12-byte UDP punch probe frame.
pub fn format_punch_probe(msg_type: u8, seq: u8) -> [u8; 12] {
    let mut pkt = [0u8; 12];
    pkt[0..10].copy_from_slice(PUNCH_MAGIC);
    pkt[10] = msg_type;
    pkt[11] = seq;
    pkt
}

/// Parses a 12-byte UDP punch probe frame.
pub fn parse_punch_probe(buf: &[u8]) -> Option<(u8, u8)> {
    if buf.len() < 12 || &buf[0..10] != PUNCH_MAGIC {
        return None;
    }
    Some((buf[10], buf[11]))
}

/// Executes a 5-probe UDP hole punching burst across candidate endpoints with a default 2,500ms timeout.
pub async fn execute_hole_punch(
    socket: &UdpSocket,
    target: &PunchCandidate,
) -> Result<SocketAddr, PunchError> {
    execute_hole_punch_with_timeout(
        socket,
        target,
        Duration::from_millis(DEFAULT_PUNCH_TIMEOUT_MS),
    )
    .await
}

/// Executes a 5-probe UDP hole punching burst across candidate endpoints with an explicit timeout.
pub async fn execute_hole_punch_with_timeout(
    socket: &UdpSocket,
    target: &PunchCandidate,
    timeout: Duration,
) -> Result<SocketAddr, PunchError> {
    let candidates = target.endpoints();
    if candidates.is_empty() {
        return Err(PunchError::NoCandidates);
    }

    // Transmit initial probe burst immediately without sleeping upfront
    for seq in 0..PUNCH_BURST_COUNT {
        let probe = format_punch_probe(PUNCH_MSG_PROBE, seq as u8);
        for &endpoint in &candidates {
            let _ = socket.send_to(&probe, endpoint).await;
        }
    }

    // Listen for peer responses or probes until timeout
    let deadline = Instant::now() + timeout;
    let mut verified_lan: Option<SocketAddr> = None;
    let mut verified_wan: Option<SocketAddr> = None;

    let mut recv_buf = [0u8; 256];

    while Instant::now() < deadline {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            break;
        }

        let recv_result = tokio::time::timeout(remaining, socket.recv_from(&mut recv_buf)).await;
        match recv_result {
            Ok(Ok((len, peer_addr))) => {
                if let Some((msg_type, seq)) = parse_punch_probe(&recv_buf[..len]) {
                    if msg_type == PUNCH_MSG_PROBE {
                        // Acknowledge incoming probe immediately
                        let ack = format_punch_probe(PUNCH_MSG_ACK, seq);
                        let _ = socket.send_to(&ack, peer_addr).await;
                    }

                    // Determine if peer_addr matches LAN or WAN candidate
                    if Some(peer_addr) == target.lan {
                        verified_lan = Some(peer_addr);
                        break; // LAN is lowest-latency; prioritize immediately
                    } else if Some(peer_addr) == target.wan {
                        verified_wan = Some(peer_addr);
                        // If peer has a LAN candidate, give it a 100ms grace period to arrive
                        // before settling on WAN
                        if target.lan.is_some() {
                            let grace_deadline = Instant::now() + Duration::from_millis(100);
                            while Instant::now() < grace_deadline && verified_lan.is_none() {
                                let grace_rem = grace_deadline.saturating_duration_since(Instant::now());
                                if let Ok(Ok((g_len, g_peer))) = tokio::time::timeout(grace_rem, socket.recv_from(&mut recv_buf)).await {
                                    if let Some((g_type, g_seq)) = parse_punch_probe(&recv_buf[..g_len]) {
                                        if g_type == PUNCH_MSG_PROBE {
                                            let g_ack = format_punch_probe(PUNCH_MSG_ACK, g_seq);
                                            let _ = socket.send_to(&g_ack, g_peer).await;
                                        }
                                        if Some(g_peer) == target.lan {
                                            verified_lan = Some(g_peer);
                                            break;
                                        }
                                    }
                                }
                            }
                        }
                        break;
                    } else if let Some(lan) = target.lan {
                        if peer_addr.ip() == lan.ip() && peer_addr.port() == lan.port() {
                            verified_lan = Some(peer_addr);
                            break;
                        }
                    } else if let Some(wan) = target.wan {
                        if peer_addr.ip() == wan.ip() {
                            verified_wan = Some(peer_addr);
                        }
                    }
                }
            }
            Ok(Err(_)) => {
                // Socket error; back off slightly or continue until deadline
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
            Err(_) => {
                // Timeout reached
                break;
            }
        }
    }

    // Select lowest-latency accessible endpoint: LAN preferred over WAN
    if let Some(lan) = verified_lan {
        Ok(lan)
    } else if let Some(wan) = verified_wan {
        Ok(wan)
    } else {
        Err(PunchError::Timeout)
    }
}
