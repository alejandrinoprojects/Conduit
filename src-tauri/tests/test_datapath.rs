//! Standalone Test Suite for Milestone 2: Non-Privileged Userspace Datapath & Minecraft Port 25565 Proxy.
//!
//! Validates:
//! - Non-privileged loopback proxy bind and connect (zero root/admin or kernel TUN/TAP driver requirement)
//! - Dynamic port fallback when port 25565 is occupied
//! - Synthetic 20-byte RFC 791 IPv4 wrapping, unwrap, checksum, and chunk boundary checks (MTU <= 1400 bytes)
//! - Reliable stream transport: chunking, sequence numbering, and out-of-order reassembly
//! - Full bidirectional loopback data transfer of simulated Minecraft protocol messages
//! - Graceful FIN/RST termination handling and multi-session reuse
//! - Full end-to-end integration with BoringTun WireGuard engine

#[path = "../src/network/datapath.rs"]
mod datapath;

#[path = "../src/network/wireguard.rs"]
mod wireguard;

#[path = "../src/network/identity.rs"]
mod identity;

use datapath::*;
use std::net::Ipv4Addr;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;

// =========================================================================
// 1. NON-PRIVILEGED LOOPBACK PROXY BIND & CONNECT TESTS
// =========================================================================

#[tokio::test]
async fn test_non_privileged_loopback_proxy_bind_and_connect() {
    let (tx, rx) = mpsc::channel(100);

    // Bind guest proxy on available port
    let (bound_port, handle) = UserspaceDatapath::start_guest_proxy(0, tx, rx)
        .await
        .expect("Guest proxy bind without privilege");

    assert!(bound_port > 0);
    assert!(handle.is_running());
    assert_eq!(handle.bound_port(), Some(bound_port));

    // Non-privileged client can connect to local loopback
    let client = TcpStream::connect(format!("127.0.0.1:{}", bound_port)).await;
    assert!(client.is_ok(), "Client connection must succeed on loopback");

    handle.stop();
    assert!(!handle.is_running());
}

#[tokio::test]
async fn test_dynamic_port_fallback_when_occupied() {
    // Occupy primary port first
    let occupier = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let occupied_port = occupier.local_addr().unwrap().port();

    let (tx, rx) = mpsc::channel(100);

    // Guest attempts to bind occupied port -> should fall back gracefully to dynamic port
    let (bound_port, handle) = UserspaceDatapath::start_guest_proxy(occupied_port, tx, rx)
        .await
        .expect("Fallback to dynamic port must succeed");

    assert_ne!(
        bound_port, occupied_port,
        "Bound port must differ from occupied port"
    );
    assert!(bound_port > 0);

    // Verify connectability on fallback port
    let client = TcpStream::connect(format!("127.0.0.1:{}", bound_port)).await;
    assert!(client.is_ok(), "Client must connect to fallback port");

    handle.stop();
    drop(occupier);
}

// =========================================================================
// 2. SYNTHETIC IPV4 FRAMING & BOUNDARY CHECKS
// =========================================================================

#[test]
fn test_synthetic_ipv4_wrapping_and_unwrap_fidelity() {
    let raw_payload = b"GET /minecraft/status HTTP/1.1\r\nHost: localhost\r\n\r\n";
    let src = [10, 88, 0, 2];
    let dst = [10, 88, 0, 1];

    let packet = UserspaceDatapath::wrap_synthetic_ipv4(src, dst, raw_payload);
    assert_eq!(packet.len(), 20 + raw_payload.len());
    assert_eq!(packet[0], 0x45); // IPv4, IHL 5
    assert_eq!(packet[8], 64);   // TTL 64
    assert_eq!(packet[9], 6);    // TCP protocol

    // Verify header checksum folds to 0xFFFF
    let mut sum: u32 = 0;
    for i in (0..20).step_by(2) {
        let word = u16::from_be_bytes([packet[i], packet[i + 1]]) as u32;
        sum += word;
    }
    while (sum >> 16) > 0 {
        sum = (sum & 0xFFFF) + (sum >> 16);
    }
    assert_eq!(sum as u16, 0xFFFF);

    // Unwrap and verify payload
    let unwrapped = UserspaceDatapath::unwrap_synthetic_ipv4(&packet).expect("Unwrap packet");
    assert_eq!(unwrapped, raw_payload);

    let (extracted, s_ip, d_ip) = parse_synthetic_ipv4(&packet).expect("Parse synthetic IPv4");
    assert_eq!(extracted, raw_payload);
    assert_eq!(s_ip, Ipv4Addr::new(10, 88, 0, 2));
    assert_eq!(d_ip, Ipv4Addr::new(10, 88, 0, 1));
}

