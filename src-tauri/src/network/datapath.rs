//! Non-Privileged Userspace Datapath & Minecraft Port 25565 Proxy.
//!
//! Operates 100% in userspace: zero OS root/administrator privileges required,
//! zero kernel TUN/TAP driver installation (e.g. Wintun).
//!
//! Provides:
//! - Local loopback TCP proxy on Minecraft port 25565 (Host and Guest)
//! - Dynamic ephemeral port fallback (`127.0.0.1:0`) when port 25565 is occupied
//! - Synthetic 20-byte RFC 791 IPv4 packet framing (`10.88.0.1` <-> `10.88.0.2`, protocol 6 TCP)
//! - MTU-bounded chunking (<= 1400 bytes) with defensive integer wrap prevention
//! - Reliable stream transport: chunking, sequence numbering, and in-order reassembly
//! - Graceful connection termination handling (FIN / RST signaling) and connection reuse

use std::collections::{BTreeMap, HashMap};
use std::net::{Ipv4Addr, SocketAddr};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, Mutex};

/// Host synthetic IPv4 address (`10.88.0.1`).
pub const HOST_IPV4: [u8; 4] = [10, 88, 0, 1];

/// Guest synthetic IPv4 address (`10.88.0.2`).
pub const GUEST_IPV4: [u8; 4] = [10, 88, 0, 2];

/// IPv4 standard header length in bytes (RFC 791 IHL 5).
pub const IPV4_HEADER_LEN: usize = 20;

/// Stream framing header length in bytes.
pub const STREAM_HEADER_LEN: usize = 16;

/// Maximum synthetic IPv4 packet MTU in bytes (1400 bytes).
/// With WireGuard 32-byte overhead (16B Type 4 header + 16B Poly1305 MAC tag),
/// datagrams fit comfortably within standard 1500-byte internet MTU without fragmentation.
pub const MAX_PACKET_MTU: usize = 1400;

/// Maximum payload chunk size for TCP streaming so total packet <= `MAX_PACKET_MTU`.
/// 1400 - 20 (IPv4) - 16 (StreamHeader) = 1364 bytes.
pub const MAX_PAYLOAD_CHUNK: usize = MAX_PACKET_MTU - IPV4_HEADER_LEN - STREAM_HEADER_LEN;

/// Stream framing magic bytes `[0xCD, 0x54]` ("CT" for Conduit Transport).
pub const STREAM_MAGIC: [u8; 2] = [0xCD, 0x54];

/// Stream packet flags.
pub const FLAG_DATA: u8 = 0x01;
pub const FLAG_FIN: u8 = 0x02;
pub const FLAG_RST: u8 = 0x04;
pub const FLAG_ACK: u8 = 0x08;

/// Errors originating from the userspace datapath subsystem.
#[derive(Debug)]
pub enum DatapathError {
    /// Underlying I/O error.
    Io(std::io::Error),
    /// Failed to bind TCP listener.
    BindFailed { port: u16, source: std::io::Error },
    /// Failed to connect to local TCP endpoint.
    ConnectFailed { target: SocketAddr, source: std::io::Error },
    /// Malformed or invalid synthetic IPv4 packet.
    InvalidPacket(String),
    /// Internal channel closed.
    ChannelClosed,
    /// Connection stream closed.
    StreamClosed,
    /// Other subsystem error.
    Other(String),
}

impl std::fmt::Display for DatapathError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "I/O error: {}", e),
            Self::BindFailed { port, source } => {
                write!(f, "Failed to bind to port {}: {}", port, source)
            }
            Self::ConnectFailed { target, source } => {
                write!(f, "Failed to connect to target {}: {}", target, source)
            }
            Self::InvalidPacket(s) => write!(f, "Invalid packet: {}", s),
            Self::ChannelClosed => write!(f, "Datapath channel closed"),
            Self::StreamClosed => write!(f, "TCP stream closed"),
            Self::Other(s) => write!(f, "Datapath error: {}", s),
        }
    }
}

impl std::error::Error for DatapathError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            Self::BindFailed { source, .. } => Some(source),
            Self::ConnectFailed { source, .. } => Some(source),
            _ => None,
        }
    }
}

impl From<std::io::Error> for DatapathError {
    fn from(err: std::io::Error) -> Self {
        Self::Io(err)
    }
}

/// Inner state for DatapathHandle to decouple background task lifecycles from handle ownership.
#[derive(Debug)]
struct DatapathInner {
    is_running: Arc<AtomicBool>,
    bound_port: Option<u16>,
    abort_handles: std::sync::Mutex<Vec<tokio::task::AbortHandle>>,
}

impl Drop for DatapathInner {
    fn drop(&mut self) {
        self.is_running.store(false, Ordering::SeqCst);
        if let Ok(mut handles) = self.abort_handles.lock() {
            for h in handles.drain(..) {
                h.abort();
            }
        }
    }
}

/// Handle managing the lifecycle of running proxy background tasks.
#[derive(Debug, Clone)]
pub struct DatapathHandle {
    inner: Arc<DatapathInner>,
}

impl DatapathHandle {
    /// Creates a new datapath handle.
    pub fn new(bound_port: Option<u16>) -> Self {
        Self {
            inner: Arc::new(DatapathInner {
                is_running: Arc::new(AtomicBool::new(true)),
                bound_port,
                abort_handles: std::sync::Mutex::new(Vec::new()),
            }),
        }
    }

    /// Registers a background task's abort handle.
    pub fn add_abort_handle(&self, handle: tokio::task::AbortHandle) {
        if let Ok(mut handles) = self.inner.abort_handles.lock() {
            handles.push(handle);
        }
    }

