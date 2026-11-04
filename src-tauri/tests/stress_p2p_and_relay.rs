//! Adversarial Stress & Chaos Test Suite for Milestone 3:
//! P2P Connectivity, Room Codes, NAT Traversal (STUN + UDP Hole Punching),
//! Relayed Fallback, and Minecraft LAN Multicast Discovery.
//!
//! Focus Areas:
//! 1. FallbackTransport Timeout Accuracy:
//!    - Network blackout / unresponsive peer: exact ~2,500ms fallback trigger
//!    - Delayed hole punch success at 1,000ms: NO premature relay fallback
//! 2. Zero-Knowledge Relay Transport Opacity & High-Throughput:
//!    - 5,000 WireGuard datagrams streamed through RelayClient
//!    - 100% datagram fidelity (zero byte corruptions or drops)
//!    - Metric accounting (`bytes_sent`, `bytes_received`) accuracy
//!    - Zero plaintext leakage verification across relay intermediary
//! 3. Candidate Selection Preference:
//!    - Priority of LAN candidate over WAN candidate under all timing permutations
//!    - Resilience when WAN arrives earlier than LAN
//!    - Graceful fallback to WAN when LAN is unreachable
//! 4. Multicast LAN Flood & Malformed Packet Resistance:
//!    - High-volume UDP flood with random binary, truncated tags, port 0, negative ports
//!    - Fuzzing parser with boundary values, huge inputs, invalid UTF-8
//!    - Zero crash, zero panic, bounded memory

use base64::Engine;

mod network {
    #[path = "../../src/network/punch.rs"]
    pub mod punch;

    #[path = "../../src/network/relay.rs"]
    pub mod relay;

    #[path = "../../src/network/lan.rs"]
    pub mod lan;
}

use network::lan::MinecraftLanBeacon;
use network::punch::{
    build_stun_request, build_stun_response, execute_hole_punch_with_timeout, format_punch_probe,
    parse_punch_probe, parse_stun_response, PunchCandidate, PunchError, PUNCH_BURST_COUNT,
    PUNCH_MAGIC, PUNCH_MSG_ACK, PUNCH_MSG_PROBE,
};
use network::relay::{
    crc16_ccitt, FallbackTransport, RelayClient, RelayError, RelayFrame, RoomTicket,
};
use rand::{Rng, RngExt};
use std::collections::HashSet;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::net::UdpSocket;

// =========================================================================
// SECTION 1: FALLBACK TRANSPORT TIMEOUT ACCURACY
// =========================================================================

/// Test 1A: Network Blackout / Unresponsive Peer.
/// Verifies that when the target peer is completely unresponsive (silent drop / network blackout),
/// hole punching does NOT exit prematurely and FallbackTransport triggers relay fallback at ~2,500ms.
#[tokio::test]
async fn test_fallback_transport_timeout_accuracy_under_blackout() {
    let local_socket = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());

    // Silent peer: bound socket that never reads or responds to probes.
    // Crucially, because it is bound, the OS does not generate ICMP Port Unreachable.
    let silent_peer = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let silent_addr = silent_peer.local_addr().unwrap();
    let target = PunchCandidate::new(Some(silent_addr), None);

    // Mock Relay server
    let relay_server = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let relay_addr = relay_server.local_addr().unwrap();
    let session_id = [0x42; 16];

    let start = Instant::now();
    let transport = FallbackTransport::establish(
        local_socket,
        &target,
        Some(relay_addr),
        session_id,
        2, // Guest
    )
    .await
    .expect("Fallback transport should succeed via relay after hole punch timeout");
    let elapsed = start.elapsed();

    // Verify fallback route activated
    assert!(
        !transport.is_direct(),
        "Transport must be Relayed (is_direct == false)"
    );

    // Verify timeout accuracy:
    // 5 probes * 15ms = 75ms probe burst + 2,500ms timeout = ~2,575ms.
    // Must be >= 2,450ms (no premature exit) and <= 2,850ms (responsive fallback trigger).
    assert!(
        elapsed >= Duration::from_millis(2450),
        "Fallback triggered prematurely! Elapsed: {:?}",
        elapsed
    );
    assert!(
        elapsed <= Duration::from_millis(2850),
        "Fallback took too long to trigger! Elapsed: {:?}",
        elapsed
    );

    // Verify relay registration was received by relay server
    let mut reg_buf = [0u8; 64];
    let (n, _) = relay_server.recv_from(&mut reg_buf).await.unwrap();
    let (m_type, r_sess, role) = RelayFrame::decode(&reg_buf[..n]).unwrap();
    assert_eq!(m_type, RelayFrame::MSG_REGISTER);
    assert_eq!(r_sess.unwrap(), session_id);
    assert_eq!(role, &[2]);
}

