//! Conduit Network Identity & Cryptographic Key Management Subsystem.
//!
//! Provides Curve25519/X25519 static identities, public/private key generation,
//! robust Base64 serialization (supporting standard WireGuard and URL-safe formats),
//! and persistent storage in `.conduit/identity.json`.
//!
//! Uses Cloudflare BoringTun's re-exported X25519 primitives (`boringtun::x25519`).

use base64::engine::general_purpose::{STANDARD, STANDARD_NO_PAD, URL_SAFE, URL_SAFE_NO_PAD};
use base64::Engine;
pub use boringtun::x25519::{PublicKey, StaticSecret};
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Base64 encoding helper for raw bytes using standard WireGuard Base64 with padding.
pub fn encode_base64(bytes: &[u8]) -> String {
    STANDARD.encode(bytes)
}

/// Base64 encoding helper using URL-safe encoding without padding (for tickets and URIs).
pub fn encode_base64_url(bytes: &[u8]) -> String {
    URL_SAFE_NO_PAD.encode(bytes)
}

/// Decodes a Base64 string that may be encoded using standard (padded or unpadded)
/// or URL-safe (padded or unpadded) alphabet.
pub fn decode_base64(s: &str) -> Result<Vec<u8>, String> {
    let trimmed = s.trim();
    if let Ok(b) = STANDARD.decode(trimmed) {
        return Ok(b);
    }
    if let Ok(b) = URL_SAFE_NO_PAD.decode(trimmed) {
        return Ok(b);
    }
    if let Ok(b) = STANDARD_NO_PAD.decode(trimmed) {
        return Ok(b);
    }
    if let Ok(b) = URL_SAFE.decode(trimmed) {
        return Ok(b);
    }
    Err(format!("Failed to decode Base64 string '{}'", s))
}

/// Decodes a 32-byte key from a Base64 string.
pub fn decode_key_bytes(s: &str) -> Result<[u8; 32], String> {
    let bytes = decode_base64(s)?;
    if bytes.len() != 32 {
        return Err(format!(
            "Key must be exactly 32 bytes, got {} bytes",
            bytes.len()
        ));
    }
    let mut arr = [0u8; 32];
    arr.copy_from_slice(&bytes);
    Ok(arr)
}

/// Parses an X25519 PublicKey from Base64.
pub fn public_key_from_base64(s: &str) -> Result<PublicKey, String> {
    let bytes = decode_key_bytes(s)?;
    Ok(PublicKey::from(bytes))
}

/// Parses an X25519 StaticSecret from Base64.
pub fn private_key_from_base64(s: &str) -> Result<StaticSecret, String> {
    let bytes = decode_key_bytes(s)?;
    Ok(StaticSecret::from(bytes))
}

/// Public Conduit player identity visible to peers and the UI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConduitIdentity {
    /// Base64 encoded 32-byte public key.
    pub public_key: String,
    /// First 8 characters of public key for compact display.
    pub short_id: String,
    /// Player handle or alias.
    pub handle: String,
    /// Unix timestamp when identity was created.
    pub created_at: u64,
}

/// Serialized on-disk format for persisting private key and metadata.
#[derive(Serialize, Deserialize)]
struct StoredIdentity {
    pub private_key_bytes: [u8; 32],
    pub handle: String,
    pub created_at: u64,
}

/// Manager managing the local peer's cryptographic identity, persistence, and DH operations.
pub struct IdentityManager {
    secret: StaticSecret,
    pub identity: ConduitIdentity,
}

impl IdentityManager {
    /// Determines storage path (`~/.conduit/identity.json` or `%APPDATA%/.conduit/identity.json`).
    pub fn storage_path() -> PathBuf {
        let base = dirs_fallback().unwrap_or_else(|| PathBuf::from("."));
        let dir = base.join(".conduit");
        let _ = fs::create_dir_all(&dir);
        dir.join("identity.json")
    }

    /// Backward-compatible identity retrieval / initialization.
    pub fn get_or_create_identity() -> Self {
        Self::load_or_create(None)
    }

    /// Loads an existing identity from storage, or generates and saves a new one.
    pub fn load_or_create(default_handle: Option<String>) -> Self {
        let path = Self::storage_path();
        if path.exists() {
            if let Ok(manager) = Self::load_from_path(&path, default_handle.clone()) {
                return manager;
            }
        }

        // Generate a new identity
        let manager = Self::generate(default_handle);
        let _ = manager.save_to_path(&path);
        manager
    }

