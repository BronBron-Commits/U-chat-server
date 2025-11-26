//! WebAuthn (Passkey) Authentication Service
//!
//! Implements passwordless authentication using FIDO2/WebAuthn.
//! Supports:
//! - Platform authenticators (Touch ID, Face ID, Windows Hello)
//! - Roaming authenticators (YubiKey, etc.)

use std::collections::HashMap;
use std::sync::Arc;

use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::{error, info, warn};
use webauthn_rs::prelude::*;

/// WebAuthn errors
#[derive(Error, Debug)]
pub enum WebAuthnError {
    #[error("WebAuthn error: {0}")]
    Internal(String),

    #[error("Registration failed: {0}")]
    RegistrationFailed(String),

    #[error("Authentication failed: {0}")]
    AuthenticationFailed(String),

    #[error("Invalid state: {0}")]
    InvalidState(String),

    #[error("Credential not found")]
    CredentialNotFound,

    #[error("User not found: {0}")]
    UserNotFound(String),
}

pub type Result<T> = std::result::Result<T, WebAuthnError>;

/// Stored passkey credential
#[derive(Clone, Serialize, Deserialize)]
pub struct StoredCredential {
    /// Credential ID (base64url encoded)
    pub credential_id: String,
    /// User ID this credential belongs to
    pub user_id: String,
    /// Display name for the credential
    pub name: String,
    /// Credential public key (COSE format, base64url encoded)
    pub public_key: String,
    /// Signature counter (for clone detection)
    pub counter: u32,
    /// Creation timestamp
    pub created_at: u64,
    /// Last used timestamp
    pub last_used: Option<u64>,
    /// Device type hint
    pub device_type: String,
}

/// Pending registration state
#[derive(Clone)]
pub struct PendingRegistration {
    pub user_id: String,
    pub username: String,
    pub state: PasskeyRegistration,
    pub created_at: u64,
}

/// Pending authentication state
#[derive(Clone)]
pub struct PendingAuthentication {
    pub state: PasskeyAuthentication,
    pub created_at: u64,
}

/// WebAuthn service configuration
pub struct WebAuthnConfig {
    /// Relying Party ID (domain)
    pub rp_id: String,
    /// Relying Party name
    pub rp_name: String,
    /// Relying Party origin (full URL)
    pub rp_origin: String,
}

impl WebAuthnConfig {
    /// Load from environment
    pub fn from_env() -> Self {
        Self {
            rp_id: std::env::var("WEBAUTHN_RP_ID")
                .unwrap_or_else(|_| "unhidra.local".to_string()),
            rp_name: std::env::var("WEBAUTHN_RP_NAME")
                .unwrap_or_else(|_| "Unhidra".to_string()),
            rp_origin: std::env::var("WEBAUTHN_RP_ORIGIN")
                .unwrap_or_else(|_| "https://unhidra.local".to_string()),
        }
    }
}

/// WebAuthn authentication service
pub struct WebAuthnService {
    /// WebAuthn instance
    webauthn: Webauthn,
    /// Pending registrations (challenge -> state)
    pending_registrations: DashMap<String, PendingRegistration>,
    /// Pending authentications (challenge -> state)
    pending_authentications: DashMap<String, PendingAuthentication>,
    /// Stored credentials (user_id -> Vec<credentials>)
    /// In production, this should be backed by a database
    credentials: DashMap<String, Vec<StoredCredential>>,
}

impl WebAuthnService {
    /// Create a new WebAuthn service
    pub fn new(config: WebAuthnConfig) -> Result<Self> {
        let rp_id = config.rp_id.clone();
        let rp_origin = Url::parse(&config.rp_origin)
            .map_err(|e| WebAuthnError::Internal(format!("Invalid RP origin: {}", e)))?;

        let builder = WebauthnBuilder::new(&rp_id, &rp_origin)
            .map_err(|e| WebAuthnError::Internal(e.to_string()))?
            .rp_name(&config.rp_name);

        let webauthn = builder
            .build()
            .map_err(|e| WebAuthnError::Internal(e.to_string()))?;

        Ok(Self {
            webauthn,
            pending_registrations: DashMap::new(),
            pending_authentications: DashMap::new(),
            credentials: DashMap::new(),
        })
    }

    /// Create with default configuration from environment
    pub fn from_env() -> Result<Self> {
        Self::new(WebAuthnConfig::from_env())
    }

