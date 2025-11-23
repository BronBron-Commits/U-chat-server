//! Session management for E2EE conversations
//!
//! Handles X3DH key agreement and session storage.

use std::collections::HashMap;

use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use serde::{Deserialize, Serialize};

use crate::{
    cipher::derive_key,
    error::{E2eeError, Result},
    keys::{KeyPair, PrekeyBundle, PublicKeyBytes},
    ratchet::{DoubleRatchet, EncryptedMessage},
};

/// X3DH protocol implementation
pub struct X3DH;

impl X3DH {
    /// Calculate X3DH shared secret as initiator (Alice)
    ///
    /// Performs:
    /// - DH1 = DH(IK_A, SPK_B)
    /// - DH2 = DH(EK_A, IK_B)
    /// - DH3 = DH(EK_A, SPK_B)
    /// - DH4 = DH(EK_A, OPK_B) (if one-time prekey available)
    /// - SK = KDF(DH1 || DH2 || DH3 || DH4)
    pub fn initiator(
        our_identity: &KeyPair,
        our_ephemeral: &KeyPair,
        their_bundle: &PrekeyBundle,
    ) -> Result<X3DHResult> {
        let their_identity = their_bundle.identity_key_bytes()?;
        let their_identity_pk = x25519_dalek::PublicKey::from(their_identity);

        let their_signed_prekey = their_bundle.signed_prekey_bytes()?;
        let their_signed_prekey_pk = x25519_dalek::PublicKey::from(their_signed_prekey);

        // DH1: IK_A * SPK_B
        let dh1 = our_identity.diffie_hellman(&their_signed_prekey_pk);

        // DH2: EK_A * IK_B
        let dh2 = our_ephemeral.diffie_hellman(&their_identity_pk);

        // DH3: EK_A * SPK_B
        let dh3 = our_ephemeral.diffie_hellman(&their_signed_prekey_pk);

        // Concatenate DH outputs
        let mut dh_concat = Vec::with_capacity(96);
        dh_concat.extend_from_slice(dh1.as_bytes());
        dh_concat.extend_from_slice(dh2.as_bytes());
        dh_concat.extend_from_slice(dh3.as_bytes());

        // DH4: EK_A * OPK_B (optional)
        if let Some(otpk) = &their_bundle.one_time_prekey {
            let otpk_bytes = BASE64
                .decode(otpk)
                .map_err(|e| E2eeError::InvalidKey(e.to_string()))?;
            if otpk_bytes.len() != 32 {
                return Err(E2eeError::InvalidKey("Invalid OPK length".to_string()));
            }
            let mut otpk_arr = [0u8; 32];
            otpk_arr.copy_from_slice(&otpk_bytes);
            let otpk_pk = x25519_dalek::PublicKey::from(otpk_arr);
            let dh4 = our_ephemeral.diffie_hellman(&otpk_pk);
            dh_concat.extend_from_slice(dh4.as_bytes());
        }

        // Derive shared secret using HKDF
        let shared_secret = derive_key(&dh_concat, &[], b"unhidra-x3dh-shared-secret");

        // The associated data for the initial message
        let mut ad = Vec::with_capacity(64);
        ad.extend_from_slice(&our_identity.public_bytes());
        ad.extend_from_slice(&their_identity);

        Ok(X3DHResult {
            shared_secret,
            ephemeral_public: PublicKeyBytes::from_public_key(our_ephemeral.public_key()),
            associated_data: ad,
        })
    }

