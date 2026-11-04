//! Integration and Unit Tests for Milestone 3:
//! P2P Connectivity, Room Codes, RFC 5389 STUN NAT Traversal, Zero-Knowledge Relay Transport,
//! Automatic 2,500ms Fallback, and Minecraft Multicast LAN Discovery.

mod network {
    #[path = "../../src/network/punch.rs"]
    pub mod punch;

    #[path = "../../src/network/relay.rs"]
    pub mod relay;

    #[path = "../../src/network/lan.rs"]
    pub mod lan;
}

use base64::Engine;
use network::lan::{
    listen_for_lan_games, start_lan_broadcast, LanBeaconHandle, MinecraftLanBeacon,
};
use network::punch::{
    build_stun_request, build_stun_response, discover_endpoints, discover_public_endpoint,
    execute_hole_punch_with_timeout, format_punch_probe, parse_punch_probe, parse_stun_response,
    PunchCandidate, PunchError, PUNCH_BURST_COUNT, PUNCH_MAGIC, PUNCH_MSG_ACK, PUNCH_MSG_PROBE,
    STUN_MAGIC_COOKIE,
};
use network::relay::{
    crc16_ccitt, generate_room_code, parse_short_room_code, FallbackTransport, RelayClient,
    RelayFrame, RoomTicket,
};
use rand::Rng;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::net::UdpSocket;

// =========================================================================
// SECTION 1: RFC 5389 STUN PROTOCOL TESTS (IPv4 & IPv6)
// =========================================================================

#[test]
fn test_stun_binding_request_structure() {
    let mut tx_id = [0u8; 12];
    rand::rng().fill_bytes(&mut tx_id);

    let req = build_stun_request(&tx_id);
    assert_eq!(req.len(), 20);
    // Message Type: 0x0001 (Binding Request)
    assert_eq!(u16::from_be_bytes([req[0], req[1]]), 0x0001);
    // Message Length: 0x0000
    assert_eq!(u16::from_be_bytes([req[2], req[3]]), 0x0000);
    // Magic Cookie: 0x2112A442
    assert_eq!(&req[4..8], &STUN_MAGIC_COOKIE);
    // Transaction ID
    assert_eq!(&req[8..20], &tx_id);
}

#[test]
fn test_stun_xor_mapped_address_ipv4_parsing() {
    let mut tx_id = [0u8; 12];
    rand::rng().fill_bytes(&mut tx_id);

    let expected_endpoint: SocketAddr = "203.0.113.195:54321".parse().unwrap();
    let resp = build_stun_response(&tx_id, expected_endpoint, true);

    let parsed = parse_stun_response(&resp, &tx_id).expect("Should parse IPv4 XOR-MAPPED-ADDRESS");
    assert_eq!(parsed, expected_endpoint);
}

#[test]
fn test_stun_xor_mapped_address_ipv6_parsing() {
    let mut tx_id = [0u8; 12];
    rand::rng().fill_bytes(&mut tx_id);

    let expected_endpoint: SocketAddr = "[2001:db8:85a3::8a2e:370:7334]:51820".parse().unwrap();
    let resp = build_stun_response(&tx_id, expected_endpoint, true);

    let parsed = parse_stun_response(&resp, &tx_id).expect("Should parse IPv6 XOR-MAPPED-ADDRESS");
    assert_eq!(parsed, expected_endpoint);
}

#[test]
fn test_stun_standard_mapped_address_ipv4_parsing() {
    let mut tx_id = [0u8; 12];
    rand::rng().fill_bytes(&mut tx_id);

    let expected_endpoint: SocketAddr = "198.51.100.42:25565".parse().unwrap();
    let resp = build_stun_response(&tx_id, expected_endpoint, false);

    let parsed = parse_stun_response(&resp, &tx_id).expect("Should parse IPv4 MAPPED-ADDRESS");
    assert_eq!(parsed, expected_endpoint);
}

#[test]
fn test_stun_standard_mapped_address_ipv6_parsing() {
    let mut tx_id = [0u8; 12];
    rand::rng().fill_bytes(&mut tx_id);

    let expected_endpoint: SocketAddr = "[fe80::1ff:fe23:4567:890a]:19302".parse().unwrap();
    let resp = build_stun_response(&tx_id, expected_endpoint, false);

    let parsed = parse_stun_response(&resp, &tx_id).expect("Should parse IPv6 MAPPED-ADDRESS");
    assert_eq!(parsed, expected_endpoint);
}

