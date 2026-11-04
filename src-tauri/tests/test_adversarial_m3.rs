//! Adversarial Stress and Edge Case Test Suite for Milestone 3:
//! STUN RFC 5389 parser bounds, RoomTicket CRC bit-flip coverage, IPv6 panic detection,
//! RelayFrame boundary handling, and Minecraft LAN beacon edge cases.

mod network {
    #[path = "../../src/network/punch.rs"]
    pub mod punch;

    #[path = "../../src/network/relay.rs"]
    pub mod relay;

    #[path = "../../src/network/lan.rs"]
    pub mod lan;
}

use base64::Engine;
use network::lan::{LanBeaconHandle, MinecraftLanBeacon};
use network::punch::{
    build_stun_response, execute_hole_punch_with_timeout, parse_stun_response,
    PunchCandidate, PunchError, STUN_ATTR_XOR_MAPPED_ADDRESS, STUN_BINDING_RESPONSE,
    STUN_MAGIC_COOKIE,
};
use network::relay::{
    parse_short_room_code, RelayClient, RelayFrame, RoomTicket,
};
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::net::UdpSocket;

// =========================================================================
// 1. ADVERSARIAL STUN RFC 5389 PARSER ATTACKS
// =========================================================================

#[test]
fn test_stun_attribute_with_interleaved_unknown_attributes() {
    let tx_id = [0x5A; 12];
    let expected_endpoint: SocketAddr = "192.0.2.146:61234".parse().unwrap();

    let mut resp = Vec::new();
    resp.extend_from_slice(&STUN_BINDING_RESPONSE.to_be_bytes());

    // Construct response with 3 attributes:
    // 1. Unknown attribute 0x8022 (SOFTWARE), len 7 (+ 1 pad = 8 bytes total)
    // 2. XOR-MAPPED-ADDRESS 0x0020, len 8
    // 3. Unknown attribute 0x8028 (FINGERPRINT), len 4
    let total_len: u16 = (4 + 8) + (4 + 8) + (4 + 4);
    resp.extend_from_slice(&total_len.to_be_bytes());
    resp.extend_from_slice(&STUN_MAGIC_COOKIE);
    resp.extend_from_slice(&tx_id);

    // Attr 1: Software "Conduit" (7 bytes + 1 pad)
    resp.extend_from_slice(&0x8022u16.to_be_bytes());
    resp.extend_from_slice(&7u16.to_be_bytes());
    resp.extend_from_slice(b"Conduit\0"); // 8 bytes padded

    // Attr 2: XOR-MAPPED-ADDRESS
    resp.extend_from_slice(&STUN_ATTR_XOR_MAPPED_ADDRESS.to_be_bytes());
    resp.extend_from_slice(&8u16.to_be_bytes());
    resp.push(0x00);
    resp.push(0x01); // IPv4
    let xor_port = 61234u16 ^ u16::from_be_bytes([STUN_MAGIC_COOKIE[0], STUN_MAGIC_COOKIE[1]]);
    resp.extend_from_slice(&xor_port.to_be_bytes());
    resp.push(192 ^ STUN_MAGIC_COOKIE[0]);
    resp.push(0 ^ STUN_MAGIC_COOKIE[1]);
    resp.push(2 ^ STUN_MAGIC_COOKIE[2]);
    resp.push(146 ^ STUN_MAGIC_COOKIE[3]);

    // Attr 3: Fingerprint (4 bytes)
    resp.extend_from_slice(&0x8028u16.to_be_bytes());
    resp.extend_from_slice(&4u16.to_be_bytes());
    resp.extend_from_slice(&[0x12, 0x34, 0x56, 0x78]);

    let parsed = parse_stun_response(&resp, &tx_id).expect("Should skip unknown attributes");
    assert_eq!(parsed, expected_endpoint);
}