    /// Calculate X3DH shared secret as responder (Bob)
    pub fn responder(
        our_identity: &KeyPair,
        our_signed_prekey: &KeyPair,
        our_one_time_prekey: Option<&KeyPair>,
        their_identity: &PublicKeyBytes,
        their_ephemeral: &PublicKeyBytes,
    ) -> Result<X3DHResult> {
        let their_identity_pk = their_identity.to_public_key();
        let their_ephemeral_pk = their_ephemeral.to_public_key();

        // DH1: SPK_B * IK_A
        let dh1 = our_signed_prekey.diffie_hellman(&their_identity_pk);

        // DH2: IK_B * EK_A
        let dh2 = our_identity.diffie_hellman(&their_ephemeral_pk);

        // DH3: SPK_B * EK_A
        let dh3 = our_signed_prekey.diffie_hellman(&their_ephemeral_pk);

        // Concatenate DH outputs
        let mut dh_concat = Vec::with_capacity(96);
        dh_concat.extend_from_slice(dh1.as_bytes());
        dh_concat.extend_from_slice(dh2.as_bytes());
        dh_concat.extend_from_slice(dh3.as_bytes());

        // DH4: OPK_B * EK_A (optional)
        if let Some(otpk) = our_one_time_prekey {
            let dh4 = otpk.diffie_hellman(&their_ephemeral_pk);
            dh_concat.extend_from_slice(dh4.as_bytes());
        }

        // Derive shared secret
        let shared_secret = derive_key(&dh_concat, &[], b"unhidra-x3dh-shared-secret");

        // Associated data
        let mut ad = Vec::with_capacity(64);
        ad.extend_from_slice(&their_identity.0);
        ad.extend_from_slice(&our_identity.public_bytes());

        Ok(X3DHResult {
            shared_secret,
            ephemeral_public: their_ephemeral.clone(),
            associated_data: ad,
        })
    }
}

/// Result of X3DH key agreement
pub struct X3DHResult {
    /// Shared secret for initializing Double Ratchet
    pub shared_secret: [u8; 32],
    /// Ephemeral public key (for initiator to send)
    pub ephemeral_public: PublicKeyBytes,
    /// Associated data for initial message
    pub associated_data: Vec<u8>,
}

/// E2EE Session for a conversation with a peer
pub struct Session {
    /// Peer's user ID
    pub peer_id: String,
    /// Double Ratchet state
    ratchet: DoubleRatchet,
    /// Session creation timestamp
    pub created_at: u64,
    /// Last activity timestamp
    pub last_activity: u64,
}

impl Session {
    /// Create session as initiator after X3DH
    pub fn new_initiator(
        peer_id: String,
        x3dh_result: X3DHResult,
        their_signed_prekey: PublicKeyBytes,
    ) -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        Self {
            peer_id,
            ratchet: DoubleRatchet::init_alice(x3dh_result.shared_secret, their_signed_prekey),
            created_at: now,
            last_activity: now,
        }
    }

    /// Create session as responder after X3DH
    pub fn new_responder(
        peer_id: String,
        x3dh_result: X3DHResult,
        our_signed_prekey: KeyPair,
    ) -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        Self {
            peer_id,
            ratchet: DoubleRatchet::init_bob(x3dh_result.shared_secret, our_signed_prekey),
            created_at: now,
            last_activity: now,
        }
    }

    /// Encrypt a message for the peer
    pub fn encrypt(&mut self, plaintext: &[u8]) -> Result<EncryptedMessage> {
        self.touch();
        self.ratchet.encrypt(plaintext)
    }

    /// Decrypt a message from the peer
    pub fn decrypt(&mut self, message: &EncryptedMessage) -> Result<Vec<u8>> {
        self.touch();
        self.ratchet.decrypt(message)
    }

    /// Update last activity timestamp
    fn touch(&mut self) {
        self.last_activity = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
    }

    /// Check if session is stale (no activity for given duration)
    pub fn is_stale(&self, max_age_secs: u64) -> bool {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        now - self.last_activity > max_age_secs
    }
}

/// Session store for managing multiple E2EE sessions
pub struct SessionStore {
    /// Sessions keyed by peer ID
    sessions: HashMap<String, Session>,
    /// Our identity key pair
    identity_keypair: KeyPair,
    /// Our signed prekey
    signed_prekey: KeyPair,
    /// One-time prekeys
    one_time_prekeys: Vec<KeyPair>,
}

impl SessionStore {
    /// Create a new session store with generated keys
    pub fn new() -> Self {
        let identity_keypair = KeyPair::generate();
        let signed_prekey = KeyPair::generate();

        // Generate 100 one-time prekeys
        let one_time_prekeys: Vec<KeyPair> = (0..100)
            .map(|_| KeyPair::generate())
            .collect();

        Self {
            sessions: HashMap::new(),
            identity_keypair,
            signed_prekey,
            one_time_prekeys,
        }
    }

