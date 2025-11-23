//! Double Ratchet implementation
//!
//! Implements the Signal Double Ratchet algorithm for forward secrecy
//! and break-in recovery.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use zeroize::Zeroize;

use crate::{
    cipher::{advance_chain_key, derive_message_key, DerivedKeys, MessageCipher},
    error::{E2eeError, Result},
    keys::{KeyPair, PublicKeyBytes},
};

/// Maximum number of skipped message keys to store
const MAX_SKIP: usize = 1000;

/// Double Ratchet state for a conversation
#[derive(Serialize, Deserialize)]
pub struct DoubleRatchet {
    /// Our current DH key pair
    #[serde(skip)]
    dh_keypair: Option<KeyPair>,
    /// Serialized keypair (for persistence)
    dh_keypair_exported: Option<crate::keys::ExportedKeyPair>,
    /// Their current DH public key
    their_dh_public: Option<PublicKeyBytes>,
    /// Root key
    root_key: [u8; 32],
    /// Sending chain key
    sending_chain_key: Option<[u8; 32]>,
    /// Receiving chain key
    receiving_chain_key: Option<[u8; 32]>,
    /// Number of messages sent in current sending chain
    send_count: u64,
    /// Number of messages received in current receiving chain
    recv_count: u64,
    /// Previous sending chain length (for header)
    prev_chain_length: u64,
    /// Skipped message keys (public key || message number -> key)
    skipped_keys: HashMap<(PublicKeyBytes, u64), [u8; 32]>,
}

impl DoubleRatchet {
    /// Initialize as the session initiator (Alice)
    ///
    /// Call this after completing the X3DH key agreement.
    pub fn init_alice(shared_secret: [u8; 32], their_public: PublicKeyBytes) -> Self {
        let dh_keypair = KeyPair::generate();
        let their_pk = their_public.to_public_key();
        let dh_output = dh_keypair.diffie_hellman(&their_pk);

        let keys = DerivedKeys::derive(&shared_secret, dh_output.as_bytes());

        Self {
            dh_keypair_exported: Some(dh_keypair.export()),
            dh_keypair: Some(dh_keypair),
            their_dh_public: Some(their_public),
            root_key: keys.next_chain_key,
            sending_chain_key: Some(keys.sending_key),
            receiving_chain_key: None,
            send_count: 0,
            recv_count: 0,
            prev_chain_length: 0,
            skipped_keys: HashMap::new(),
        }
    }

    /// Initialize as the session responder (Bob)
    ///
    /// Call this after completing the X3DH key agreement.
    pub fn init_bob(shared_secret: [u8; 32], our_keypair: KeyPair) -> Self {
        Self {
            dh_keypair_exported: Some(our_keypair.export()),
            dh_keypair: Some(our_keypair),
            their_dh_public: None,
            root_key: shared_secret,
            sending_chain_key: None,
            receiving_chain_key: None,
            send_count: 0,
            recv_count: 0,
            prev_chain_length: 0,
            skipped_keys: HashMap::new(),
        }
    }

    /// Encrypt a message
    pub fn encrypt(&mut self, plaintext: &[u8]) -> Result<EncryptedMessage> {
        let dh_keypair = self.dh_keypair.as_ref()
            .ok_or(E2eeError::SessionNotInitialized)?;
        let sending_chain_key = self.sending_chain_key.as_ref()
            .ok_or(E2eeError::SessionNotInitialized)?;

        // Derive message key
        let message_key = derive_message_key(sending_chain_key, self.send_count);

        // Create header
        let header = MessageHeader {
            dh_public: PublicKeyBytes::from_public_key(dh_keypair.public_key()),
            prev_chain_length: self.prev_chain_length,
            message_number: self.send_count,
        };

        // Serialize header as AAD
        let header_bytes = serde_json::to_vec(&header)?;

        // Encrypt
        let cipher = MessageCipher::new(&message_key);
        let ciphertext = cipher.encrypt(plaintext, &header_bytes)?;

        // Advance sending chain
        self.sending_chain_key = Some(advance_chain_key(sending_chain_key));
        self.send_count += 1;

        Ok(EncryptedMessage { header, ciphertext })
    }

