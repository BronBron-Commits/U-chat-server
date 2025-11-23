//! End-to-End Encryption Library for Unhidra
//!
//! This crate provides Signal-protocol-like E2EE with:
//! - X3DH key agreement
//! - Double Ratchet for forward secrecy and break-in recovery
//! - X25519 for key exchange
//! - ChaCha20Poly1305 for symmetric encryption
//!
//! # Example
//!
//! ```rust,ignore
//! use e2ee::{SessionStore, PrekeyBundle};
//!
//! // Alice creates her session store
//! let mut alice_store = SessionStore::new();
//!
//! // Bob creates his session store and shares his bundle
//! let mut bob_store = SessionStore::new();
//! let bob_bundle = bob_store.get_identity_bundle();
//!
//! // Alice initiates a session with Bob
//! let bob_prekey_bundle = PrekeyBundle {
//!     identity_key: bob_bundle.identity_key.clone(),
//!     signed_prekey: bob_bundle.signed_prekey.clone(),
//!     one_time_prekey: bob_bundle.one_time_prekeys.first().map(|(_, k)| k.clone()),
//!     prekey_id: 0,
//! };
//! let initial_msg = alice_store.initiate_session("bob".to_string(), &bob_prekey_bundle)?;
//!
//! // Bob accepts the session
//! bob_store.accept_session("alice".to_string(), &initial_msg, Some(0))?;
//!
//! // Now they can exchange encrypted messages
//! let encrypted = alice_store.encrypt("bob", b"Hello, Bob!")?;
//! let decrypted = bob_store.decrypt("alice", &encrypted)?;
//! ```

pub mod cipher;
pub mod error;
pub mod keys;
pub mod ratchet;
pub mod session;

// Re-export main types
pub use cipher::{derive_key, DerivedKeys, MessageCipher};
pub use error::{E2eeError, Result};
pub use keys::{ExportedKeyPair, IdentityBundle, KeyPair, PrekeyBundle, PublicKeyBytes};
pub use ratchet::{DoubleRatchet, EncryptedMessage, MessageHeader};
pub use session::{IdentityBundlePublic, InitialMessage, Session, SessionStore, X3DH, X3DHResult};

/// Protocol version for compatibility checking
pub const PROTOCOL_VERSION: u32 = 1;

/// Maximum message size (64 KB)
pub const MAX_MESSAGE_SIZE: usize = 64 * 1024;

/// E2EE message envelope for transport
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct E2eeEnvelope {
    /// Protocol version
    pub version: u32,
    /// Sender's identity (for routing)
    pub sender: String,
    /// Recipient's identity
    pub recipient: String,
    /// Message type
    pub message_type: MessageType,
    /// Encrypted payload
    pub payload: String,
    /// Timestamp (Unix seconds)
    pub timestamp: u64,
}

/// Message types in the E2EE protocol
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MessageType {
    /// Session initiation (contains InitialMessage)
    SessionInit,
    /// Regular encrypted message
    Message,
    /// Key refresh notification
    KeyRefresh,
    /// Session close
    SessionClose,
}

impl E2eeEnvelope {
    /// Create a new message envelope
    pub fn new_message(sender: String, recipient: String, encrypted: &EncryptedMessage) -> Self {
        Self {
            version: PROTOCOL_VERSION,
            sender,
            recipient,
            message_type: MessageType::Message,
            payload: encrypted.to_json().unwrap_or_default(),
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
        }
    }

    /// Create a session initiation envelope
    pub fn new_session_init(sender: String, recipient: String, initial: &InitialMessage) -> Self {
        Self {
            version: PROTOCOL_VERSION,
            sender,
            recipient,
            message_type: MessageType::SessionInit,
            payload: serde_json::to_string(initial).unwrap_or_default(),
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
        }
    }

    /// Parse payload as encrypted message
    pub fn parse_message(&self) -> Result<EncryptedMessage> {
        EncryptedMessage::from_json(&self.payload)
    }

    /// Parse payload as initial message
    pub fn parse_initial(&self) -> Result<InitialMessage> {
        serde_json::from_str(&self.payload).map_err(|e| E2eeError::Serialization(e.to_string()))
    }

    /// Serialize envelope to JSON
    pub fn to_json(&self) -> Result<String> {
        serde_json::to_string(self).map_err(|e| E2eeError::Serialization(e.to_string()))
    }

    /// Deserialize envelope from JSON
    pub fn from_json(json: &str) -> Result<Self> {
        serde_json::from_str(json).map_err(|e| E2eeError::Serialization(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_full_e2ee_flow() {
        // Create session stores
        let mut alice_store = SessionStore::new();
        let mut bob_store = SessionStore::new();

        // Get Bob's bundle
        let bob_bundle = bob_store.get_identity_bundle();
        let bob_prekey_bundle = PrekeyBundle {
            identity_key: bob_bundle.identity_key.clone(),
            signed_prekey: bob_bundle.signed_prekey.clone(),
            one_time_prekey: bob_bundle.one_time_prekeys.first().map(|(_, k)| k.clone()),
            prekey_id: 0,
        };

        // Alice initiates
        let initial_msg = alice_store
            .initiate_session("bob".to_string(), &bob_prekey_bundle)
            .unwrap();

        // Create and send session init envelope
        let init_envelope = E2eeEnvelope::new_session_init(
            "alice".to_string(),
            "bob".to_string(),
            &initial_msg,
        );

        // Serialize and deserialize (simulating network transport)
        let json = init_envelope.to_json().unwrap();
        let received_envelope = E2eeEnvelope::from_json(&json).unwrap();

        // Bob accepts
        let parsed_initial = received_envelope.parse_initial().unwrap();
        bob_store
            .accept_session("alice".to_string(), &parsed_initial, Some(0))
            .unwrap();

        // Alice sends message
        let encrypted = alice_store.encrypt("bob", b"Secret message").unwrap();
        let msg_envelope = E2eeEnvelope::new_message(
            "alice".to_string(),
            "bob".to_string(),
            &encrypted,
        );

        // Transport
        let msg_json = msg_envelope.to_json().unwrap();
        let received_msg = E2eeEnvelope::from_json(&msg_json).unwrap();

        // Bob decrypts
        let encrypted_msg = received_msg.parse_message().unwrap();
        let decrypted = bob_store.decrypt("alice", &encrypted_msg).unwrap();

        assert_eq!(decrypted, b"Secret message");
    }

    #[test]
    fn test_envelope_serialization() {
        let mut store = SessionStore::new();
        let bundle = store.get_identity_bundle();
        let prekey = PrekeyBundle {
            identity_key: bundle.identity_key.clone(),
            signed_prekey: bundle.signed_prekey.clone(),
            one_time_prekey: None,
            prekey_id: 0,
        };

        let initial = store.initiate_session("test".to_string(), &prekey).unwrap();
        let envelope = E2eeEnvelope::new_session_init("a".to_string(), "b".to_string(), &initial);

        let json = envelope.to_json().unwrap();
        let parsed = E2eeEnvelope::from_json(&json).unwrap();

        assert_eq!(parsed.version, PROTOCOL_VERSION);
        assert_eq!(parsed.sender, "a");
        assert_eq!(parsed.recipient, "b");
        assert_eq!(parsed.message_type, MessageType::SessionInit);
    }
}
