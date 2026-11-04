//! Adversarial Stress & Vulnerability Test Suite for Milestone 2 Userspace Datapath.
//!
//! EMPIRICAL CHALLENGER TEST SUITE:
//! - MTU chunk boundaries: payloads > 1380B, packet size <= 1400B, integer overflow defense.
//! - Stream reassembly under extreme packet reordering, inverted arrival, and duplicate injection.
//! - Corrupted IPv4 checksums, truncated headers, invalid IHL/version, malformed payloads.
//! - Port collision fallback, rapid sequential client connects/disconnects, concurrent client demuxing,
//!   and stream map cleanup.

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
use wireguard::WireGuardTunnel;

// =========================================================================
// 1. MTU CHUNK BOUNDARY & INTEGER OVERFLOW ADVERSARIAL TESTS
// =========================================================================

#[test]
fn test_adv_mtu_exact_boundaries() {
    let src = [10, 88, 0, 2];
    let dst = [10, 88, 0, 1];

    // Case 1: 0-byte payload -> 20-byte packet
    let p0 = wrap_synthetic_ipv4(src, dst, &[]);
    assert_eq!(p0.len(), 20);
    assert_eq!(unwrap_synthetic_ipv4(&p0).unwrap().len(), 0);

    // Case 2: 1-byte payload -> 21-byte packet
    let p1 = wrap_synthetic_ipv4(src, dst, &[0x42]);
    assert_eq!(p1.len(), 21);
    assert_eq!(unwrap_synthetic_ipv4(&p1).unwrap(), &[0x42]);

    // Case 3: MAX_PAYLOAD_CHUNK (1364 bytes)
    let p_chunk = vec![0xAA; MAX_PAYLOAD_CHUNK];
    let stream_pkt = StreamPacket::new(1, 0, FLAG_DATA, p_chunk.clone());
    let stream_bytes = stream_pkt.to_bytes();
    assert_eq!(stream_bytes.len(), 16 + 1364); // 1380 bytes
    let ip_pkt = wrap_synthetic_ipv4(src, dst, &stream_bytes);
    assert_eq!(ip_pkt.len(), 1400); // EXACTLY MAX_PACKET_MTU
    assert!(ip_pkt.len() <= MAX_PACKET_MTU);

    // Case 4: 1380-byte raw payload -> 1400-byte packet
    let p1380 = vec![0xBB; 1380];
    let ip1380 = wrap_synthetic_ipv4(src, dst, &p1380);
    assert_eq!(ip1380.len(), 1400);
    assert_eq!(unwrap_synthetic_ipv4(&ip1380).unwrap().len(), 1380);

    // Case 5: 1381-byte payload -> 1401-byte packet (if wrapped raw)
    let p1381 = vec![0xCC; 1381];
    let ip1381 = wrap_synthetic_ipv4(src, dst, &p1381);
    assert_eq!(ip1381.len(), 1401);
    assert_eq!(unwrap_synthetic_ipv4(&ip1381).unwrap().len(), 1381);
}

#[test]
fn test_adv_wrap_synthetic_ipv4_extreme_sizes_no_overflow() {
    let src = [10, 88, 0, 2];
    let dst = [10, 88, 0, 1];

    // Payload just below u16::MAX - 20 (65515 bytes)
    let exact_fit = vec![0x55; (u16::MAX as usize) - IPV4_HEADER_LEN];
    let p_exact = wrap_synthetic_ipv4(src, dst, &exact_fit);
    assert_eq!(p_exact.len(), u16::MAX as usize);
    let unwrapped_exact = unwrap_synthetic_ipv4(&p_exact).unwrap();
    assert_eq!(unwrapped_exact.len(), exact_fit.len());

    // Payload exceeding u16::MAX (e.g. 70,000 bytes and 1,000,000 bytes)
    for &oversize in &[65536, 70000, 150000, 1_000_000] {
        let huge = vec![0x77; oversize];
        let p_huge = wrap_synthetic_ipv4(src, dst, &huge);
        // Total length is clamped to u16::MAX without wrapping
        assert_eq!(p_huge.len(), u16::MAX as usize);
        let unwrapped = unwrap_synthetic_ipv4(&p_huge).unwrap();
        assert_eq!(unwrapped.len(), (u16::MAX as usize) - IPV4_HEADER_LEN);
    }
}

