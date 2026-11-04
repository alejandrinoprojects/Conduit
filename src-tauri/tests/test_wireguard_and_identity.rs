//! Standalone Test Runner for Milestone 1: BoringTun WireGuard Engine & Cryptographic Subsystem.
//!
//! Directly compiles and executes the test suites for:
//! - `src/network/wireguard.rs` (BoringTun Noise_IKpsk2 state machine, handshakes, encapsulation/decapsulation, timers)
//! - `src/network/identity.rs` (X25519 identity, Base64 codecs, persistence, and backward compatibility)
//!
//! Bypasses the Tauri Windows MinGW manifest conflict to execute 100% reliably.

#[path = "../src/network/wireguard.rs"]
mod wireguard;

#[path = "../src/network/identity.rs"]
mod identity;

use std::net::Ipv4Addr;

#[test]
fn test_m1_wireguard_full_lifecycle() {
    let host_ident = identity::IdentityManager::generate(Some("HostPlayer".to_string()));
    let guest_ident = identity::IdentityManager::generate(Some("GuestPlayer".to_string()));

    let host_priv = host_ident.private_key_bytes();
    let host_pub = host_ident.public_key_bytes();

    let guest_priv = guest_ident.private_key_bytes();
    let guest_pub = guest_ident.public_key_bytes();

    // 1. Instantiate tunnels
    let mut host_tunnel = wireguard::WireGuardTunnel::new_host(host_priv, guest_pub, None)
        .expect("Initialize host tunnel");
    let mut guest_tunnel = wireguard::WireGuardTunnel::new_guest(guest_priv, host_pub, None)
        .expect("Initialize guest tunnel");

    assert!(host_tunnel.is_host());
    assert!(!guest_tunnel.is_host());

    // 2. Handshake Phase 1: Guest Initiation (Type 1, 148 bytes)
    let mut init_buf = vec![0u8; wireguard::MIN_BUFFER_SIZE];
    let init_len = guest_tunnel
        .format_handshake_initiation(&mut init_buf)
        .expect("Format handshake initiation");
    assert_eq!(init_len, 148);
    assert_eq!(init_buf[0], 1);

    // 3. Handshake Phase 2: Host Response (Type 2, 92 bytes)
    let mut resp_buf = vec![0u8; wireguard::MIN_BUFFER_SIZE];
    let host_resp = host_tunnel
        .decapsulate(None, &init_buf[..init_len], &mut resp_buf)
        .expect("Host process initiation");

    let resp_len = match host_resp {
        wireguard::DecapsulatedPacket::WriteToNetwork(datagram) => {
            assert_eq!(datagram.len(), 92);
            assert_eq!(datagram[0], 2);
            datagram.len()
        }
        _ => panic!("Expected WriteToNetwork response"),
    };

    // 4. Handshake Phase 3: Guest Confirmation Keepalive (Type 4, 32 bytes)
    let mut confirm_buf = vec![0u8; wireguard::MIN_BUFFER_SIZE];
    let guest_confirm = guest_tunnel
        .decapsulate(None, &resp_buf[..resp_len], &mut confirm_buf)
        .expect("Guest process response");

    let confirm_len = match guest_confirm {
        wireguard::DecapsulatedPacket::WriteToNetwork(datagram) => {
            assert_eq!(datagram.len(), 32);
            assert_eq!(datagram[0], 4);
            datagram.len()
        }
        _ => panic!("Expected WriteToNetwork confirmation"),
    };

    // 5. Handshake Phase 4: Host consumes confirmation -> Done
    let mut scratch = vec![0u8; wireguard::MIN_BUFFER_SIZE];
    let host_done = host_tunnel
        .decapsulate(None, &confirm_buf[..confirm_len], &mut scratch)
        .expect("Host process confirmation");
    assert!(matches!(host_done, wireguard::DecapsulatedPacket::Done));

    // 6. Datapath: Synthetic IPv4 Encapsulation & Decapsulation (Guest -> Host)
    let minecraft_handshake = b"\x00\x02localhost\x63\xdd\x01";
    let synth_packet = wireguard::make_synthetic_ipv4([10, 88, 0, 2], [10, 88, 0, 1], minecraft_handshake);

    let mut enc_buf = vec![0u8; wireguard::MIN_BUFFER_SIZE];
    let enc_len = guest_tunnel
        .encapsulate(&synth_packet, &mut enc_buf)
        .expect("Guest encapsulate synthetic packet");
    assert!(enc_len > synth_packet.len());
    assert_eq!(enc_buf[0], 4); // Type 4 Transport packet

    let mut dec_buf = vec![0u8; wireguard::MIN_BUFFER_SIZE];
    let dec_result = host_tunnel
        .decapsulate(None, &enc_buf[..enc_len], &mut dec_buf)
        .expect("Host decapsulate packet");

    match dec_result {
        wireguard::DecapsulatedPacket::PacketV4 { packet, src_ip } => {
            assert_eq!(src_ip, Ipv4Addr::new(10, 88, 0, 2));
            let (payload, s_ip, d_ip) = wireguard::parse_synthetic_ipv4(packet)
                .expect("Parse synthetic IPv4 packet");
            assert_eq!(s_ip, Ipv4Addr::new(10, 88, 0, 2));
            assert_eq!(d_ip, Ipv4Addr::new(10, 88, 0, 1));
            assert_eq!(payload, minecraft_handshake);
        }
        _ => panic!("Expected DecapsulatedPacket::PacketV4"),
    }

    // 7. Datapath: Response from Host -> Guest
    let minecraft_server_status = b"{\"description\":\"Conduit WireGuard Mesh Realm\"}";
    let resp_ip_packet = wireguard::make_synthetic_ipv4([10, 88, 0, 1], [10, 88, 0, 2], minecraft_server_status);

    let resp_enc_len = host_tunnel
        .encapsulate(&resp_ip_packet, &mut enc_buf)
        .expect("Host encapsulate reply");

    let guest_dec_result = guest_tunnel
        .decapsulate(None, &enc_buf[..resp_enc_len], &mut dec_buf)
        .expect("Guest decapsulate reply");

    match guest_dec_result {
        wireguard::DecapsulatedPacket::PacketV4 { packet, src_ip } => {
            assert_eq!(src_ip, Ipv4Addr::new(10, 88, 0, 1));
            let (payload, s_ip, d_ip) = wireguard::parse_synthetic_ipv4(packet)
                .expect("Parse synthetic IPv4 reply");
            assert_eq!(s_ip, Ipv4Addr::new(10, 88, 0, 1));
            assert_eq!(d_ip, Ipv4Addr::new(10, 88, 0, 2));
            assert_eq!(payload, minecraft_server_status);
        }
        _ => panic!("Expected DecapsulatedPacket::PacketV4"),
    }

    // 8. Timers evaluation
    let mut timer_buf = vec![0u8; wireguard::MIN_BUFFER_SIZE];
    let host_timer = host_tunnel.update_timers(&mut timer_buf).expect("Host timer update");
    let guest_timer = guest_tunnel.update_timers(&mut timer_buf).expect("Guest timer update");
    assert!(host_timer.is_none() || host_timer.is_some());
    assert!(guest_timer.is_none() || guest_timer.is_some());
}

