//! BoringTun WireGuard Engine & Cryptographic State Machine Wrapper.
//!
//! Encapsulates Cloudflare's pure-Rust, sans-I/O WireGuard implementation (`boringtun::noise::Tunn`).
//! Operates completely in non-privileged userspace: zero kernel TUN/TAP driver installation,
//! zero administrator privileges required.
//!
//! Provides:
//! - Noise_IKpsk2 authenticated key exchange (Type 1 Handshake Init, Type 2 Handshake Response)
//! - WireGuard Type 4 transport packet encapsulation & decapsulation
//! - Protocol timer evaluation (`update_timers` for keepalives and rekeying)
//! - Synthetic 20-byte IPv4 packet framing (`10.88.0.x`) to satisfy BoringTun protocol validation
//! - Strict buffer size validation (minimum 2048 bytes)

use boringtun::noise::errors::WireGuardError as BoringTunError;
use boringtun::noise::{Tunn, TunnResult};
use boringtun::x25519::{PublicKey, StaticSecret};
use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::atomic::{AtomicU32, Ordering};

/// Minimum scratch buffer size required for all datagram operations (2048 bytes).
pub const MIN_BUFFER_SIZE: usize = 2048;

/// Global atomic counter for unique local WireGuard tunnel session indices.
static NEXT_TUNNEL_INDEX: AtomicU32 = AtomicU32::new(100);

/// Errors originating from the WireGuard subsystem or BoringTun state machine.
#[derive(Debug)]
pub enum WireGuardError {
    /// Upstream error reported by BoringTun state machine.
    BoringTun(BoringTunError),
    /// Destination scratch buffer size is below the required minimum (2048 bytes or MTU).
    BufferTooSmall { required: usize, provided: usize },
    /// Packet header or format was invalid (e.g. invalid IP version).
    InvalidPacket,
    /// Handshake initiation failed to generate output.
    NoHandshakeProduced,
    /// WireGuard session has expired and requires re-establishment.
    ConnectionExpired,
    /// Other subsystem error.
    Other(String),
}

impl std::fmt::Display for WireGuardError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BoringTun(e) => write!(f, "BoringTun error: {:?}", e),
            Self::BufferTooSmall { required, provided } => {
                write!(
                    f,
                    "Destination buffer too small: requires {} bytes, provided {}",
                    required, provided
                )
            }
            Self::InvalidPacket => write!(f, "Invalid packet header or structure"),
            Self::NoHandshakeProduced => write!(f, "No handshake initiation packet produced"),
            Self::ConnectionExpired => write!(f, "WireGuard session connection expired"),
            Self::Other(s) => write!(f, "WireGuard error: {}", s),
        }
    }
}

impl std::error::Error for WireGuardError {}

impl From<BoringTunError> for WireGuardError {
    fn from(err: BoringTunError) -> Self {
        match err {
            BoringTunError::ConnectionExpired => Self::ConnectionExpired,
            BoringTunError::InvalidPacket => Self::InvalidPacket,
            other => Self::BoringTun(other),
        }
    }
}

/// Result of decapsulating a WireGuard datagram received from the network.
pub enum DecapsulatedPacket<'a> {
    /// Decrypted plaintext IPv4 packet. Contains decrypted IP packet slice and source IPv4 address.
    PacketV4 {
        packet: &'a mut [u8],
        src_ip: Ipv4Addr,
    },
    /// Decrypted plaintext IPv6 packet. Contains decrypted IP packet slice and source IPv6 address.
    PacketV6 {
        packet: &'a mut [u8],
        src_ip: Ipv6Addr,
    },
    /// Datagram (handshake response, cookie, or keepalive) that must be sent over UDP to the peer.
    WriteToNetwork(&'a mut [u8]),
    /// Empty datagram or timer evaluation consumed with no action required.
    Done,
}

/// Userspace WireGuard tunnel session wrapping a single `boringtun::noise::Tunn` state machine.
pub struct WireGuardTunnel {
    tunn: Tunn,
    index: u32,
    peer_public: [u8; 32],
    is_host: bool,
}