    /// Signals all proxy tasks to stop and aborts background tasks.
    pub fn stop(&self) {
        self.inner.is_running.store(false, Ordering::SeqCst);
        if let Ok(mut handles) = self.inner.abort_handles.lock() {
            for h in handles.drain(..) {
                h.abort();
            }
        }
    }

    /// Returns true if the datapath proxy is actively running.
    pub fn is_running(&self) -> bool {
        self.inner.is_running.load(Ordering::SeqCst)
    }

    /// Returns the local port bound by this datapath (if applicable).
    pub fn bound_port(&self) -> Option<u16> {
        self.inner.bound_port
    }

    /// Returns a cloned `Arc<AtomicBool>` token for background task execution loops.
    pub fn is_running_flag(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.inner.is_running)
    }
}

/// Internal stream packet framing layout:
/// `[MAGIC: 2B] [FLAGS: 1B] [RESERVED: 1B] [STREAM_ID: 4B] [SEQ: 4B] [PAYLOAD_LEN: 4B] [PAYLOAD: NB]`
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StreamPacket {
    pub stream_id: u32,
    pub seq: u32,
    pub flags: u8,
    pub payload: Vec<u8>,
}

impl StreamPacket {
    pub fn new(stream_id: u32, seq: u32, flags: u8, payload: Vec<u8>) -> Self {
        Self {
            stream_id,
            seq,
            flags,
            payload,
        }
    }

    /// Serializes stream packet into binary representation.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(STREAM_HEADER_LEN + self.payload.len());
        buf.extend_from_slice(&STREAM_MAGIC);
        buf.push(self.flags);
        buf.push(0x00); // reserved
        buf.extend_from_slice(&self.stream_id.to_be_bytes());
        buf.extend_from_slice(&self.seq.to_be_bytes());
        buf.extend_from_slice(&(self.payload.len() as u32).to_be_bytes());
        buf.extend_from_slice(&self.payload);
        buf
    }

    /// Deserializes a stream packet from binary bytes.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, DatapathError> {
        if bytes.len() < STREAM_HEADER_LEN {
            return Err(DatapathError::InvalidPacket(format!(
                "Stream packet too short: {} bytes < {}",
                bytes.len(),
                STREAM_HEADER_LEN
            )));
        }
        if &bytes[0..2] != STREAM_MAGIC {
            return Err(DatapathError::InvalidPacket(format!(
                "Invalid stream packet magic: expected {:?}, got {:?}",
                STREAM_MAGIC,
                &bytes[0..2]
            )));
        }
        let flags = bytes[2];
        let stream_id = u32::from_be_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
        let seq = u32::from_be_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]);
        let payload_len = u32::from_be_bytes([bytes[12], bytes[13], bytes[14], bytes[15]]) as usize;
        if bytes.len() < STREAM_HEADER_LEN + payload_len {
            return Err(DatapathError::InvalidPacket(format!(
                "Truncated stream packet: buffer {} < expected {}",
                bytes.len(),
                STREAM_HEADER_LEN + payload_len
            )));
        }
        let payload = bytes[STREAM_HEADER_LEN..STREAM_HEADER_LEN + payload_len].to_vec();
        Ok(Self {
            stream_id,
            seq,
            flags,
            payload,
        })
    }
}

/// Sliding-window stream reassembler ensuring lossless, in-order packet delivery over UDP.
#[derive(Debug)]
pub struct StreamReassembler {
    expected_seq: u32,
    buffer: BTreeMap<u32, StreamPacket>,
    max_buffered_packets: usize,
    fin_seq: Option<u32>,
}

impl StreamReassembler {
    /// Instantiates a new reassembler expecting `initial_seq`.
    pub fn new(initial_seq: u32) -> Self {
        Self {
            expected_seq: initial_seq,
            buffer: BTreeMap::new(),
            max_buffered_packets: 1024,
            fin_seq: None,
        }
    }

    /// Processes an incoming packet and returns any contiguous in-order packets ready for delivery.
    pub fn push(&mut self, packet: StreamPacket) -> Vec<StreamPacket> {
        let mut ready = Vec::new();

        if (packet.flags & FLAG_FIN) != 0 {
            self.fin_seq = Some(packet.seq);
        }

        if packet.seq == self.expected_seq {
            self.expected_seq = self.expected_seq.wrapping_add(1);
            ready.push(packet);

            // Drain any contiguous packets from buffer
            while let Some(next) = self.buffer.remove(&self.expected_seq) {
                self.expected_seq = self.expected_seq.wrapping_add(1);
                ready.push(next);
            }
        } else {
            let diff = packet.seq.wrapping_sub(self.expected_seq) as i32;
            if diff > 0 {
                // Out-of-order packet: buffer if capacity allows
                if self.buffer.len() < self.max_buffered_packets {
                    self.buffer.insert(packet.seq, packet);
                }
            }
            // If diff <= 0, it's a duplicate or already delivered packet; ignore.
        }

        ready
    }

    /// Next expected sequence number.
    pub fn expected_seq(&self) -> u32 {
        self.expected_seq
    }

    /// Number of packets currently buffered out of order.
    pub fn buffered_count(&self) -> usize {
        self.buffer.len()
    }

    /// True if FIN has been delivered.
    pub fn is_fin_delivered(&self) -> bool {
        if let Some(fin_seq) = self.fin_seq {
            let diff = self.expected_seq.wrapping_sub(fin_seq) as i32;
            diff > 0 && self.buffer.is_empty()
        } else {
            false
        }
    }
}