    /// Create from existing identity key
    pub fn with_identity(identity_keypair: KeyPair) -> Self {
        let signed_prekey = KeyPair::generate();
        let one_time_prekeys: Vec<KeyPair> = (0..100)
            .map(|_| KeyPair::generate())
            .collect();

        Self {
            sessions: HashMap::new(),
            identity_keypair,
            signed_prekey,
            one_time_prekeys,
        }
    }

    /// Get our public identity bundle for sharing
    pub fn get_identity_bundle(&self) -> IdentityBundlePublic {
        IdentityBundlePublic {
            identity_key: self.identity_keypair.export_public(),
            signed_prekey: self.signed_prekey.export_public(),
            one_time_prekeys: self.one_time_prekeys
                .iter()
                .enumerate()
                .map(|(i, kp)| (i as u32, kp.export_public()))
                .collect(),
        }
    }

    /// Consume a one-time prekey
    pub fn consume_one_time_prekey(&mut self, index: u32) -> Option<KeyPair> {
        if (index as usize) < self.one_time_prekeys.len() {
            Some(self.one_time_prekeys.remove(index as usize))
        } else {
            None
        }
    }

    /// Initiate a session with a peer
    pub fn initiate_session(&mut self, peer_id: String, their_bundle: &PrekeyBundle) -> Result<InitialMessage> {
        let ephemeral = KeyPair::generate();

        let x3dh_result = X3DH::initiator(
            &self.identity_keypair,
            &ephemeral,
            their_bundle,
        )?;

        let their_signed_prekey = PublicKeyBytes::from_base64(&their_bundle.signed_prekey)?;
        let session = Session::new_initiator(peer_id.clone(), x3dh_result, their_signed_prekey.clone());

        // Store session
        self.sessions.insert(peer_id.clone(), session);

        Ok(InitialMessage {
            identity_key: self.identity_keypair.export_public(),
            ephemeral_key: ephemeral.export_public(),
            prekey_id: their_bundle.prekey_id,
        })
    }

    /// Accept a session from a peer's initial message
    pub fn accept_session(
        &mut self,
        peer_id: String,
        initial_msg: &InitialMessage,
        one_time_prekey_id: Option<u32>,
    ) -> Result<()> {
        let their_identity = PublicKeyBytes::from_base64(&initial_msg.identity_key)?;
        let their_ephemeral = PublicKeyBytes::from_base64(&initial_msg.ephemeral_key)?;

        let one_time_prekey = one_time_prekey_id.and_then(|id| self.consume_one_time_prekey(id));

        let x3dh_result = X3DH::responder(
            &self.identity_keypair,
            &self.signed_prekey,
            one_time_prekey.as_ref(),
            &their_identity,
            &their_ephemeral,
        )?;

        // Clone the signed prekey for the session
        let signed_prekey_clone = KeyPair::from_secret_bytes(&self.signed_prekey.secret_bytes());
        let session = Session::new_responder(peer_id.clone(), x3dh_result, signed_prekey_clone);

        self.sessions.insert(peer_id, session);
        Ok(())
    }

    /// Get a session for a peer
    pub fn get_session(&self, peer_id: &str) -> Option<&Session> {
        self.sessions.get(peer_id)
    }

    /// Get a mutable session for a peer
    pub fn get_session_mut(&mut self, peer_id: &str) -> Option<&mut Session> {
        self.sessions.get_mut(peer_id)
    }

    /// Check if a session exists
    pub fn has_session(&self, peer_id: &str) -> bool {
        self.sessions.contains_key(peer_id)
    }

    /// Remove a session
    pub fn remove_session(&mut self, peer_id: &str) -> Option<Session> {
        self.sessions.remove(peer_id)
    }

    /// Encrypt a message for a peer
    pub fn encrypt(&mut self, peer_id: &str, plaintext: &[u8]) -> Result<EncryptedMessage> {
        let session = self.sessions.get_mut(peer_id)
            .ok_or(E2eeError::SessionNotInitialized)?;
        session.encrypt(plaintext)
    }

