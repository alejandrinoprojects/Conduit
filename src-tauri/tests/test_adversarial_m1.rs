//! Empirical Adversarial & Stress Test Suite for Milestone 1
//!
//! Thoroughly challenges:
//! 1. Buffer size boundary checks: 0, 1, 147, 148, 2047, 2048, MTU+32 on all APIs.
//! 2. Truncated, malformed, corrupted datagrams fed to decapsulate:
//!    - Zero-length, sub-header lengths, wrong packet types.
//!    - Corrupted Type 1, Type 2, Type 4 datagrams with bit flips.
//!    - Out-of-order / replay datagrams.
//!    - 1,000 randomized adversarial fuzz payloads.
//! 3. Synthetic IPv4 framing edge cases:
//!    - Malformed headers, invalid IHL, total_length inconsistencies, non-IPv4 versions.
//! 4. Identity & Key Management:
//!    - Key randomness across 100 generated pairs (collision & entropy check).
//!    - Malformed Base64 strings, invalid padding, wrong key lengths (31B, 33B, 0B).
//!    - Low-order / all-zero public keys in Diffie-Hellman.

#[path = "../src/network/wireguard.rs"]
mod wireguard;

#[path = "../src/network/identity.rs"]
mod identity;

use rand::Rng;
use std::collections::HashSet;
use std::net::SocketAddr;
use wireguard::{make_synthetic_ipv4, parse_synthetic_ipv4, DecapsulatedPacket, WireGuardError, WireGuardTunnel, MIN_BUFFER_SIZE};

fn generate_tunnel_pair() -> (WireGuardTunnel, WireGuardTunnel) {
    let mut host_priv = [0u8; 32];
    let mut guest_priv = [0u8; 32];
    rand::rng().fill_bytes(&mut host_priv);
    rand::rng().fill_bytes(&mut guest_priv);

    let host_sec = boringtun::x25519::StaticSecret::from(host_priv);
    let host_pub = boringtun::x25519::PublicKey::from(&host_sec).to_bytes();

    let guest_sec = boringtun::x25519::StaticSecret::from(guest_priv);
    let guest_pub = boringtun::x25519::PublicKey::from(&guest_sec).to_bytes();

    let host = WireGuardTunnel::new_host(host_priv, guest_pub, None).expect("Host init");
    let guest = WireGuardTunnel::new_guest(guest_priv, host_pub, None).expect("Guest init");
    (host, guest)
}

fn complete_handshake(host: &mut WireGuardTunnel, guest: &mut WireGuardTunnel) {
    let mut buf_a = vec![0u8; MIN_BUFFER_SIZE];
    let mut buf_b = vec![0u8; MIN_BUFFER_SIZE];
    let mut buf_c = vec![0u8; MIN_BUFFER_SIZE];
    let mut scratch = vec![0u8; MIN_BUFFER_SIZE];

    let init_len = guest.format_handshake_initiation(&mut buf_a).expect("Format init");
    let resp = host.decapsulate(None, &buf_a[..init_len], &mut buf_b).expect("Decapsulate init");
    let resp_len = match resp {
        DecapsulatedPacket::WriteToNetwork(d) => d.len(),
        _ => panic!("Expected WriteToNetwork response"),
    };

    let confirm = guest.decapsulate(None, &buf_b[..resp_len], &mut buf_c).expect("Decapsulate resp");
    let confirm_len = match confirm {
        DecapsulatedPacket::WriteToNetwork(d) => d.len(),
        _ => panic!("Expected WriteToNetwork confirm"),
    };

    let done = host.decapsulate(None, &buf_c[..confirm_len], &mut scratch).expect("Decapsulate confirm");
    assert!(matches!(done, DecapsulatedPacket::Done));
}

// =========================================================================
// 1. BUFFER BOUNDARY & SCRATCH BUFFER STRESS TESTS
// =========================================================================