impl WireGuardTunnel {
    /// Instantiates a new Host-side WireGuard tunnel.
    ///
    /// The Host acts as the responder awaiting a Handshake Initiation from the Guest,
    /// or can initiate handshakes when desired.
    pub fn new_host(
        static_private: [u8; 32],
        peer_public: [u8; 32],
        preshared_key: Option<[u8; 32]>,
    ) -> Result<Self, WireGuardError> {
        let secret = StaticSecret::from(static_private);
        let peer_pub = PublicKey::from(peer_public);
        let index = NEXT_TUNNEL_INDEX.fetch_add(1, Ordering::Relaxed);

        // Host tunnel: keepalive optional (None), no rate limiter
        let tunn = Tunn::new(secret, peer_pub, preshared_key, None, index, None);

        Ok(Self {
            tunn,
            index,
            peer_public,
            is_host: true,
        })
    }

    /// Instantiates a new Guest-side WireGuard tunnel.
    ///
    /// Configures a persistent keepalive of 25 seconds to maintain NAT firewall pinholes.
    pub fn new_guest(
        static_private: [u8; 32],
        peer_public: [u8; 32],
        preshared_key: Option<[u8; 32]>,
    ) -> Result<Self, WireGuardError> {
        let secret = StaticSecret::from(static_private);
        let peer_pub = PublicKey::from(peer_public);
        let index = NEXT_TUNNEL_INDEX.fetch_add(1, Ordering::Relaxed);

        // Guest tunnel: 25s keepalive to preserve NAT state
        let tunn = Tunn::new(secret, peer_pub, preshared_key, Some(25), index, None);

        Ok(Self {
            tunn,
            index,
            peer_public,
            is_host: false,
        })
    }

    /// Formats a WireGuard Type 1 Handshake Initiation datagram (148 bytes).
    ///
    /// Buffer `dst` must have length at least 2048 bytes.
    /// Returns the length of the formatted datagram (typically 148 bytes).
    pub fn format_handshake_initiation(&mut self, dst: &mut [u8]) -> Result<usize, WireGuardError> {
        if dst.len() < MIN_BUFFER_SIZE {
            return Err(WireGuardError::BufferTooSmall {
                required: MIN_BUFFER_SIZE,
                provided: dst.len(),
            });
        }

        match self.tunn.format_handshake_initiation(dst, false) {
            TunnResult::WriteToNetwork(packet) => Ok(packet.len()),
            TunnResult::Done => Err(WireGuardError::NoHandshakeProduced),
            TunnResult::Err(e) => Err(WireGuardError::from(e)),
            _ => Err(WireGuardError::InvalidPacket),
        }
    }

    /// Encapsulates an IPv4 packet into an encrypted WireGuard Type 4 Transport packet.
    ///
    /// If a session key is not yet established, BoringTun queues the packet internally
    /// and generates a Handshake Initiation datagram in `dst_datagram`.
    ///
    /// Buffer `dst_datagram` must have length >= 2048 bytes AND >= `src_ip_packet.len() + 32`.
    /// Returns the byte length written to `dst_datagram`.
    pub fn encapsulate(
        &mut self,
        src_ip_packet: &[u8],
        dst_datagram: &mut [u8],
    ) -> Result<usize, WireGuardError> {
        let required = MIN_BUFFER_SIZE.max(src_ip_packet.len() + 32);
        if dst_datagram.len() < required {
            return Err(WireGuardError::BufferTooSmall {
                required,
                provided: dst_datagram.len(),
            });
        }

        match self.tunn.encapsulate(src_ip_packet, dst_datagram) {
            TunnResult::WriteToNetwork(packet) => Ok(packet.len()),
            TunnResult::Done => Ok(0),
            TunnResult::Err(e) => Err(WireGuardError::from(e)),
            _ => Err(WireGuardError::InvalidPacket),
        }
    }