    /// Decrypt a message from a peer
    pub fn decrypt(&mut self, peer_id: &str, message: &EncryptedMessage) -> Result<Vec<u8>> {
        let session = self.sessions.get_mut(peer_id)
            .ok_or(E2eeError::SessionNotInitialized)?;
        session.decrypt(message)
    }

    /// Clean up stale sessions
    pub fn cleanup_stale(&mut self, max_age_secs: u64) {
        self.sessions.retain(|_, session| !session.is_stale(max_age_secs));
    }

    /// Get our identity public key
    pub fn identity_public(&self) -> String {
        self.identity_keypair.export_public()
    }
}

impl Default for SessionStore {
    fn default() -> Self {
        Self::new()
    }
}

/// Public identity bundle for sharing
#[derive(Serialize, Deserialize, Clone)]
pub struct IdentityBundlePublic {
    pub identity_key: String,
    pub signed_prekey: String,
    pub one_time_prekeys: Vec<(u32, String)>,
}

/// Initial message for establishing a session
#[derive(Serialize, Deserialize, Clone)]
pub struct InitialMessage {
    pub identity_key: String,
    pub ephemeral_key: String,
    pub prekey_id: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_x3dh_key_agreement() {
        // Alice's keys
        let alice_identity = KeyPair::generate();
        let alice_ephemeral = KeyPair::generate();

        // Bob's keys
        let bob_identity = KeyPair::generate();
        let bob_signed_prekey = KeyPair::generate();
        let bob_one_time_prekey = KeyPair::generate();

        // Bob's prekey bundle
        let bob_bundle = PrekeyBundle {
            identity_key: bob_identity.export_public(),
            signed_prekey: bob_signed_prekey.export_public(),
            one_time_prekey: Some(bob_one_time_prekey.export_public()),
            prekey_id: 0,
        };

        // Alice performs X3DH
        let alice_result = X3DH::initiator(
            &alice_identity,
            &alice_ephemeral,
            &bob_bundle,
        ).unwrap();

        // Bob performs X3DH
        let alice_identity_pk = PublicKeyBytes::from_public_key(alice_identity.public_key());
        let alice_ephemeral_pk = PublicKeyBytes::from_public_key(alice_ephemeral.public_key());

        let bob_result = X3DH::responder(
            &bob_identity,
            &bob_signed_prekey,
            Some(&bob_one_time_prekey),
            &alice_identity_pk,
            &alice_ephemeral_pk,
        ).unwrap();

        // Both should derive the same shared secret
        assert_eq!(alice_result.shared_secret, bob_result.shared_secret);
    }

    #[test]
    fn test_session_store_full_flow() {
        // Create stores for Alice and Bob
        let mut alice_store = SessionStore::new();
        let mut bob_store = SessionStore::new();

        // Get Bob's identity bundle
        let bob_bundle = bob_store.get_identity_bundle();
        let bob_prekey_bundle = PrekeyBundle {
            identity_key: bob_bundle.identity_key.clone(),
            signed_prekey: bob_bundle.signed_prekey.clone(),
            one_time_prekey: bob_bundle.one_time_prekeys.first().map(|(_, k)| k.clone()),
            prekey_id: 0,
        };

        // Alice initiates session
        let initial_msg = alice_store
            .initiate_session("bob".to_string(), &bob_prekey_bundle)
            .unwrap();

        // Bob accepts session
        bob_store
            .accept_session("alice".to_string(), &initial_msg, Some(0))
            .unwrap();

        // Alice encrypts a message
        let plaintext = b"Hello, Bob!";
        let encrypted = alice_store.encrypt("bob", plaintext).unwrap();

        // Bob decrypts
        let decrypted = bob_store.decrypt("alice", &encrypted).unwrap();
        assert_eq!(decrypted, plaintext);

        // Bob replies
        let reply = b"Hello, Alice!";
        let encrypted_reply = bob_store.encrypt("alice", reply).unwrap();

        // Alice decrypts
        let decrypted_reply = alice_store.decrypt("bob", &encrypted_reply).unwrap();
        assert_eq!(decrypted_reply, reply);
    }
}