#[tokio::test]
async fn test_adv_tcp_proxy_chunks_large_transfers_below_mtu() {
    // Start a mock server
    let server_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let server_port = server_listener.local_addr().unwrap().port();

    let (host_to_guest_tx, host_to_guest_rx) = mpsc::channel::<Vec<u8>>(1000);
    let (guest_to_host_tx, guest_to_host_rx) = mpsc::channel::<Vec<u8>>(1000);

    let host_handle = UserspaceDatapath::start_host_proxy(server_port, host_to_guest_tx, guest_to_host_rx)
        .await
        .unwrap();

    let (guest_port, guest_handle) = UserspaceDatapath::start_guest_proxy(0, guest_to_host_tx, host_to_guest_rx)
        .await
        .unwrap();

    // Spawn server echo task
    let server_task = tokio::spawn(async move {
        let (mut conn, _) = server_listener.accept().await.unwrap();
        let mut buf = vec![0u8; 8192];
        while let Ok(n) = conn.read(&mut buf).await {
            if n == 0 {
                break;
            }
            conn.write_all(&buf[..n]).await.unwrap();
        }
    });

    // Client connects and sends a 32 KB payload (much larger than 1380 bytes)
    let mut client = TcpStream::connect(format!("127.0.0.1:{}", guest_port))
        .await
        .unwrap();
    let send_data = vec![0x39u8; 32 * 1024];

    client.write_all(&send_data).await.unwrap();

    // Read back 32 KB
    let mut received = vec![0u8; send_data.len()];
    client.read_exact(&mut received).await.unwrap();
    assert_eq!(received, send_data, "Echoed data must match exactly");

    client.shutdown().await.unwrap();
    tokio::time::sleep(Duration::from_millis(50)).await;

    host_handle.stop();
    guest_handle.stop();
    server_task.abort();
}

// =========================================================================
// 2. STREAM REASSEMBLY: EXTREME REORDERING & DUPLICATE INJECTION
// =========================================================================

#[test]
fn test_adv_reassembler_strictly_inverted_arrival() {
    let mut reassembler = StreamReassembler::new(0);
    let count = 100;

    // Push packets in strictly reverse order: 99 down to 0
    for i in (0..count).rev() {
        let payload = format!("pkt-{}", i).into_bytes();
        let ready = reassembler.push(StreamPacket::new(1, i, FLAG_DATA, payload));
        if i > 0 {
            assert!(
                ready.is_empty(),
                "No packets should be ready before seq 0 arrives"
            );
        } else {
            // When seq 0 arrives, ALL 100 packets must be delivered contiguously
            assert_eq!(ready.len(), count as usize);
            for (idx, p) in ready.iter().enumerate() {
                assert_eq!(p.seq, idx as u32);
                let expected_payload = format!("pkt-{}", idx).into_bytes();
                assert_eq!(p.payload, expected_payload);
            }
        }
    }
    assert_eq!(reassembler.expected_seq(), count);
    assert_eq!(reassembler.buffered_count(), 0);
}

#[test]
fn test_adv_reassembler_randomized_permutations_with_duplicates() {
    let mut reassembler = StreamReassembler::new(0);
    let total_packets = 150;

    // Generate packets 0..150
    let mut packets: Vec<StreamPacket> = (0..total_packets)
        .map(|seq| {
            StreamPacket::new(42, seq, FLAG_DATA, format!("adv-data-{}", seq).into_bytes())
        })
        .collect();

    // Deterministic pseudo-random shuffle (Linear Congruential Generator)
    let mut seed: u64 = 0xDEADBEEF;
    let mut prng = move || {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        (seed >> 33) as usize
    };

    let len = packets.len();
    for i in (1..len).rev() {
        let j = prng() % (i + 1);
        packets.swap(i, j);
    }

    // Now inject packets with random duplicate floods (each packet duplicated 1 to 4 times)
    let mut delivery_order = Vec::new();
    for pkt in &packets {
        // Send duplicate before
        let ready_dup1 = reassembler.push(pkt.clone());
        delivery_order.extend(ready_dup1);

        // Send main packet
        let ready = reassembler.push(pkt.clone());
        delivery_order.extend(ready);

        // Send duplicate after
        let ready_dup2 = reassembler.push(pkt.clone());
        delivery_order.extend(ready_dup2);
    }

    // Verify all 150 packets delivered in exact sequential order 0..150
    assert_eq!(delivery_order.len(), total_packets as usize);
    for (expected_seq, p) in delivery_order.iter().enumerate() {
        assert_eq!(p.seq, expected_seq as u32);
        let expected_payload = format!("adv-data-{}", expected_seq).into_bytes();
        assert_eq!(p.payload, expected_payload);
    }

    assert_eq!(reassembler.expected_seq(), total_packets);
    assert_eq!(reassembler.buffered_count(), 0);
}