    /// Start passkey registration for a user
    pub fn start_registration(
        &self,
        user_id: &str,
        username: &str,
        display_name: &str,
    ) -> Result<(CreationChallengeResponse, String)> {
        // Get existing credentials for this user (to exclude)
        let exclude_credentials: Vec<CredentialID> = self
            .credentials
            .get(user_id)
            .map(|creds| {
                creds
                    .iter()
                    .filter_map(|c| {
                        base64_url_decode(&c.credential_id)
                            .map(|id| CredentialID::from(id))
                            .ok()
                    })
                    .collect()
            })
            .unwrap_or_default();

        // Create user entity
        let user_unique_id = uuid::Uuid::new_v4().as_bytes().to_vec();

        let (ccr, reg_state) = self
            .webauthn
            .start_passkey_registration(
                Uuid::new_v4(),
                username,
                display_name,
                Some(exclude_credentials),
            )
            .map_err(|e| WebAuthnError::RegistrationFailed(e.to_string()))?;

        // Store pending registration
        let challenge = base64_url_encode(ccr.public_key.challenge.0.as_slice());
        let pending = PendingRegistration {
            user_id: user_id.to_string(),
            username: username.to_string(),
            state: reg_state,
            created_at: now(),
        };
        self.pending_registrations.insert(challenge.clone(), pending);

        info!(user_id = user_id, "Started passkey registration");

        Ok((ccr, challenge))
    }

    /// Complete passkey registration
    pub fn complete_registration(
        &self,
        challenge: &str,
        response: RegisterPublicKeyCredential,
        device_name: &str,
    ) -> Result<StoredCredential> {
        // Get pending registration
        let pending = self
            .pending_registrations
            .remove(challenge)
            .map(|(_, v)| v)
            .ok_or_else(|| WebAuthnError::InvalidState("Unknown challenge".to_string()))?;

        // Check expiration (5 minute timeout)
        if now() - pending.created_at > 300 {
            return Err(WebAuthnError::InvalidState("Registration expired".to_string()));
        }

        // Complete registration
        let passkey = self
            .webauthn
            .finish_passkey_registration(&response, &pending.state)
            .map_err(|e| WebAuthnError::RegistrationFailed(e.to_string()))?;

        // Create stored credential
        let credential = StoredCredential {
            credential_id: base64_url_encode(passkey.cred_id().as_slice()),
            user_id: pending.user_id.clone(),
            name: device_name.to_string(),
            public_key: base64_url_encode(&serde_json::to_vec(&passkey).unwrap_or_default()),
            counter: 0,
            created_at: now(),
            last_used: None,
            device_type: "passkey".to_string(),
        };

        // Store credential
        self.credentials
            .entry(pending.user_id.clone())
            .or_default()
            .push(credential.clone());

        info!(
            user_id = pending.user_id,
            credential_id = credential.credential_id,
            "Passkey registration completed"
        );

        Ok(credential)
    }

    /// Start passkey authentication
    pub fn start_authentication(&self, user_id: Option<&str>) -> Result<(RequestChallengeResponse, String)> {
        // Get allowed credentials if user is specified
        let allowed_credentials: Vec<Passkey> = if let Some(uid) = user_id {
            self.credentials
                .get(uid)
                .map(|creds| {
                    creds
                        .iter()
                        .filter_map(|c| {
                            let pk_bytes = base64_url_decode(&c.public_key).ok()?;
                            serde_json::from_slice(&pk_bytes).ok()
                        })
                        .collect()
                })
                .unwrap_or_default()
        } else {
            // Discoverable credentials (resident keys) - allow any
            vec![]
        };

        if user_id.is_some() && allowed_credentials.is_empty() {
            return Err(WebAuthnError::CredentialNotFound);
        }

        let (rcr, auth_state) = if allowed_credentials.is_empty() {
            // Discoverable credentials flow
            self.webauthn
                .start_discoverable_authentication()
                .map_err(|e| WebAuthnError::AuthenticationFailed(e.to_string()))?
        } else {
            self.webauthn
                .start_passkey_authentication(&allowed_credentials)
                .map_err(|e| WebAuthnError::AuthenticationFailed(e.to_string()))?
        };

        // Store pending authentication
        let challenge = base64_url_encode(rcr.public_key.challenge.0.as_slice());
        let pending = PendingAuthentication {
            state: auth_state,
            created_at: now(),
        };
        self.pending_authentications.insert(challenge.clone(), pending);

        info!(user_id = ?user_id, "Started passkey authentication");

        Ok((rcr, challenge))
    }

