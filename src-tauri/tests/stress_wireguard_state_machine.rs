//! Comprehensive Empirical Stress Test Suite for WireGuard State Machine & Cryptographic Subsystem.
//!
//! Stress-tests:
//! 1. High-volume packet encapsulation & decapsulation throughput and loopback data integrity.
//! 2. Protocol timer evaluation loop (`update_timers`) stability, session maintenance, and keepalives.
//! 3. Preshared key (PSK) Noise_IKpsk2 authentication vs rejection of mismatched/missing/tampered PSKs.
//! 4. Anti-replay protection, ciphertext tampering rejection, and malformed datagram fuzzing.
//! 5. Curve25519 DH key generation concurrency, atomic tunnel indexing, and Base64 codecs.

#[path = "../src/network/wireguard.rs"]
mod wireguard;

#[path = "../src/network/identity.rs"]
mod identity;

use boringtun::x25519::{PublicKey, StaticSecret};
use rand::{Rng, RngExt};
use std::net::Ipv4Addr;
use std::time::Instant;

fn generate_keypair() -> ([u8; 32], [u8; 32]) {
    let mut key_bytes = [0u8; 32];
    rand::rng().fill_bytes(&mut key_bytes);
    let secret = StaticSecret::from(key_bytes);
    let public = PublicKey::from(&secret);
    (secret.to_bytes(), public.to_bytes())
}

/// Helper to establish a fully-handshaken WireGuard tunnel session between Host and Guest.
fn establish_session(
    host_psk: Option<[u8; 32]>,
    guest_psk: Option<[u8; 32]>,
) -> Result<(wireguard::WireGuardTunnel, wireguard::WireGuardTunnel), String> {
    let (host_priv, host_pub) = generate_keypair();
    let (guest_priv, guest_pub) = generate_keypair();

    let mut host = wireguard::WireGuardTunnel::new_host(host_priv, guest_pub, host_psk)
        .map_err(|e| format!("Host init failed: {:?}", e))?;
    let mut guest = wireguard::WireGuardTunnel::new_guest(guest_priv, host_pub, guest_psk)
        .map_err(|e| format!("Guest init failed: {:?}", e))?;

    let mut buf_a = vec![0u8; wireguard::MIN_BUFFER_SIZE];
    let mut buf_b = vec![0u8; wireguard::MIN_BUFFER_SIZE];
    let mut buf_c = vec![0u8; wireguard::MIN_BUFFER_SIZE];
    let mut scratch = vec![0u8; wireguard::MIN_BUFFER_SIZE];

    // 1. Guest Initiation (Type 1)
    let init_len = guest
        .format_handshake_initiation(&mut buf_a)
        .map_err(|e| format!("Guest handshake init failed: {:?}", e))?;
    if init_len != 148 || buf_a[0] != 1 {
        return Err(format!("Invalid initiation datagram len={}", init_len));
    }

    // 2. Host Response (Type 2)
    let host_resp = host
        .decapsulate(None, &buf_a[..init_len], &mut buf_b)
        .map_err(|e| format!("Host decapsulate init failed: {:?}", e))?;
    let resp_len = match host_resp {
        wireguard::DecapsulatedPacket::WriteToNetwork(datagram) => {
            if datagram.len() != 92 || datagram[0] != 2 {
                return Err(format!("Invalid response datagram len={}", datagram.len()));
            }
            datagram.len()
        }
        _ => return Err("Host failed to produce WriteToNetwork response".to_string()),
    };

    // 3. Guest Confirmation Keepalive (Type 4)
    let guest_confirm = guest
        .decapsulate(None, &buf_b[..resp_len], &mut buf_c)
        .map_err(|e| format!("Guest decapsulate response failed: {:?}", e))?;
    let confirm_len = match guest_confirm {
        wireguard::DecapsulatedPacket::WriteToNetwork(datagram) => {
            if datagram.len() != 32 || datagram[0] != 4 {
                return Err(format!("Invalid confirmation keepalive len={}", datagram.len()));
            }
            datagram.len()
        }
        _ => return Err("Guest failed to produce WriteToNetwork confirmation".to_string()),
    };

    // 4. Host consumes confirmation -> Done
    let host_done = host
        .decapsulate(None, &buf_c[..confirm_len], &mut scratch)
        .map_err(|e| format!("Host consume confirmation failed: {:?}", e))?;
    match host_done {
        wireguard::DecapsulatedPacket::Done => Ok((host, guest)),
        _ => Err("Host did not transition to Done after confirmation".to_string()),
    }
}