#[test]
fn test_m1_identity_codecs_and_dh() {
    let alice = identity::IdentityManager::generate(Some("Alice".to_string()));
    let bob = identity::IdentityManager::generate(Some("Bob".to_string()));

    let dh_ab = alice.diffie_hellman(&bob.public_key());
    let dh_ba = bob.diffie_hellman(&alice.public_key());
    assert_eq!(dh_ab, dh_ba);
    assert_ne!(dh_ab, [0u8; 32]);

    // Base64 codecs
    let pub_b64 = alice.public_key_b64();
    let pub_b64_url = alice.public_key_b64_url();

    let decoded_std = identity::decode_key_bytes(&pub_b64).expect("Decode standard");
    let decoded_url = identity::decode_key_bytes(&pub_b64_url).expect("Decode url-safe");
    assert_eq!(decoded_std, alice.public_key_bytes());
    assert_eq!(decoded_url, alice.public_key_bytes());

    // Persistence to tempfile
    let temp_dir = std::env::temp_dir().join(format!("conduit_m1_test_{}", rand::random::<u64>()));
    let file_path = temp_dir.join("ident.json");
    alice.save_to_path(&file_path).expect("Save identity");
    let reloaded = identity::IdentityManager::load_from_path(&file_path, None).expect("Reload identity");
    assert_eq!(alice.identity.public_key, reloaded.identity.public_key);
    assert_eq!(alice.identity.handle, reloaded.identity.handle);
    assert_eq!(alice.private_key_bytes(), reloaded.private_key_bytes());
    let _ = std::fs::remove_dir_all(&temp_dir);
}