    /// Decrypt a message
    pub fn decrypt(&mut self, message: &EncryptedMessage) -> Result<Vec<u8>> {
        // Try skipped keys first
        if let Some(key) = self.skipped_keys.remove(&(
            message.header.dh_public.clone(),
            message.header.message_number,
        )) {
            let header_bytes = serde_json::to_vec(&message.header)?;
            let cipher = MessageCipher::new(&key);
            return cipher.decrypt(&message.ciphertext, &header_bytes);
        }

        // Check if we need to perform a DH ratchet step
        let their_public = message.header.dh_public.clone();
        if self.their_dh_public.as_ref() != Some(&their_public) {
            // Skip any remaining messages in current receiving chain
            self.skip_message_keys(message.header.prev_chain_length)?;

            // Perform DH ratchet
            self.dh_ratchet(&their_public)?;
        }

        // Skip messages if needed
        self.skip_message_keys(message.header.message_number)?;

        // Decrypt with current receiving chain
        let receiving_chain_key = self.receiving_chain_key.as_ref()
            .ok_or(E2eeError::SessionNotInitialized)?;
        let message_key = derive_message_key(receiving_chain_key, self.recv_count);

        let header_bytes = serde_json::to_vec(&message.header)?;
        let cipher = MessageCipher::new(&message_key);
        let plaintext = cipher.decrypt(&message.ciphertext, &header_bytes)?;

        // Advance receiving chain
        self.receiving_chain_key = Some(advance_chain_key(receiving_chain_key));
        self.recv_count += 1;

        Ok(plaintext)
    }

    /// Perform a DH ratchet step
    fn dh_ratchet(&mut self, their_public: &PublicKeyBytes) -> Result<()> {
        let dh_keypair = self.dh_keypair.as_ref()
            .ok_or(E2eeError::SessionNotInitialized)?;

        // Store previous chain length
        self.prev_chain_length = self.send_count;

        // Reset counters
        self.send_count = 0;
        self.recv_count = 0;

        // Update their public key
        self.their_dh_public = Some(their_public.clone());

        // Derive receiving chain key
        let their_pk = their_public.to_public_key();
        let dh_output = dh_keypair.diffie_hellman(&their_pk);
        let keys = DerivedKeys::derive(&self.root_key, dh_output.as_bytes());
        self.root_key = keys.next_chain_key;
        self.receiving_chain_key = Some(keys.receiving_key);

        // Generate new DH key pair
        let new_keypair = KeyPair::generate();
        let new_dh_output = new_keypair.diffie_hellman(&their_pk);
        let new_keys = DerivedKeys::derive(&self.root_key, new_dh_output.as_bytes());

        self.root_key = new_keys.next_chain_key;
        self.sending_chain_key = Some(new_keys.sending_key);
        self.dh_keypair_exported = Some(new_keypair.export());
        self.dh_keypair = Some(new_keypair);

        Ok(())
    }

    /// Skip message keys and store them for out-of-order delivery
    fn skip_message_keys(&mut self, until: u64) -> Result<()> {
        if until > self.recv_count + MAX_SKIP as u64 {
            return Err(E2eeError::OutOfOrder {
                expected: self.recv_count,
                actual: until,
            });
        }

        let receiving_chain_key = match &self.receiving_chain_key {
            Some(key) => key,
            None => return Ok(()), // No receiving chain yet
        };

        let their_public = match &self.their_dh_public {
            Some(pk) => pk.clone(),
            None => return Ok(()),
        };

        let mut chain_key = *receiving_chain_key;
        while self.recv_count < until {
            let message_key = derive_message_key(&chain_key, self.recv_count);
            self.skipped_keys.insert((their_public.clone(), self.recv_count), message_key);
            chain_key = advance_chain_key(&chain_key);
            self.recv_count += 1;

            // Limit stored keys
            if self.skipped_keys.len() > MAX_SKIP {
                // Remove oldest key
                if let Some(oldest) = self.skipped_keys.keys().next().cloned() {
                    self.skipped_keys.remove(&oldest);
                }
            }
        }

        self.receiving_chain_key = Some(chain_key);
        self.recv_count = 0; // Reset for actual message

        Ok(())
    }

    /// Get our current public key for header
    pub fn our_public_key(&self) -> Option<PublicKeyBytes> {
        self.dh_keypair.as_ref()
            .map(|kp| PublicKeyBytes::from_public_key(kp.public_key()))
    }

    /// Restore keypair from exported state (after deserialization)
    pub fn restore_keypair(&mut self) -> Result<()> {
        if self.dh_keypair.is_none() {
            if let Some(exported) = &self.dh_keypair_exported {
                self.dh_keypair = Some(KeyPair::import(exported)?);
            }
        }
        Ok(())
    }
}