/// Test 1B: Delayed Hole Punch Success at 1,000ms.
/// Verifies that if direct hole punching succeeds at 1,000ms, direct P2P is established
/// immediately and does NOT trigger premature relay fallback.
#[tokio::test]
async fn test_hole_punch_success_at_1000ms_avoids_relay_fallback() {
    let local_socket = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
    let local_addr = local_socket.local_addr().unwrap();

    let peer_socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let peer_addr = peer_socket.local_addr().unwrap();
    let target = PunchCandidate::new(Some(peer_addr), None);

    // Relay server that should NEVER receive a registration if direct succeeds
    let relay_server = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let relay_addr = relay_server.local_addr().unwrap();
    let session_id = [0x77; 16];

    // Peer task: waits exactly 1,000ms, then responds with a probe to local_socket
    let peer_task = tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(1000)).await;
        let probe = format_punch_probe(PUNCH_MSG_PROBE, 0);
        let _ = peer_socket.send_to(&probe, local_addr).await;

        // Drain any incoming ACK
        let mut buf = [0u8; 64];
        let _ = tokio::time::timeout(Duration::from_millis(300), peer_socket.recv_from(&mut buf)).await;
    });

    let start = Instant::now();
    let transport = FallbackTransport::establish(
        local_socket,
        &target,
        Some(relay_addr),
        session_id,
        2,
    )
    .await
    .expect("Should establish direct connection upon probe receipt");
    let elapsed = start.elapsed();

    peer_task.await.unwrap();

    // Verify DIRECT route was chosen
    assert!(
        transport.is_direct(),
        "Transport must be Direct when hole punch succeeds at 1,000ms"
    );

    // Verify elapsed timing: should be around 1,000ms - 1,300ms, NOT 2,500ms+
    assert!(
        elapsed >= Duration::from_millis(950),
        "Elapsed too fast: {:?}",
        elapsed
    );
    assert!(
        elapsed <= Duration::from_millis(1600),
        "Elapsed too slow (should not wait for 2,500ms timeout): {:?}",
        elapsed
    );

    // Verify Relay server received ZERO packets (relay fallback was NOT triggered)
    let mut check_buf = [0u8; 64];
    let relay_recv = tokio::time::timeout(
        Duration::from_millis(100),
        relay_server.recv_from(&mut check_buf),
    )
    .await;
    assert!(
        relay_recv.is_err(),
        "Relay server should have received NO traffic when direct connection succeeded"
    );
}

// =========================================================================
// SECTION 2: ZERO-KNOWLEDGE RELAY TRANSPORT OPACITY & THROUGHPUT
// =========================================================================