#[test]
fn test_stun_transaction_id_mismatch_rejection() {
    let tx_id_orig = [0x11; 12];
    let tx_id_wrong = [0x22; 12];

    let endpoint: SocketAddr = "1.2.3.4:5678".parse().unwrap();
    let resp = build_stun_response(&tx_id_orig, endpoint, true);

    let err = parse_stun_response(&resp, &tx_id_wrong).unwrap_err();
    assert!(err.to_string().contains("Transaction ID mismatch"));
}

#[test]
fn test_stun_magic_cookie_mismatch_rejection() {
    let tx_id = [0x33; 12];
    let endpoint: SocketAddr = "1.2.3.4:5678".parse().unwrap();
    let mut resp = build_stun_response(&tx_id, endpoint, true);

    // Corrupt magic cookie
    resp[4] ^= 0xFF;

    let err = parse_stun_response(&resp, &tx_id).unwrap_err();
    assert!(err.to_string().contains("Magic Cookie mismatch"));
}

#[test]
fn test_stun_truncated_response_rejection() {
    let tx_id = [0x44; 12];
    let short_resp = [0x01, 0x01, 0x00, 0x04];
    let err = parse_stun_response(&short_resp, &tx_id).unwrap_err();
    assert!(err.to_string().contains("too short"));
}

// =========================================================================
// SECTION 2: ENDPOINT DISCOVERY & CANDIDATES
// =========================================================================

#[tokio::test]
async fn test_candidate_discovery_local_socket() {
    let socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let bound_port = socket.local_addr().unwrap().port();

    let candidate = discover_endpoints(&socket).await.unwrap();
    assert!(candidate.lan.is_some());
    assert_eq!(candidate.lan.unwrap().port(), bound_port);

    let endpoints = candidate.endpoints();
    assert!(!endpoints.is_empty());
    assert!(endpoints.contains(&candidate.lan.unwrap()));

    // Verify discover_public_endpoint returns Option<SocketAddr>
    let _ = discover_public_endpoint(&socket).await;
}

#[test]
fn test_candidate_endpoints_deduplication() {
    let addr: SocketAddr = "192.168.1.10:25565".parse().unwrap();
    let candidate = PunchCandidate::new(Some(addr), Some(addr));
    let endpoints = candidate.endpoints();
    assert_eq!(endpoints.len(), 1);
    assert_eq!(endpoints[0], addr);
}

// =========================================================================
// SECTION 3: 5-PROBE UDP HOLE PUNCHING BURST TESTS
// =========================================================================

#[test]
fn test_hole_punch_probe_formatting_and_parsing() {
    assert_eq!(PUNCH_BURST_COUNT, 5);

    let probe = format_punch_probe(PUNCH_MSG_PROBE, 3);
    assert_eq!(probe.len(), 12);
    assert_eq!(&probe[0..10], PUNCH_MAGIC);
    assert_eq!(probe[10], PUNCH_MSG_PROBE);
    assert_eq!(probe[11], 3);

    let (m_type, seq) = parse_punch_probe(&probe).expect("Probe should parse");
    assert_eq!(m_type, PUNCH_MSG_PROBE);
    assert_eq!(seq, 3);

    let ack = format_punch_probe(PUNCH_MSG_ACK, 4);
    let (ack_type, ack_seq) = parse_punch_probe(&ack).expect("Ack should parse");
    assert_eq!(ack_type, PUNCH_MSG_ACK);
    assert_eq!(ack_seq, 4);
}