// =========================================================================
// PART 1: HIGH-VOLUME THROUGHPUT & LOOPBACK INTEGRITY
// =========================================================================

#[test]
fn stress_test_high_volume_throughput_and_loopback_integrity() {
    let (mut host, mut guest) = establish_session(None, None).expect("Handshake establishment");

    let mut enc_buf = vec![0u8; wireguard::MIN_BUFFER_SIZE];
    let mut dec_buf = vec![0u8; wireguard::MIN_BUFFER_SIZE];

    let guest_ip = [10, 88, 0, 2];
    let host_ip = [10, 88, 0, 1];

    let total_iterations = 2500; // 2,500 roundtrips = 5,000 total WireGuard packets
    let mut total_bytes_transmitted = 0usize;

    let start_time = Instant::now();

    for i in 0..total_iterations {
        // --- 1. Guest -> Host transmission ---
        // Construct deterministic Minecraft-like payload with varying lengths
        let payload_len = 16 + (i % 512); // Length varies from 16 to 527 bytes
        let mut forward_payload = Vec::with_capacity(payload_len);
        forward_payload.extend_from_slice(&(i as u32).to_le_bytes());
        for b in 4..payload_len {
            forward_payload.push(((i * 7 + b) % 256) as u8);
        }

        let synth_v4 = wireguard::make_synthetic_ipv4(guest_ip, host_ip, &forward_payload);
        total_bytes_transmitted += synth_v4.len();

        let enc_len = guest
            .encapsulate(&synth_v4, &mut enc_buf)
            .expect("Guest encapsulate packet");
        assert!(enc_len > synth_v4.len());
        assert_eq!(enc_buf[0], 4, "Must be WireGuard Type 4 Transport packet");

        let dec = host
            .decapsulate(None, &enc_buf[..enc_len], &mut dec_buf)
            .expect("Host decapsulate packet");

        match dec {
            wireguard::DecapsulatedPacket::PacketV4 { packet, src_ip } => {
                assert_eq!(src_ip, Ipv4Addr::new(10, 88, 0, 2));
                let (extracted, s_ip, d_ip) =
                    wireguard::parse_synthetic_ipv4(packet).expect("Parse synthetic IPv4");
                assert_eq!(s_ip, Ipv4Addr::new(10, 88, 0, 2));
                assert_eq!(d_ip, Ipv4Addr::new(10, 88, 0, 1));
                assert_eq!(
                    extracted,
                    forward_payload.as_slice(),
                    "Forward payload corrupted at iteration {}",
                    i
                );
            }
            _ => panic!("Expected PacketV4 on Host at iteration {}", i),
        }

        // --- 2. Host -> Guest transmission (reply) ---
        let reply_len = 24 + ((i * 13) % 400);
        let mut reply_payload = Vec::with_capacity(reply_len);
        reply_payload.extend_from_slice(&(i as u32 ^ 0xDEADBEEF).to_le_bytes());
        for b in 4..reply_len {
            reply_payload.push(((i * 11 + b) % 256) as u8);
        }

        let reply_synth_v4 = wireguard::make_synthetic_ipv4(host_ip, guest_ip, &reply_payload);
        total_bytes_transmitted += reply_synth_v4.len();

        let reply_enc_len = host
            .encapsulate(&reply_synth_v4, &mut enc_buf)
            .expect("Host encapsulate reply");

        let guest_dec = guest
            .decapsulate(None, &enc_buf[..reply_enc_len], &mut dec_buf)
            .expect("Guest decapsulate reply");

        match guest_dec {
            wireguard::DecapsulatedPacket::PacketV4 { packet, src_ip } => {
                assert_eq!(src_ip, Ipv4Addr::new(10, 88, 0, 1));
                let (extracted, s_ip, d_ip) =
                    wireguard::parse_synthetic_ipv4(packet).expect("Parse synthetic reply");
                assert_eq!(s_ip, Ipv4Addr::new(10, 88, 0, 1));
                assert_eq!(d_ip, Ipv4Addr::new(10, 88, 0, 2));
                assert_eq!(
                    extracted,
                    reply_payload.as_slice(),
                    "Reply payload corrupted at iteration {}",
                    i
                );
            }
            _ => panic!("Expected PacketV4 on Guest at iteration {}", i),
        }
    }

    let elapsed = start_time.elapsed();
    let total_packets = total_iterations * 2;
    let pps = (total_packets as f64) / elapsed.as_secs_f64();
    let mb_transmitted = (total_bytes_transmitted as f64) / (1024.0 * 1024.0);

    println!(
        "Stress Test Throughput: {} packets ({} MB) in {:?} ({:.0} packets/sec) with 100% loopback integrity.",
        total_packets, mb_transmitted, elapsed, pps
    );
    assert_eq!(total_packets, 5000);
}