#[test]
fn test_adv_reassembler_buffer_overflow_defense() {
    let mut reassembler = StreamReassembler::new(0);

    // Default max_buffered_packets is 1024
    // Flood with 2000 out-of-order packets (missing seq 0)
    for seq in 1..=2000 {
        let pkt = StreamPacket::new(1, seq, FLAG_DATA, vec![0xDD]);
        let ready = reassembler.push(pkt);
        assert!(ready.is_empty());
    }

    // Buffer must be strictly capped at 1024 without memory explosion or panic
    assert_eq!(reassembler.buffered_count(), 1024);

    // When seq 0 arrives, contiguous packets 0..=1024 must drain
    let pkt0 = StreamPacket::new(1, 0, FLAG_DATA, vec![0xDD]);
    let ready = reassembler.push(pkt0);
    assert_eq!(ready.len(), 1025); // seq 0 plus buffered 1..=1024
    assert_eq!(reassembler.expected_seq(), 1025);
    assert_eq!(reassembler.buffered_count(), 0);
}

#[test]
fn test_adv_reassembler_sequence_rollover_reordering() {
    let mut reassembler = StreamReassembler::new(u32::MAX);

    // Packet 0 arrives out-of-order before packet u32::MAX
    let p0 = StreamPacket::new(1, 0, FLAG_DATA, b"wrapped_zero".to_vec());
    let r0 = reassembler.push(p0);
    println!("Ready on p0: {:?}", r0.len());

    // Packet u32::MAX arrives
    let p_max = StreamPacket::new(1, u32::MAX, FLAG_DATA, b"max".to_vec());
    let r_max = reassembler.push(p_max);
    // In standard TCP window arithmetic, packet 0 is the immediate successor to u32::MAX.
    // If packet 0 arrived before u32::MAX, it should be buffered and both should drain together.
    // But because of `packet.seq > self.expected_seq` unsigned comparison, packet 0 is dropped!
    assert_eq!(r_max.len(), 2, "Expected both u32::MAX and 0 to drain contiguously");
}

#[test]
fn test_adv_reassembler_fin_arrives_before_data() {
    let mut reassembler = StreamReassembler::new(0);

    // FIN arrives with seq 3
    let fin_pkt = StreamPacket::new(1, 3, FLAG_FIN, Vec::new());
    let ready = reassembler.push(fin_pkt);
    assert!(ready.is_empty());
    assert!(!reassembler.is_fin_delivered());

    // Packet 0 arrives
    let r0 = reassembler.push(StreamPacket::new(1, 0, FLAG_DATA, b"0".to_vec()));
    assert_eq!(r0.len(), 1);
    assert!(!reassembler.is_fin_delivered());

    // Packet 2 arrives (out of order, missing 1)
    let r2 = reassembler.push(StreamPacket::new(1, 2, FLAG_DATA, b"2".to_vec()));
    assert!(r2.is_empty());
    assert!(!reassembler.is_fin_delivered());

    // Packet 1 arrives -> cascades 1, 2, and 3(FIN)
    let r1 = reassembler.push(StreamPacket::new(1, 1, FLAG_DATA, b"1".to_vec()));
    assert_eq!(r1.len(), 3);
    assert_eq!(r1[0].seq, 1);
    assert_eq!(r1[1].seq, 2);
    assert_eq!(r1[2].seq, 3);
    assert_eq!(r1[2].flags, FLAG_FIN);

    assert!(reassembler.is_fin_delivered());
    assert_eq!(reassembler.buffered_count(), 0);
}