#[test]
fn test_synthetic_ipv4_chunk_boundary_and_truncation_defense() {
    // 1. Exact MTU payload test (1380 bytes payload + 20 bytes header = 1400 bytes)
    let mtu_payload = vec![0xEEu8; 1380];
    let pkt = UserspaceDatapath::wrap_synthetic_ipv4([10, 88, 0, 2], [10, 88, 0, 1], &mtu_payload);
    assert_eq!(pkt.len(), 1400);
    let unwrapped = UserspaceDatapath::unwrap_synthetic_ipv4(&pkt).unwrap();
    assert_eq!(unwrapped.len(), 1380);

    // 2. Defensive integer wrap check with 100,000 bytes
    let huge_payload = vec![0x77u8; 100_000];
    let bounded_pkt = UserspaceDatapath::wrap_synthetic_ipv4([10, 88, 0, 2], [10, 88, 0, 1], &huge_payload);
    assert_eq!(bounded_pkt.len(), u16::MAX as usize);

    // 3. Corrupted / truncated packet checks
    assert!(UserspaceDatapath::unwrap_synthetic_ipv4(&[]).is_err());
    assert!(UserspaceDatapath::unwrap_synthetic_ipv4(&[0x45, 0x00, 0x00]).is_err());

    let mut bad_ver = pkt.clone();
    bad_ver[0] = 0x55; // Version 5
    assert!(UserspaceDatapath::unwrap_synthetic_ipv4(&bad_ver).is_err());

    let mut bad_ihl = pkt.clone();
    bad_ihl[0] = 0x42; // IHL 2 (8 bytes < 20)
    assert!(UserspaceDatapath::unwrap_synthetic_ipv4(&bad_ihl).is_err());
}

// =========================================================================
// 3. RELIABLE STREAM TRANSPORT & OUT-OF-ORDER REASSEMBLY TESTS
// =========================================================================

#[test]
fn test_stream_packet_serialization_roundtrip() {
    let payload = b"ChunkPayload123456789".to_vec();
    let original = StreamPacket::new(100, 5, FLAG_DATA, payload.clone());

    let encoded = original.to_bytes();
    assert_eq!(&encoded[0..2], &STREAM_MAGIC);

    let decoded = StreamPacket::from_bytes(&encoded).expect("Decode stream packet");
    assert_eq!(decoded.stream_id, 100);
    assert_eq!(decoded.seq, 5);
    assert_eq!(decoded.flags, FLAG_DATA);
    assert_eq!(decoded.payload, payload);
}

#[test]
fn test_reliable_stream_reassembly_out_of_order() {
    let mut reassembler = StreamReassembler::new(0);

    let p0 = StreamPacket::new(1, 0, FLAG_DATA, b"AAAA_".to_vec());
    let p1 = StreamPacket::new(1, 1, FLAG_DATA, b"BBBB_".to_vec());
    let p2 = StreamPacket::new(1, 2, FLAG_DATA, b"CCCC_".to_vec());
    let p3 = StreamPacket::new(1, 3, FLAG_DATA, b"DDDD_".to_vec());
    let p4 = StreamPacket::new(1, 4, FLAG_FIN, b"EEEE".to_vec());

    // Deliver in scrambled order: [2, 4, 1]
    let r2 = reassembler.push(p2);
    assert!(r2.is_empty());
    assert_eq!(reassembler.buffered_count(), 1);

    let r4 = reassembler.push(p4);
    assert!(r4.is_empty());
    assert_eq!(reassembler.buffered_count(), 2);

    let r1 = reassembler.push(p1);
    assert!(r1.is_empty());
    assert_eq!(reassembler.buffered_count(), 3);

    // Now push 0 -> should emit 0, 1, 2 (length 3)!
    let r0 = reassembler.push(p0);
    assert_eq!(r0.len(), 3);
    assert_eq!(r0[0].payload, b"AAAA_");
    assert_eq!(r0[1].payload, b"BBBB_");
    assert_eq!(r0[2].payload, b"CCCC_");
    assert_eq!(reassembler.expected_seq(), 3);
    assert_eq!(reassembler.buffered_count(), 1); // Only p4 remains in buffer

    // Now push 3 -> should emit 3, 4 (length 2)!
    let r3 = reassembler.push(p3);
    assert_eq!(r3.len(), 2);
    assert_eq!(r3[0].payload, b"DDDD_");
    assert_eq!(r3[1].payload, b"EEEE");
    assert_eq!(reassembler.buffered_count(), 0);
    assert_eq!(reassembler.expected_seq(), 5);
    assert!(reassembler.is_fin_delivered());
}