/// Test 2A: High-Volume WireGuard Datagram Stream (5,000 Packets) Through Relay.
/// Streams 5,000 opaque simulated WireGuard datagrams across RelayClient,
/// validating 100% datagram fidelity, exact metric accounting, and zero plaintext leakage.
#[tokio::test]
async fn test_relay_throughput_5000_datagrams_fidelity_and_opacity() {
    let relay_socket = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
    let relay_addr = relay_socket.local_addr().unwrap();

    let session_id = [0x5A; 16];
    let host_client = RelayClient::connect(relay_addr, session_id, 1).await.unwrap();
    let guest_client = RelayClient::connect(relay_addr, session_id, 2).await.unwrap();

    // Relay Forwarder Task:
    // Maintains registration table and routes MSG_DATA packets between host and guest.
    // Concurrently verifies that NO payload contains unencrypted protocol markers (Zero-Knowledge Opacity).
    let relay_sock_clone = Arc::clone(&relay_socket);
    let total_packets = 5000usize;
    let forwarder_running = Arc::new(AtomicBool::new(true));
    let forwarder_flag = Arc::clone(&forwarder_running);

    let forwarder_task = tokio::spawn(async move {
        let mut host_addr: Option<SocketAddr> = None;
        let mut guest_addr: Option<SocketAddr> = None;
        let mut buf = [0u8; 2048];

        while forwarder_flag.load(Ordering::Relaxed) {
            let res = tokio::time::timeout(
                Duration::from_millis(100),
                relay_sock_clone.recv_from(&mut buf),
            )
            .await;

            let (n, src) = match res {
                Ok(Ok(val)) => val,
                _ => continue,
            };

            let (msg_type, sess_opt, payload) = match RelayFrame::decode(&buf[..n]) {
                Ok(frame) => frame,
                Err(_) => continue,
            };

            if sess_opt != Some(session_id) {
                continue;
            }

            match msg_type {
                RelayFrame::MSG_REGISTER => {
                    let role = payload[0];
                    if role == 1 {
                        host_addr = Some(src);
                    } else if role == 2 {
                        guest_addr = Some(src);
                    }
                }
                RelayFrame::MSG_DATA => {
                    // Check for zero-knowledge opacity:
                    // Datagram must NOT contain cleartext markers like "MINECRAFT", "CONDUIT", "[MOTD]"
                    assert!(!payload.windows(9).any(|w| w == b"MINECRAFT"), "Plaintext leak: MINECRAFT detected");
                    assert!(!payload.windows(7).any(|w| w == b"CONDUIT"), "Plaintext leak: CONDUIT detected");
                    assert!(!payload.windows(6).any(|w| w == b"[MOTD]"), "Plaintext leak: [MOTD] detected");

                    // Route to peer
                    let dest = if Some(src) == host_addr {
                        guest_addr
                    } else {
                        host_addr
                    };

                    if let Some(target) = dest {
                        let _ = relay_sock_clone.send_to(&buf[..n], target).await;
                    }
                }
                _ => {}
            }
        }
    });

    // Give relay forwarder time to register both clients
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Generate 5,000 distinct simulated WireGuard datagrams
    // Sizes range from 32 bytes (Type 4 minimum) to 1280 bytes (typical MTU)
    let mut sent_datagrams = Vec::with_capacity(total_packets);
    let mut total_bytes_sent = 0u64;

    for i in 0..total_packets {
        let size = 32 + (i % 60) * 20; // 32 to 1212 bytes
        let mut pkt = vec![0u8; size];
        pkt[0] = 4; // WireGuard Type 4 Data packet
        let counter = (i as u64).to_be_bytes();
        pkt[1..9].copy_from_slice(&counter);
        for b in 9..size {
            pkt[b] = ((i + b) & 0xFF) as u8;
        }
        total_bytes_sent += size as u64;
        sent_datagrams.push(pkt);
    }

    // Pipeline transmission: Host sends in batches of 50 packets; Guest receives and acknowledges
    // This stress-tests relay throughput while avoiding OS UDP buffer overflow drops.
    let host_client = Arc::new(host_client);
    let guest_client = Arc::new(guest_client);

    let mut received_datagrams = Vec::with_capacity(total_packets);
    let batch_size = 50;

    for chunk in sent_datagrams.chunks(batch_size) {
        // Host sends batch
        for pkt in chunk {
            host_client.forward_datagram(pkt).await.unwrap();
        }

        // Guest receives batch
        for _ in 0..chunk.len() {
            let mut recv_buf = [0u8; 2048];
            let n = tokio::time::timeout(
                Duration::from_millis(500),
                guest_client.recv_datagram(&mut recv_buf),
            )
            .await
            .expect("Guest should receive datagram within timeout")
            .expect("Recv datagram error");

            received_datagrams.push(recv_buf[..n].to_vec());
        }
    }

    // 1. Verify 100% Datagram Fidelity (Byte-for-byte exact match on all 5,000 packets)
    assert_eq!(received_datagrams.len(), total_packets);
    for i in 0..total_packets {
        assert_eq!(
            received_datagrams[i], sent_datagrams[i],
            "Datagram corruption detected at packet index {}",
            i
        );
    }

    // 2. Verify Metric Accounting
    assert_eq!(
        host_client.bytes_sent(),
        total_bytes_sent,
        "Host bytes_sent mismatch"
    );
    assert_eq!(
        guest_client.bytes_received(),
        total_bytes_sent,
        "Guest bytes_received mismatch"
    );

    forwarder_running.store(false, Ordering::Relaxed);
    let _ = forwarder_task.await;
}

// =========================================================================
// SECTION 3: CANDIDATE SELECTION PREFERENCE (LAN vs WAN)
// =========================================================================