    /// Generates a fresh ephemeral or initial identity without immediately saving to disk.
    pub fn generate(default_handle: Option<String>) -> Self {
        let mut key_bytes = [0u8; 32];
        rand::rng().fill_bytes(&mut key_bytes);
        let secret = StaticSecret::from(key_bytes);
        let public = PublicKey::from(&secret);

        let pub_b64 = encode_base64_url(public.as_bytes());
        let short: String = pub_b64.chars().take(8).collect();

        let handle = default_handle.unwrap_or_else(|| {
            format!("Player_{}", &short[..4.min(short.len())])
        });

        let created_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        Self {
            secret,
            identity: ConduitIdentity {
                public_key: pub_b64,
                short_id: short,
                handle,
                created_at,
            },
        }
    }

    /// Constructs an `IdentityManager` from raw 32-byte private key bytes and handle.
    pub fn from_private_key_bytes(key_bytes: [u8; 32], handle: Option<String>) -> Self {
        let secret = StaticSecret::from(key_bytes);
        let public = PublicKey::from(&secret);
        let pub_b64 = encode_base64_url(public.as_bytes());
        let short: String = pub_b64.chars().take(8).collect();

        let player_handle = handle.unwrap_or_else(|| {
            format!("Player_{}", &short[..4.min(short.len())])
        });

        let created_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        Self {
            secret,
            identity: ConduitIdentity {
                public_key: pub_b64,
                short_id: short,
                handle: player_handle,
                created_at,
            },
        }
    }

    /// Loads identity from a specific JSON file path.
    pub fn load_from_path(path: &Path, default_handle: Option<String>) -> Result<Self, std::io::Error> {
        let content = fs::read_to_string(path)?;
        let stored: StoredIdentity = serde_json::from_str(&content)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

        let secret = StaticSecret::from(stored.private_key_bytes);
        let public = PublicKey::from(&secret);
        let pub_b64 = encode_base64_url(public.as_bytes());
        let short: String = pub_b64.chars().take(8).collect();

        let handle = if stored.handle.is_empty() {
            default_handle.unwrap_or_else(|| format!("Player_{}", &short[..4.min(short.len())]))
        } else {
            stored.handle
        };

        Ok(Self {
            secret,
            identity: ConduitIdentity {
                public_key: pub_b64,
                short_id: short,
                handle,
                created_at: stored.created_at,
            },
        })
    }

    /// Persists the identity to a specific JSON file path.
    pub fn save_to_path(&self, path: &Path) -> Result<(), std::io::Error> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let stored = StoredIdentity {
            private_key_bytes: self.secret.to_bytes(),
            handle: self.identity.handle.clone(),
            created_at: self.identity.created_at,
        };
        let json = serde_json::to_string_pretty(&stored)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        fs::write(path, json)
    }

    /// Reference to the internal X25519 StaticSecret.
    pub fn secret(&self) -> &StaticSecret {
        &self.secret
    }

    /// The 32 raw bytes of the private key.
    pub fn private_key_bytes(&self) -> [u8; 32] {
        self.secret.to_bytes()
    }

    /// The derived X25519 PublicKey.
    pub fn public_key(&self) -> PublicKey {
        PublicKey::from(&self.secret)
    }

    /// The 32 raw bytes of the public key.
    pub fn public_key_bytes(&self) -> [u8; 32] {
        self.public_key().to_bytes()
    }

    /// Standard Base64 encoded public key string.
    pub fn public_key_b64(&self) -> String {
        encode_base64(&self.public_key_bytes())
    }

    /// URL-Safe Base64 encoded public key string.
    pub fn public_key_b64_url(&self) -> String {
        encode_base64_url(&self.public_key_bytes())
    }

    /// Computes Diffie-Hellman shared secret with a remote peer's public key.
    pub fn diffie_hellman(&self, peer_public_key: &PublicKey) -> [u8; 32] {
        self.secret.diffie_hellman(peer_public_key).to_bytes()
    }
}