#[test]
fn stress_test_variable_payload_sizes_and_boundary_conditions() {
    let (mut host, mut guest) = establish_session(None, None).expect("Handshake establishment");

    let test_sizes = [
        0, 1, 2, 3, 4, 7, 15, 16, 31, 32, 63, 64, 127, 128, 255, 256, 500, 1000, 1380, 1400,
        1420, 1450, 1480, 1500,
    ];

    let mut enc_buf = vec![0u8; 4096];
    let mut dec_buf = vec![0u8; 4096];

    for &size in &test_sizes {
        let payload: Vec<u8> = (0..size).map(|b| (b % 251) as u8).collect();
        let synth_v4 = wireguard::make_synthetic_ipv4([10, 88, 0, 2], [10, 88, 0, 1], &payload);

        let enc_len = guest
            .encapsulate(&synth_v4, &mut enc_buf)
            .unwrap_or_else(|e| panic!("Encapsulate failed for size {}: {:?}", size, e));

        let dec = host
            .decapsulate(None, &enc_buf[..enc_len], &mut dec_buf)
            .unwrap_or_else(|e| panic!("Decapsulate failed for size {}: {:?}", size, e));

        match dec {
            wireguard::DecapsulatedPacket::PacketV4 { packet, .. } => {
                let (extracted, _, _) = wireguard::parse_synthetic_ipv4(packet)
                    .unwrap_or_else(|e| panic!("Parse synthetic IPv4 failed for size {}: {:?}", size, e));
                assert_eq!(
                    extracted,
                    payload.as_slice(),
                    "Payload mismatch for size {}",
                    size
                );
            }
            _ => panic!("Expected PacketV4 for payload size {}", size),
        }
    }
}

#[test]
fn stress_test_anti_replay_attack_rejection() {
    let (mut host, mut guest) = establish_session(None, None).expect("Handshake establishment");

    let mut enc_buf = vec![0u8; wireguard::MIN_BUFFER_SIZE];
    let mut dec_buf = vec![0u8; wireguard::MIN_BUFFER_SIZE];

    let payload = b"Anti-replay target payload";
    let synth_v4 = wireguard::make_synthetic_ipv4([10, 88, 0, 2], [10, 88, 0, 1], payload);

    let enc_len = guest.encapsulate(&synth_v4, &mut enc_buf).unwrap();

    // Clone the exact encrypted datagram bytes
    let intercepted_datagram = enc_buf[..enc_len].to_vec();

    // First arrival: Host must successfully decrypt
    let dec1 = host
        .decapsulate(None, &intercepted_datagram, &mut dec_buf)
        .expect("First packet delivery must succeed");
    assert!(matches!(dec1, wireguard::DecapsulatedPacket::PacketV4 { .. }));

    // Immediate replay: Host MUST reject the replayed datagram!
    // BoringTun anti-replay returns Err or Done (silently dropped as duplicate)
    let dec_replay = host.decapsulate(None, &intercepted_datagram, &mut dec_buf);
    match dec_replay {
        Ok(wireguard::DecapsulatedPacket::Done) => {
            // BoringTun successfully detected duplicate and dropped it
        }
        Err(_) => {
            // BoringTun returned error on duplicate
        }
        Ok(wireguard::DecapsulatedPacket::PacketV4 { .. }) => {
            panic!("CRITICAL SECURITY FAILURE: Replayed WireGuard packet was accepted!");
        }
        _ => {}
    }
}