#[tokio::test]
async fn test_hole_punch_burst_bidirectional_success() {
    let host_socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let guest_socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();

    let host_addr = host_socket.local_addr().unwrap();
    let guest_addr = guest_socket.local_addr().unwrap();

    let host_candidate = PunchCandidate::new(Some(host_addr), None);
    let guest_candidate = PunchCandidate::new(Some(guest_addr), None);

    // Host responds to incoming probes
    let host_task = tokio::spawn(async move {
        execute_hole_punch_with_timeout(
            &host_socket,
            &guest_candidate,
            Duration::from_millis(1500),
        )
        .await
    });

    // Guest initiates punch burst to host
    let guest_task = tokio::spawn(async move {
        execute_hole_punch_with_timeout(
            &guest_socket,
            &host_candidate,
            Duration::from_millis(1500),
        )
        .await
    });

    let (host_res, guest_res) = tokio::join!(host_task, guest_task);
    let host_selected = host_res.unwrap().expect("Host hole punch should succeed");
    let guest_selected = guest_res.unwrap().expect("Guest hole punch should succeed");

    assert_eq!(host_selected, guest_addr);
    assert_eq!(guest_selected, host_addr);
}

#[tokio::test]
async fn test_hole_punch_timeout_when_peer_unresponsive() {
    let socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let dead_addr: SocketAddr = "127.0.0.1:59999".parse().unwrap();
    let target = PunchCandidate::new(Some(dead_addr), None);

    let start = std::time::Instant::now();
    let res = execute_hole_punch_with_timeout(&socket, &target, Duration::from_millis(100)).await;
    let elapsed = start.elapsed();

    assert!(res.is_err());
    match res.unwrap_err() {
        PunchError::Timeout => {}
        other => panic!("Expected PunchError::Timeout, got: {:?}", other),
    }
    assert!(elapsed >= Duration::from_millis(90));
}

#[tokio::test]
async fn test_hole_punch_no_candidates_error() {
    let socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let target = PunchCandidate::new(None, None);

    let res = execute_hole_punch_with_timeout(&socket, &target, Duration::from_millis(100)).await;
    assert!(res.is_err());
    match res.unwrap_err() {
        PunchError::NoCandidates => {}
        other => panic!("Expected PunchError::NoCandidates, got: {:?}", other),
    }
}

// =========================================================================
// SECTION 4: ROOM TICKETS & SHORT CODES
// =========================================================================

#[test]
fn test_room_ticket_encode_decode_roundtrip() {
    let mut host_pubkey = [0u8; 32];
    rand::rng().fill_bytes(&mut host_pubkey);
    let wan = "203.0.113.88:51820".parse().unwrap();
    let lan = "192.168.1.50:51820".parse().unwrap();
    let flags = 0x0042;

    let ticket = RoomTicket::new(host_pubkey, wan, lan, flags);
    let encoded = ticket.encode();

    assert!(encoded.starts_with("CND-1-"));
    let decoded = RoomTicket::decode(&encoded).expect("Ticket decode should succeed");

    assert_eq!(decoded.version, 1);
    assert_eq!(decoded.host_pubkey, host_pubkey);
    assert_eq!(decoded.wan_endpoint, wan);
    assert_eq!(decoded.lan_endpoint, lan);
    assert_eq!(decoded.relay_flags, flags);
    assert_eq!(decoded, ticket);
}

#[test]
fn test_room_ticket_crc16_integrity_validation() {
    let host_pubkey = [0xAA; 32];
    let wan = "1.1.1.1:80".parse().unwrap();
    let lan = "2.2.2.2:80".parse().unwrap();
    let ticket = RoomTicket::new(host_pubkey, wan, lan, 0);

    let encoded = ticket.encode();

    // Mutate one base64 character in the middle
    let mut chars: Vec<char> = encoded.chars().collect();
    let mid = chars.len() / 2;
    chars[mid] = if chars[mid] == 'A' { 'B' } else { 'A' };
    let corrupted: String = chars.into_iter().collect();

    let err = RoomTicket::decode(&corrupted).unwrap_err();
    assert!(err.to_string().contains("CRC16 checksum mismatch"));
}

#[test]
fn test_room_ticket_missing_prefix_rejection() {
    let err = RoomTicket::decode("NOT-CND-CODE-12345").unwrap_err();
    assert!(err.to_string().contains("missing required 'CND-1-' prefix"));
}

#[test]
fn test_room_ticket_invalid_base64_rejection() {
    let err = RoomTicket::decode("CND-1-!@#$%^&*()").unwrap_err();
    assert!(err.to_string().contains("Base64URL decode error"));
}