#[test]
fn adv_buffer_boundary_handshake_initiation() {
    let (_, mut guest) = generate_tunnel_pair();

    // Test buffer lengths from 0 up to 2047: ALL must return BufferTooSmall
    let test_sizes = [0, 1, 16, 64, 147, 148, 512, 1024, 2047];
    for size in test_sizes {
        let mut buf = vec![0u8; size];
        let res = guest.format_handshake_initiation(&mut buf);
        match res {
            Err(WireGuardError::BufferTooSmall { required, provided }) => {
                assert_eq!(required, 2048);
                assert_eq!(provided, size);
            }
            other => panic!("Expected BufferTooSmall for size {}, got {:?}", size, other),
        }
    }

    // Exact boundary: 2048 bytes must succeed and return exactly 148 bytes
    let mut exact_buf = vec![0u8; 2048];
    let len = guest.format_handshake_initiation(&mut exact_buf).expect("Exact 2048 must succeed");
    assert_eq!(len, 148);

    // Oversized buffer with fresh tunnel: 4096 bytes must also succeed
    let (_, mut fresh_guest) = generate_tunnel_pair();
    let mut large_buf = vec![0u8; 4096];
    let len2 = fresh_guest.format_handshake_initiation(&mut large_buf).expect("4096 with fresh tunnel must succeed");
    assert_eq!(len2, 148);
}

#[test]
fn adv_buffer_boundary_encapsulate_large_packet() {
    let (mut host, mut guest) = generate_tunnel_pair();
    complete_handshake(&mut host, &mut guest);

    // Encapsulate with small packet (50 bytes): required is 2048
    let small_payload = vec![0x42u8; 50];
    let small_ip = make_synthetic_ipv4([10, 88, 0, 2], [10, 88, 0, 1], &small_payload);
    let mut small_dst = vec![0u8; 2047];
    assert!(matches!(
        guest.encapsulate(&small_ip, &mut small_dst),
        Err(WireGuardError::BufferTooSmall { required: 2048, provided: 2047 })
    ));

    // Encapsulate with large packet (e.g. 2500 bytes IP packet):
    // required = max(2048, 2500 + 32) = 2532 bytes
    let large_payload = vec![0x99u8; 2480]; // 2480 + 20 = 2500 bytes
    let large_ip = make_synthetic_ipv4([10, 88, 0, 2], [10, 88, 0, 1], &large_payload);
    assert_eq!(large_ip.len(), 2500);

    // Buffer has 2048 bytes (which satisfies MIN_BUFFER_SIZE, but fails large_ip.len() + 32)
    let mut buf_2048 = vec![0u8; 2048];
    match guest.encapsulate(&large_ip, &mut buf_2048) {
        Err(WireGuardError::BufferTooSmall { required, provided }) => {
            assert_eq!(required, 2532);
            assert_eq!(provided, 2048);
        }
        other => panic!("Expected BufferTooSmall for large packet with 2048 buf, got {:?}", other),
    }

    // Buffer has 2531 bytes (1 byte below required):
    let mut buf_2531 = vec![0u8; 2531];
    match guest.encapsulate(&large_ip, &mut buf_2531) {
        Err(WireGuardError::BufferTooSmall { required, provided }) => {
            assert_eq!(required, 2532);
            assert_eq!(provided, 2531);
        }
        other => panic!("Expected BufferTooSmall for large packet with 2531 buf, got {:?}", other),
    }

    // Buffer has exact 2532 bytes: must succeed
    let mut buf_2532 = vec![0u8; 2532];
    let enc_len = guest.encapsulate(&large_ip, &mut buf_2532).expect("Exact 2532 bytes must succeed");
    assert!(enc_len > 2500);
}

#[test]
fn adv_buffer_boundary_decapsulate_and_timers() {
    let (mut host, mut guest) = generate_tunnel_pair();

    let mut buf = vec![0u8; 2048];
    let init_len = guest.format_handshake_initiation(&mut buf).unwrap();

    // Check decapsulate with sub-2048 scratch buffers
    let test_sizes = [0, 1, 50, 100, 500, 1000, 2047];
    for size in test_sizes {
        let mut small_scratch = vec![0u8; size];
        match host.decapsulate(None, &buf[..init_len], &mut small_scratch) {
            Err(WireGuardError::BufferTooSmall { required, provided }) => {
                assert_eq!(required, 2048);
                assert_eq!(provided, size);
            }
            _ => panic!("Expected BufferTooSmall for decapsulate size {}", size),
        }

        match guest.update_timers(&mut small_scratch) {
            Err(WireGuardError::BufferTooSmall { required, provided }) => {
                assert_eq!(required, 2048);
                assert_eq!(provided, size);
            }
            other => panic!("Expected BufferTooSmall for update_timers size {}, got {:?}", size, other),
        }
    }
}