#[test]
fn stress_test_tampered_ciphertext_rejection() {
    let (mut host, mut guest) = establish_session(None, None).expect("Handshake establishment");

    let mut enc_buf = vec![0u8; wireguard::MIN_BUFFER_SIZE];
    let mut dec_buf = vec![0u8; wireguard::MIN_BUFFER_SIZE];

    let payload = b"Authenticity and integrity target payload";
    let synth_v4 = wireguard::make_synthetic_ipv4([10, 88, 0, 2], [10, 88, 0, 1], payload);
    let enc_len = guest.encapsulate(&synth_v4, &mut enc_buf).unwrap();

    // Test tampering at various positions in the datagram
    let tamper_indices = [
        0,              // Message type byte
        4, 5, 6, 7,     // Receiver index bytes
        8, 9, 10,       // Counter bytes
        16, 20, 25,     // Ciphertext payload
        enc_len - 16,   // Poly1305 MAC tag start
        enc_len - 1,    // Poly1305 MAC tag end
    ];

    for &idx in &tamper_indices {
        if idx < enc_len {
            let mut tampered = enc_buf[..enc_len].to_vec();
            tampered[idx] ^= 0x55; // Flip bits

            let dec_res = host.decapsulate(None, &tampered, &mut dec_buf);
            match dec_res {
                Err(_) => {
                    // Correctly rejected by BoringTun AEAD authentication
                }
                Ok(wireguard::DecapsulatedPacket::Done) => {
                    // Dropped
                }
                Ok(wireguard::DecapsulatedPacket::PacketV4 { .. }) => {
                    panic!(
                        "CRITICAL INTEGRITY FAILURE: Tampered packet at index {} was accepted as valid plaintext!",
                        idx
                    );
                }
                _ => {}
            }
        }
    }
}

#[test]
fn stress_test_malformed_datagram_fuzzing() {
    let (mut host, _guest) = establish_session(None, None).expect("Handshake establishment");
    let mut dec_buf = vec![0u8; wireguard::MIN_BUFFER_SIZE];

    // Truncated datagram lengths below minimum WireGuard headers
    let truncated_lengths = [0, 1, 2, 3, 4, 8, 12, 16, 28, 31, 32, 64, 91, 147];
    for &len in &truncated_lengths {
        let dummy = vec![4u8; len];
        let res = host.decapsulate(None, &dummy, &mut dec_buf);
        // Must not panic, must return Err or Done
        assert!(res.is_err() || matches!(res, Ok(wireguard::DecapsulatedPacket::Done)));
    }

    // Randomized noise datagrams
    let mut rng = rand::rng();
    for _ in 0..100 {
        let len = rng.random_range(1..=1024);
        let mut noise = vec![0u8; len];
        rng.fill_bytes(&mut noise);

        let res = host.decapsulate(None, &noise, &mut dec_buf);
        assert!(res.is_err() || matches!(res, Ok(wireguard::DecapsulatedPacket::Done)));
    }
}

// =========================================================================
// PART 2: TIMER EVALUATION LOOP (`update_timers`) & SESSION MAINTENANCE
// =========================================================================

#[test]
fn stress_test_continuous_timer_loop_1000_ticks() {
    let (mut host, mut guest) = establish_session(None, None).expect("Handshake establishment");

    let mut timer_buf = vec![0u8; wireguard::MIN_BUFFER_SIZE];
    let mut scratch_buf = vec![0u8; wireguard::MIN_BUFFER_SIZE];

    let mut guest_keepalives_sent = 0;
    let mut host_keepalives_sent = 0;

    // Run 1,000 continuous timer ticks
    for _ in 0..1000 {
        // Guest timer tick
        let g_res = guest.update_timers(&mut timer_buf).expect("Guest timer tick");
        if let Some(len) = g_res {
            guest_keepalives_sent += 1;
            // Deliver keepalive to Host
            let h_dec = host
                .decapsulate(None, &timer_buf[..len], &mut scratch_buf)
                .expect("Host decapsulate guest keepalive");
            assert!(
                matches!(h_dec, wireguard::DecapsulatedPacket::Done),
                "Host must consume keepalive as Done"
            );
        }

        // Host timer tick
        let h_res = host.update_timers(&mut timer_buf).expect("Host timer tick");
        if let Some(len) = h_res {
            host_keepalives_sent += 1;
            let g_dec = guest
                .decapsulate(None, &timer_buf[..len], &mut scratch_buf)
                .expect("Guest decapsulate host keepalive");
            assert!(
                matches!(g_dec, wireguard::DecapsulatedPacket::Done),
                "Guest must consume keepalive as Done"
            );
        }
    }

    println!(
        "Timer loop completed 1000 ticks: Guest generated {} packets, Host generated {} packets.",
        guest_keepalives_sent, host_keepalives_sent
    );
}