#[test]
fn test_stun_attribute_length_exceeds_packet_boundary() {
    let tx_id = [0x5A; 12];
    let mut resp = Vec::new();
    resp.extend_from_slice(&STUN_BINDING_RESPONSE.to_be_bytes());
    resp.extend_from_slice(&20u16.to_be_bytes()); // Claims 20 bytes payload
    resp.extend_from_slice(&STUN_MAGIC_COOKIE);
    resp.extend_from_slice(&tx_id);

    // Attr claiming length 500 when packet only has 4 bytes
    resp.extend_from_slice(&STUN_ATTR_XOR_MAPPED_ADDRESS.to_be_bytes());
    resp.extend_from_slice(&500u16.to_be_bytes());
    resp.extend_from_slice(&[0, 0]);

    let err = parse_stun_response(&resp, &tx_id).unwrap_err();
    assert!(err.to_string().contains("extends beyond response length"));
}

#[test]
fn test_stun_ipv6_boundary_addresses() {
    let tx_id = [0x77; 12];

    // All zeroes IPv6 (::)
    let zero_addr: SocketAddr = "[::]:0".parse().unwrap();
    let resp_zero = build_stun_response(&tx_id, zero_addr, true);
    let parsed_zero = parse_stun_response(&resp_zero, &tx_id).unwrap();
    assert_eq!(parsed_zero, zero_addr);

    // Max IPv6 ([ffff:ffff:ffff:ffff:ffff:ffff:ffff:ffff]:65535)
    let max_addr: SocketAddr = "[ffff:ffff:ffff:ffff:ffff:ffff:ffff:ffff]:65535".parse().unwrap();
    let resp_max = build_stun_response(&tx_id, max_addr, true);
    let parsed_max = parse_stun_response(&resp_max, &tx_id).unwrap();
    assert_eq!(parsed_max, max_addr);
}

#[test]
fn test_stun_unsupported_address_family() {
    let tx_id = [0x99; 12];
    let mut resp = Vec::new();
    resp.extend_from_slice(&STUN_BINDING_RESPONSE.to_be_bytes());
    resp.extend_from_slice(&12u16.to_be_bytes());
    resp.extend_from_slice(&STUN_MAGIC_COOKIE);
    resp.extend_from_slice(&tx_id);

    resp.extend_from_slice(&STUN_ATTR_XOR_MAPPED_ADDRESS.to_be_bytes());
    resp.extend_from_slice(&8u16.to_be_bytes());
    resp.push(0x00);
    resp.push(0x03); // Family 0x03 is invalid in STUN
    resp.extend_from_slice(&[0u8; 6]);

    let res = parse_stun_response(&resp, &tx_id);
    assert!(res.is_err());
}

// =========================================================================
// 2. ROOM TICKET BIT-FLIP INTEGRITY & PANIC AUDIT
// =========================================================================

#[test]
fn test_room_ticket_exhaustive_single_bit_flip_detection() {
    let host_pubkey = [0x42; 32];
    let wan = "198.51.100.1:25565".parse().unwrap();
    let lan = "10.0.0.2:25565".parse().unwrap();
    let ticket = RoomTicket::new(host_pubkey, wan, lan, 1);

    // Extract the raw 49-byte serialized payload (including CRC)
    let encoded = ticket.encode();
    let raw_b64 = &encoded["CND-1-".len()..];
    let original_raw = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(raw_b64)
        .unwrap();

    assert_eq!(original_raw.len(), 49);

    // Mutate every single bit across all 49 bytes (392 bits)
    // Every single bit flip MUST be detected as an error (CRC mismatch or version rejection)
    for byte_idx in 0..49 {
        for bit_idx in 0..8 {
            let mut corrupted = original_raw.clone();
            corrupted[byte_idx] ^= 1 << bit_idx;

            let corrupted_str = format!(
                "CND-1-{}",
                base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(&corrupted)
            );

            let res = RoomTicket::decode(&corrupted_str);
            assert!(
                res.is_err(),
                "Bit flip at byte {}, bit {} was not detected!",
                byte_idx,
                bit_idx
            );
        }
    }
}