    /// Complete passkey authentication
    pub fn complete_authentication(
        &self,
        challenge: &str,
        response: PublicKeyCredential,
    ) -> Result<String> {
        // Get pending authentication
        let pending = self
            .pending_authentications
            .remove(challenge)
            .map(|(_, v)| v)
            .ok_or_else(|| WebAuthnError::InvalidState("Unknown challenge".to_string()))?;

        // Check expiration
        if now() - pending.created_at > 300 {
            return Err(WebAuthnError::InvalidState("Authentication expired".to_string()));
        }

        // Find the credential
        let cred_id = base64_url_encode(response.id.as_slice());
        let mut authenticated_user_id = None;

        for entry in self.credentials.iter() {
            for cred in entry.value().iter() {
                if cred.credential_id == cred_id {
                    authenticated_user_id = Some(entry.key().clone());
                    break;
                }
            }
            if authenticated_user_id.is_some() {
                break;
            }
        }

        let user_id = authenticated_user_id
            .ok_or_else(|| WebAuthnError::CredentialNotFound)?;

        // Get the passkey for verification
        let passkey: Passkey = self
            .credentials
            .get(&user_id)
            .and_then(|creds| {
                creds.iter().find(|c| c.credential_id == cred_id).and_then(|c| {
                    let pk_bytes = base64_url_decode(&c.public_key).ok()?;
                    serde_json::from_slice(&pk_bytes).ok()
                })
            })
            .ok_or_else(|| WebAuthnError::CredentialNotFound)?;

        // Verify authentication
        let auth_result = self
            .webauthn
            .finish_passkey_authentication(&response, &pending.state)
            .map_err(|e| WebAuthnError::AuthenticationFailed(e.to_string()))?;

        // Update counter and last_used
        if let Some(mut creds) = self.credentials.get_mut(&user_id) {
            for cred in creds.iter_mut() {
                if cred.credential_id == cred_id {
                    cred.counter = auth_result.counter();
                    cred.last_used = Some(now());
                    break;
                }
            }
        }

        info!(
            user_id = user_id,
            credential_id = cred_id,
            "Passkey authentication completed"
        );

        Ok(user_id)
    }

    /// List credentials for a user
    pub fn list_credentials(&self, user_id: &str) -> Vec<CredentialInfo> {
        self.credentials
            .get(user_id)
            .map(|creds| {
                creds
                    .iter()
                    .map(|c| CredentialInfo {
                        credential_id: c.credential_id.clone(),
                        name: c.name.clone(),
                        device_type: c.device_type.clone(),
                        created_at: c.created_at,
                        last_used: c.last_used,
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Revoke a credential
    pub fn revoke_credential(&self, user_id: &str, credential_id: &str) -> Result<()> {
        if let Some(mut creds) = self.credentials.get_mut(user_id) {
            let initial_len = creds.len();
            creds.retain(|c| c.credential_id != credential_id);
            if creds.len() < initial_len {
                info!(user_id = user_id, credential_id = credential_id, "Passkey revoked");
                return Ok(());
            }
        }
        Err(WebAuthnError::CredentialNotFound)
    }

    /// Check if user has any passkeys registered
    pub fn has_passkeys(&self, user_id: &str) -> bool {
        self.credentials
            .get(user_id)
            .map(|creds| !creds.is_empty())
            .unwrap_or(false)
    }

    /// Clean up expired pending states
    pub fn cleanup_expired(&self) {
        let current = now();
        self.pending_registrations.retain(|_, v| current - v.created_at < 300);
        self.pending_authentications.retain(|_, v| current - v.created_at < 300);
    }
}

/// Credential info for listing
#[derive(Clone, Serialize, Deserialize)]
pub struct CredentialInfo {
    pub credential_id: String,
    pub name: String,
    pub device_type: String,
    pub created_at: u64,
    pub last_used: Option<u64>,
}

/// Helper: Get current Unix timestamp
fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Helper: Base64 URL encode
fn base64_url_encode(data: &[u8]) -> String {
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
    URL_SAFE_NO_PAD.encode(data)
}

/// Helper: Base64 URL decode
fn base64_url_decode(data: &str) -> std::result::Result<Vec<u8>, base64::DecodeError> {
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
    URL_SAFE_NO_PAD.decode(data)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_from_env() {
        let config = WebAuthnConfig::from_env();
        assert!(!config.rp_id.is_empty());
        assert!(!config.rp_name.is_empty());
    }

    #[test]
    fn test_base64_url_roundtrip() {
        let data = b"test data for encoding";
        let encoded = base64_url_encode(data);
        let decoded = base64_url_decode(&encoded).unwrap();
        assert_eq!(data.as_slice(), decoded.as_slice());
    }
}