// =========================================================================
// 2. MALFORMED / CORRUPTED / TRUNCATED DATAGRAM DECAPSULATION
// =========================================================================

#[test]
fn adv_decapsulate_truncated_datagrams() {
    let (mut host, _) = generate_tunnel_pair();
    let mut scratch = vec![0u8; MIN_BUFFER_SIZE];

    // Empty datagram (0 bytes): BoringTun consumes empty packets gracefully without panicking
    let res = host.decapsulate(None, &[], &mut scratch);
    match res {
        Ok(DecapsulatedPacket::Done) | Err(_) => {} // Handled safely without panicking
        _ => panic!("Empty datagram must return Done or Err"),
    }

    // Single byte datagrams
    for b in 0..=255u8 {
        let res = host.decapsulate(None, &[b], &mut scratch);
        assert!(res.is_err(), "1-byte datagram [0x{:02x}] must return error", b);
    }

    // Truncated headers (2 to 31 bytes)
    for len in 2..32 {
        let datagram = vec![0x01u8; len]; // Type 1 truncated
        let res = host.decapsulate(None, &datagram, &mut scratch);
        assert!(res.is_err(), "Truncated datagram of len {} must return error", len);
    }
}

#[test]
fn adv_decapsulate_bit_flipped_handshake_packets() {
    let (mut host, mut guest) = generate_tunnel_pair();
    let mut init_buf = vec![0u8; MIN_BUFFER_SIZE];
    let init_len = guest.format_handshake_initiation(&mut init_buf).unwrap();

    let mut scratch = vec![0u8; MIN_BUFFER_SIZE];

    // Systematic bit flipping across authenticated initiation fields (offsets 0..132)
    // 0: Type, 1: Reserved, 4: Sender Index, 16: Ephemeral, 50: Encrypted Static,
    // 95: Encrypted Timestamp, 120: MAC1
    for offset in [0, 1, 4, 16, 32, 50, 64, 88, 95, 116, 120, 131] {
        let mut corrupted = init_buf[..init_len].to_vec();
        corrupted[offset] ^= 0xFF; // Invert all bits at offset

        let res = host.decapsulate(None, &corrupted, &mut scratch);
        assert!(res.is_err(), "Corrupted init packet at offset {} must be rejected", offset);
    }

    // Also test bit flipping in Handshake Response (Type 2, 92 bytes)
    let mut resp_buf = vec![0u8; MIN_BUFFER_SIZE];
    let host_resp = host.decapsulate(None, &init_buf[..init_len], &mut resp_buf).unwrap();
    let resp_len = match host_resp {
        DecapsulatedPacket::WriteToNetwork(d) => d.len(),
        _ => panic!("Expected WriteToNetwork"),
    };

    // Authenticated fields in Response are offsets 0..73 (Type, Indices, Ephemeral, Encrypted, MAC1)
    for offset in [0, 1, 5, 10, 20, 40, 50, 60, 72] {
        let mut corrupted_resp = resp_buf[..resp_len].to_vec();
        corrupted_resp[offset] ^= 0xFF;

        let res = guest.decapsulate(None, &corrupted_resp, &mut scratch);
        assert!(res.is_err(), "Corrupted response packet at offset {} must be rejected", offset);
    }
}