#[test]
fn test_room_ticket_ipv6_panic_behavior() {
    let host_pubkey = [0x55; 32];
    let wan_v6: SocketAddr = "[2001:db8::1]:25565".parse().unwrap();
    let lan_v4: SocketAddr = "192.168.1.1:25565".parse().unwrap();

    let ticket = RoomTicket {
        version: 1,
        host_pubkey,
        wan_endpoint: wan_v6,
        lan_endpoint: lan_v4,
        relay_flags: 0,
    };

    // Stress test: RoomTicket::encode() panics on IPv6 addresses
    let panic_res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        ticket.encode();
    }));

    // Document confirmed panic behavior as adversarial finding
    assert!(panic_res.is_err(), "RoomTicket::encode should panic on IPv6 endpoints as currently coded");
}

#[test]
fn test_room_ticket_boundary_ports() {
    let host_pubkey = [0x77; 32];
    let wan: SocketAddr = "0.0.0.0:0".parse().unwrap();
    let lan: SocketAddr = "255.255.255.255:65535".parse().unwrap();

    let ticket = RoomTicket::new(host_pubkey, wan, lan, 0xFFFF);
    let enc = ticket.encode();
    let dec = RoomTicket::decode(&enc).expect("Boundary ports should encode/decode cleanly");

    assert_eq!(dec.wan_endpoint, wan);
    assert_eq!(dec.lan_endpoint, lan);
    assert_eq!(dec.relay_flags, 0xFFFF);
}

#[test]
fn test_short_room_code_adversarial_inputs() {
    // Malformed prefixes
    assert!(parse_short_room_code("").is_none());
    assert!(parse_short_room_code("cnd-abcd").is_none()); // lowercase prefix
    assert!(parse_short_room_code("CND").is_none());
    assert!(parse_short_room_code("CND-").is_none());
    assert!(parse_short_room_code("CND-123").is_none()); // 3 chars
    assert!(parse_short_room_code("CND-12345").is_none()); // 5 chars
    assert!(parse_short_room_code("CND-1234-A-B").is_none()); // 4 parts

    // Valid short codes
    let (code, name) = parse_short_room_code("CND-ABCD").unwrap();
    assert_eq!(code, "ABCD");
    assert_eq!(name, None);

    let (code2, name2) = parse_short_room_code("CND-WXYZ-REALM").unwrap();
    assert_eq!(code2, "WXYZ");
    assert_eq!(name2, Some("REALM".to_string()));
}

// =========================================================================
// 3. RELAY FRAMING STRESS & MTU BOUNDARIES
// =========================================================================

#[test]
fn test_relay_frame_large_payload() {
    let sess = [0xEE; 16];
    let big_payload = vec![0xAB; 65000]; // Near maximum UDP datagram size

    let frame = RelayFrame::encode_data(&sess, &big_payload);
    let (msg_type, sess_opt, extracted) = RelayFrame::decode(&frame).unwrap();

    assert_eq!(msg_type, RelayFrame::MSG_DATA);
    assert_eq!(sess_opt.unwrap(), sess);
    assert_eq!(extracted.len(), 65000);
}

#[test]
fn test_relay_frame_empty_and_unknown_types() {
    assert!(RelayFrame::decode(&[]).is_err());

    // Unknown message type 0xFF
    let unknown = vec![0xFF, 0x01, 0x02, 0x03];
    let err = RelayFrame::decode(&unknown).unwrap_err();
    assert!(err.to_string().contains("Unknown relay message type"));
}

#[tokio::test]
async fn test_relay_client_wrong_session_id_rejection() {
    let relay_socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let relay_addr = relay_socket.local_addr().unwrap();

    let client_sess = [0x11; 16];
    let attacker_sess = [0x22; 16];

    let client = RelayClient::connect(relay_addr, client_sess, 1).await.unwrap();

    // Consume registration
    let mut reg_buf = [0u8; 64];
    let (_, client_addr) = relay_socket.recv_from(&mut reg_buf).await.unwrap();

    // Attacker sends data with wrong session ID
    let rogue_frame = RelayFrame::encode_data(&attacker_sess, b"MaliciousInjection");
    relay_socket.send_to(&rogue_frame, client_addr).await.unwrap();

    // Client must reject wrong session datagram
    let mut buf = [0u8; 128];
    let res = client.recv_datagram(&mut buf).await;
    assert!(res.is_err());
    assert!(res.unwrap_err().to_string().contains("session mismatch"));
}