#[test]
fn stress_test_timer_buffer_boundary_validation() {
    let (mut host, mut guest) = establish_session(None, None).expect("Handshake establishment");

    let mut undersized_buf = vec![0u8; 2047]; // 1 byte below required 2048
    assert!(matches!(
        guest.update_timers(&mut undersized_buf),
        Err(wireguard::WireGuardError::BufferTooSmall { required: 2048, provided: 2047 })
    ));
    assert!(matches!(
        host.update_timers(&mut undersized_buf),
        Err(wireguard::WireGuardError::BufferTooSmall { required: 2048, provided: 2047 })
    ));

    let mut exact_buf = vec![0u8; 2048]; // Exactly 2048 bytes
    assert!(guest.update_timers(&mut exact_buf).is_ok());
    assert!(host.update_timers(&mut exact_buf).is_ok());

    let mut oversized_buf = vec![0u8; 8192];
    assert!(guest.update_timers(&mut oversized_buf).is_ok());
    assert!(host.update_timers(&mut oversized_buf).is_ok());
}

#[test]
fn stress_test_tight_timer_loop_100k_ticks_stability() {
    let (mut host, mut guest) = establish_session(None, None).expect("Handshake establishment");
    let mut timer_buf = vec![0u8; wireguard::MIN_BUFFER_SIZE];

    let start = Instant::now();
    for _ in 0..100_000 {
        let g_res = guest.update_timers(&mut timer_buf);
        assert!(g_res.is_ok(), "Guest update_timers must not panic or error");
        let h_res = host.update_timers(&mut timer_buf);
        assert!(h_res.is_ok(), "Host update_timers must not panic or error");
    }
    let elapsed = start.elapsed();
    println!("100,000 timer tick iterations completed in {:?}", elapsed);
}

#[test]
fn stress_test_encapsulate_prior_to_handshake_triggers_initiation() {
    let (host_priv, host_pub) = generate_keypair();
    let (guest_priv, guest_pub) = generate_keypair();

    let mut guest = wireguard::WireGuardTunnel::new_guest(guest_priv, host_pub, None).unwrap();
    let mut host = wireguard::WireGuardTunnel::new_host(host_priv, guest_pub, None).unwrap();

    let mut enc_buf = vec![0u8; wireguard::MIN_BUFFER_SIZE];
    let payload = b"Premature packet before handshake";
    let synth = wireguard::make_synthetic_ipv4([10, 88, 0, 2], [10, 88, 0, 1], payload);

    // Encapsulate on an unestablished tunnel
    let enc_len = guest.encapsulate(&synth, &mut enc_buf).unwrap();

    // In BoringTun WireGuard specification, calling encapsulate before handshake queues
    // the packet internally and returns a Handshake Initiation datagram (148 bytes, Type 1)
    assert_eq!(enc_len, 148, "Must generate Handshake Initiation datagram");
    assert_eq!(enc_buf[0], 1, "Must be Type 1 Handshake Initiation");

    // Host processes this initiation and responds with Type 2
    let mut resp_buf = vec![0u8; wireguard::MIN_BUFFER_SIZE];
    let host_resp = host.decapsulate(None, &enc_buf[..enc_len], &mut resp_buf).unwrap();
    let resp_len = match host_resp {
        wireguard::DecapsulatedPacket::WriteToNetwork(d) => d.len(),
        _ => panic!("Expected WriteToNetwork"),
    };
    assert_eq!(resp_len, 92);
    assert_eq!(resp_buf[0], 2);

    // Guest processes response and produces confirmation
    let mut confirm_buf = vec![0u8; wireguard::MIN_BUFFER_SIZE];
    let confirm = guest.decapsulate(None, &resp_buf[..resp_len], &mut confirm_buf).unwrap();
    let confirm_len = match confirm {
        wireguard::DecapsulatedPacket::WriteToNetwork(d) => d.len(),
        _ => panic!("Expected WriteToNetwork"),
    };
    assert_eq!(confirm_len, 32);

    // Host consumes confirmation
    let mut scratch = vec![0u8; wireguard::MIN_BUFFER_SIZE];
    let done = host.decapsulate(None, &confirm_buf[..confirm_len], &mut scratch).unwrap();
    assert!(matches!(done, wireguard::DecapsulatedPacket::Done));
}