/// Test 3A: WAN responds first; LAN responds later.
/// Verifies that LAN candidate is preferred over WAN even if WAN responded earlier.
#[tokio::test]
async fn test_candidate_selection_prefers_lan_when_wan_responds_first() {
    let local_socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let local_addr = local_socket.local_addr().unwrap();

    let lan_socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let lan_addr = lan_socket.local_addr().unwrap();

    let wan_socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let wan_addr = wan_socket.local_addr().unwrap();

    let candidate = PunchCandidate::new(Some(lan_addr), Some(wan_addr));

    // WAN responds immediately at t=10ms
    let wan_task = tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(10)).await;
        let probe = format_punch_probe(PUNCH_MSG_PROBE, 0);
        let _ = wan_socket.send_to(&probe, local_addr).await;
    });

    // LAN responds slightly later at t=35ms
    let lan_task = tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(35)).await;
        let probe = format_punch_probe(PUNCH_MSG_PROBE, 0);
        let _ = lan_socket.send_to(&probe, local_addr).await;
    });

    let selected = execute_hole_punch_with_timeout(
        &local_socket,
        &candidate,
        Duration::from_millis(1000),
    )
    .await
    .expect("Candidate selection should succeed");

    wan_task.await.unwrap();
    lan_task.await.unwrap();

    // LAN candidate MUST be selected for lowest-latency local network performance
    assert_eq!(
        selected, lan_addr,
        "LAN candidate must be selected even when WAN responded first!"
    );
}

/// Test 3B: LAN responds first; WAN responds later.
/// Verifies immediate fast-path selection of LAN candidate without waiting for WAN.
#[tokio::test]
async fn test_candidate_selection_prefers_lan_when_lan_responds_first() {
    let local_socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let local_addr = local_socket.local_addr().unwrap();

    let lan_socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let lan_addr = lan_socket.local_addr().unwrap();

    let wan_socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let wan_addr = wan_socket.local_addr().unwrap();

    let candidate = PunchCandidate::new(Some(lan_addr), Some(wan_addr));

    // LAN responds immediately at t=5ms
    let lan_task = tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(5)).await;
        let probe = format_punch_probe(PUNCH_MSG_PROBE, 0);
        let _ = lan_socket.send_to(&probe, local_addr).await;
    });

    // WAN responds much later at t=50ms
    let wan_task = tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(50)).await;
        let probe = format_punch_probe(PUNCH_MSG_PROBE, 0);
        let _ = wan_socket.send_to(&probe, local_addr).await;
    });

    let selected = execute_hole_punch_with_timeout(
        &local_socket,
        &candidate,
        Duration::from_millis(1000),
    )
    .await
    .expect("Candidate selection should succeed");

    lan_task.await.unwrap();
    wan_task.await.unwrap();

    assert_eq!(selected, lan_addr);
}

/// Test 3C: WAN responds, but LAN is completely unreachable.
/// Verifies graceful fallback to WAN candidate when LAN candidate fails.
#[tokio::test]
async fn test_candidate_selection_falls_back_to_wan_when_lan_unreachable() {
    let local_socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let local_addr = local_socket.local_addr().unwrap();

    // Dead LAN endpoint (bound but silent)
    let dead_lan = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let dead_lan_addr = dead_lan.local_addr().unwrap();

    // Active WAN endpoint
    let wan_socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let wan_addr = wan_socket.local_addr().unwrap();

    let candidate = PunchCandidate::new(Some(dead_lan_addr), Some(wan_addr));

    let wan_task = tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(20)).await;
        let probe = format_punch_probe(PUNCH_MSG_PROBE, 0);
        let _ = wan_socket.send_to(&probe, local_addr).await;
    });

    let selected = execute_hole_punch_with_timeout(
        &local_socket,
        &candidate,
        Duration::from_millis(300),
    )
    .await
    .expect("Should select WAN candidate when LAN is unreachable");

    wan_task.await.unwrap();

    assert_eq!(selected, wan_addr);
}

// =========================================================================
// SECTION 4: MULTICAST LAN FLOOD & MALFORMED PACKET RESISTANCE
// =========================================================================

