//! E2EE Error types
//!
//! This module defines all error types for the E2EE library.

use thiserror::Error;

/// Errors that can occur during E2EE operations
#[derive(Error, Debug)]
pub enum E2eeError {
    /// Key generation failed
    #[error("Key generation failed: {0}")]
    KeyGeneration(String),

    /// Key exchange failed
    #[error("Key exchange failed: {0}")]
    KeyExchange(String),

    /// Encryption failed
    #[error("Encryption failed: {0}")]
    Encryption(String),

    /// Decryption failed
    #[error("Decryption failed: {0}")]
    Decryption(String),

    /// Invalid key format
    #[error("Invalid key format: {0}")]
    InvalidKey(String),

    /// Session not initialized
    #[error("Session not initialized")]
    SessionNotInitialized,

    /// Handshake not complete
    #[error("Handshake not complete")]
    HandshakeIncomplete,

    /// Noise protocol error
    #[error("Noise protocol error: {0}")]
    NoiseError(String),

    /// Serialization error
    #[error("Serialization error: {0}")]
    Serialization(String),

    /// Ratchet state corrupted
    #[error("Ratchet state corrupted: {0}")]
    RatchetCorrupted(String),

    /// Message authentication failed
    #[error("Message authentication failed")]
    AuthenticationFailed,

    /// Replay attack detected
    #[error("Replay attack detected: message {0} already processed")]
    ReplayDetected(u64),

    /// Out of order message
    #[error("Out of order message: expected {expected}, got {actual}")]
    OutOfOrder { expected: u64, actual: u64 },
}

impl From<snow::Error> for E2eeError {
    fn from(e: snow::Error) -> Self {
        E2eeError::NoiseError(e.to_string())
    }
}

impl From<chacha20poly1305::Error> for E2eeError {
    fn from(_: chacha20poly1305::Error) -> Self {
        E2eeError::AuthenticationFailed
    }
}

impl From<serde_json::Error> for E2eeError {
    fn from(e: serde_json::Error) -> Self {
        E2eeError::Serialization(e.to_string())
    }
}

impl From<base64::DecodeError> for E2eeError {
    fn from(e: base64::DecodeError) -> Self {
        E2eeError::InvalidKey(e.to_string())
    }
}

pub type Result<T> = std::result::Result<T, E2eeError>;