fn dirs_fallback() -> Option<PathBuf> {
    if let Ok(appdata) = std::env::var("APPDATA") {
        return Some(PathBuf::from(appdata));
    }
    if let Ok(userprofile) = std::env::var("USERPROFILE") {
        return Some(PathBuf::from(userprofile));
    }
    if let Ok(home) = std::env::var("HOME") {
        return Some(PathBuf::from(home));
    }
    None
}

// =========================================================================
// UNIT TESTS
// =========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_key_generation_and_dh_exchange() {
        let peer_a = IdentityManager::generate(Some("Alice".to_string()));
        let peer_b = IdentityManager::generate(Some("Bob".to_string()));

        assert_eq!(peer_a.identity.handle, "Alice");
        assert_eq!(peer_b.identity.handle, "Bob");
        assert_eq!(peer_a.identity.short_id.len(), 8);
        assert_eq!(peer_b.identity.short_id.len(), 8);

        let dh_a = peer_a.diffie_hellman(&peer_b.public_key());
        let dh_b = peer_b.diffie_hellman(&peer_a.public_key());

        assert_eq!(dh_a, dh_b, "Diffie-Hellman shared secrets must match symmetrically");
        assert_ne!(dh_a, [0u8; 32], "Shared secret must not be all zeroes");
    }

    #[test]
    fn test_base64_roundtrip_standard_and_url_safe() {
        let mut raw_key = [0u8; 32];
        rand::rng().fill_bytes(&mut raw_key);

        let std_b64 = encode_base64(&raw_key);
        let url_b64 = encode_base64_url(&raw_key);

        let decoded_std = decode_base64(&std_b64).expect("Decode standard Base64");
        let decoded_url = decode_base64(&url_b64).expect("Decode URL-safe Base64");

        assert_eq!(decoded_std, raw_key);
        assert_eq!(decoded_url, raw_key);

        let key_std = decode_key_bytes(&std_b64).expect("Decode standard key bytes");
        let key_url = decode_key_bytes(&url_b64).expect("Decode url key bytes");
        assert_eq!(key_std, raw_key);
        assert_eq!(key_url, raw_key);

        let pub_key = public_key_from_base64(&std_b64).expect("Parse public key from std b64");
        assert_eq!(pub_key.as_bytes(), &raw_key);
    }

    #[test]
    fn test_identity_persistence_and_reload() {
        let temp_dir = std::env::temp_dir().join(format!("conduit_test_ident_{}", rand::random::<u64>()));
        let file_path = temp_dir.join("identity.json");

        let original = IdentityManager::generate(Some("PersistencePlayer".to_string()));
        original.save_to_path(&file_path).expect("Save identity to disk");

        assert!(file_path.exists());

        let loaded = IdentityManager::load_from_path(&file_path, None).expect("Load identity from disk");

        assert_eq!(original.identity.public_key, loaded.identity.public_key);
        assert_eq!(original.identity.short_id, loaded.identity.short_id);
        assert_eq!(original.identity.handle, loaded.identity.handle);
        assert_eq!(original.identity.created_at, loaded.identity.created_at);
        assert_eq!(original.private_key_bytes(), loaded.private_key_bytes());

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_get_or_create_identity_backward_compatibility() {
        let ident = IdentityManager::get_or_create_identity();
        assert!(!ident.identity.public_key.is_empty());
        assert_eq!(ident.identity.short_id.len(), 8);
        assert!(!ident.identity.handle.is_empty());
        assert!(ident.identity.created_at > 0);
    }

    #[test]
    fn test_invalid_base64_key_handling() {
        let invalid_base64 = "not-valid-base64!@#$%";
        assert!(decode_base64(invalid_base64).is_err());
        assert!(decode_key_bytes(invalid_base64).is_err());

        // Valid base64, but not 32 bytes (16 bytes)
        let short_bytes = [42u8; 16];
        let short_b64 = encode_base64(&short_bytes);
        assert_eq!(decode_base64(&short_b64).unwrap().len(), 16);
        assert!(decode_key_bytes(&short_b64).is_err());
    }

    #[test]
    fn test_from_private_key_bytes() {
        let raw = [7u8; 32];
        let ident = IdentityManager::from_private_key_bytes(raw, Some("Seven".to_string()));
        assert_eq!(ident.private_key_bytes(), raw);
        assert_eq!(ident.identity.handle, "Seven");
    }
}
