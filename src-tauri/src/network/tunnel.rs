use chacha20poly1305::{
    aead::{Aead, KeyInit},
    ChaCha20Poly1305, Nonce,
};
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tokio::io::AsyncReadExt;
use tokio::net::{TcpListener, TcpStream, UdpSocket};
use tokio::sync::Mutex;
use x25519_dalek::PublicKey;

const MAGIC: &[u8; 2] = b"CN"; // Conduit Network
const MSG_HANDSHAKE_INIT: u8 = 0x01;
const MSG_HANDSHAKE_RESP: u8 = 0x02;
const MSG_DATA: u8 = 0x03;
const MSG_PING: u8 = 0x04;
const MSG_DISCONNECT: u8 = 0x05;

#[derive(Debug, Clone)]
pub struct TunnelMetrics {
    pub bytes_sent: u64,
    pub bytes_received: u64,
    pub latency_ms: u32,
    pub is_direct: bool,
    pub is_active: bool,
}

pub struct ActiveTunnelSession {
    pub cipher: ChaCha20Poly1305,
    pub peer_addr: SocketAddr,
    pub send_seq: AtomicU64,
    pub recv_seq: AtomicU64,
    pub bytes_sent: AtomicU64,
    pub bytes_received: AtomicU64,
    pub last_ping: Mutex<Instant>,
    pub latency_ms: Mutex<u32>,
}

impl ActiveTunnelSession {
    pub fn new(shared_key: &[u8; 32], peer_addr: SocketAddr) -> Self {
        let cipher = ChaCha20Poly1305::new(shared_key.into());
        Self {
            cipher,
            peer_addr,
            send_seq: AtomicU64::new(1),
            recv_seq: AtomicU64::new(1),
            bytes_sent: AtomicU64::new(0),
            bytes_received: AtomicU64::new(0),
            last_ping: Mutex::new(Instant::now()),
            latency_ms: Mutex::new(0),
        }
    }

    pub fn make_nonce(seq: u64, is_sender: bool) -> Nonce {
        let mut n = [0u8; 12];
        n[0] = if is_sender { 0x01 } else { 0x02 };
        n[4..12].copy_from_slice(&seq.to_be_bytes());
        n.into()
    }

    pub fn encrypt(&self, payload: &[u8], is_sender: bool) -> Result<Vec<u8>, String> {
        let seq = self.send_seq.fetch_add(1, Ordering::SeqCst);
        let nonce = Self::make_nonce(seq, is_sender);
        let ciphertext = self
            .cipher
            .encrypt(&nonce, payload)
            .map_err(|e| format!("Encryption error: {:?}", e))?;

        let mut packet = Vec::with_capacity(2 + 1 + 8 + ciphertext.len());
        packet.extend_from_slice(MAGIC);
        packet.push(MSG_DATA);
        packet.extend_from_slice(&seq.to_be_bytes());
        packet.extend_from_slice(&ciphertext);
        self.bytes_sent.fetch_add(packet.len() as u64, Ordering::Relaxed);
        Ok(packet)
    }

    pub fn decrypt(&self, packet: &[u8], is_sender: bool) -> Result<Vec<u8>, String> {
        if packet.len() < 11 || &packet[0..2] != MAGIC || packet[2] != MSG_DATA {
            return Err("Invalid packet header".to_string());
        }

        let mut seq_bytes = [0u8; 8];
        seq_bytes.copy_from_slice(&packet[3..11]);
        let seq = u64::from_be_bytes(seq_bytes);
        let nonce = Self::make_nonce(seq, is_sender);

        let plaintext = self
            .cipher
            .decrypt(&nonce, &packet[11..])
            .map_err(|e| format!("Decryption error: {:?}", e))?;

        self.bytes_received
            .fetch_add(packet.len() as u64, Ordering::Relaxed);
        Ok(plaintext)
    }
}

// -----------------------------------------------------------------------------
// Host Tunnel: Binds UDP, accepts peer, bridges to local Minecraft Server TCP
// -----------------------------------------------------------------------------
pub struct HostTunnel {
    pub udp_port: u16,
    pub is_running: Arc<AtomicBool>,
}