impl Drop for DoubleRatchet {
    fn drop(&mut self) {
        self.root_key.zeroize();
        if let Some(key) = &mut self.sending_chain_key {
            key.zeroize();
        }
        if let Some(key) = &mut self.receiving_chain_key {
            key.zeroize();
        }
        for key in self.skipped_keys.values_mut() {
            key.zeroize();
        }
    }
}

/// Message header containing ratchet state
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct MessageHeader {
    /// Sender's current DH public key
    pub dh_public: PublicKeyBytes,
    /// Previous sending chain length
    pub prev_chain_length: u64,
    /// Message number in current chain
    pub message_number: u64,
}

/// Encrypted message with header
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct EncryptedMessage {
    /// Message header (not encrypted, but authenticated)
    pub header: MessageHeader,
    /// Encrypted payload (nonce || ciphertext || tag)
    #[serde(with = "base64_bytes")]
    pub ciphertext: Vec<u8>,
}

impl EncryptedMessage {
    /// Serialize to JSON
    pub fn to_json(&self) -> Result<String> {
        Ok(serde_json::to_string(self)?)
    }

    /// Deserialize from JSON
    pub fn from_json(json: &str) -> Result<Self> {
        Ok(serde_json::from_str(json)?)
    }
}

/// Custom serde module for base64 encoding of bytes
mod base64_bytes {
    use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(bytes: &Vec<u8>, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&BASE64.encode(bytes))
    }

    pub fn deserialize<'de, D>(deserializer: D) -> std::result::Result<Vec<u8>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        BASE64.decode(&s).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_double_ratchet_basic() {
        // Simulate X3DH shared secret
        let shared_secret = [42u8; 32];

        // Bob generates his signed prekey
        let bob_prekey = KeyPair::generate();
        let bob_prekey_public = PublicKeyBytes::from_public_key(bob_prekey.public_key());

        // Alice initializes her ratchet
        let mut alice = DoubleRatchet::init_alice(shared_secret, bob_prekey_public);

        // Bob initializes his ratchet
        let mut bob = DoubleRatchet::init_bob(shared_secret, bob_prekey);

        // Alice sends a message
        let plaintext = b"Hello, Bob!";
        let encrypted = alice.encrypt(plaintext).unwrap();

        // Bob decrypts
        let decrypted = bob.decrypt(&encrypted).unwrap();
        assert_eq!(decrypted, plaintext);

        // Bob replies
        let reply = b"Hello, Alice!";
        let encrypted_reply = bob.encrypt(reply).unwrap();

        // Alice decrypts
        let decrypted_reply = alice.decrypt(&encrypted_reply).unwrap();
        assert_eq!(decrypted_reply, reply);
    }

    #[test]
    fn test_double_ratchet_multiple_messages() {
        let shared_secret = [42u8; 32];
        let bob_prekey = KeyPair::generate();
        let bob_prekey_public = PublicKeyBytes::from_public_key(bob_prekey.public_key());

        let mut alice = DoubleRatchet::init_alice(shared_secret, bob_prekey_public);
        let mut bob = DoubleRatchet::init_bob(shared_secret, bob_prekey);

        // Multiple messages from Alice
        for i in 0..5 {
            let msg = format!("Message {}", i);
            let encrypted = alice.encrypt(msg.as_bytes()).unwrap();
            let decrypted = bob.decrypt(&encrypted).unwrap();
            assert_eq!(decrypted, msg.as_bytes());
        }

        // Multiple replies from Bob
        for i in 0..5 {
            let msg = format!("Reply {}", i);
            let encrypted = bob.encrypt(msg.as_bytes()).unwrap();
            let decrypted = alice.decrypt(&encrypted).unwrap();
            assert_eq!(decrypted, msg.as_bytes());
        }
    }

    #[test]
    fn test_encrypted_message_serialization() {
        let shared_secret = [42u8; 32];
        let bob_prekey = KeyPair::generate();
        let bob_prekey_public = PublicKeyBytes::from_public_key(bob_prekey.public_key());

        let mut alice = DoubleRatchet::init_alice(shared_secret, bob_prekey_public);
        let mut bob = DoubleRatchet::init_bob(shared_secret, bob_prekey);

        let plaintext = b"Test message";
        let encrypted = alice.encrypt(plaintext).unwrap();

        // Serialize and deserialize
        let json = encrypted.to_json().unwrap();
        let deserialized = EncryptedMessage::from_json(&json).unwrap();

        // Bob should still be able to decrypt
        let decrypted = bob.decrypt(&deserialized).unwrap();
        assert_eq!(decrypted, plaintext);
    }
}