#[test]
fn test_room_ticket_truncated_payload_rejection() {
    let short_bytes = vec![0x01, 0x02, 0x03];
    let encoded = format!(
        "CND-1-{}",
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(&short_bytes)
    );
    let err = RoomTicket::decode(&encoded).unwrap_err();
    assert!(err.to_string().contains("too short"));
}

#[test]
fn test_room_ticket_unsupported_version_rejection() {
    let mut raw = vec![0x99]; // Version 0x99
    raw.extend_from_slice(&[0u8; 32]); // pubkey
    raw.extend_from_slice(&[1, 1, 1, 1, 0x1F, 0x90]); // wan
    raw.extend_from_slice(&[2, 2, 2, 2, 0x1F, 0x90]); // lan
    raw.extend_from_slice(&[0, 0]); // flags
    let crc = crc16_ccitt(&raw);
    raw.extend_from_slice(&crc.to_be_bytes());

    let encoded = format!(
        "CND-1-{}",
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(&raw)
    );
    let err = RoomTicket::decode(&encoded).unwrap_err();
    assert!(err.to_string().contains("Unsupported ticket version"));
}

#[test]
fn test_short_room_code_generation_and_parsing() {
    let code_simple = generate_room_code(None);
    assert!(code_simple.starts_with("CND-"));
    let parsed_simple = parse_short_room_code(&code_simple).expect("Should parse simple code");
    assert_eq!(parsed_simple.0.len(), 4);
    assert_eq!(parsed_simple.1, None);

    let code_with_name = generate_room_code(Some("SMP Server"));
    assert!(code_with_name.starts_with("CND-"));
    assert!(code_with_name.ends_with("-SMPSERVER"));
    let parsed_named = parse_short_room_code(&code_with_name).expect("Should parse named code");
    assert_eq!(parsed_named.0.len(), 4);
    assert_eq!(parsed_named.1.unwrap(), "SMPSERVER");
}

// =========================================================================
// SECTION 5: RELAY FRAMING & ZERO-KNOWLEDGE TRANSPORT TESTS
// =========================================================================

#[test]
fn test_relay_frame_register_codec() {
    let sess = [0x55; 16];
    let reg = RelayFrame::encode_register(&sess, 1);
    let (msg_type, sess_opt, role_slice) = RelayFrame::decode(&reg).unwrap();
    assert_eq!(msg_type, RelayFrame::MSG_REGISTER);
    assert_eq!(sess_opt.unwrap(), sess);
    assert_eq!(role_slice, &[1]);
}

#[test]
fn test_relay_frame_data_codec() {
    let sess = [0x77; 16];
    let payload = b"WireGuardType4EncryptedDatagramPayload123";
    let frame = RelayFrame::encode_data(&sess, payload);

    let (msg_type, sess_opt, extracted) = RelayFrame::decode(&frame).unwrap();
    assert_eq!(msg_type, RelayFrame::MSG_DATA);
    assert_eq!(sess_opt.unwrap(), sess);
    assert_eq!(extracted, payload);
}

#[test]
fn test_relay_frame_ping_pong_codec() {
    let ts = 1730721600123u64;
    let ping = RelayFrame::encode_ping(ts);
    let (p_type, _, p_payload) = RelayFrame::decode(&ping).unwrap();
    assert_eq!(p_type, RelayFrame::MSG_PING);
    assert_eq!(u64::from_be_bytes(p_payload.try_into().unwrap()), ts);

    let pong = RelayFrame::encode_pong(ts);
    let (po_type, _, po_payload) = RelayFrame::decode(&pong).unwrap();
    assert_eq!(po_type, RelayFrame::MSG_PONG);
    assert_eq!(u64::from_be_bytes(po_payload.try_into().unwrap()), ts);
}

#[test]
fn test_relay_frame_close_codec() {
    let sess = [0x99; 16];
    let close = RelayFrame::encode_close(&sess);
    let (msg_type, sess_opt, _) = RelayFrame::decode(&close).unwrap();
    assert_eq!(msg_type, RelayFrame::MSG_CLOSE);
    assert_eq!(sess_opt.unwrap(), sess);
}