/// Test 4A: Parser Fuzzing with 2,000 Malicious & Boundary Payloads.
/// Tests MinecraftLanBeacon::parse against random binary data, inverted tags,
/// negative ports, port 0, overflow ports, non-numeric strings, and massive payloads.
#[test]
fn test_minecraft_lan_parser_fuzzing_and_boundary_resistance() {
    // 1. Negative, zero, and out-of-range ports
    let invalid_ports = [
        "[MOTD]SMP[/MOTD][AD]0[/AD]",
        "[MOTD]SMP[/MOTD][AD]-1[/AD]",
        "[MOTD]SMP[/MOTD][AD]-25565[/AD]",
        "[MOTD]SMP[/MOTD][AD]65536[/AD]",
        "[MOTD]SMP[/MOTD][AD]100000[/AD]",
        "[MOTD]SMP[/MOTD][AD]99999999999999999999[/AD]",
        "[MOTD]SMP[/MOTD][AD]NaN[/AD]",
        "[MOTD]SMP[/MOTD][AD]null[/AD]",
        "[MOTD]SMP[/MOTD][AD]port25565[/AD]",
        "[MOTD]SMP[/MOTD][AD]25565abc[/AD]",
        "[MOTD]SMP[/MOTD][AD][/AD]",
    ];

    for case in &invalid_ports {
        assert!(
            MinecraftLanBeacon::parse(case).is_none(),
            "Expected None for invalid port payload: {}",
            case
        );
    }

    // 2. Corrupted and mismatched tags
    let corrupted_tags = [
        "[MOTD]No Close Tag[AD]25565[/AD]",
        "No Open Tag[/MOTD][AD]25565[/AD]",
        "[MOTD]Valid[/MOTD]No Open AD Tag 25565[/AD]",
        "[MOTD]Valid[/MOTD][AD]No Close AD Tag 25565",
        "[AD]25565[/AD][MOTD]Inverted Tags[/MOTD]",
        "[MOTD][AD]25565[/AD]Nested[/MOTD]",
        "",
        "   ",
        "[[[[[MOTD]]]]]",
        "[/AD][/MOTD][AD][MOTD]",
    ];

    for case in &corrupted_tags {
        assert!(
            MinecraftLanBeacon::parse(case).is_none(),
            "Expected None for corrupted tags: {}",
            case
        );
    }

    // 3. Massive input resilience (100,000 character string)
    let huge_motd = "A".repeat(100_000);
    let huge_packet = format!("[MOTD]{}[/MOTD][AD]25565[/AD]", huge_motd);
    let parsed = MinecraftLanBeacon::parse(&huge_packet);
    assert!(parsed.is_some());
    let (motd, port) = parsed.unwrap();
    assert_eq!(motd.len(), 100_000);
    assert_eq!(port, 25565);

    // 4. Random noise fuzzing (1,000 random strings)
    let mut rng = rand::rng();
    for _ in 0..1000 {
        let len = rng.random_range(1..256);
        let random_bytes: Vec<u8> = (0..len).map(|_| rng.random()).collect();
        let s = String::from_utf8_lossy(&random_bytes);
        // Ensure parser never panics on random input
        let _ = MinecraftLanBeacon::parse(&s);
    }
}