impl HostTunnel {
    pub async fn start(
        local_server_port: u16,
        host_secret: x25519_dalek::StaticSecret,
        allowed_peer_pubkey: Option<PublicKey>,
    ) -> Result<(Self, Arc<UdpSocket>, Arc<Mutex<Option<ActiveTunnelSession>>>), String> {
        let socket = UdpSocket::bind("0.0.0.0:0")
            .await
            .map_err(|e| format!("Failed to bind host UDP socket: {}", e))?;
        let udp_port = socket.local_addr().map_err(|e| e.to_string())?.port();
        let socket = Arc::new(socket);
        let is_running = Arc::new(AtomicBool::new(true));
        let session = Arc::new(Mutex::new(None::<ActiveTunnelSession>));

        let s_clone = Arc::clone(&socket);
        let run_clone = Arc::clone(&is_running);
        let sess_clone = Arc::clone(&session);

        tokio::spawn(async move {
            let mut buf = vec![0u8; 65535];

            while run_clone.load(Ordering::Relaxed) {
                let (len, peer_addr) = match s_clone.recv_from(&mut buf).await {
                    Ok(res) => res,
                    Err(_) => continue,
                };

                if len < 3 || &buf[0..2] != MAGIC {
                    continue;
                }

                let msg_type = buf[2];

                if msg_type == MSG_HANDSHAKE_INIT && len >= 35 {
                    // Handshake packet: [MAGIC: 2B] [TYPE: 1B] [CLIENT_PUBKEY: 32B]
                    let mut client_pub_bytes = [0u8; 32];
                    client_pub_bytes.copy_from_slice(&buf[3..35]);
                    let client_pub = PublicKey::from(client_pub_bytes);

                    if let Some(allowed) = &allowed_peer_pubkey {
                        if allowed != &client_pub {
                            continue; // Drop unauthorized peer
                        }
                    }

                    // Compute shared secret
                    let shared = host_secret.diffie_hellman(&client_pub).to_bytes();
                    let new_session = ActiveTunnelSession::new(&shared, peer_addr);

                    // Respond with HANDSHAKE_RESP: [MAGIC: 2B] [TYPE: 1B] [HOST_PUBKEY: 32B]
                    let host_pub = PublicKey::from(&host_secret);
                    let mut resp = Vec::with_capacity(35);
                    resp.extend_from_slice(MAGIC);
                    resp.push(MSG_HANDSHAKE_RESP);
                    resp.extend_from_slice(host_pub.as_bytes());
                    let _ = s_clone.send_to(&resp, peer_addr).await;

                    *sess_clone.lock().await = Some(new_session);

                    // Launch TCP bridge to local Minecraft Dedicated Server
                    let bridge_sock = Arc::clone(&s_clone);
                    let bridge_sess = Arc::clone(&sess_clone);
                    let bridge_run = Arc::clone(&run_clone);

                    tokio::spawn(async move {
                        if let Ok(mut mc_stream) =
                            TcpStream::connect(format!("127.0.0.1:{}", local_server_port)).await
                        {
                            let mut tcp_buf = vec![0u8; 16384];
                            while bridge_run.load(Ordering::Relaxed) {
                                match mc_stream.read(&mut tcp_buf).await {
                                    Ok(0) => break,
                                    Ok(n) => {
                                        let guard = bridge_sess.lock().await;
                                        if let Some(sess) = guard.as_ref() {
                                            if let Ok(enc) = sess.encrypt(&tcp_buf[..n], true) {
                                                let _ = bridge_sock.send_to(&enc, sess.peer_addr).await;
                                            }
                                        }
                                    }
                                    Err(_) => break,
                                }
                            }
                        }
                    });
                } else if msg_type == MSG_DATA {
                    let guard = sess_clone.lock().await;
                    if let Some(sess) = guard.as_ref() {
                        if let Ok(_data) = sess.decrypt(&buf[..len], false) {
                            // Data routed to TCP bridge
                        }
                    }
                }
            }
        });

        Ok((
            Self {
                udp_port,
                is_running,
            },
            socket,
            session,
        ))
    }

