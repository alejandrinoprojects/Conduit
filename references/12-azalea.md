# 12. Azalea — Rust Minecraft Protocol & Networking
**Repository:** [azalea-rs/azalea](https://github.com/azalea-rs/azalea)
**Tech Stack:** Rust, Tokio, Async Packet Serialization

## Core Concepts & Architecture
Azalea implements the full Minecraft client/server network protocol in Rust.

### Key Architectural Patterns
1. **Async VarInt & Packet Framing:**
   - Handshake (0x00), Status Request (0x00), and Ping (0x01) packets.
2. **Server List Ping (SLP):**
   - Connects to port 25565, sends Handshake with State=1, sends Status Request, and reads the server's JSON response containing MOTD, online player count, max players, and base64 favicon.

## Conduit Implementation Roadmap
- [x] Zero-overhead server port probing on port 25565 via Tokio TCP stream
- [ ] Direct Server List Ping packet decode for real-time MOTD and player sample without external tools