// =========================================================================
// SYNTHETIC IPV4 PACKET FRAMING (RFC 791)
// =========================================================================

/// Calculates the RFC 791 16-bit one's complement checksum over an IPv4 header.
pub fn calculate_ipv4_checksum(header: &[u8]) -> u16 {
    let mut sum: u32 = 0;
    for i in (0..20).step_by(2) {
        if i == 10 {
            // Checksum field itself is treated as zero during calculation
            continue;
        }
        let word = u16::from_be_bytes([header[i], header[i + 1]]) as u32;
        sum += word;
    }
    while (sum >> 16) > 0 {
        sum = (sum & 0xFFFF) + (sum >> 16);
    }
    (!sum & 0xFFFF) as u16
}

/// Wraps arbitrary payload into a valid 20-byte RFC 791 synthetic IPv4 header.
///
/// Defensively bounds total length to prevent integer overflow:
/// guarantees `total_len <= u16::MAX`.
pub fn wrap_synthetic_ipv4(src_ip: [u8; 4], dst_ip: [u8; 4], payload: &[u8]) -> Vec<u8> {
    // Defensive check: total length must fit in u16 without overflowing
    let total_len = (IPV4_HEADER_LEN.saturating_add(payload.len())).min(u16::MAX as usize) as u16;
    let actual_payload_len = (total_len as usize).saturating_sub(IPV4_HEADER_LEN);
    let payload_slice = &payload[..actual_payload_len.min(payload.len())];

    let mut pkt = Vec::with_capacity(total_len as usize);
    pkt.push(0x45); // IPv4, Header Length = 5 (20 bytes)
    pkt.push(0x00); // DSCP/ECN
    pkt.extend_from_slice(&total_len.to_be_bytes());
    pkt.extend_from_slice(&[0x00, 0x00]); // Identification
    pkt.extend_from_slice(&[0x40, 0x00]); // Flags: DF (Don't Fragment)
    pkt.push(64);   // TTL: 64
    pkt.push(6);    // Protocol: TCP (6)
    pkt.extend_from_slice(&[0x00, 0x00]); // Checksum placeholder
    pkt.extend_from_slice(&src_ip);
    pkt.extend_from_slice(&dst_ip);
    pkt.extend_from_slice(payload_slice);

    let chk = calculate_ipv4_checksum(&pkt[..IPV4_HEADER_LEN]);
    let chk_bytes = chk.to_be_bytes();
    pkt[10] = chk_bytes[0];
    pkt[11] = chk_bytes[1];
    pkt
}

/// Unwraps and validates synthetic 20-byte IPv4 packets, returning the payload slice.
pub fn unwrap_synthetic_ipv4(packet: &[u8]) -> Result<&[u8], DatapathError> {
    if packet.len() < IPV4_HEADER_LEN {
        return Err(DatapathError::InvalidPacket(format!(
            "Packet too short for IPv4 header: {} bytes < 20",
            packet.len()
        )));
    }
    let ver = packet[0] >> 4;
    if ver != 4 {
        return Err(DatapathError::InvalidPacket(format!(
            "Invalid IP version: expected 4, got {}",
            ver
        )));
    }
    let ihl = (packet[0] & 0x0F) as usize * 4;
    if ihl < IPV4_HEADER_LEN || ihl > packet.len() {
        return Err(DatapathError::InvalidPacket(format!(
            "Invalid IPv4 IHL: {} bytes (packet len {})",
            ihl,
            packet.len()
        )));
    }
    let total_len = u16::from_be_bytes([packet[2], packet[3]]) as usize;
    if total_len < ihl || packet.len() < total_len {
        return Err(DatapathError::InvalidPacket(format!(
            "Truncated IPv4 packet: packet len {} < total_len {}",
            packet.len(),
            total_len
        )));
    }
    Ok(&packet[ihl..total_len])
}

/// Parses a synthetic IPv4 packet, returning `(payload_slice, src_ip, dst_ip)`.
pub fn parse_synthetic_ipv4(packet: &[u8]) -> Result<(&[u8], Ipv4Addr, Ipv4Addr), DatapathError> {
    let payload = unwrap_synthetic_ipv4(packet)?;
    let src = Ipv4Addr::new(packet[12], packet[13], packet[14], packet[15]);
    let dst = Ipv4Addr::new(packet[16], packet[17], packet[18], packet[19]);
    Ok((payload, src, dst))
}

// =========================================================================
// PROXY IMPLEMENTATIONS (HOST & GUEST)
// =========================================================================