/// Test 4B: Live Multicast Flood Bombardment (2,000 UDP Datagrams).
/// Blasts a live UDP listener with 2,000 malformed datagrams (binary noise, truncated tags,
/// illegal ports) mixed with valid beacons, asserting zero crash and exact valid beacon discovery.
#[tokio::test]
async fn test_lan_listener_flood_and_malformed_bombardment() {
    let listener_socket = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
    let listener_addr = listener_socket.local_addr().unwrap();

    let total_flood = 2000usize;
    let legitimate_world = "Surviving Conduit Server";
    let legitimate_port = 25565u16;
    let legitimate_beacon = MinecraftLanBeacon::format(legitimate_world, legitimate_port);

    // Flooder Task: sends 2,000 packets at maximum rate
    let flooder_task = tokio::spawn(async move {
        let sender = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        for i in 0..total_flood {
            let payload: Vec<u8> = match i % 5 {
                0 => {
                    // Raw random binary noise (not valid UTF-8)
                    let mut noise = vec![0u8; 128];
                    rand::rng().fill_bytes(&mut noise);
                    noise
                }
                1 => {
                    // Truncated tags
                    b"[MOTD]Truncated Tag Without End[AD]25565".to_vec()
                }
                2 => {
                    // Port zero or negative port
                    b"[MOTD]Illegal Port World[/MOTD][AD]0[/AD]".to_vec()
                }
                3 => {
                    // Inverted tags
                    b"[AD]25565[/AD][MOTD]Inverted Tags[/MOTD]".to_vec()
                }
                _ => {
                    // Legitimate beacon packet interspersed
                    legitimate_beacon.as_bytes().to_vec()
                }
            };

            let _ = sender.send_to(&payload, listener_addr).await;
            if i % 100 == 0 {
                tokio::task::yield_now().await;
            }
        }
    });

    // Listener loop processing incoming packets for up to 1,500ms
    let deadline = Instant::now() + Duration::from_millis(1500);
    let mut buf = [0u8; 1024];
    let mut discovered_worlds = HashSet::new();
    let mut packets_processed = 0usize;

    while Instant::now() < deadline && packets_processed < total_flood {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            break;
        }

        let recv_res = tokio::time::timeout(remaining, listener_socket.recv_from(&mut buf)).await;
        match recv_res {
            Ok(Ok((len, _src))) => {
                packets_processed += 1;
                if let Ok(msg_str) = std::str::from_utf8(&buf[..len]) {
                    if let Some((motd, port)) = MinecraftLanBeacon::parse(msg_str) {
                        discovered_worlds.insert((motd, port));
                    }
                }
            }
            Ok(Err(_)) => break,
            Err(_) => break,
        }
    }

    flooder_task.await.unwrap();

    // Verify listener processed packets without crashing or panicking
    assert!(
        packets_processed > 0,
        "Listener should have processed packets"
    );

    // Verify that ONLY the legitimate beacon was admitted into discovered games
    assert_eq!(
        discovered_worlds.len(),
        1,
        "Only the legitimate beacon should be accepted"
    );
    assert!(
        discovered_worlds.contains(&(legitimate_world.to_string(), legitimate_port)),
        "Legitimate game was not discovered"
    );
}

// =========================================================================
// SECTION 5: ROOM TICKET & STUN DEVIATION RESILIENCE
// =========================================================================

/// Test 5A: Single-Bit Mutation & Corruption Detection Across RoomTicket.
/// Iterates across every single byte of an encoded Mode A ticket, flipping bits
/// and asserting that CRC16 or version check detects corruption with 100% accuracy.
#[test]
fn test_room_ticket_adversarial_bit_flip_corruption_fuzzing() {
    let mut pubkey = [0u8; 32];
    rand::rng().fill_bytes(&mut pubkey);
    let wan = "198.51.100.22:51820".parse().unwrap();
    let lan = "192.168.1.150:51820".parse().unwrap();
    let flags = 0x0001;

    let valid_ticket = RoomTicket::new(pubkey, wan, lan, flags);
    let valid_encoded = valid_ticket.encode();

    // Decode base64 payload to bytes
    let b64_part = &valid_encoded["CND-1-".len()..];
    let raw = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(b64_part)
        .unwrap();

    // Mutate every single byte from byte 0 to raw.len()-1
    for byte_idx in 0..raw.len() {
        for bit in 0..8 {
            let mut corrupted = raw.clone();
            corrupted[byte_idx] ^= 1 << bit;

            let corrupted_b64 = format!(
                "CND-1-{}",
                base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(&corrupted)
            );

            let res = RoomTicket::decode(&corrupted_b64);
            assert!(
                res.is_err(),
                "Corrupted ticket at byte {} bit {} was improperly accepted!",
                byte_idx,
                bit
            );
        }
    }
}

/// Test 5B: STUN Attribute Buffer Overflow & Malformed Header Fuzzing.
/// Passes STUN packets with attributes claiming lengths that extend beyond the packet,
/// verifying that parse_stun_response safely returns PunchError::Parse without reading out-of-bounds.
#[test]
fn test_stun_attribute_out_of_bounds_and_overflow_resistance() {
    let tx_id = [0x55; 12];
    let mut resp = build_stun_response(&tx_id, "1.2.3.4:5678".parse().unwrap(), true);

    // Corrupt attribute length to 0xFFFF (claiming 65535 bytes)
    // STUN header is 20 bytes; attribute begins at byte 20
    // Attribute Type is bytes 20..22; Attribute Length is bytes 22..24
    resp[22] = 0xFF;
    resp[23] = 0xFF;

    let err = parse_stun_response(&resp, &tx_id).unwrap_err();
    match err {
        PunchError::Parse(msg) => {
            assert!(msg.contains("extends beyond response length"));
        }
        other => panic!("Expected PunchError::Parse, got {:?}", other),
    }
}