// =========================================================================
// 4. MINECRAFT LAN DISCOVERY ADVERSARIAL MOTD & PORT TESTING
// =========================================================================

#[test]
fn test_minecraft_lan_beacon_special_characters_and_formatting() {
    // Unicode, emoji, Minecraft formatting section signs (§)
    let complex_motd = "§a§lConduit SMP §r🚀 [1.20.4] <Modded> / PvP & Creative!";
    let port = 25565;

    let beacon = MinecraftLanBeacon::format(complex_motd, port);
    let (parsed_motd, parsed_port) = MinecraftLanBeacon::parse(&beacon).expect("Should parse complex UTF-8 MOTD");

    assert_eq!(parsed_motd, complex_motd);
    assert_eq!(parsed_port, port);
}

#[test]
fn test_minecraft_lan_beacon_empty_motd() {
    let beacon = "[MOTD][/MOTD][AD]25565[/AD]";
    let (motd, port) = MinecraftLanBeacon::parse(beacon).expect("Should parse empty MOTD");
    assert_eq!(motd, "");
    assert_eq!(port, 25565);
}

#[test]
fn test_minecraft_lan_beacon_port_with_surrounding_whitespace() {
    let beacon = "[MOTD]World[/MOTD][AD]   25565   [/AD]";
    let (motd, port) = MinecraftLanBeacon::parse(beacon).expect("Should trim whitespace from port");
    assert_eq!(motd, "World");
    assert_eq!(port, 25565);
}

#[test]
fn test_minecraft_lan_beacon_invalid_ports() {
    // Port out of u16 range
    assert!(MinecraftLanBeacon::parse("[MOTD]W[/MOTD][AD]65536[/AD]").is_none());
    assert!(MinecraftLanBeacon::parse("[MOTD]W[/MOTD][AD]999999[/AD]").is_none());
    assert!(MinecraftLanBeacon::parse("[MOTD]W[/MOTD][AD]-1[/AD]").is_none());
    assert!(MinecraftLanBeacon::parse("[MOTD]W[/MOTD][AD]abc[/AD]").is_none());
    assert!(MinecraftLanBeacon::parse("[MOTD]W[/MOTD][AD][/AD]").is_none());
}

#[tokio::test]
async fn test_lan_beacon_handle_abort_releases_task() {
    let is_running = Arc::new(AtomicBool::new(true));
    let is_running_clone = Arc::clone(&is_running);

    let task = tokio::spawn(async move {
        while is_running_clone.load(Ordering::Relaxed) {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    });

    let mut handle = LanBeaconHandle::new(is_running, task);
    assert!(handle.is_active());

    handle.stop();
    assert!(!handle.is_active());

    // Task is aborted; join should yield a cancelled error
    // Note: since handle.stop() took the task, double stop is safe
    handle.stop();
    assert!(!handle.is_active());
}

// =========================================================================
// 5. HOLE PUNCHING BOUNDARY CONDITIONS
// =========================================================================

#[tokio::test]
async fn test_hole_punch_immediate_return_on_empty_candidates() {
    let socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let empty_candidate = PunchCandidate::new(None, None);

    let start = std::time::Instant::now();
    let res = execute_hole_punch_with_timeout(
        &socket,
        &empty_candidate,
        Duration::from_millis(2500),
    )
    .await;

    let elapsed = start.elapsed();
    assert!(res.is_err());
    match res.unwrap_err() {
        PunchError::NoCandidates => {}
        other => panic!("Expected NoCandidates, got: {:?}", other),
    }
    // Must return immediately without sleeping for 2,500ms
    assert!(elapsed < Duration::from_millis(50));
}