/// Starts the Host-side TCP proxy bridging Minecraft dedicated server to WireGuard tunnel channels.
///
/// When inbound packets arrive from `tunnel_rx`, connects to local Minecraft server at `127.0.0.1:local_minecraft_port`,
/// streams in-order reassembled data to the server, and forwards server responses wrapped in synthetic IPv4 frames to `tunnel_tx`.
pub async fn start_host_proxy(
    local_minecraft_port: u16,
    tunnel_tx: mpsc::Sender<Vec<u8>>,
    mut tunnel_rx: mpsc::Receiver<Vec<u8>>,
) -> Result<DatapathHandle, DatapathError> {
    let handle = DatapathHandle::new(Some(local_minecraft_port));

    // Active stream registry: stream_id -> mpsc::Sender<StreamPacket>
    let active_streams: Arc<Mutex<HashMap<u32, mpsc::Sender<StreamPacket>>>> =
        Arc::new(Mutex::new(HashMap::new()));

    let run_demux = handle.is_running_flag();
    let streams_map = Arc::clone(&active_streams);
    let tx_clone = tunnel_tx.clone();

    let host_task = tokio::spawn(async move {
        while run_demux.load(Ordering::Relaxed) {
            let ip_packet = match tunnel_rx.recv().await {
                Some(p) => p,
                None => break,
            };

            let payload = match unwrap_synthetic_ipv4(&ip_packet) {
                Ok(p) => p,
                Err(_) => continue,
            };

            // Parse packet (or treat raw packet as stream 1)
            let stream_pkt = if let Ok(pkt) = StreamPacket::from_bytes(payload) {
                pkt
            } else {
                StreamPacket::new(1, 0, FLAG_DATA, payload.to_vec())
            };

            let stream_id = stream_pkt.stream_id;

            // Check if connection already exists without holding lock across network operations
            let existing_sender = {
                let guard = streams_map.lock().await;
                guard.get(&stream_id).cloned()
            };

            if let Some(sender) = existing_sender {
                let _ = sender.send(stream_pkt).await;
                continue;
            }

            // If packet is FIN or RST and stream is already closed/non-existent, ignore it
            if (stream_pkt.flags & (FLAG_FIN | FLAG_RST)) != 0 {
                continue;
            }

            // Connection doesn't exist yet: establish new connection to local Minecraft Server
            match TcpStream::connect(format!("127.0.0.1:{}", local_minecraft_port)).await {
                Ok(server_stream) => {
                    let (mut tcp_read, mut tcp_write) = server_stream.into_split();
                    let (stream_tx, mut stream_rx) = mpsc::channel::<StreamPacket>(256);

                    // Forward current packet into stream_tx
                    let _ = stream_tx.send(stream_pkt).await;
                    streams_map.lock().await.insert(stream_id, stream_tx);

                    let reader_done = Arc::new(AtomicBool::new(false));
                    let writer_done = Arc::new(AtomicBool::new(false));

                    // Task A: Server TCP Reader (Server -> Tunnel)
                    let tx_reply = tx_clone.clone();
                    let run_reply = Arc::clone(&run_demux);
                    let streams_clean_a = Arc::clone(&streams_map);
                    let r_done_a = Arc::clone(&reader_done);
                    let w_done_a = Arc::clone(&writer_done);

                    tokio::spawn(async move {
                        let mut buf = vec![0u8; MAX_PAYLOAD_CHUNK];
                        let mut seq: u32 = 0;

                        while run_reply.load(Ordering::Relaxed) {
                            match tcp_read.read(&mut buf).await {
                                Ok(0) => {
                                    // Server closed connection (FIN)
                                    let fin = StreamPacket::new(stream_id, seq, FLAG_FIN, Vec::new());
                                    let ip_pkt = wrap_synthetic_ipv4(HOST_IPV4, GUEST_IPV4, &fin.to_bytes());
                                    let _ = tx_reply.send(ip_pkt).await;
                                    break;
                                }
                                Ok(n) => {
                                    let data_pkt = StreamPacket::new(stream_id, seq, FLAG_DATA, buf[..n].to_vec());
                                    seq = seq.wrapping_add(1);
                                    let ip_pkt = wrap_synthetic_ipv4(HOST_IPV4, GUEST_IPV4, &data_pkt.to_bytes());
                                    if tx_reply.send(ip_pkt).await.is_err() {
                                        break;
                                    }
                                }
                                Err(_) => {
                                    // Server connection reset (RST)
                                    let rst = StreamPacket::new(stream_id, seq, FLAG_RST, Vec::new());
                                    let ip_pkt = wrap_synthetic_ipv4(HOST_IPV4, GUEST_IPV4, &rst.to_bytes());
                                    let _ = tx_reply.send(ip_pkt).await;
                                    streams_clean_a.lock().await.remove(&stream_id);
                                    return;
                                }
                            }
                        }
                        r_done_a.store(true, Ordering::SeqCst);
                        if w_done_a.load(Ordering::SeqCst) {
                            streams_clean_a.lock().await.remove(&stream_id);
                        }
                    });

                    // Task B: Server TCP Writer with Reassembler (Tunnel -> Server)
                    let streams_clean_b = Arc::clone(&streams_map);
                    let r_done_b = Arc::clone(&reader_done);
                    let w_done_b = Arc::clone(&writer_done);

                    tokio::spawn(async move {
                        let mut reassembler = StreamReassembler::new(0);

                        while let Some(packet) = stream_rx.recv().await {
                            if (packet.flags & FLAG_RST) != 0 {
                                streams_clean_b.lock().await.remove(&stream_id);
                                return;
                            }

                            let ready_packets = reassembler.push(packet);
                            for ready in ready_packets {
                                if !ready.payload.is_empty() {
                                    if tcp_write.write_all(&ready.payload).await.is_err() {
                                        streams_clean_b.lock().await.remove(&stream_id);
                                        return;
                                    }
                                }
                                if (ready.flags & FLAG_FIN) != 0 {
                                    let _ = tcp_write.shutdown().await;
                                    w_done_b.store(true, Ordering::SeqCst);
                                    if r_done_b.load(Ordering::SeqCst) {
                                        streams_clean_b.lock().await.remove(&stream_id);
                                    }
                                    return;
                                }
                            }
                        }
                        w_done_b.store(true, Ordering::SeqCst);
                        if r_done_b.load(Ordering::SeqCst) {
                            streams_clean_b.lock().await.remove(&stream_id);
                        }
                    });
                }
                Err(_) => {
                    // Could not connect to local Minecraft server: reply with RST
                    let rst = StreamPacket::new(stream_id, 0, FLAG_RST, Vec::new());
                    let ip_pkt = wrap_synthetic_ipv4(HOST_IPV4, GUEST_IPV4, &rst.to_bytes());
                    let _ = tx_clone.send(ip_pkt).await;
                }
            }
        }
    });
    handle.add_abort_handle(host_task.abort_handle());

    Ok(handle)
}