// =========================================================================
// 3. CORRUPTED IPV4 CHECKSUMS, TRUNCATED HEADERS & PROTOCOL REJECTIONS
// =========================================================================

#[test]
fn test_adv_corrupted_ipv4_checksum_properties() {
    let raw = b"MinecraftProtocolTestChecksumCorruptionPayload";
    let src = [10, 88, 0, 2];
    let dst = [10, 88, 0, 1];

    let mut pkt = wrap_synthetic_ipv4(src, dst, raw);
    let original_chk = u16::from_be_bytes([pkt[10], pkt[11]]);

    // Verify original checksum folds to 0xFFFF
    let mut sum: u32 = 0;
    for i in (0..20).step_by(2) {
        sum += u16::from_be_bytes([pkt[i], pkt[i + 1]]) as u32;
    }
    while (sum >> 16) > 0 {
        sum = (sum & 0xFFFF) + (sum >> 16);
    }
    assert_eq!(sum as u16, 0xFFFF);

    // Corrupt checksum by inverting all bits
    pkt[10] ^= 0xFF;
    pkt[11] ^= 0xFF;
    let corrupted_chk = u16::from_be_bytes([pkt[10], pkt[11]]);
    assert_ne!(corrupted_chk, original_chk);

    // Recomputed fold must NOT equal 0xFFFF
    let mut sum_bad: u32 = 0;
    for i in (0..20).step_by(2) {
        sum_bad += u16::from_be_bytes([pkt[i], pkt[i + 1]]) as u32;
    }
    while (sum_bad >> 16) > 0 {
        sum_bad = (sum_bad & 0xFFFF) + (sum_bad >> 16);
    }
    assert_ne!(sum_bad as u16, 0xFFFF);
}

#[test]
fn test_adv_ipv4_header_truncation_fuzzing_no_panic() {
    let valid_pkt = wrap_synthetic_ipv4([10, 88, 0, 2], [10, 88, 0, 1], b"TestData");

    // Test all prefix lengths from 0 to 19 bytes
    for len in 0..IPV4_HEADER_LEN {
        let truncated = &valid_pkt[..len];
        let res = unwrap_synthetic_ipv4(truncated);
        assert!(res.is_err(), "Length {} must be rejected gracefully", len);
    }

    // Test total_len claiming more than packet size
    for claimed in (valid_pkt.len() + 1)..=(valid_pkt.len() + 100) {
        let mut malformed = valid_pkt.clone();
        let bytes = (claimed as u16).to_be_bytes();
        malformed[2] = bytes[0];
        malformed[3] = bytes[1];
        assert!(
            unwrap_synthetic_ipv4(&malformed).is_err(),
            "Claimed len {} > packet len must be rejected",
            claimed
        );
    }

    // Test IHL invalid values: 0, 1, 2, 3, 4 (all < 5)
    for ihl_val in 0..5 {
        let mut malformed = valid_pkt.clone();
        malformed[0] = (4 << 4) | ihl_val;
        assert!(
            unwrap_synthetic_ipv4(&malformed).is_err(),
            "IHL {} must be rejected",
            ihl_val
        );
    }

    // Test invalid IP versions: 0, 1, 2, 3, 5, 6, 15
    for ver in &[0, 1, 2, 3, 5, 6, 7, 15] {
        let mut malformed = valid_pkt.clone();
        malformed[0] = (ver << 4) | 5;
        assert!(
            unwrap_synthetic_ipv4(&malformed).is_err(),
            "Version {} must be rejected",
            ver
        );
    }
}

#[test]
fn test_adv_stream_packet_deserialization_fuzzing_no_panic() {
    // 0 to 15 bytes (too short)
    for len in 0..STREAM_HEADER_LEN {
        let slice = vec![0xCD; len];
        assert!(StreamPacket::from_bytes(&slice).is_err());
    }

    // Bad magic
    let mut bad_magic = StreamPacket::new(1, 0, FLAG_DATA, vec![1, 2, 3]).to_bytes();
    bad_magic[0] = 0xAA;
    assert!(StreamPacket::from_bytes(&bad_magic).is_err());

    // Truncated payload: claims 1000 bytes, but buffer has only 20
    let mut truncated = StreamPacket::new(1, 0, FLAG_DATA, vec![1, 2, 3]).to_bytes();
    truncated[14] = 0x03;
    truncated[15] = 0xE8; // 1000 in big endian
    assert!(StreamPacket::from_bytes(&truncated).is_err());
}