#[test]
fn test_relay_zero_knowledge_payload_opacity() {
    let mut encrypted_wg_packet = [0u8; 128];
    rand::rng().fill_bytes(&mut encrypted_wg_packet);
    encrypted_wg_packet[0] = 4; // Type 4 Data Packet

    let sess_id = [0xAB; 16];
    let frame = RelayFrame::encode_data(&sess_id, &encrypted_wg_packet);

    let (m_type, dec_sess, payload) = RelayFrame::decode(&frame).unwrap();
    assert_eq!(m_type, RelayFrame::MSG_DATA);
    assert_eq!(dec_sess.unwrap(), sess_id);
    assert_eq!(payload, &encrypted_wg_packet);
}

#[tokio::test]
async fn test_relay_client_forward_and_receive() {
    let relay_socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let relay_addr = relay_socket.local_addr().unwrap();

    let sess_id = [0x33; 16];

    // Client connects to relay
    let client = RelayClient::connect(relay_addr, sess_id, 1).await.unwrap();

    // Verify registration received by relay
    let mut reg_buf = [0u8; 64];
    let (n_reg, client_addr) = relay_socket.recv_from(&mut reg_buf).await.unwrap();
    let (m_type, r_sess, role) = RelayFrame::decode(&reg_buf[..n_reg]).unwrap();
    assert_eq!(m_type, RelayFrame::MSG_REGISTER);
    assert_eq!(r_sess.unwrap(), sess_id);
    assert_eq!(role, &[1]);

    // Client sends datagram
    let datagram = b"WireGuardHandshakeInitiationPacket";
    client.forward_datagram(datagram).await.unwrap();
    assert_eq!(client.bytes_sent(), datagram.len() as u64);
    assert!(!client.is_direct());

    // Relay receives datagram
    let mut data_buf = [0u8; 128];
    let (n_data, _) = relay_socket.recv_from(&mut data_buf).await.unwrap();
    let (m_type, r_sess, payload) = RelayFrame::decode(&data_buf[..n_data]).unwrap();
    assert_eq!(m_type, RelayFrame::MSG_DATA);
    assert_eq!(r_sess.unwrap(), sess_id);
    assert_eq!(payload, datagram);

    // Relay echoes datagram back to client
    let reply_frame = RelayFrame::encode_data(&sess_id, b"WireGuardHandshakeResponsePacket");
    relay_socket.send_to(&reply_frame, client_addr).await.unwrap();

    // Client receives datagram
    let mut recv_buf = [0u8; 128];
    let n_recv = client.recv_datagram(&mut recv_buf).await.unwrap();
    assert_eq!(&recv_buf[..n_recv], b"WireGuardHandshakeResponsePacket");
    assert_eq!(client.bytes_received(), n_recv as u64);
}

#[tokio::test]
async fn test_2500ms_fallback_transport_on_timeout() {
    let local_socket = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());

    // Target is unreachable endpoint to simulate blocked UDP
    let unreachable_target = PunchCandidate::new(Some("127.0.0.1:59998".parse().unwrap()), None);

    // Relay server socket
    let relay_server = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let relay_addr = relay_server.local_addr().unwrap();

    let sess_id = [0x88; 16];

    // Establish fallback transport: direct UDP hole punch will fail and fallback to relay
    let transport = FallbackTransport::establish(
        local_socket,
        &unreachable_target,
        Some(relay_addr),
        sess_id,
        2, // Guest role
    )
    .await
    .expect("Fallback transport should cleanly activate relay");

    assert!(!transport.is_direct());
    assert!(transport.latency_ms() > 0);

    // First packet from connect() is MSG_REGISTER
    let mut reg_buf = [0u8; 128];
    let (n_reg, _) = relay_server.recv_from(&mut reg_buf).await.unwrap();
    let (m_type, r_sess, role) = RelayFrame::decode(&reg_buf[..n_reg]).unwrap();
    assert_eq!(m_type, RelayFrame::MSG_REGISTER);
    assert_eq!(r_sess.unwrap(), sess_id);
    assert_eq!(role, &[2]);

    // Second packet from send_datagram() is MSG_DATA
    let packet = b"PingOverRelayedFallbackRoute";
    transport.send_datagram(packet).await.unwrap();
    assert_eq!(transport.bytes_sent(), packet.len() as u64);

    let mut buf = [0u8; 128];
    let (n, _) = relay_server.recv_from(&mut buf).await.unwrap();
    let (m_type, r_sess, payload) = RelayFrame::decode(&buf[..n]).unwrap();
    assert_eq!(m_type, RelayFrame::MSG_DATA);
    assert_eq!(r_sess.unwrap(), sess_id);
    assert_eq!(payload, packet);
}