#[test]
fn test_stream_reassembler_cascade_drain() {
    let mut reassembler = StreamReassembler::new(0);

    let p0 = StreamPacket::new(1, 0, FLAG_DATA, b"0".to_vec());
    let p1 = StreamPacket::new(1, 1, FLAG_DATA, b"1".to_vec());
    let p2 = StreamPacket::new(1, 2, FLAG_DATA, b"2".to_vec());
    let p3 = StreamPacket::new(1, 3, FLAG_DATA, b"3".to_vec());

    // Buffer 1, 2, 3
    assert!(reassembler.push(p1).is_empty());
    assert!(reassembler.push(p2).is_empty());
    assert!(reassembler.push(p3).is_empty());
    assert_eq!(reassembler.buffered_count(), 3);

    // Push 0 -> must drain 0, 1, 2, 3 in order!
    let drained = reassembler.push(p0);
    assert_eq!(drained.len(), 4);
    assert_eq!(drained[0].payload, b"0");
    assert_eq!(drained[1].payload, b"1");
    assert_eq!(drained[2].payload, b"2");
    assert_eq!(drained[3].payload, b"3");
    assert_eq!(reassembler.buffered_count(), 0);
    assert_eq!(reassembler.expected_seq(), 4);
}

#[test]
fn test_stream_reassembler_duplicate_rejection() {
    let mut reassembler = StreamReassembler::new(0);
    let p0 = StreamPacket::new(1, 0, FLAG_DATA, b"ChunkZero".to_vec());

    let r1 = reassembler.push(p0.clone());
    assert_eq!(r1.len(), 1);
    assert_eq!(reassembler.expected_seq(), 1);

    // Duplicate p0
    let r2 = reassembler.push(p0);
    assert!(r2.is_empty());
    assert_eq!(reassembler.expected_seq(), 1);
}

// =========================================================================
// 4. FULL BIDIRECTIONAL LOOPBACK DATA TRANSFER (MINECRAFT PROTOCOL)
// =========================================================================