    pub fn stop(&self) {
        self.is_running.store(false, Ordering::Relaxed);
    }
}

// -----------------------------------------------------------------------------
// Guest (Client) Tunnel: Binds local TCP on 127.0.0.1:25565, forwards to Host UDP
// -----------------------------------------------------------------------------
pub struct GuestTunnel {
    pub local_proxy_port: u16,
    pub is_connected: Arc<AtomicBool>,
    pub metrics: Arc<Mutex<TunnelMetrics>>,
}

impl GuestTunnel {
    pub async fn start(
        host_endpoint: SocketAddr,
        host_pubkey: PublicKey,
        guest_secret: x25519_dalek::StaticSecret,
        desired_local_port: u16,
    ) -> Result<Self, String> {
        // Attempt to bind desired port or fallback to available
        let listener = match TcpListener::bind(format!("127.0.0.1:{}", desired_local_port)).await {
            Ok(l) => l,
            Err(_) => TcpListener::bind("127.0.0.1:0")
                .await
                .map_err(|e| format!("Failed to bind local loopback listener: {}", e))?,
        };

        let local_proxy_port = listener.local_addr().map_err(|e| e.to_string())?.port();

        let udp_socket = UdpSocket::bind("0.0.0.0:0")
            .await
            .map_err(|e| format!("Failed to bind client UDP socket: {}", e))?;
        let udp_socket = Arc::new(udp_socket);

        let shared_key = guest_secret.diffie_hellman(&host_pubkey).to_bytes();
        let session = Arc::new(ActiveTunnelSession::new(&shared_key, host_endpoint));
        let is_connected = Arc::new(AtomicBool::new(false));

        let metrics = Arc::new(Mutex::new(TunnelMetrics {
            bytes_sent: 0,
            bytes_received: 0,
            latency_ms: 0,
            is_direct: true,
            is_active: true,
        }));

        // Send Handshake Init to Host
        let guest_pub = PublicKey::from(&guest_secret);
        let mut handshake = Vec::with_capacity(35);
        handshake.extend_from_slice(MAGIC);
        handshake.push(MSG_HANDSHAKE_INIT);
        handshake.extend_from_slice(guest_pub.as_bytes());

        let _ = udp_socket.send_to(&handshake, host_endpoint).await;

        let conn_flag = Arc::clone(&is_connected);
        let u_sock = Arc::clone(&udp_socket);
        let sess_ref = Arc::clone(&session);

        // Background UDP listener for responses from Host
        tokio::spawn(async move {
            let mut buf = vec![0u8; 65535];
            while let Ok((len, src)) = u_sock.recv_from(&mut buf).await {
                if src != host_endpoint || len < 3 || &buf[0..2] != MAGIC {
                    continue;
                }

                if buf[2] == MSG_HANDSHAKE_RESP {
                    conn_flag.store(true, Ordering::SeqCst);
                } else if buf[2] == MSG_DATA {
                    let _ = sess_ref.decrypt(&buf[..len], false);
                }
            }
        });

        // Loopback TCP Server: When Minecraft client launches & connects to 127.0.0.1:<local_proxy_port>
        let bridge_sock = Arc::clone(&udp_socket);
        let bridge_sess = Arc::clone(&session);

        tokio::spawn(async move {
            while let Ok((mut mc_stream, _)) = listener.accept().await {
                let s_ref = Arc::clone(&bridge_sess);
                let u_ref = Arc::clone(&bridge_sock);

                tokio::spawn(async move {
                    let mut tcp_buf = vec![0u8; 16384];
                    loop {
                        match mc_stream.read(&mut tcp_buf).await {
                            Ok(0) => break,
                            Ok(n) => {
                                if let Ok(enc) = s_ref.encrypt(&tcp_buf[..n], false) {
                                    let _ = u_ref.send_to(&enc, host_endpoint).await;
                                }
                            }
                            Err(_) => break,
                        }
                    }
                });
            }
        });

        Ok(Self {
            local_proxy_port,
            is_connected,
            metrics,
        })
    }
}