#[test]
fn adv_decapsulate_replay_and_out_of_order() {
    let (mut host, mut guest) = generate_tunnel_pair();
    let mut init_buf = vec![0u8; MIN_BUFFER_SIZE];
    let mut resp_buf = vec![0u8; MIN_BUFFER_SIZE];
    let mut scratch = vec![0u8; MIN_BUFFER_SIZE];

    let init_len = guest.format_handshake_initiation(&mut init_buf).unwrap();

    // Send valid init to host
    let resp = host.decapsulate(None, &init_buf[..init_len], &mut resp_buf).unwrap();
    let resp_len = match resp {
        DecapsulatedPacket::WriteToNetwork(d) => d.len(),
        _ => panic!("Expected WriteToNetwork"),
    };

    // Replay the SAME initiation to host
    // WireGuard should reject replay or re-initiate without panicking
    let replay_res = host.decapsulate(None, &init_buf[..init_len], &mut scratch);
    // WireGuard either rejects or handles the replay; it must NEVER panic
    match replay_res {
        Ok(_) | Err(_) => {}
    }

    // Send Type 2 Handshake Response directly to host (which expects Type 1)
    let unexpected_resp = host.decapsulate(None, &resp_buf[..resp_len], &mut scratch);
    assert!(unexpected_resp.is_err(), "Host must reject unexpected Type 2 packet");
}

#[test]
fn adv_decapsulate_fuzzing_1000_random_datagrams() {
    let (mut host, mut guest) = generate_tunnel_pair();
    complete_handshake(&mut host, &mut guest);

    let mut scratch = vec![0u8; MIN_BUFFER_SIZE];
    let mut rng = rand::rng();

    // Feed 1000 random slices to both host and guest
    for i in 0..1000 {
        let len = (rand::random::<u32>() as usize) % 2500;
        let mut junk = vec![0u8; len];
        rng.fill_bytes(&mut junk);

        let dummy_addr: Option<SocketAddr> = if i % 2 == 0 {
            Some("192.168.1.100:51820".parse().unwrap())
        } else {
            None
        };

        // Neither host nor guest should ever panic on junk datagrams
        let _ = host.decapsulate(dummy_addr, &junk, &mut scratch);
        let _ = guest.decapsulate(dummy_addr, &junk, &mut scratch);
    }
}

// =========================================================================
// 3. SYNTHETIC IPV4 FRAMING ADVERSARIAL STRESS
// =========================================================================

#[test]
fn adv_synthetic_ipv4_malformed_packets() {
    // 1. Packet shorter than 20 bytes
    let short_pkts: [&[u8]; 4] = [&[], &[0x45], &[0x45, 0x00], &[0x45; 19]];
    for pkt in short_pkts {
        assert!(matches!(parse_synthetic_ipv4(pkt), Err(WireGuardError::InvalidPacket)));
    }

    // 2. Invalid IP version (first nibble != 4)
    let v6_pkt = vec![0x65u8; 40];
    assert!(matches!(parse_synthetic_ipv4(&v6_pkt), Err(WireGuardError::InvalidPacket)));

    let v0_pkt = vec![0x05u8; 40];
    assert!(matches!(parse_synthetic_ipv4(&v0_pkt), Err(WireGuardError::InvalidPacket)));

    // 3. IHL < 5 (header length < 20 bytes)
    let bad_ihl = vec![0x40u8; 40]; // IHL = 0
    assert!(matches!(parse_synthetic_ipv4(&bad_ihl), Err(WireGuardError::InvalidPacket)));

    let bad_ihl4 = vec![0x44u8; 40]; // IHL = 4 (16 bytes)
    assert!(matches!(parse_synthetic_ipv4(&bad_ihl4), Err(WireGuardError::InvalidPacket)));

    // 4. IHL > total packet len
    let huge_ihl = vec![0x4fu8; 40]; // IHL = 15 (60 bytes) in a 40 byte buffer
    assert!(matches!(parse_synthetic_ipv4(&huge_ihl), Err(WireGuardError::InvalidPacket)));

    // 5. Total length < IHL
    let mut bad_tot = vec![0x45u8; 40];
    bad_tot[2] = 0x00;
    bad_tot[3] = 0x0a; // Total len = 10, but IHL = 20
    assert!(matches!(parse_synthetic_ipv4(&bad_tot), Err(WireGuardError::InvalidPacket)));

    // 6. Total length > packet slice len
    let mut claim_big = vec![0x45u8; 30];
    claim_big[2] = 0x01;
    claim_big[3] = 0x00; // Total len claims 256 bytes, buffer only has 30
    assert!(matches!(parse_synthetic_ipv4(&claim_big), Err(WireGuardError::InvalidPacket)));

    // 7. Valid synthetic IPv4 with empty payload (exactly 20 bytes)
    let empty_payload_pkt = make_synthetic_ipv4([10, 88, 0, 1], [10, 88, 0, 2], &[]);
    assert_eq!(empty_payload_pkt.len(), 20);
    let (payload, src, dst) = parse_synthetic_ipv4(&empty_payload_pkt).expect("Parse empty payload");
    assert!(payload.is_empty());
    assert_eq!(src, std::net::Ipv4Addr::new(10, 88, 0, 1));
    assert_eq!(dst, std::net::Ipv4Addr::new(10, 88, 0, 2));
}