#[tokio::test]
async fn test_full_bidirectional_minecraft_protocol_loopback_transfer() {
    // 1. Mock Minecraft Server listening on loopback
    let mc_server_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let mc_server_port = mc_server_listener.local_addr().unwrap().port();

    let server_task = tokio::spawn(async move {
        let (mut socket, _) = mc_server_listener.accept().await.unwrap();

        // 1a. Receive Minecraft Handshake Packet (ID 0x00)
        let mut buf = [0u8; 256];
        let n = socket.read(&mut buf).await.unwrap();
        assert!(n >= 15);
        assert_eq!(buf[0], 0x00); // Handshake ID

        // Verify hostname "localhost" in binary payload
        assert!(buf[..n].windows(9).any(|w| w == b"localhost"));

        // 1b. Send Minecraft Login Success Packet (ID 0x02)
        let mut reply = Vec::new();
        reply.push(0x02); // Login Success Packet ID
        reply.extend_from_slice(b"UUID-1111-2222-3333-4444");
        reply.extend_from_slice(b"ConduitPlayer");
        socket.write_all(&reply).await.unwrap();

        // 1c. Receive Chat Message from Client
        let n_chat = socket.read(&mut buf).await.unwrap();
        assert_eq!(&buf[..n_chat], b"Hello Minecraft Server!");

        // 1d. Reply with Chat Ack
        socket.write_all(b"Welcome to Conduit Mesh Realm!").await.unwrap();
    });

    // 2. Wire channels simulating network transport between Host and Guest
    let (host_to_tunnel_tx, mut host_to_tunnel_rx) = mpsc::channel::<Vec<u8>>(100);
    let (guest_from_tunnel_tx, guest_from_tunnel_rx) = mpsc::channel::<Vec<u8>>(100);

    let (guest_to_tunnel_tx, mut guest_to_tunnel_rx) = mpsc::channel::<Vec<u8>>(100);
    let (host_from_tunnel_tx, host_from_tunnel_rx) = mpsc::channel::<Vec<u8>>(100);

    // 3. Launch Host and Guest proxies
    let host_handle = UserspaceDatapath::start_host_proxy(
        mc_server_port,
        host_to_tunnel_tx,
        host_from_tunnel_rx,
    )
    .await
    .expect("Start host proxy");

    let (guest_port, guest_handle) = UserspaceDatapath::start_guest_proxy(
        0,
        guest_to_tunnel_tx,
        guest_from_tunnel_rx,
    )
    .await
    .expect("Start guest proxy");

    // 4. Channel forwarding tasks
    let bridge_g2h = tokio::spawn(async move {
        while let Some(packet) = guest_to_tunnel_rx.recv().await {
            if host_from_tunnel_tx.send(packet).await.is_err() {
                break;
            }
        }
    });

    let bridge_h2g = tokio::spawn(async move {
        while let Some(packet) = host_to_tunnel_rx.recv().await {
            if guest_from_tunnel_tx.send(packet).await.is_err() {
                break;
            }
        }
    });

    // 5. Connect simulated Minecraft Client to guest proxy
    let mut client = TcpStream::connect(format!("127.0.0.1:{}", guest_port))
        .await
        .expect("Client connect to guest proxy");

    // 5a. Send Handshake
    let mut handshake = Vec::new();
    handshake.push(0x00);
    handshake.extend_from_slice(b"\x00\x02localhost\x63\xdd\x02");
    client.write_all(&handshake).await.unwrap();

    // 5b. Read Login Success
    let mut resp = [0u8; 128];
    let n_resp = client.read(&mut resp).await.unwrap();
    assert_eq!(resp[0], 0x02);
    let resp_text = std::str::from_utf8(&resp[..n_resp]).unwrap_or_default();
    assert!(resp_text.contains("UUID-1111-2222-3333-4444"));
    assert!(resp_text.contains("ConduitPlayer"));

    // 5c. Send Chat Message
    client.write_all(b"Hello Minecraft Server!").await.unwrap();

    // 5d. Read Chat Ack
    let n_ack = client.read(&mut resp).await.unwrap();
    assert_eq!(&resp[..n_ack], b"Welcome to Conduit Mesh Realm!");

    // Clean shutdown
    client.shutdown().await.unwrap();
    server_task.await.unwrap();
    host_handle.stop();
    guest_handle.stop();
    bridge_g2h.abort();
    bridge_h2g.abort();
}

// =========================================================================
// 5. GRACEFUL TERMINATION & CONNECTION REUSE
// =========================================================================

#[tokio::test]
async fn test_graceful_stream_fin_termination() {
    let server_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let server_port = server_listener.local_addr().unwrap().port();

    let server_task = tokio::spawn(async move {
        let (mut socket, _) = server_listener.accept().await.unwrap();
        let mut buf = [0u8; 32];
        let n = socket.read(&mut buf).await.unwrap();
        assert_eq!(&buf[..n], b"PingBeforeFin");

        // Expect EOF (0 bytes) after client shuts down write half
        let eof = socket.read(&mut buf).await.unwrap();
        assert_eq!(eof, 0, "Server must observe clean EOF");
    });

    let (h_tx, mut h_rx) = mpsc::channel(100);
    let (g_from_tx, g_from_rx) = mpsc::channel(100);
    let (g_tx, mut g_rx) = mpsc::channel(100);
    let (h_from_tx, h_from_rx) = mpsc::channel(100);

    let host_handle = UserspaceDatapath::start_host_proxy(server_port, h_tx, h_from_rx)
        .await
        .unwrap();
    let (guest_port, guest_handle) =
        UserspaceDatapath::start_guest_proxy(0, g_tx, g_from_rx).await.unwrap();

    let b1 = tokio::spawn(async move {
        while let Some(p) = g_rx.recv().await {
            if h_from_tx.send(p).await.is_err() {
                break;
            }
        }
    });
    let b2 = tokio::spawn(async move {
        while let Some(p) = h_rx.recv().await {
            if g_from_tx.send(p).await.is_err() {
                break;
            }
        }
    });

    let mut client = TcpStream::connect(format!("127.0.0.1:{}", guest_port)).await.unwrap();
    client.write_all(b"PingBeforeFin").await.unwrap();
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Gracefully shutdown client write half
    client.shutdown().await.unwrap();

    server_task.await.unwrap();
    host_handle.stop();
    guest_handle.stop();
    b1.abort();
    b2.abort();
}

