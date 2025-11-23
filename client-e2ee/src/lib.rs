//! Client-side E2EE operations
//!
//! Provides a high-level API for managing E2EE sessions on clients,
//! with support for both web (IndexedDB) and native (keychain) storage.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use e2ee::{
    E2eeEnvelope, EncryptedMessage, IdentityBundlePublic, InitialMessage,
    MessageType, PrekeyBundle, SessionStore, PROTOCOL_VERSION,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Client E2EE errors
#[derive(Error, Debug)]
pub enum ClientE2eeError {
    #[error("E2EE error: {0}")]
    E2ee(#[from] e2ee::E2eeError),

    #[error("Session not found for peer: {0}")]
    SessionNotFound(String),

    #[error("Storage error: {0}")]
    Storage(String),

    #[error("Serialization error: {0}")]
    Serialization(String),

    #[error("Protocol version mismatch: expected {expected}, got {actual}")]
    VersionMismatch { expected: u32, actual: u32 },
}

pub type Result<T> = std::result::Result<T, ClientE2eeError>;

/// Client-side E2EE manager
///
/// Thread-safe wrapper around SessionStore with additional
/// client-specific functionality.
pub struct E2eeClient {
    /// Session store
    store: Arc<RwLock<SessionStore>>,
    /// Our user ID
    user_id: String,
    /// Pending session initiations (peer_id -> InitialMessage)
    pending_sessions: Arc<RwLock<HashMap<String, InitialMessage>>>,
}

impl E2eeClient {
    /// Create a new E2EE client
    pub fn new(user_id: String) -> Self {
        Self {
            store: Arc::new(RwLock::new(SessionStore::new())),
            user_id,
            pending_sessions: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Get our identity bundle for sharing
    pub fn get_identity_bundle(&self) -> Result<IdentityBundlePublic> {
        let store = self.store.read().map_err(|e| {
            ClientE2eeError::Storage(format!("Lock error: {}", e))
        })?;
        Ok(store.get_identity_bundle())
    }

    /// Get our identity public key
    pub fn identity_public(&self) -> Result<String> {
        let store = self.store.read().map_err(|e| {
            ClientE2eeError::Storage(format!("Lock error: {}", e))
        })?;
        Ok(store.identity_public())
    }

    /// Initiate a session with a peer
    ///
    /// Returns the session initialization envelope to send to the peer.
    pub fn initiate_session(&self, peer_id: &str, their_bundle: &PrekeyBundle) -> Result<E2eeEnvelope> {
        let mut store = self.store.write().map_err(|e| {
            ClientE2eeError::Storage(format!("Lock error: {}", e))
        })?;

        let initial_msg = store.initiate_session(peer_id.to_string(), their_bundle)?;

        // Store pending session
        let mut pending = self.pending_sessions.write().map_err(|e| {
            ClientE2eeError::Storage(format!("Lock error: {}", e))
        })?;
        pending.insert(peer_id.to_string(), initial_msg.clone());

        Ok(E2eeEnvelope::new_session_init(
            self.user_id.clone(),
            peer_id.to_string(),
            &initial_msg,
        ))
    }

    /// Accept a session from a peer's initialization message
    pub fn accept_session(&self, envelope: &E2eeEnvelope) -> Result<()> {
        if envelope.version != PROTOCOL_VERSION {
            return Err(ClientE2eeError::VersionMismatch {
                expected: PROTOCOL_VERSION,
                actual: envelope.version,
            });
        }

        if envelope.message_type != MessageType::SessionInit {
            return Err(ClientE2eeError::E2ee(e2ee::E2eeError::Serialization(
                "Expected SessionInit message".to_string(),
            )));
        }

        let initial_msg = envelope.parse_initial()?;

        let mut store = self.store.write().map_err(|e| {
            ClientE2eeError::Storage(format!("Lock error: {}", e))
        })?;

        // Accept with one-time prekey if available
        store.accept_session(
            envelope.sender.clone(),
            &initial_msg,
            Some(initial_msg.prekey_id),
        )?;

        Ok(())
    }

    /// Encrypt a message for a peer
    pub fn encrypt(&self, peer_id: &str, plaintext: &[u8]) -> Result<E2eeEnvelope> {
        let mut store = self.store.write().map_err(|e| {
            ClientE2eeError::Storage(format!("Lock error: {}", e))
        })?;

        if !store.has_session(peer_id) {
            return Err(ClientE2eeError::SessionNotFound(peer_id.to_string()));
        }

        let encrypted = store.encrypt(peer_id, plaintext)?;

        Ok(E2eeEnvelope::new_message(
            self.user_id.clone(),
            peer_id.to_string(),
            &encrypted,
        ))
    }

    /// Decrypt a message from a peer
    pub fn decrypt(&self, envelope: &E2eeEnvelope) -> Result<Vec<u8>> {
        if envelope.version != PROTOCOL_VERSION {
            return Err(ClientE2eeError::VersionMismatch {
                expected: PROTOCOL_VERSION,
                actual: envelope.version,
            });
        }

        let mut store = self.store.write().map_err(|e| {
            ClientE2eeError::Storage(format!("Lock error: {}", e))
        })?;

        let peer_id = &envelope.sender;

        if !store.has_session(peer_id) {
            return Err(ClientE2eeError::SessionNotFound(peer_id.to_string()));
        }

        let encrypted = envelope.parse_message()?;
        let plaintext = store.decrypt(peer_id, &encrypted)?;

        Ok(plaintext)
    }

    /// Process an incoming envelope (handles both session init and messages)
    pub fn process_envelope(&self, envelope: &E2eeEnvelope) -> Result<ProcessResult> {
        match envelope.message_type {
            MessageType::SessionInit => {
                self.accept_session(envelope)?;
                Ok(ProcessResult::SessionEstablished {
                    peer_id: envelope.sender.clone(),
                })
            }
            MessageType::Message => {
                let plaintext = self.decrypt(envelope)?;
                Ok(ProcessResult::Message {
                    peer_id: envelope.sender.clone(),
                    plaintext,
                })
            }
            MessageType::KeyRefresh => {
                // Handle key refresh notification
                Ok(ProcessResult::KeyRefresh {
                    peer_id: envelope.sender.clone(),
                })
            }
            MessageType::SessionClose => {
                // Remove the session
                let mut store = self.store.write().map_err(|e| {
                    ClientE2eeError::Storage(format!("Lock error: {}", e))
                })?;
                store.remove_session(&envelope.sender);
                Ok(ProcessResult::SessionClosed {
                    peer_id: envelope.sender.clone(),
                })
            }
        }
    }

    /// Check if we have a session with a peer
    pub fn has_session(&self, peer_id: &str) -> bool {
        self.store
            .read()
            .map(|store| store.has_session(peer_id))
            .unwrap_or(false)
    }

    /// Close a session with a peer
    pub fn close_session(&self, peer_id: &str) -> Result<Option<E2eeEnvelope>> {
        let mut store = self.store.write().map_err(|e| {
            ClientE2eeError::Storage(format!("Lock error: {}", e))
        })?;

        if store.remove_session(peer_id).is_some() {
            Ok(Some(E2eeEnvelope {
                version: PROTOCOL_VERSION,
                sender: self.user_id.clone(),
                recipient: peer_id.to_string(),
                message_type: MessageType::SessionClose,
                payload: String::new(),
                timestamp: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0),
            }))
        } else {
            Ok(None)
        }
    }

    /// Get list of active session peer IDs
    pub fn active_sessions(&self) -> Result<Vec<String>> {
        let store = self.store.read().map_err(|e| {
            ClientE2eeError::Storage(format!("Lock error: {}", e))
        })?;
        // We'd need to add this method to SessionStore, for now return empty
        Ok(Vec::new())
    }

    /// Export client state for persistence
    pub fn export_state(&self) -> Result<ClientState> {
        let store = self.store.read().map_err(|e| {
            ClientE2eeError::Storage(format!("Lock error: {}", e))
        })?;

        Ok(ClientState {
            user_id: self.user_id.clone(),
            identity_public: store.identity_public(),
            // Sessions would need serialization support
        })
    }
}

/// Result of processing an envelope
#[derive(Debug)]
pub enum ProcessResult {
    /// A new session was established
    SessionEstablished { peer_id: String },
    /// A message was decrypted
    Message { peer_id: String, plaintext: Vec<u8> },
    /// Key refresh notification received
    KeyRefresh { peer_id: String },
    /// Session was closed
    SessionClosed { peer_id: String },
}

/// Exported client state for persistence
#[derive(Serialize, Deserialize)]
pub struct ClientState {
    pub user_id: String,
    pub identity_public: String,
}

/// Helper for encrypting JSON-serializable messages
pub fn encrypt_json<T: Serialize>(client: &E2eeClient, peer_id: &str, message: &T) -> Result<E2eeEnvelope> {
    let json = serde_json::to_vec(message)
        .map_err(|e| ClientE2eeError::Serialization(e.to_string()))?;
    client.encrypt(peer_id, &json)
}

/// Helper for decrypting JSON messages
pub fn decrypt_json<'a, T: Deserialize<'a>>(plaintext: &'a [u8]) -> Result<T> {
    serde_json::from_slice(plaintext)
        .map_err(|e| ClientE2eeError::Serialization(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_e2ee_client_full_flow() {
        // Create clients
        let alice = E2eeClient::new("alice".to_string());
        let bob = E2eeClient::new("bob".to_string());

        // Get Bob's bundle
        let bob_bundle = bob.get_identity_bundle().unwrap();
        let bob_prekey = PrekeyBundle {
            identity_key: bob_bundle.identity_key.clone(),
            signed_prekey: bob_bundle.signed_prekey.clone(),
            one_time_prekey: bob_bundle.one_time_prekeys.first().map(|(_, k)| k.clone()),
            prekey_id: 0,
        };

        // Alice initiates session
        let init_envelope = alice.initiate_session("bob", &bob_prekey).unwrap();

        // Bob processes init
        let result = bob.process_envelope(&init_envelope).unwrap();
        assert!(matches!(result, ProcessResult::SessionEstablished { .. }));

        // Alice sends message
        let msg_envelope = alice.encrypt("bob", b"Hello, Bob!").unwrap();

        // Bob decrypts
        let result = bob.process_envelope(&msg_envelope).unwrap();
        match result {
            ProcessResult::Message { plaintext, .. } => {
                assert_eq!(plaintext, b"Hello, Bob!");
            }
            _ => panic!("Expected Message result"),
        }

        // Bob replies
        let reply_envelope = bob.encrypt("alice", b"Hello, Alice!").unwrap();

        // Alice decrypts
        let result = alice.process_envelope(&reply_envelope).unwrap();
        match result {
            ProcessResult::Message { plaintext, .. } => {
                assert_eq!(plaintext, b"Hello, Alice!");
            }
            _ => panic!("Expected Message result"),
        }
    }

    #[test]
    fn test_session_not_found() {
        let client = E2eeClient::new("test".to_string());
        let result = client.encrypt("nonexistent", b"test");
        assert!(matches!(result, Err(ClientE2eeError::SessionNotFound(_))));
    }
}