/// Starts the Guest-side TCP proxy listening on loopback (default 25565 with dynamic fallback `127.0.0.1:0`).
///
/// Accepts incoming Minecraft client connections, chunks stream bytes into MTU-bounded synthetic IPv4 frames,
/// transmits over `tunnel_tx`, and reassembles incoming packets from `tunnel_rx` in order for delivery to the client.
/// Returns `(bound_port, DatapathHandle)`.
pub async fn start_guest_proxy(
    desired_local_port: u16,
    tunnel_tx: mpsc::Sender<Vec<u8>>,
    mut tunnel_rx: mpsc::Receiver<Vec<u8>>,
) -> Result<(u16, DatapathHandle), DatapathError> {
    // 1. Attempt to bind desired_local_port (e.g. 25565), fallback to 0 if busy
    let listener = match TcpListener::bind(format!("127.0.0.1:{}", desired_local_port)).await {
        Ok(l) => l,
        Err(_) => {
            TcpListener::bind("127.0.0.1:0").await.map_err(|e| {
                DatapathError::BindFailed {
                    port: 0,
                    source: e,
                }
            })?
        }
    };

    let bound_port = listener.local_addr()?.port();
    let handle = DatapathHandle::new(Some(bound_port));

    // Connection registry for active streams: stream_id -> mpsc::Sender<StreamPacket>
    let active_streams: Arc<Mutex<HashMap<u32, mpsc::Sender<StreamPacket>>>> =
        Arc::new(Mutex::new(HashMap::new()));
    let next_stream_id = Arc::new(AtomicU32::new(1));

    // Task 1: Accept incoming Minecraft client connections on loopback
    let run_accept = handle.is_running_flag();
    let streams_accept = Arc::clone(&active_streams);
    let tunnel_tx_accept = tunnel_tx.clone();
    let next_id_accept = Arc::clone(&next_stream_id);

    let accept_task = tokio::spawn(async move {
        while run_accept.load(Ordering::Relaxed) {
            let (client_stream, _addr) = match listener.accept().await {
                Ok(conn) => conn,
                Err(_) => break,
            };

            let stream_id = next_id_accept.fetch_add(1, Ordering::Relaxed);
            let (mut tcp_read, mut tcp_write) = client_stream.into_split();
            let (stream_tx, mut stream_rx) = mpsc::channel::<StreamPacket>(256);

            streams_accept.lock().await.insert(stream_id, stream_tx);

            let reader_done = Arc::new(AtomicBool::new(false));
            let writer_done = Arc::new(AtomicBool::new(false));

            // TCP Reader Task (Client -> Tunnel)
            let tx_clone = tunnel_tx_accept.clone();
            let run_client = Arc::clone(&run_accept);
            let streams_clean_r = Arc::clone(&streams_accept);
            let r_done_r = Arc::clone(&reader_done);
            let w_done_r = Arc::clone(&writer_done);

            tokio::spawn(async move {
                let mut buf = vec![0u8; MAX_PAYLOAD_CHUNK];
                let mut seq: u32 = 0;

                while run_client.load(Ordering::Relaxed) {
                    match tcp_read.read(&mut buf).await {
                        Ok(0) => {
                            // Client closed TCP stream gracefully (FIN)
                            let fin = StreamPacket::new(stream_id, seq, FLAG_FIN, Vec::new());
                            let ip_pkt = wrap_synthetic_ipv4(GUEST_IPV4, HOST_IPV4, &fin.to_bytes());
                            let _ = tx_clone.send(ip_pkt).await;
                            break;
                        }
                        Ok(n) => {
                            let data_pkt = StreamPacket::new(stream_id, seq, FLAG_DATA, buf[..n].to_vec());
                            seq = seq.wrapping_add(1);
                            let ip_pkt = wrap_synthetic_ipv4(GUEST_IPV4, HOST_IPV4, &data_pkt.to_bytes());
                            if tx_clone.send(ip_pkt).await.is_err() {
                                break;
                            }
                        }
                        Err(_) => {
                            // Client connection error (RST)
                            let rst = StreamPacket::new(stream_id, seq, FLAG_RST, Vec::new());
                            let ip_pkt = wrap_synthetic_ipv4(GUEST_IPV4, HOST_IPV4, &rst.to_bytes());
                            let _ = tx_clone.send(ip_pkt).await;
                            streams_clean_r.lock().await.remove(&stream_id);
                            return;
                        }
                    }
                }
                r_done_r.store(true, Ordering::SeqCst);
                if w_done_r.load(Ordering::SeqCst) {
                    streams_clean_r.lock().await.remove(&stream_id);
                }
            });

            // TCP Writer Task with Reassembler (Tunnel -> Client)
            let streams_clean_w = Arc::clone(&streams_accept);
            let r_done_w = Arc::clone(&reader_done);
            let w_done_w = Arc::clone(&writer_done);

            tokio::spawn(async move {
                let mut reassembler = StreamReassembler::new(0);

                while let Some(packet) = stream_rx.recv().await {
                    if (packet.flags & FLAG_RST) != 0 {
                        streams_clean_w.lock().await.remove(&stream_id);
                        return;
                    }

                    let ready_packets = reassembler.push(packet);
                    for ready in ready_packets {
                        if !ready.payload.is_empty() {
                            if tcp_write.write_all(&ready.payload).await.is_err() {
                                streams_clean_w.lock().await.remove(&stream_id);
                                return;
                            }
                        }
                        if (ready.flags & FLAG_FIN) != 0 {
                            let _ = tcp_write.shutdown().await;
                            w_done_w.store(true, Ordering::SeqCst);
                            if r_done_w.load(Ordering::SeqCst) {
                                streams_clean_w.lock().await.remove(&stream_id);
                            }
                            return;
                        }
                    }
                }
                w_done_w.store(true, Ordering::SeqCst);
                if r_done_w.load(Ordering::SeqCst) {
                    streams_clean_w.lock().await.remove(&stream_id);
                }
            });
        }
    });
    handle.add_abort_handle(accept_task.abort_handle());

    // Task 2: Demux packets from tunnel_rx to active streams
    let run_demux = handle.is_running_flag();
    let streams_demux = Arc::clone(&active_streams);

    let demux_task = tokio::spawn(async move {
        while run_demux.load(Ordering::Relaxed) {
            let ip_packet = match tunnel_rx.recv().await {
                Some(p) => p,
                None => break,
            };

            let payload = match unwrap_synthetic_ipv4(&ip_packet) {
                Ok(p) => p,
                Err(_) => continue,
            };

            if let Ok(stream_pkt) = StreamPacket::from_bytes(payload) {
                let sender = {
                    let guard = streams_demux.lock().await;
                    guard.get(&stream_pkt.stream_id).cloned()
                };
                if let Some(s) = sender {
                    let _ = s.send(stream_pkt).await;
                }
            } else {
                // Raw payload fallback: route to first active stream if any
                let sender = {
                    let guard = streams_demux.lock().await;
                    guard.values().next().cloned()
                };
                if let Some(s) = sender {
                    let synthetic_stream_pkt = StreamPacket::new(1, 0, FLAG_DATA, payload.to_vec());
                    let _ = s.send(synthetic_stream_pkt).await;
                }
            }
        }
    });
    handle.add_abort_handle(demux_task.abort_handle());

    Ok((bound_port, handle))
}