#[tokio::test]
async fn test_successive_connections_reuse() {
    let server_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let server_port = server_listener.local_addr().unwrap().port();

    let server_task = tokio::spawn(async move {
        for expected_id in 0..3u8 {
            let (mut socket, _) = server_listener.accept().await.unwrap();
            let mut byte = [0u8; 1];
            socket.read_exact(&mut byte).await.unwrap();
            assert_eq!(byte[0], expected_id);
            socket.write_all(&[byte[0] + 100]).await.unwrap();
        }
    });

    let (h_tx, mut h_rx) = mpsc::channel(100);
    let (g_from_tx, g_from_rx) = mpsc::channel(100);
    let (g_tx, mut g_rx) = mpsc::channel(100);
    let (h_from_tx, h_from_rx) = mpsc::channel(100);

    let host_handle = UserspaceDatapath::start_host_proxy(server_port, h_tx, h_from_rx)
        .await
        .unwrap();
    let (guest_port, guest_handle) =
        UserspaceDatapath::start_guest_proxy(0, g_tx, g_from_rx).await.unwrap();

    let b1 = tokio::spawn(async move {
        while let Some(p) = g_rx.recv().await {
            if h_from_tx.send(p).await.is_err() {
                break;
            }
        }
    });
    let b2 = tokio::spawn(async move {
        while let Some(p) = h_rx.recv().await {
            if g_from_tx.send(p).await.is_err() {
                break;
            }
        }
    });

    for id in 0..3u8 {
        let mut client = TcpStream::connect(format!("127.0.0.1:{}", guest_port)).await.unwrap();
        client.write_all(&[id]).await.unwrap();
        let mut resp = [0u8; 1];
        client.read_exact(&mut resp).await.unwrap();
        assert_eq!(resp[0], id + 100);
        client.shutdown().await.unwrap();
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    server_task.await.unwrap();
    host_handle.stop();
    guest_handle.stop();
    b1.abort();
    b2.abort();
}

// =========================================================================
// 6. END-TO-END INTEGRATION WITH BORINGTUN WIREGUARD ENGINE
// =========================================================================

#[tokio::test]
async fn test_datapath_over_boringtun_wireguard_tunnel() {
    let host_ident = identity::IdentityManager::generate(Some("Host".to_string()));
    let guest_ident = identity::IdentityManager::generate(Some("Guest".to_string()));

    let mut host_wg = wireguard::WireGuardTunnel::new_host(
        host_ident.private_key_bytes(),
        guest_ident.public_key_bytes(),
        None,
    )
    .unwrap();

    let mut guest_wg = wireguard::WireGuardTunnel::new_guest(
        guest_ident.private_key_bytes(),
        host_ident.public_key_bytes(),
        None,
    )
    .unwrap();

    // 1. Establish Noise_IKpsk2 handshake
    let mut init_buf = vec![0u8; wireguard::MIN_BUFFER_SIZE];
    let init_len = guest_wg.format_handshake_initiation(&mut init_buf).unwrap();

    let mut resp_buf = vec![0u8; wireguard::MIN_BUFFER_SIZE];
    let host_resp = host_wg.decapsulate(None, &init_buf[..init_len], &mut resp_buf).unwrap();
    let resp_len = match host_resp {
        wireguard::DecapsulatedPacket::WriteToNetwork(d) => d.len(),
        _ => panic!("Expected WriteToNetwork"),
    };

    let mut conf_buf = vec![0u8; wireguard::MIN_BUFFER_SIZE];
    let guest_conf = guest_wg.decapsulate(None, &resp_buf[..resp_len], &mut conf_buf).unwrap();
    let conf_len = match guest_conf {
        wireguard::DecapsulatedPacket::WriteToNetwork(d) => d.len(),
        _ => panic!("Expected WriteToNetwork"),
    };

    let mut scratch = vec![0u8; wireguard::MIN_BUFFER_SIZE];
    let done = host_wg.decapsulate(None, &conf_buf[..conf_len], &mut scratch).unwrap();
    assert!(matches!(done, wireguard::DecapsulatedPacket::Done));

    // 2. Start mock server
    let server_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let server_port = server_listener.local_addr().unwrap().port();

    let server_task = tokio::spawn(async move {
        let (mut socket, _) = server_listener.accept().await.unwrap();
        let mut buf = [0u8; 128];
        let n = socket.read(&mut buf).await.unwrap();
        assert_eq!(&buf[..n], b"PingOverWireGuardDatapath");
        socket.write_all(b"PongOverWireGuardDatapath").await.unwrap();
    });

    // 3. Channels and proxies
    let (h_tx, mut h_rx) = mpsc::channel(100);
    let (g_from_tx, g_from_rx) = mpsc::channel(100);
    let (g_tx, mut g_rx) = mpsc::channel(100);
    let (h_from_tx, h_from_rx) = mpsc::channel(100);

    let host_handle = UserspaceDatapath::start_host_proxy(server_port, h_tx, h_from_rx)
        .await
        .unwrap();
    let (guest_port, guest_handle) =
        UserspaceDatapath::start_guest_proxy(0, g_tx, g_from_rx).await.unwrap();

    // 4. Encapsulation/decapsulation bridge simulating UDP network
    let host_wg_arc = std::sync::Arc::new(tokio::sync::Mutex::new(host_wg));
    let guest_wg_arc = std::sync::Arc::new(tokio::sync::Mutex::new(guest_wg));

    let hwg1 = host_wg_arc.clone();
    let gwg1 = guest_wg_arc.clone();

    // Guest -> Host bridge: Guest enc, Host dec
    let b_g2h = tokio::spawn(async move {
        let mut enc_buf = vec![0u8; wireguard::MIN_BUFFER_SIZE];
        let mut dec_buf = vec![0u8; wireguard::MIN_BUFFER_SIZE];

        while let Some(ip_packet) = g_rx.recv().await {
            let enc_len = gwg1.lock().await.encapsulate(&ip_packet, &mut enc_buf).unwrap();
            let dec_res = hwg1.lock().await.decapsulate(None, &enc_buf[..enc_len], &mut dec_buf).unwrap();

            match dec_res {
                wireguard::DecapsulatedPacket::PacketV4 { packet, .. } => {
                    if h_from_tx.send(packet.to_vec()).await.is_err() {
                        break;
                    }
                }
                _ => panic!("Expected PacketV4"),
            }
        }
    });

    let hwg2 = host_wg_arc.clone();
    let gwg2 = guest_wg_arc.clone();

    // Host -> Guest bridge: Host enc, Guest dec
    let b_h2g = tokio::spawn(async move {
        let mut enc_buf = vec![0u8; wireguard::MIN_BUFFER_SIZE];
        let mut dec_buf = vec![0u8; wireguard::MIN_BUFFER_SIZE];

        while let Some(ip_packet) = h_rx.recv().await {
            let enc_len = hwg2.lock().await.encapsulate(&ip_packet, &mut enc_buf).unwrap();
            let dec_res = gwg2.lock().await.decapsulate(None, &enc_buf[..enc_len], &mut dec_buf).unwrap();

            match dec_res {
                wireguard::DecapsulatedPacket::PacketV4 { packet, .. } => {
                    if g_from_tx.send(packet.to_vec()).await.is_err() {
                        break;
                    }
                }
                _ => panic!("Expected PacketV4"),
            }
        }
    });

    // 5. Client connects through guest proxy
    let mut client = TcpStream::connect(format!("127.0.0.1:{}", guest_port)).await.unwrap();
    client.write_all(b"PingOverWireGuardDatapath").await.unwrap();

    let mut resp = [0u8; 128];
    let n = client.read(&mut resp).await.unwrap();
    assert_eq!(&resp[..n], b"PongOverWireGuardDatapath");

    client.shutdown().await.unwrap();
    server_task.await.unwrap();
    host_handle.stop();
    guest_handle.stop();
    b_g2h.abort();
    b_h2g.abort();
}
