//! Key management for E2EE
//!
//! Handles X25519 key pairs, identity keys, and prekeys.

use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};
use x25519_dalek::{PublicKey, StaticSecret};
use zeroize::Zeroize;

use crate::error::{E2eeError, Result};

/// X25519 key pair for key exchange
#[derive(Clone)]
pub struct KeyPair {
    /// Private key (secret)
    secret: StaticSecret,
    /// Public key
    public: PublicKey,
}

impl KeyPair {
    /// Generate a new random key pair
    pub fn generate() -> Self {
        let secret = StaticSecret::random_from_rng(OsRng);
        let public = PublicKey::from(&secret);
        Self { secret, public }
    }

    /// Create key pair from existing secret bytes
    pub fn from_secret_bytes(bytes: &[u8; 32]) -> Self {
        let secret = StaticSecret::from(*bytes);
        let public = PublicKey::from(&secret);
        Self { secret, public }
    }

    /// Get the public key
    pub fn public_key(&self) -> &PublicKey {
        &self.public
    }

    /// Get the public key bytes
    pub fn public_bytes(&self) -> [u8; 32] {
        self.public.to_bytes()
    }

    /// Get the secret key bytes (use carefully!)
    pub fn secret_bytes(&self) -> [u8; 32] {
        self.secret.to_bytes()
    }

    /// Perform Diffie-Hellman key exchange
    pub fn diffie_hellman(&self, their_public: &PublicKey) -> SharedSecret {
        let shared = self.secret.diffie_hellman(their_public);
        SharedSecret(shared.to_bytes())
    }

    /// Export public key as base64
    pub fn export_public(&self) -> String {
        BASE64.encode(self.public.as_bytes())
    }

    /// Export keypair for storage (encrypted externally)
    pub fn export(&self) -> ExportedKeyPair {
        ExportedKeyPair {
            secret: BASE64.encode(self.secret.to_bytes()),
            public: BASE64.encode(self.public.as_bytes()),
        }
    }

    /// Import keypair from storage
    pub fn import(exported: &ExportedKeyPair) -> Result<Self> {
        let secret_bytes = BASE64
            .decode(&exported.secret)
            .map_err(|e| E2eeError::InvalidKey(e.to_string()))?;
        let public_bytes = BASE64
            .decode(&exported.public)
            .map_err(|e| E2eeError::InvalidKey(e.to_string()))?;

        if secret_bytes.len() != 32 || public_bytes.len() != 32 {
            return Err(E2eeError::InvalidKey("Invalid key length".to_string()));
        }

        let mut secret_arr = [0u8; 32];
        secret_arr.copy_from_slice(&secret_bytes);
        let keypair = Self::from_secret_bytes(&secret_arr);

        // Verify public key matches
        if keypair.public_bytes() != public_bytes.as_slice() {
            return Err(E2eeError::InvalidKey(
                "Public key mismatch".to_string(),
            ));
        }

        Ok(keypair)
    }
}

impl Drop for KeyPair {
    fn drop(&mut self) {
        // Zeroize is handled by x25519_dalek internally
    }
}

/// Shared secret from DH exchange
#[derive(Zeroize)]
#[zeroize(drop)]
pub struct SharedSecret(pub(crate) [u8; 32]);

impl SharedSecret {
    /// Get the raw bytes
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

/// Exported key pair for serialization
#[derive(Serialize, Deserialize, Clone)]
pub struct ExportedKeyPair {
    pub secret: String,
    pub public: String,
}

impl Zeroize for ExportedKeyPair {
    fn zeroize(&mut self) {
        self.secret.zeroize();
        self.public.zeroize();
    }
}

impl Drop for ExportedKeyPair {
    fn drop(&mut self) {
        self.zeroize();
    }
}

/// Identity key bundle for a user
#[derive(Serialize, Deserialize, Clone)]
pub struct IdentityBundle {
    /// Long-term identity key (public)
    pub identity_key: String,
    /// Signed prekey (public)
    pub signed_prekey: String,
    /// Signature of the prekey by identity key
    pub prekey_signature: String,
    /// One-time prekeys (public)
    pub one_time_prekeys: Vec<String>,
}

/// Prekey bundle for initiating sessions
#[derive(Serialize, Deserialize, Clone)]
pub struct PrekeyBundle {
    /// Their identity key (public)
    pub identity_key: String,
    /// Their signed prekey (public)
    pub signed_prekey: String,
    /// One-time prekey used (public)
    pub one_time_prekey: Option<String>,
    /// Prekey ID for tracking
    pub prekey_id: u32,
}

impl PrekeyBundle {
    /// Parse identity key from base64
    pub fn identity_key_bytes(&self) -> Result<[u8; 32]> {
        let bytes = BASE64
            .decode(&self.identity_key)
            .map_err(|e| E2eeError::InvalidKey(e.to_string()))?;
        if bytes.len() != 32 {
            return Err(E2eeError::InvalidKey("Invalid identity key length".to_string()));
        }
        let mut arr = [0u8; 32];
        arr.copy_from_slice(&bytes);
        Ok(arr)
    }

    /// Parse signed prekey from base64
    pub fn signed_prekey_bytes(&self) -> Result<[u8; 32]> {
        let bytes = BASE64
            .decode(&self.signed_prekey)
            .map_err(|e| E2eeError::InvalidKey(e.to_string()))?;
        if bytes.len() != 32 {
            return Err(E2eeError::InvalidKey("Invalid signed prekey length".to_string()));
        }
        let mut arr = [0u8; 32];
        arr.copy_from_slice(&bytes);
        Ok(arr)
    }
}

/// Public key wrapper for serialization
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Hash)]
pub struct PublicKeyBytes(pub [u8; 32]);

impl PublicKeyBytes {
    pub fn from_public_key(pk: &PublicKey) -> Self {
        Self(pk.to_bytes())
    }

    pub fn to_public_key(&self) -> PublicKey {
        PublicKey::from(self.0)
    }

    pub fn to_base64(&self) -> String {
        BASE64.encode(self.0)
    }

    pub fn from_base64(s: &str) -> Result<Self> {
        let bytes = BASE64
            .decode(s)
            .map_err(|e| E2eeError::InvalidKey(e.to_string()))?;
        if bytes.len() != 32 {
            return Err(E2eeError::InvalidKey("Invalid public key length".to_string()));
        }
        let mut arr = [0u8; 32];
        arr.copy_from_slice(&bytes);
        Ok(Self(arr))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_keypair_generation() {
        let kp = KeyPair::generate();
        assert_eq!(kp.public_bytes().len(), 32);
        assert_eq!(kp.secret_bytes().len(), 32);
    }

    #[test]
    fn test_keypair_export_import() {
        let kp = KeyPair::generate();
        let exported = kp.export();
        let imported = KeyPair::import(&exported).unwrap();
        assert_eq!(kp.public_bytes(), imported.public_bytes());
    }

    #[test]
    fn test_diffie_hellman() {
        let alice = KeyPair::generate();
        let bob = KeyPair::generate();

        let alice_shared = alice.diffie_hellman(bob.public_key());
        let bob_shared = bob.diffie_hellman(alice.public_key());

        assert_eq!(alice_shared.as_bytes(), bob_shared.as_bytes());
    }

    #[test]
    fn test_public_key_bytes_roundtrip() {
        let kp = KeyPair::generate();
        let pkb = PublicKeyBytes::from_public_key(kp.public_key());
        let b64 = pkb.to_base64();
        let recovered = PublicKeyBytes::from_base64(&b64).unwrap();
        assert_eq!(pkb, recovered);
    }
}