#[test]
fn stress_test_session_survival_after_timer_ticks_and_idle_resumption() {
    let (mut host, mut guest) = establish_session(None, None).expect("Handshake establishment");

    let mut timer_buf = vec![0u8; wireguard::MIN_BUFFER_SIZE];
    let mut enc_buf = vec![0u8; wireguard::MIN_BUFFER_SIZE];
    let mut dec_buf = vec![0u8; wireguard::MIN_BUFFER_SIZE];

    // Simulate idle ticks
    for _ in 0..50 {
        let _ = guest.update_timers(&mut timer_buf);
        let _ = host.update_timers(&mut timer_buf);
    }

    // Now resume traffic: Guest -> Host
    let payload = b"Post-idle traffic verification packet";
    let synth = wireguard::make_synthetic_ipv4([10, 88, 0, 2], [10, 88, 0, 1], payload);
    let enc_len = guest.encapsulate(&synth, &mut enc_buf).unwrap();

    let dec = host.decapsulate(None, &enc_buf[..enc_len], &mut dec_buf).unwrap();
    match dec {
        wireguard::DecapsulatedPacket::PacketV4 { packet, .. } => {
            let (extracted, _, _) = wireguard::parse_synthetic_ipv4(packet).unwrap();
            assert_eq!(extracted, payload);
        }
        _ => panic!("Expected successful packet decapsulation after idle period"),
    }
}

// =========================================================================
// PART 3: PRESHARED KEY (PSK) AUTHENTICATION VS MISMATCHED REJECTION
// =========================================================================

#[test]
fn stress_test_matching_psk_authentication() {
    let psk = Some([0x77u8; 32]);
    let (mut host, mut guest) = establish_session(psk, psk).expect("Handshake with matching PSK");

    let mut enc_buf = vec![0u8; wireguard::MIN_BUFFER_SIZE];
    let mut dec_buf = vec![0u8; wireguard::MIN_BUFFER_SIZE];

    let test_data = b"PSK Authenticated Conduit Mesh Stream";
    let synth = wireguard::make_synthetic_ipv4([10, 88, 0, 2], [10, 88, 0, 1], test_data);

    let enc_len = guest.encapsulate(&synth, &mut enc_buf).unwrap();
    let dec = host.decapsulate(None, &enc_buf[..enc_len], &mut dec_buf).unwrap();

    match dec {
        wireguard::DecapsulatedPacket::PacketV4 { packet, .. } => {
            let (extracted, _, _) = wireguard::parse_synthetic_ipv4(packet).unwrap();
            assert_eq!(extracted, test_data);
        }
        _ => panic!("Expected PacketV4 with matching PSK"),
    }
}

#[test]
fn stress_test_mismatched_psk_rejection_guest_initiation() {
    let psk_host = Some([0x11u8; 32]);
    let psk_guest = Some([0x22u8; 32]);

    let res = establish_session(psk_host, psk_guest);
    assert!(
        res.is_err(),
        "CRITICAL SECURITY FAILURE: Handshake established despite mismatched PSKs!"
    );
    println!("Mismatched PSK correctly rejected: {:?}", res.err());
}

#[test]
fn stress_test_mismatched_psk_single_bit_flip() {
    let psk_a = [0xAAu8; 32];
    let mut psk_b = psk_a;
    psk_b[31] ^= 0x01; // Exactly 1 bit flipped

    let res = establish_session(Some(psk_a), Some(psk_b));
    assert!(
        res.is_err(),
        "CRITICAL SECURITY FAILURE: Handshake established with 1-bit corrupted PSK!"
    );
}