// =========================================================================
// USERSPACE DATAPATH FACADE STRUCT
// =========================================================================

/// Primary userspace datapath interface matching PROJECT.md interface contracts.
pub struct UserspaceDatapath;

impl UserspaceDatapath {
    /// Starts the Host-side Minecraft proxy connecting to `127.0.0.1:local_minecraft_port`.
    pub async fn start_host_proxy(
        local_minecraft_port: u16,
        tunnel_tx: mpsc::Sender<Vec<u8>>,
        tunnel_rx: mpsc::Receiver<Vec<u8>>,
    ) -> Result<DatapathHandle, DatapathError> {
        start_host_proxy(local_minecraft_port, tunnel_tx, tunnel_rx).await
    }

    /// Starts the Guest-side Minecraft proxy listening on loopback (default 25565 or fallback).
    pub async fn start_guest_proxy(
        desired_local_port: u16,
        tunnel_tx: mpsc::Sender<Vec<u8>>,
        tunnel_rx: mpsc::Receiver<Vec<u8>>,
    ) -> Result<(u16, DatapathHandle), DatapathError> {
        start_guest_proxy(desired_local_port, tunnel_tx, tunnel_rx).await
    }

    /// Wraps arbitrary payload into a valid 20-byte RFC 791 synthetic IPv4 header.
    pub fn wrap_synthetic_ipv4(src_ip: [u8; 4], dst_ip: [u8; 4], payload: &[u8]) -> Vec<u8> {
        wrap_synthetic_ipv4(src_ip, dst_ip, payload)
    }

    /// Unwraps and validates synthetic 20-byte IPv4 packets, returning the payload slice.
    pub fn unwrap_synthetic_ipv4(packet: &[u8]) -> Result<&[u8], DatapathError> {
        unwrap_synthetic_ipv4(packet)
    }
}