// =========================================================================
// SECTION 6: MINECRAFT MULTICAST LAN DISCOVERY TESTS
// =========================================================================

#[test]
fn test_minecraft_lan_beacon_formatting() {
    let motd = "Conduit: Survival SMP";
    let port = 25565;
    let beacon = MinecraftLanBeacon::format(motd, port);
    assert_eq!(beacon, "[MOTD]Conduit: Survival SMP[/MOTD][AD]25565[/AD]");
}

#[test]
fn test_minecraft_lan_beacon_parsing_success() {
    let beacon = "[MOTD]My Minecraft Realm[/MOTD][AD]54321[/AD]";
    let (motd, port) = MinecraftLanBeacon::parse(beacon).expect("Should parse valid beacon");
    assert_eq!(motd, "My Minecraft Realm");
    assert_eq!(port, 54321);
}

#[test]
fn test_minecraft_lan_beacon_parsing_invalid_motd() {
    let malformed = "[MOTD]Missing End Tag[AD]25565[/AD]";
    assert!(MinecraftLanBeacon::parse(malformed).is_none());
}

#[test]
fn test_minecraft_lan_beacon_parsing_invalid_port() {
    let malformed_port = "[MOTD]Valid World[/MOTD][AD]NotANumber[/AD]";
    assert!(MinecraftLanBeacon::parse(malformed_port).is_none());

    let zero_port = "[MOTD]Valid World[/MOTD][AD]0[/AD]";
    assert!(MinecraftLanBeacon::parse(zero_port).is_none());
}

#[test]
fn test_minecraft_lan_beacon_parsing_reversed_tags() {
    let reversed = "[AD]25565[/AD][MOTD]Reversed Tags[/MOTD]";
    assert!(MinecraftLanBeacon::parse(reversed).is_none());
}

#[tokio::test]
async fn test_minecraft_lan_broadcast_and_listener() {
    let listener = match UdpSocket::bind("127.0.0.1:4445").await {
        Ok(s) => s,
        Err(_) => {
            // Port 4445 occupied in test runner environment; skip live multicast bind test
            return;
        }
    };

    let realm_name = "Conduit Automated Test Realm";
    let proxy_port = 25565;
    let mut handle = start_lan_broadcast(realm_name.to_string(), proxy_port);
    assert!(handle.is_active());

    let mut buf = [0u8; 512];
    let res = tokio::time::timeout(Duration::from_millis(2500), listener.recv_from(&mut buf)).await;

    handle.stop();
    assert!(!handle.is_active());

    let (len, _) = res.expect("Should receive LAN beacon").expect("Receive error");
    let msg = std::str::from_utf8(&buf[..len]).unwrap();
    let (motd, port) = MinecraftLanBeacon::parse(msg).expect("Should parse received beacon");
    assert_eq!(motd, realm_name);
    assert_eq!(port, proxy_port);
}

#[tokio::test]
async fn test_lan_discovery_listener_function() {
    // Test the listen_for_lan_games function with a short timeout
    let res = listen_for_lan_games(Duration::from_millis(50)).await;
    assert!(res.is_ok());
}

#[tokio::test]
async fn test_lan_beacon_handle_raii_drop() {
    let is_running = Arc::new(AtomicBool::new(true));
    let is_running_clone = Arc::clone(&is_running);

    let task = tokio::spawn(async move {
        while is_running_clone.load(Ordering::Relaxed) {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    });

    let handle = LanBeaconHandle::new(is_running, task);
    assert!(handle.is_active());

    // Explicit drop triggers RAII cleanup
    drop(handle);

    // Wait a brief tick for task abortion
    tokio::time::sleep(Duration::from_millis(20)).await;
}