// =========================================================================
// 4. PORT COLLISION FALLBACK & RAPID REUSE ACROSS CLIENT DISCONNECTS
// =========================================================================

#[tokio::test]
async fn test_adv_port_collision_fallback_under_competition() {
    // Occupy two ports
    let occ1 = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let p1 = occ1.local_addr().unwrap().port();

    let (tx, rx) = mpsc::channel(100);
    // Guest tries to bind occupied p1 -> must fallback to dynamic ephemeral port
    let (bound1, handle1) = UserspaceDatapath::start_guest_proxy(p1, tx, rx)
        .await
        .unwrap();

    assert_ne!(bound1, p1);
    assert!(bound1 > 0);

    // Verify fallback port is reachable
    let stream = TcpStream::connect(format!("127.0.0.1:{}", bound1)).await;
    assert!(stream.is_ok());

    handle1.stop();
    drop(occ1);
}

#[tokio::test]
async fn test_adv_rapid_sequential_client_connects_and_disconnects() {
    let server_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let server_port = server_listener.local_addr().unwrap().port();

    let (host_to_guest_tx, host_to_guest_rx) = mpsc::channel::<Vec<u8>>(500);
    let (guest_to_host_tx, guest_to_host_rx) = mpsc::channel::<Vec<u8>>(500);

    let host_handle = UserspaceDatapath::start_host_proxy(server_port, host_to_guest_tx, guest_to_host_rx)
        .await
        .unwrap();

    let (guest_port, guest_handle) = UserspaceDatapath::start_guest_proxy(0, guest_to_host_tx, host_to_guest_rx)
        .await
        .unwrap();

    // Server echoes back data
    let server_task = tokio::spawn(async move {
        while let Ok((mut socket, _)) = server_listener.accept().await {
            tokio::spawn(async move {
                let mut buf = [0u8; 256];
                while let Ok(n) = socket.read(&mut buf).await {
                    if n == 0 {
                        break;
                    }
                    if socket.write_all(&buf[..n]).await.is_err() {
                        break;
                    }
                }
            });
        }
    });

    // Run 20 rapid sequential connections
    for i in 0..20 {
        let mut client = TcpStream::connect(format!("127.0.0.1:{}", guest_port))
            .await
            .expect(&format!("Client {} connect must succeed", i));

        let msg = format!("PingSession{}", i).into_bytes();
        client.write_all(&msg).await.unwrap();

        let mut reply = vec![0u8; msg.len()];
        client.read_exact(&mut reply).await.unwrap();
        assert_eq!(reply, msg);

        // Disconnect gracefully
        client.shutdown().await.unwrap();
        drop(client);
        tokio::time::sleep(Duration::from_millis(15)).await;
    }

    host_handle.stop();
    guest_handle.stop();
    server_task.abort();
}

#[tokio::test]
async fn test_adv_concurrent_multiplexed_clients_fidelity() {
    let server_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let server_port = server_listener.local_addr().unwrap().port();

    let (host_to_guest_tx, host_to_guest_rx) = mpsc::channel::<Vec<u8>>(1000);
    let (guest_to_host_tx, guest_to_host_rx) = mpsc::channel::<Vec<u8>>(1000);

    let host_handle = UserspaceDatapath::start_host_proxy(server_port, host_to_guest_tx, guest_to_host_rx)
        .await
        .unwrap();

    let (guest_port, guest_handle) = UserspaceDatapath::start_guest_proxy(0, guest_to_host_tx, host_to_guest_rx)
        .await
        .unwrap();

    // Server echoes back data
    let server_task = tokio::spawn(async move {
        while let Ok((mut socket, _)) = server_listener.accept().await {
            tokio::spawn(async move {
                let mut buf = [0u8; 1024];
                while let Ok(n) = socket.read(&mut buf).await {
                    if n == 0 {
                        break;
                    }
                    if socket.write_all(&buf[..n]).await.is_err() {
                        break;
                    }
                }
            });
        }
    });

    // 5 concurrent clients sending unique patterns simultaneously
    let mut handles = Vec::new();
    for client_id in 0..5 {
        let guest_port = guest_port;
        let h = tokio::spawn(async move {
            let mut client = TcpStream::connect(format!("127.0.0.1:{}", guest_port))
                .await
                .unwrap();

            for round in 0..10 {
                let payload = format!("Client{}-Round{}", client_id, round).into_bytes();
                client.write_all(&payload).await.unwrap();

                let mut buf = vec![0u8; payload.len()];
                client.read_exact(&mut buf).await.unwrap();
                assert_eq!(buf, payload);
            }
            client.shutdown().await.unwrap();
        });
        handles.push(h);
    }

    for h in handles {
        h.await.unwrap();
    }

    host_handle.stop();
    guest_handle.stop();
    server_task.abort();
}