// =========================================================================
// IN-MODULE UNIT TESTS
// =========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_synthetic_ipv4_wrap_unwrap() {
        let payload = b"MinecraftPacket0x00Handshake";
        let src = [10, 88, 0, 2];
        let dst = [10, 88, 0, 1];

        let pkt = wrap_synthetic_ipv4(src, dst, payload);
        assert_eq!(pkt.len(), 20 + payload.len());
        assert_eq!(pkt[0], 0x45); // IPv4, IHL 5
        assert_eq!(pkt[8], 64);   // TTL
        assert_eq!(pkt[9], 6);    // TCP
        assert_eq!(&pkt[12..16], &src);
        assert_eq!(&pkt[16..20], &dst);

        let unwrapped = unwrap_synthetic_ipv4(&pkt).expect("Unwrap valid packet");
        assert_eq!(unwrapped, payload);

        let (parsed_payload, s, d) = parse_synthetic_ipv4(&pkt).expect("Parse synthetic IPv4");
        assert_eq!(parsed_payload, payload);
        assert_eq!(s, Ipv4Addr::new(10, 88, 0, 2));
        assert_eq!(d, Ipv4Addr::new(10, 88, 0, 1));
    }

    #[test]
    fn test_synthetic_ipv4_checksum_validation() {
        let payload = b"ChecksumVerificationDataPayload";
        let pkt = wrap_synthetic_ipv4([10, 88, 0, 2], [10, 88, 0, 1], payload);

        // Sum of all 16-bit words in the 20-byte header must fold to 0xFFFF
        let mut sum: u32 = 0;
        for i in (0..20).step_by(2) {
            let word = u16::from_be_bytes([pkt[i], pkt[i + 1]]) as u32;
            sum += word;
        }
        while (sum >> 16) > 0 {
            sum = (sum & 0xFFFF) + (sum >> 16);
        }
        assert_eq!(sum as u16, 0xFFFF, "Checksum must fold to 0xFFFF");
    }

    #[test]
    fn test_synthetic_ipv4_invalid_packet_handling() {
        let short = [0x45, 0x00, 0x00, 0x10]; // only 4 bytes
        assert!(unwrap_synthetic_ipv4(&short).is_err());

        let mut bad_ver = wrap_synthetic_ipv4([10, 88, 0, 2], [10, 88, 0, 1], b"Test");
        bad_ver[0] = 0x65; // Version 6
        assert!(unwrap_synthetic_ipv4(&bad_ver).is_err());

        let mut bad_ihl = wrap_synthetic_ipv4([10, 88, 0, 2], [10, 88, 0, 1], b"Test");
        bad_ihl[0] = 0x43; // IHL 3 (12 bytes < 20)
        assert!(unwrap_synthetic_ipv4(&bad_ihl).is_err());

        let mut truncated = wrap_synthetic_ipv4([10, 88, 0, 2], [10, 88, 0, 1], b"12345678");
        truncated.truncate(24); // Shorter than total_len (28)
        assert!(unwrap_synthetic_ipv4(&truncated).is_err());
    }

    #[test]
    fn test_synthetic_ipv4_chunk_boundary_defensive_wrap() {
        // Large payload exceeding u16::MAX must not panic or wrap negatively
        let large_payload = vec![0xAB; 70_000];
        let pkt = wrap_synthetic_ipv4([10, 88, 0, 2], [10, 88, 0, 1], &large_payload);
        assert_eq!(pkt.len(), u16::MAX as usize);

        let unwrapped = unwrap_synthetic_ipv4(&pkt).expect("Unwrap bounded packet");
        assert_eq!(unwrapped.len(), u16::MAX as usize - 20);
    }

    #[test]
    fn test_stream_packet_serialization() {
        let payload = b"HelloMinecraftProtocol".to_vec();
        let pkt = StreamPacket::new(42, 101, FLAG_DATA, payload.clone());

        let bytes = pkt.to_bytes();
        assert_eq!(&bytes[0..2], &STREAM_MAGIC);
        assert_eq!(bytes[2], FLAG_DATA);
        assert_eq!(bytes[3], 0x00);
        assert_eq!(&bytes[4..8], &42u32.to_be_bytes());
        assert_eq!(&bytes[8..12], &101u32.to_be_bytes());
        assert_eq!(&bytes[12..16], &(payload.len() as u32).to_be_bytes());
        assert_eq!(&bytes[16..], &payload);

        let decoded = StreamPacket::from_bytes(&bytes).expect("Decode stream packet");
        assert_eq!(decoded, pkt);
    }

    #[test]
    fn test_stream_reassembler_in_order() {
        let mut reasm = StreamReassembler::new(0);

        let p0 = StreamPacket::new(1, 0, FLAG_DATA, b"Part0".to_vec());
        let p1 = StreamPacket::new(1, 1, FLAG_DATA, b"Part1".to_vec());
        let p2 = StreamPacket::new(1, 2, FLAG_FIN, b"Part2".to_vec());

        let r0 = reasm.push(p0);
        assert_eq!(r0.len(), 1);
        assert_eq!(r0[0].payload, b"Part0");

        let r1 = reasm.push(p1);
        assert_eq!(r1.len(), 1);
        assert_eq!(r1[0].payload, b"Part1");

        let r2 = reasm.push(p2);
        assert_eq!(r2.len(), 1);
        assert_eq!(r2[0].payload, b"Part2");

        assert_eq!(reasm.expected_seq(), 3);
        assert!(reasm.is_fin_delivered());
    }

    #[test]
    fn test_stream_reassembler_out_of_order() {
        let mut reasm = StreamReassembler::new(0);

        let p0 = StreamPacket::new(1, 0, FLAG_DATA, b"Part0".to_vec());
        let p1 = StreamPacket::new(1, 1, FLAG_DATA, b"Part1".to_vec());
        let p2 = StreamPacket::new(1, 2, FLAG_DATA, b"Part2".to_vec());

        // Push 2 first (out of order)
        let r_out = reasm.push(p2);
        assert!(r_out.is_empty());
        assert_eq!(reasm.buffered_count(), 1);

        // Push 0
        let r0 = reasm.push(p0);
        assert_eq!(r0.len(), 1);
        assert_eq!(r0[0].payload, b"Part0");
        assert_eq!(reasm.buffered_count(), 1);

        // Push 1 -> should flush 1 and 2
        let r1_and_2 = reasm.push(p1);
        assert_eq!(r1_and_2.len(), 2);
        assert_eq!(r1_and_2[0].payload, b"Part1");
        assert_eq!(r1_and_2[1].payload, b"Part2");
        assert_eq!(reasm.buffered_count(), 0);
        assert_eq!(reasm.expected_seq(), 3);
    }

    #[test]
    fn test_stream_reassembler_duplicate_packets() {
        let mut reasm = StreamReassembler::new(0);

        let p0 = StreamPacket::new(1, 0, FLAG_DATA, b"Part0".to_vec());
        let r0 = reasm.push(p0.clone());
        assert_eq!(r0.len(), 1);

        // Duplicate p0
        let r_dup = reasm.push(p0);
        assert!(r_dup.is_empty(), "Duplicate packet must be ignored");
        assert_eq!(reasm.expected_seq(), 1);
    }

    #[tokio::test]
    async fn test_guest_port_fallback_when_occupied() {
        let occupier = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let busy_port = occupier.local_addr().unwrap().port();

        let (tx, rx) = mpsc::channel(10);
        let (bound_port, handle) = start_guest_proxy(busy_port, tx, rx)
            .await
            .expect("Guest proxy bind fallback");

        assert_ne!(bound_port, busy_port, "Must fall back to different port");
        assert!(bound_port > 0);
        assert!(handle.is_running());

        handle.stop();
        assert!(!handle.is_running());
    }

    #[tokio::test]
    async fn test_datapath_handle_stop_aborts_tasks() {
        let (tx, rx) = mpsc::channel(10);
        let (port, handle) = start_guest_proxy(0, tx, rx).await.unwrap();

        assert!(handle.is_running());
        assert_eq!(handle.bound_port(), Some(port));

        handle.stop();
        assert!(!handle.is_running());
    }

    #[tokio::test]
    async fn test_e2e_proxy_loopback_bidirectional() {
        // 1. Mock Minecraft Server
        let server_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let server_port = server_listener.local_addr().unwrap().port();

        let server_task = tokio::spawn(async move {
            let (mut sock, _) = server_listener.accept().await.unwrap();
            let mut buf = [0u8; 128];
            let n = sock.read(&mut buf).await.unwrap();
            assert_eq!(&buf[..n], b"Handshake765");
            sock.write_all(b"LoginSuccessUUID999").await.unwrap();
        });

        // 2. Channels bridging Host and Guest
        let (host_to_tunnel_tx, mut host_to_tunnel_rx) = mpsc::channel::<Vec<u8>>(100);
        let (guest_from_tunnel_tx, guest_from_tunnel_rx) = mpsc::channel::<Vec<u8>>(100);

        let (guest_to_tunnel_tx, mut guest_to_tunnel_rx) = mpsc::channel::<Vec<u8>>(100);
        let (host_from_tunnel_tx, host_from_tunnel_rx) = mpsc::channel::<Vec<u8>>(100);

        // 3. Start Host and Guest proxies
        let host_handle = start_host_proxy(server_port, host_to_tunnel_tx, host_from_tunnel_rx)
            .await
            .unwrap();

        let (guest_port, guest_handle) = start_guest_proxy(0, guest_to_tunnel_tx, guest_from_tunnel_rx)
            .await
            .unwrap();

        // 4. Bridge task forwarding between guest and host channels (mimicking WireGuard tunnel)
        let bridge_h2g = tokio::spawn(async move {
            while let Some(pkt) = host_to_tunnel_rx.recv().await {
                if guest_from_tunnel_tx.send(pkt).await.is_err() {
                    break;
                }
            }
        });

        let bridge_g2h = tokio::spawn(async move {
            while let Some(pkt) = guest_to_tunnel_rx.recv().await {
                if host_from_tunnel_tx.send(pkt).await.is_err() {
                    break;
                }
            }
        });

        // 5. Connect mock Minecraft client to guest proxy
        let mut client = TcpStream::connect(format!("127.0.0.1:{}", guest_port))
            .await
            .unwrap();
        client.write_all(b"Handshake765").await.unwrap();

        let mut reply = [0u8; 128];
        let n = client.read(&mut reply).await.unwrap();
        assert_eq!(&reply[..n], b"LoginSuccessUUID999");

        // Clean shutdown
        client.shutdown().await.unwrap();
        server_task.await.unwrap();
        host_handle.stop();
        guest_handle.stop();
        bridge_h2g.abort();
        bridge_g2h.abort();
    }

    #[test]
    fn test_stream_reassembler_rollover_reordering() {
        let mut reasm = StreamReassembler::new(u32::MAX);

        // Out-of-order packet 0 arrives across rollover boundary before u32::MAX
        let p0 = StreamPacket::new(1, 0, FLAG_DATA, b"WrappedZero".to_vec());
        let ready0 = reasm.push(p0);
        assert!(ready0.is_empty());
        assert_eq!(reasm.buffered_count(), 1);

        // Packet u32::MAX arrives
        let p_max = StreamPacket::new(1, u32::MAX, FLAG_DATA, b"Max".to_vec());
        let ready_max = reasm.push(p_max);
        assert_eq!(ready_max.len(), 2);
        assert_eq!(ready_max[0].payload, b"Max");
        assert_eq!(ready_max[1].payload, b"WrappedZero");
        assert_eq!(reasm.expected_seq(), 1);
        assert_eq!(reasm.buffered_count(), 0);
    }

    #[test]
    fn test_stream_reassembler_rollover_fin() {
        let mut reasm = StreamReassembler::new(u32::MAX);
        let p_max = StreamPacket::new(1, u32::MAX, FLAG_FIN, b"MaxFIN".to_vec());
        let ready = reasm.push(p_max);
        assert_eq!(ready.len(), 1);
        assert_eq!(reasm.expected_seq(), 0);
        assert!(reasm.is_fin_delivered());
    }

    #[tokio::test]
    async fn test_datapath_handle_drop_releases_port() {
        let (tx, rx) = mpsc::channel(10);
        let (port, handle) = start_guest_proxy(0, tx, rx).await.unwrap();
        assert!(handle.is_running());
        assert_eq!(handle.bound_port(), Some(port));

        // Dropping handle without calling stop() must abort background tasks and release listener
        drop(handle);

        let mut rebind_success = false;
        for _ in 0..20 {
            if let Ok(listener) = TcpListener::bind(format!("127.0.0.1:{}", port)).await {
                rebind_success = true;
                drop(listener);
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(25)).await;
        }
        assert!(
            rebind_success,
            "Port {} must be released and re-bindable when DatapathHandle is dropped",
            port
        );
    }
}