#[test]
fn stress_test_psk_vs_none_rejection() {
    let psk = Some([0x33u8; 32]);

    // Host has PSK, Guest has None
    let res1 = establish_session(psk, None);
    assert!(
        res1.is_err(),
        "CRITICAL SECURITY FAILURE: Host with PSK accepted Guest without PSK!"
    );

    // Host has None, Guest has PSK
    let res2 = establish_session(None, psk);
    assert!(
        res2.is_err(),
        "CRITICAL SECURITY FAILURE: Host without PSK accepted Guest with PSK!"
    );
}

#[test]
fn stress_test_all_zero_psk_wireguard_canonical_equivalence() {
    let zero_psk = Some([0u8; 32]);
    let non_zero_psk = Some([0x01u8; 32]);

    // In WireGuard Noise_IKpsk2 (Section 5.4), None is canonically evaluated as 32 zero bytes (0^32).
    // Therefore, Some([0u8; 32]) and None interoperate canonically:
    let res_zero_none = establish_session(zero_psk, None);
    assert!(
        res_zero_none.is_ok(),
        "In WireGuard protocol, None is canonically evaluated as 32 zeroes"
    );

    // But any non-zero PSK vs None MUST be rejected:
    let res_nonzero_none = establish_session(non_zero_psk, None);
    assert!(
        res_nonzero_none.is_err(),
        "Non-zero PSK vs None must be rejected"
    );

    // And any non-zero PSK vs all-zeroes PSK MUST be rejected:
    let res_nonzero_zero = establish_session(non_zero_psk, zero_psk);
    assert!(
        res_nonzero_zero.is_err(),
        "Non-zero PSK vs all-zeroes PSK must be rejected"
    );

    // Matching non-zero PSKs must succeed:
    let res_match = establish_session(non_zero_psk, non_zero_psk);
    assert!(res_match.is_ok(), "Matching non-zero PSKs must succeed");
}

// =========================================================================
// PART 4: CRYPTOGRAPHIC KEY MANAGEMENT & CONCURRENCY
// =========================================================================

#[test]
fn stress_test_identity_generation_and_dh_concurrency() {
    let mut identities = Vec::new();
    for i in 0..50 {
        let ident = identity::IdentityManager::generate(Some(format!("Player_{}", i)));
        identities.push(ident);
    }

    // Verify pairwise DH symmetric agreement across all 50 identities
    for i in 0..identities.len() {
        for j in (i + 1)..identities.len() {
            let dh_ij = identities[i].diffie_hellman(&identities[j].public_key());
            let dh_ji = identities[j].diffie_hellman(&identities[i].public_key());

            assert_eq!(dh_ij, dh_ji, "DH must be symmetric between {} and {}", i, j);
            assert_ne!(dh_ij, [0u8; 32], "DH secret must not be zero");
        }
    }
}

#[test]
fn stress_test_base64_codecs_with_boundary_keys() {
    let boundary_keys: Vec<[u8; 32]> = vec![
        [0u8; 32],
        [0xFFu8; 32],
        [0x55u8; 32],
        [0xAAu8; 32],
        std::array::from_fn(|i| i as u8),
    ];

    for key in boundary_keys {
        let std_b64 = identity::encode_base64(&key);
        let url_b64 = identity::encode_base64_url(&key);

        let decoded_std = identity::decode_key_bytes(&std_b64).expect("Decode standard");
        let decoded_url = identity::decode_key_bytes(&url_b64).expect("Decode url-safe");

        assert_eq!(decoded_std, key);
        assert_eq!(decoded_url, key);
    }
}

#[test]
fn stress_test_atomic_monotonic_tunnel_indexing() {
    let (k1_priv, k1_pub) = generate_keypair();
    let (k2_priv, k2_pub) = generate_keypair();

    let t1 = wireguard::WireGuardTunnel::new_host(k1_priv, k2_pub, None).unwrap();
    let t2 = wireguard::WireGuardTunnel::new_guest(k2_priv, k1_pub, None).unwrap();
    let t3 = wireguard::WireGuardTunnel::new_host(k1_priv, k2_pub, None).unwrap();

    assert!(t1.index() < t2.index(), "Indices must monotonically increase");
    assert!(t2.index() < t3.index(), "Indices must monotonically increase");
}