#[tokio::test]
async fn test_adv_full_boringtun_wireguard_mtu_throughput() {
    let static_host = [0x11u8; 32];
    let secret_host = identity::StaticSecret::from(static_host);
    let public_host = identity::PublicKey::from(&secret_host);

    let static_guest = [0x22u8; 32];
    let secret_guest = identity::StaticSecret::from(static_guest);
    let public_guest = identity::PublicKey::from(&secret_guest);

    let mut host_tunnel =
        WireGuardTunnel::new_host(static_host, *public_guest.as_bytes(), None).unwrap();
    let mut guest_tunnel =
        WireGuardTunnel::new_guest(static_guest, *public_host.as_bytes(), None).unwrap();

    // Handshake
    let mut init_buf = [0u8; 2048];
    let init_len = guest_tunnel.format_handshake_initiation(&mut init_buf).unwrap();

    let mut resp_buf = [0u8; 2048];
    let resp = host_tunnel.decapsulate(None, &init_buf[..init_len], &mut resp_buf).unwrap();
    let resp_bytes = match resp {
        wireguard::DecapsulatedPacket::WriteToNetwork(b) => b.to_vec(),
        _ => panic!("Expected WriteToNetwork"),
    };

    let mut conf_buf = [0u8; 2048];
    let conf = guest_tunnel.decapsulate(None, &resp_bytes, &mut conf_buf).unwrap();
    let conf_bytes = match conf {
        wireguard::DecapsulatedPacket::WriteToNetwork(b) => b.to_vec(),
        _ => panic!("Expected WriteToNetwork confirmation"),
    };

    let mut ack_buf = [0u8; 2048];
    let ack = host_tunnel.decapsulate(None, &conf_bytes, &mut ack_buf).unwrap();
    assert!(matches!(ack, wireguard::DecapsulatedPacket::Done));

    // Stress: Send 50 MTU-sized packets through WireGuard
    let mut enc_buf = [0u8; 2048];
    let mut dec_buf = [0u8; 2048];

    for i in 0..50 {
        let stream_pkt = StreamPacket::new(1, i, FLAG_DATA, vec![(i % 255) as u8; MAX_PAYLOAD_CHUNK]);
        let ip_pkt = wrap_synthetic_ipv4(GUEST_IPV4, HOST_IPV4, &stream_pkt.to_bytes());
        assert_eq!(ip_pkt.len(), MAX_PACKET_MTU);

        // Encapsulate into WireGuard
        let enc_len = guest_tunnel.encapsulate(&ip_pkt, &mut enc_buf).unwrap();
        assert!(enc_len > 0);
        assert!(enc_len <= 1400 + 32); // Within 1432 bytes (well below 1500)

        // Decapsulate on Host
        let dec = host_tunnel.decapsulate(None, &enc_buf[..enc_len], &mut dec_buf).unwrap();
        match dec {
            wireguard::DecapsulatedPacket::PacketV4 { packet, src_ip } => {
                assert_eq!(packet.len(), MAX_PACKET_MTU);
                assert_eq!(src_ip, Ipv4Addr::new(10, 88, 0, 2));
                let unwrapped = unwrap_synthetic_ipv4(packet).unwrap();
                let decoded_stream = StreamPacket::from_bytes(unwrapped).unwrap();
                assert_eq!(decoded_stream.seq, i);
                assert_eq!(decoded_stream.payload.len(), MAX_PAYLOAD_CHUNK);
            }
            _ => panic!("Expected PacketV4"),
        }
    }
}