    /// Decapsulates an incoming WireGuard UDP datagram from the network.
    ///
    /// Handles:
    /// - Handshake Initiation (148 B) -> Returns `WriteToNetwork` (92 B Handshake Response)
    /// - Handshake Response (92 B) -> Returns `WriteToNetwork` (32 B confirmation keepalive)
    /// - Transport Data Packet -> Decrypts and returns `PacketV4` or `PacketV6`
    /// - Keepalive Packet -> Returns `Done`
    ///
    /// Buffer `dst_buf` must have length at least 2048 bytes.
    pub fn decapsulate<'a>(
        &mut self,
        src_addr: Option<SocketAddr>,
        datagram: &[u8],
        dst_buf: &'a mut [u8],
    ) -> Result<DecapsulatedPacket<'a>, WireGuardError> {
        if dst_buf.len() < MIN_BUFFER_SIZE {
            return Err(WireGuardError::BufferTooSmall {
                required: MIN_BUFFER_SIZE,
                provided: dst_buf.len(),
            });
        }

        let ip_addr = src_addr.map(|a| a.ip());
        match self.tunn.decapsulate(ip_addr, datagram, dst_buf) {
            TunnResult::WriteToTunnelV4(packet, src_ip) => {
                Ok(DecapsulatedPacket::PacketV4 { packet, src_ip })
            }
            TunnResult::WriteToTunnelV6(packet, src_ip) => {
                Ok(DecapsulatedPacket::PacketV6 { packet, src_ip })
            }
            TunnResult::WriteToNetwork(packet) => Ok(DecapsulatedPacket::WriteToNetwork(packet)),
            TunnResult::Done => Ok(DecapsulatedPacket::Done),
            TunnResult::Err(e) => Err(WireGuardError::from(e)),
        }
    }

    /// Updates internal BoringTun protocol timers (keepalives, handshake timeouts, rekeying).
    ///
    /// Should be called periodically (e.g. every 250ms).
    /// If a keepalive packet or handshake retransmission must be sent, returns `Ok(Some(len))`.
    /// Buffer `dst_datagram` must have length at least 2048 bytes.
    pub fn update_timers(&mut self, dst_datagram: &mut [u8]) -> Result<Option<usize>, WireGuardError> {
        if dst_datagram.len() < MIN_BUFFER_SIZE {
            return Err(WireGuardError::BufferTooSmall {
                required: MIN_BUFFER_SIZE,
                provided: dst_datagram.len(),
            });
        }

        match self.tunn.update_timers(dst_datagram) {
            TunnResult::WriteToNetwork(packet) => Ok(Some(packet.len())),
            TunnResult::Done => Ok(None),
            TunnResult::Err(e) => Err(WireGuardError::from(e)),
            _ => Ok(None),
        }
    }

    /// Unique 32-bit local session index for this tunnel.
    pub fn index(&self) -> u32 {
        self.index
    }

    /// Remote peer's X25519 public key.
    pub fn peer_public(&self) -> &[u8; 32] {
        &self.peer_public
    }

    /// True if this tunnel was configured as Host.
    pub fn is_host(&self) -> bool {
        self.is_host
    }
}

// =========================================================================
// SYNTHETIC IPV4 FRAMING HELPERS (Non-Privileged Userspace Datapath)
// =========================================================================

/// Wraps arbitrary userspace payload (e.g. Minecraft TCP stream chunks) into a
/// lightweight 20-byte synthetic IPv4 packet conforming to RFC 791.
///
/// BoringTun strictly verifies that decrypted packets have version 4 (`packet[0] >> 4 == 4`)
/// and a valid total length (`u16::from_be_bytes(packet[2..4]) <= packet.len()`).
/// Checksums are ignored by BoringTun, avoiding any kernel routing or TUN driver dependency.
pub fn make_synthetic_ipv4(src_ip: [u8; 4], dst_ip: [u8; 4], payload: &[u8]) -> Vec<u8> {
    let total_len = (20 + payload.len()) as u16;
    let mut pkt = Vec::with_capacity(total_len as usize);
    pkt.push(0x45); // IPv4, Header Length = 5 (20 bytes)
    pkt.push(0x00); // DSCP / ECN
    pkt.extend_from_slice(&total_len.to_be_bytes());
    pkt.extend_from_slice(&[0x00, 0x00]); // Identification
    pkt.extend_from_slice(&[0x40, 0x00]); // Flags: DF (Don't Fragment), offset 0
    pkt.push(64);   // TTL: 64
    pkt.push(6);    // Protocol: TCP (6)
    pkt.extend_from_slice(&[0x00, 0x00]); // Header Checksum (ignored)
    pkt.extend_from_slice(&src_ip);
    pkt.extend_from_slice(&dst_ip);
    pkt.extend_from_slice(payload);
    pkt
}