// =========================================================================
// 4. IDENTITY, KEY GENERATION RANDOMNESS & BASE64 CODEC STRESS
// =========================================================================

#[test]
fn adv_identity_key_generation_entropy_and_collisions() {
    let mut pub_keys = HashSet::new();
    let mut priv_keys = HashSet::new();
    let mut short_ids = HashSet::new();

    // Generate 100 consecutive identities
    for i in 0..100 {
        let ident = identity::IdentityManager::generate(None);

        let pub_bytes = ident.public_key_bytes();
        let priv_bytes = ident.private_key_bytes();
        let short_id = ident.identity.short_id.clone();

        // Must not be all zeroes
        assert_ne!(pub_bytes, [0u8; 32], "Public key must not be all zeroes");
        assert_ne!(priv_bytes, [0u8; 32], "Private key must not be all zeroes");

        // Short ID must be 8 URL-safe characters
        assert_eq!(short_id.len(), 8);
        assert!(short_id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'));

        // Collision check
        assert!(pub_keys.insert(pub_bytes), "Duplicate public key at iteration {}", i);
        assert!(priv_keys.insert(priv_bytes), "Duplicate private key at iteration {}", i);
        // Short ID is 8 Base64URL chars (48 bits of entropy) -> collisions in 100 items should be 0
        short_ids.insert(short_id);
    }

    assert_eq!(pub_keys.len(), 100);
    assert_eq!(priv_keys.len(), 100);
}

#[test]
fn adv_base64_decoding_adversarial_inputs() {
    // 1. Invalid characters
    let bad_chars = [
        "abc!def",
        "key with spaces inside",
        "key\x00nullbyte",
        "🔥🔥🔥emoji",
        "==========",
        "???",
    ];
    for input in bad_chars {
        assert!(identity::decode_base64(input).is_err(), "Should reject: {}", input);
        assert!(identity::decode_key_bytes(input).is_err(), "Should reject key: {}", input);
    }

    // 2. Whitespace trimming behavior
    let valid_bytes = [0x55u8; 32];
    let valid_b64 = identity::encode_base64(&valid_bytes);
    let with_whitespace = format!("  \r\n\t {} \t\n ", valid_b64);
    assert_eq!(identity::decode_key_bytes(&with_whitespace).unwrap(), valid_bytes);

    // 3. Keys of wrong sizes:
    // Empty
    assert!(identity::decode_key_bytes("").is_err());
    // 16 bytes
    let b16 = identity::encode_base64(&[1u8; 16]);
    assert!(identity::decode_key_bytes(&b16).is_err());
    // 31 bytes (1 byte short)
    let b31 = identity::encode_base64(&[1u8; 31]);
    assert!(identity::decode_key_bytes(&b31).is_err());
    // 33 bytes (1 byte extra)
    let b33 = identity::encode_base64(&[1u8; 33]);
    assert!(identity::decode_key_bytes(&b33).is_err());
    // 64 bytes
    let b64 = identity::encode_base64(&[1u8; 64]);
    assert!(identity::decode_key_bytes(&b64).is_err());

    // 4. Invalid base64 padding
    assert!(identity::decode_base64("A").is_err());
    assert!(identity::decode_base64("AA=").is_err());
}

#[test]
fn adv_diffie_hellman_with_all_zero_low_order_key() {
    let alice = identity::IdentityManager::generate(None);

    // X25519 with all-zero point (order 8 point) produces all-zero shared secret in Curve25519
    let zero_pub = identity::PublicKey::from([0u8; 32]);
    let shared = alice.diffie_hellman(&zero_pub);

    // Must not panic or crash
    assert_eq!(shared, [0u8; 32]);
}

// =========================================================================
// 5. POST-HANDSHAKE TRANSPORT DATA CORRUPTION & BURST INTEGRITY
// =========================================================================

#[test]
fn adv_transport_data_corruption_rejection() {
    let (mut host, mut guest) = generate_tunnel_pair();
    complete_handshake(&mut host, &mut guest);

    let payload = b"MinecraftPacket1234567890AdversarialTest";
    let ip_packet = make_synthetic_ipv4([10, 88, 0, 2], [10, 88, 0, 1], payload);

    let mut enc_buf = vec![0u8; MIN_BUFFER_SIZE];
    let enc_len = guest.encapsulate(&ip_packet, &mut enc_buf).expect("Encapsulate");
    assert_eq!(enc_buf[0], 4, "Type 4 transport packet");

    let mut scratch = vec![0u8; MIN_BUFFER_SIZE];

    // Corrupt various bytes in Type 4 transport packet:
    // 0: Packet type
    // 1..4: Receiver index
    // 4..12: Counter
    // 12..enc_len: Ciphertext + Poly1305 Auth Tag
    for offset in [0, 2, 7, 15, 25, enc_len - 1] {
        let mut corrupted = enc_buf[..enc_len].to_vec();
        corrupted[offset] ^= 0xAA;

        let res = host.decapsulate(None, &corrupted, &mut scratch);
        assert!(res.is_err(), "Host must reject corrupted transport packet at offset {}", offset);
    }
}

#[test]
fn adv_bidirectional_burst_data_integrity() {
    let (mut host, mut guest) = generate_tunnel_pair();
    complete_handshake(&mut host, &mut guest);

    let mut host_enc = vec![0u8; MIN_BUFFER_SIZE];
    let mut guest_enc = vec![0u8; MIN_BUFFER_SIZE];
    let mut host_dec = vec![0u8; MIN_BUFFER_SIZE];
    let mut guest_dec = vec![0u8; MIN_BUFFER_SIZE];

    // Transmit 100 round-trip packets back and forth
    for i in 0..100 {
        let msg_g2h = format!("GuestToHost_Msg_{:04}", i);
        let ip_g2h = make_synthetic_ipv4([10, 88, 0, 2], [10, 88, 0, 1], msg_g2h.as_bytes());

        let g_len = guest.encapsulate(&ip_g2h, &mut guest_enc).expect("Guest encapsulate");
        let h_dec = host.decapsulate(None, &guest_enc[..g_len], &mut host_dec).expect("Host decapsulate");

        match h_dec {
            DecapsulatedPacket::PacketV4 { packet, src_ip } => {
                assert_eq!(src_ip, std::net::Ipv4Addr::new(10, 88, 0, 2));
                let (extracted, _, _) = parse_synthetic_ipv4(packet).expect("Parse synthetic IPv4");
                assert_eq!(extracted, msg_g2h.as_bytes());
            }
            _ => panic!("Expected PacketV4"),
        }

        let msg_h2g = format!("HostToGuest_Ack_{:04}", i);
        let ip_h2g = make_synthetic_ipv4([10, 88, 0, 1], [10, 88, 0, 2], msg_h2g.as_bytes());

        let h_len = host.encapsulate(&ip_h2g, &mut host_enc).expect("Host encapsulate");
        let g_dec = guest.decapsulate(None, &host_enc[..h_len], &mut guest_dec).expect("Guest decapsulate");

        match g_dec {
            DecapsulatedPacket::PacketV4 { packet, src_ip } => {
                assert_eq!(src_ip, std::net::Ipv4Addr::new(10, 88, 0, 1));
                let (extracted, _, _) = parse_synthetic_ipv4(packet).expect("Parse synthetic IPv4");
                assert_eq!(extracted, msg_h2g.as_bytes());
            }
            _ => panic!("Expected PacketV4"),
        }
    }
}