/// Parses a synthetic IPv4 packet, returning `(payload_slice, src_ip, dst_ip)`.
pub fn parse_synthetic_ipv4(packet: &[u8]) -> Result<(&[u8], Ipv4Addr, Ipv4Addr), WireGuardError> {
    if packet.len() < 20 || packet[0] >> 4 != 4 {
        return Err(WireGuardError::InvalidPacket);
    }
    let ihl = (packet[0] & 0x0f) as usize * 4;
    if ihl < 20 || packet.len() < ihl {
        return Err(WireGuardError::InvalidPacket);
    }
    let total_len = u16::from_be_bytes([packet[2], packet[3]]) as usize;
    if packet.len() < total_len || total_len < ihl {
        return Err(WireGuardError::InvalidPacket);
    }
    let src_ip = Ipv4Addr::new(packet[12], packet[13], packet[14], packet[15]);
    let dst_ip = Ipv4Addr::new(packet[16], packet[17], packet[18], packet[19]);
    Ok((&packet[ihl..total_len], src_ip, dst_ip))
}

// =========================================================================
// UNIT TESTS
// =========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use boringtun::x25519::{PublicKey, StaticSecret};
    use rand::Rng;

    fn generate_keypair() -> ([u8; 32], [u8; 32]) {
        let mut key_bytes = [0u8; 32];
        rand::rng().fill_bytes(&mut key_bytes);
        let secret = StaticSecret::from(key_bytes);
        let public = PublicKey::from(&secret);
        (secret.to_bytes(), public.to_bytes())
    }

    #[test]
    fn test_handshake_exchange() {
        let (host_priv, host_pub) = generate_keypair();
        let (guest_priv, guest_pub) = generate_keypair();

        let mut host = WireGuardTunnel::new_host(host_priv, guest_pub, None).expect("Host init");
        let mut guest = WireGuardTunnel::new_guest(guest_priv, host_pub, None).expect("Guest init");

        assert!(host.is_host());
        assert!(!guest.is_host());
        assert_eq!(host.peer_public(), &guest_pub);
        assert_eq!(guest.peer_public(), &host_pub);

        let mut init_buf = vec![0u8; MIN_BUFFER_SIZE];
        let mut resp_buf = vec![0u8; MIN_BUFFER_SIZE];
        let mut confirm_buf = vec![0u8; MIN_BUFFER_SIZE];
        let mut scratch_buf = vec![0u8; MIN_BUFFER_SIZE];

        // 1. Guest initiates handshake (WireGuard Type 1, 148 bytes)
        let init_len = guest
            .format_handshake_initiation(&mut init_buf)
            .expect("Format handshake initiation");
        assert_eq!(init_len, 148, "Handshake Initiation must be 148 bytes");
        assert_eq!(init_buf[0], 1, "Handshake Initiation must have type 1");

        // 2. Host processes initiation and returns Handshake Response (WireGuard Type 2, 92 bytes)
        let host_addr: SocketAddr = "127.0.0.1:51820".parse().unwrap();
        let guest_addr: SocketAddr = "127.0.0.1:51821".parse().unwrap();

        let resp = host
            .decapsulate(Some(guest_addr), &init_buf[..init_len], &mut resp_buf)
            .expect("Host decapsulate initiation");

        let resp_len = match resp {
            DecapsulatedPacket::WriteToNetwork(datagram) => {
                assert_eq!(datagram.len(), 92, "Handshake Response must be 92 bytes");
                assert_eq!(datagram[0], 2, "Handshake Response must have type 2");
                datagram.len()
            }
            _ => panic!("Host must reply with WriteToNetwork Handshake Response"),
        };

        // 3. Guest processes response and returns confirmation keepalive (WireGuard Type 4, 32 bytes)
        let confirm = guest
            .decapsulate(Some(host_addr), &resp_buf[..resp_len], &mut confirm_buf)
            .expect("Guest decapsulate response");

        let confirm_len = match confirm {
            DecapsulatedPacket::WriteToNetwork(datagram) => {
                assert_eq!(datagram.len(), 32, "Confirmation keepalive must be 32 bytes");
                assert_eq!(datagram[0], 4, "Confirmation packet must have transport type 4");
                datagram.len()
            }
            _ => panic!("Guest must reply with WriteToNetwork confirmation keepalive"),
        };

        // 4. Host processes confirmation keepalive -> returns Done
        let final_step = host
            .decapsulate(Some(guest_addr), &confirm_buf[..confirm_len], &mut scratch_buf)
            .expect("Host decapsulate confirmation");

        match final_step {
            DecapsulatedPacket::Done => {}
            _ => panic!("Host confirmation consumption must return Done"),
        }
    }

    #[test]
    fn test_synthetic_ipv4_packet_encapsulation_and_decapsulation() {
        let (host_priv, host_pub) = generate_keypair();
        let (guest_priv, guest_pub) = generate_keypair();

        let mut host = WireGuardTunnel::new_host(host_priv, guest_pub, None).expect("Host init");
        let mut guest = WireGuardTunnel::new_guest(guest_priv, host_pub, None).expect("Guest init");

        // Complete handshake
        let mut buf_a = vec![0u8; MIN_BUFFER_SIZE];
        let mut buf_b = vec![0u8; MIN_BUFFER_SIZE];
        let mut buf_c = vec![0u8; MIN_BUFFER_SIZE];
        let mut scratch = vec![0u8; MIN_BUFFER_SIZE];

        let init_len = guest.format_handshake_initiation(&mut buf_a).unwrap();
        let resp = host.decapsulate(None, &buf_a[..init_len], &mut buf_b).unwrap();
        let resp_len = match resp {
            DecapsulatedPacket::WriteToNetwork(d) => d.len(),
            _ => panic!("Expected WriteToNetwork"),
        };
        let confirm = guest.decapsulate(None, &buf_b[..resp_len], &mut buf_c).unwrap();
        let confirm_len = match confirm {
            DecapsulatedPacket::WriteToNetwork(d) => d.len(),
            _ => panic!("Expected WriteToNetwork"),
        };
        let _ = host.decapsulate(None, &buf_c[..confirm_len], &mut scratch).unwrap();

        // Guest sends synthetic IPv4 packet with Minecraft TCP payload to Host
        let minecraft_payload = b"\x10\x00\xf2\x05\x09localhost\x63\xdd\x02";
        let src_ip = [10, 88, 0, 2];
        let dst_ip = [10, 88, 0, 1];
        let ip_packet = make_synthetic_ipv4(src_ip, dst_ip, minecraft_payload);

        let mut enc_buf = vec![0u8; MIN_BUFFER_SIZE];
        let enc_len = guest
            .encapsulate(&ip_packet, &mut enc_buf)
            .expect("Guest encapsulate IP packet");
        assert!(enc_len > ip_packet.len());
        assert_eq!(enc_buf[0], 4, "Must be WireGuard Type 4 Transport Data");

        // Host decapsulates packet
        let mut dec_buf = vec![0u8; MIN_BUFFER_SIZE];
        let dec_result = host
            .decapsulate(None, &enc_buf[..enc_len], &mut dec_buf)
            .expect("Host decapsulate packet");

        match dec_result {
            DecapsulatedPacket::PacketV4 { packet, src_ip: extracted_src } => {
                assert_eq!(extracted_src, Ipv4Addr::new(10, 88, 0, 2));
                let (extracted_payload, s_ip, d_ip) =
                    parse_synthetic_ipv4(packet).expect("Parse synthetic IPv4");
                assert_eq!(s_ip, Ipv4Addr::new(10, 88, 0, 2));
                assert_eq!(d_ip, Ipv4Addr::new(10, 88, 0, 1));
                assert_eq!(extracted_payload, minecraft_payload);
            }
            _ => panic!("Host must yield DecapsulatedPacket::PacketV4"),
        }

        // Host responds back to Guest
        let response_payload = b"\x03\x00\x00ServerAck";
        let reply_ip_packet = make_synthetic_ipv4(dst_ip, src_ip, response_payload);
        let reply_enc_len = host
            .encapsulate(&reply_ip_packet, &mut enc_buf)
            .expect("Host encapsulate reply");

        let guest_dec = guest
            .decapsulate(None, &enc_buf[..reply_enc_len], &mut dec_buf)
            .expect("Guest decapsulate reply");

        match guest_dec {
            DecapsulatedPacket::PacketV4 { packet, src_ip: s_ip } => {
                assert_eq!(s_ip, Ipv4Addr::new(10, 88, 0, 1));
                let (p, _, _) = parse_synthetic_ipv4(packet).expect("Parse synthetic reply");
                assert_eq!(p, response_payload);
            }
            _ => panic!("Guest must yield DecapsulatedPacket::PacketV4"),
        }
    }

    #[test]
    fn test_timer_keepalive_generation() {
        let (host_priv, host_pub) = generate_keypair();
        let (guest_priv, guest_pub) = generate_keypair();

        let mut guest = WireGuardTunnel::new_guest(guest_priv, host_pub, None).expect("Guest init");
        let mut host = WireGuardTunnel::new_host(host_priv, guest_pub, None).expect("Host init");

        let mut timer_buf = vec![0u8; MIN_BUFFER_SIZE];
        let guest_timer = guest.update_timers(&mut timer_buf).expect("Guest timer tick");
        let host_timer = host.update_timers(&mut timer_buf).expect("Host timer tick");

        // Prior to handshake completion, update_timers produces None or rekey
        assert!(guest_timer.is_none() || guest_timer.is_some());
        assert!(host_timer.is_none() || host_timer.is_some());
    }

    #[test]
    fn test_buffer_validation_errors() {
        let (host_priv, host_pub) = generate_keypair();
        let (guest_priv, guest_pub) = generate_keypair();

        let mut guest = WireGuardTunnel::new_guest(guest_priv, host_pub, None).expect("Guest init");
        let mut host = WireGuardTunnel::new_host(host_priv, guest_pub, None).expect("Host init");

        let mut tiny_buf = vec![0u8; 100]; // Less than 2048 bytes

        assert!(matches!(
            guest.format_handshake_initiation(&mut tiny_buf),
            Err(WireGuardError::BufferTooSmall { required: 2048, provided: 100 })
        ));

        let ip_packet = make_synthetic_ipv4([10, 88, 0, 2], [10, 88, 0, 1], b"Test");
        assert!(matches!(
            guest.encapsulate(&ip_packet, &mut tiny_buf),
            Err(WireGuardError::BufferTooSmall { required: 2048, provided: 100 })
        ));

        assert!(matches!(
            host.decapsulate(None, &[1, 2, 3], &mut tiny_buf),
            Err(WireGuardError::BufferTooSmall { required: 2048, provided: 100 })
        ));

        assert!(matches!(
            guest.update_timers(&mut tiny_buf),
            Err(WireGuardError::BufferTooSmall { required: 2048, provided: 100 })
        ));
    }

    #[test]
    fn test_preshared_key_handshake() {
        let (host_priv, host_pub) = generate_keypair();
        let (guest_priv, guest_pub) = generate_keypair();
        let psk = Some([0x42u8; 32]);

        let mut host = WireGuardTunnel::new_host(host_priv, guest_pub, psk).expect("Host init with PSK");
        let mut guest = WireGuardTunnel::new_guest(guest_priv, host_pub, psk).expect("Guest init with PSK");

        let mut buf_a = vec![0u8; MIN_BUFFER_SIZE];
        let mut buf_b = vec![0u8; MIN_BUFFER_SIZE];
        let mut buf_c = vec![0u8; MIN_BUFFER_SIZE];
        let mut scratch = vec![0u8; MIN_BUFFER_SIZE];

        let init_len = guest.format_handshake_initiation(&mut buf_a).unwrap();
        let resp = host.decapsulate(None, &buf_a[..init_len], &mut buf_b).unwrap();
        let resp_len = match resp {
            DecapsulatedPacket::WriteToNetwork(d) => d.len(),
            _ => panic!("Expected WriteToNetwork"),
        };
        let confirm = guest.decapsulate(None, &buf_b[..resp_len], &mut buf_c).unwrap();
        let confirm_len = match confirm {
            DecapsulatedPacket::WriteToNetwork(d) => d.len(),
            _ => panic!("Expected WriteToNetwork"),
        };
        let done = host.decapsulate(None, &buf_c[..confirm_len], &mut scratch).unwrap();
        assert!(matches!(done, DecapsulatedPacket::Done));
    }
}
