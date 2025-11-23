//! OpenID Connect SSO Implementation
//!
//! Supports enterprise identity providers:
//! - Okta
//! - Azure AD
//! - Keycloak
//! - Google Workspace
//! - Generic OIDC providers

use std::collections::HashMap;
use std::sync::Arc;

use dashmap::DashMap;
use openidconnect::{
    core::{CoreClient, CoreProviderMetadata, CoreResponseType},
    reqwest::http_client,
    AuthenticationFlow, AuthorizationCode, ClientId, ClientSecret, CsrfToken, IssuerUrl, Nonce,
    OAuth2TokenResponse, PkceCodeChallenge, PkceCodeVerifier, RedirectUrl, Scope, TokenResponse,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::{error, info, warn};

/// OIDC errors
#[derive(Error, Debug)]
pub enum OidcError {
    #[error("Provider not configured: {0}")]
    ProviderNotConfigured(String),

    #[error("Discovery failed: {0}")]
    DiscoveryFailed(String),

    #[error("Token exchange failed: {0}")]
    TokenExchangeFailed(String),

    #[error("Invalid state: {0}")]
    InvalidState(String),

    #[error("User info retrieval failed: {0}")]
    UserInfoFailed(String),

    #[error("Configuration error: {0}")]
    ConfigurationError(String),
}

pub type Result<T> = std::result::Result<T, OidcError>;

/// OIDC provider configuration
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OidcProviderConfig {
    /// Provider name (okta, azure, keycloak, google, custom)
    pub name: String,
    /// Issuer URL (e.g., https://your-domain.okta.com)
    pub issuer_url: String,
    /// Client ID
    pub client_id: String,
    /// Client Secret (optional for public clients)
    pub client_secret: Option<String>,
    /// Redirect URI for callback
    pub redirect_uri: String,
    /// Additional scopes beyond openid
    pub scopes: Vec<String>,
    /// Whether this provider is enabled
    pub enabled: bool,
}

impl OidcProviderConfig {
    /// Load provider config from environment
    pub fn from_env(provider: &str) -> Option<Self> {
        let prefix = format!("OIDC_{}", provider.to_uppercase());

        let issuer_url = std::env::var(format!("{}_ISSUER_URL", prefix)).ok()?;
        let client_id = std::env::var(format!("{}_CLIENT_ID", prefix)).ok()?;
        let client_secret = std::env::var(format!("{}_CLIENT_SECRET", prefix)).ok();
        let redirect_uri = std::env::var(format!("{}_REDIRECT_URI", prefix))
            .unwrap_or_else(|_| format!("https://unhidra.local/auth/callback/{}", provider));
        let scopes = std::env::var(format!("{}_SCOPES", prefix))
            .map(|s| s.split(',').map(|s| s.trim().to_string()).collect())
            .unwrap_or_else(|_| vec!["email".to_string(), "profile".to_string()]);
        let enabled = std::env::var(format!("{}_ENABLED", prefix))
            .map(|v| v.to_lowercase() == "true")
            .unwrap_or(true);

        Some(Self {
            name: provider.to_string(),
            issuer_url,
            client_id,
            client_secret,
            redirect_uri,
            scopes,
            enabled,
        })
    }
}

/// Pending OIDC authentication flow
#[derive(Clone)]
pub struct PendingAuth {
    pub csrf_token: CsrfToken,
    pub nonce: Nonce,
    pub pkce_verifier: String,
    pub provider: String,
    pub created_at: u64,
}

/// User info from OIDC provider
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OidcUserInfo {
    /// Unique subject identifier from provider
    pub sub: String,
    /// Email address (if available)
    pub email: Option<String>,
    /// Whether email is verified
    pub email_verified: Option<bool>,
    /// Display name
    pub name: Option<String>,
    /// Given name
    pub given_name: Option<String>,
    /// Family name
    pub family_name: Option<String>,
    /// Profile picture URL
    pub picture: Option<String>,
    /// Provider name
    pub provider: String,
}

/// OIDC service for managing SSO flows
pub struct OidcService {
    /// Configured providers
    providers: HashMap<String, OidcProviderConfig>,
    /// Cached OIDC clients
    clients: DashMap<String, Arc<CoreClient>>,
    /// Pending authentication flows (csrf_token -> PendingAuth)
    pending_auths: DashMap<String, PendingAuth>,
}

impl OidcService {
    /// Create a new OIDC service
    pub fn new() -> Self {
        Self {
            providers: HashMap::new(),
            clients: DashMap::new(),
            pending_auths: DashMap::new(),
        }
    }

    /// Load configuration from environment for common providers
    pub fn from_env() -> Self {
        let mut service = Self::new();

        // Try to load common providers
        for provider in &["okta", "azure", "keycloak", "google"] {
            if let Some(config) = OidcProviderConfig::from_env(provider) {
                if config.enabled {
                    info!(provider = provider, "Loaded OIDC provider configuration");
                    service.providers.insert(provider.to_string(), config);
                }
            }
        }

        service
    }

    /// Register a provider configuration
    pub fn register_provider(&mut self, config: OidcProviderConfig) {
        self.providers.insert(config.name.clone(), config);
    }

    /// Get list of enabled providers
    pub fn enabled_providers(&self) -> Vec<String> {
        self.providers
            .iter()
            .filter(|(_, c)| c.enabled)
            .map(|(k, _)| k.clone())
            .collect()
    }

    /// Initialize OIDC client for a provider (with discovery)
    async fn get_or_create_client(&self, provider: &str) -> Result<Arc<CoreClient>> {
        // Check cache
        if let Some(client) = self.clients.get(provider) {
            return Ok(client.clone());
        }

        let config = self
            .providers
            .get(provider)
            .ok_or_else(|| OidcError::ProviderNotConfigured(provider.to_string()))?;

        let issuer_url = IssuerUrl::new(config.issuer_url.clone())
            .map_err(|e| OidcError::ConfigurationError(e.to_string()))?;

        // Perform OIDC discovery
        let provider_metadata =
            CoreProviderMetadata::discover(&issuer_url, http_client)
                .map_err(|e| OidcError::DiscoveryFailed(e.to_string()))?;

        let redirect_url = RedirectUrl::new(config.redirect_uri.clone())
            .map_err(|e| OidcError::ConfigurationError(e.to_string()))?;

        let mut client = CoreClient::from_provider_metadata(
            provider_metadata,
            ClientId::new(config.client_id.clone()),
            config.client_secret.clone().map(ClientSecret::new),
        )
        .set_redirect_uri(redirect_url);

        let client = Arc::new(client);
        self.clients.insert(provider.to_string(), client.clone());

        Ok(client)
    }

    /// Start OIDC authentication flow
    ///
    /// Returns the authorization URL to redirect the user to.
    pub async fn start_auth(&self, provider: &str) -> Result<(String, String)> {
        let client = self.get_or_create_client(provider).await?;
        let config = self
            .providers
            .get(provider)
            .ok_or_else(|| OidcError::ProviderNotConfigured(provider.to_string()))?;

        // Generate PKCE challenge
        let (pkce_challenge, pkce_verifier) = PkceCodeChallenge::new_random_sha256();

        // Build authorization URL
        let mut auth_request = client
            .authorize_url(
                AuthenticationFlow::<CoreResponseType>::AuthorizationCode,
                CsrfToken::new_random,
                Nonce::new_random,
            )
            .set_pkce_challenge(pkce_challenge);

        // Add requested scopes
        for scope in &config.scopes {
            auth_request = auth_request.add_scope(Scope::new(scope.clone()));
        }

        let (auth_url, csrf_token, nonce) = auth_request.url();

        // Store pending auth
        let pending = PendingAuth {
            csrf_token: csrf_token.clone(),
            nonce,
            pkce_verifier: pkce_verifier.secret().clone(),
            provider: provider.to_string(),
            created_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
        };

        let state = csrf_token.secret().clone();
        self.pending_auths.insert(state.clone(), pending);

        info!(provider = provider, "Started OIDC auth flow");

        Ok((auth_url.to_string(), state))
    }

    /// Complete OIDC authentication flow
    ///
    /// Exchanges the authorization code for tokens and retrieves user info.
    pub async fn complete_auth(&self, state: &str, code: &str) -> Result<OidcUserInfo> {
        // Get pending auth
        let pending = self
            .pending_auths
            .remove(state)
            .map(|(_, v)| v)
            .ok_or_else(|| OidcError::InvalidState("Unknown or expired state".to_string()))?;

        // Check if auth is expired (15 minute timeout)
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        if now - pending.created_at > 900 {
            return Err(OidcError::InvalidState("Auth flow expired".to_string()));
        }

        let client = self.get_or_create_client(&pending.provider).await?;

        // Exchange code for tokens
        let pkce_verifier = PkceCodeVerifier::new(pending.pkce_verifier);

        let token_response = client
            .exchange_code(AuthorizationCode::new(code.to_string()))
            .set_pkce_verifier(pkce_verifier)
            .request(http_client)
            .map_err(|e| OidcError::TokenExchangeFailed(e.to_string()))?;

        // Get ID token claims
        let id_token = token_response
            .id_token()
            .ok_or_else(|| OidcError::TokenExchangeFailed("No ID token in response".to_string()))?;

        let claims = id_token
            .claims(&client.id_token_verifier(), &pending.nonce)
            .map_err(|e| OidcError::TokenExchangeFailed(e.to_string()))?;

        // Build user info
        let user_info = OidcUserInfo {
            sub: claims.subject().to_string(),
            email: claims.email().map(|e| e.to_string()),
            email_verified: claims.email_verified(),
            name: claims
                .name()
                .and_then(|n| n.get(None))
                .map(|n| n.to_string()),
            given_name: claims
                .given_name()
                .and_then(|n| n.get(None))
                .map(|n| n.to_string()),
            family_name: claims
                .family_name()
                .and_then(|n| n.get(None))
                .map(|n| n.to_string()),
            picture: claims
                .picture()
                .and_then(|n| n.get(None))
                .map(|n| n.to_string()),
            provider: pending.provider,
        };

        info!(
            provider = user_info.provider,
            sub = user_info.sub,
            email = ?user_info.email,
            "OIDC authentication completed"
        );

        Ok(user_info)
    }

    /// Clean up expired pending auths
    pub fn cleanup_expired(&self) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        self.pending_auths.retain(|_, v| now - v.created_at < 900);
    }
}

impl Default for OidcService {
    fn default() -> Self {
        Self::new()
    }
}

/// Response for SSO providers endpoint
#[derive(Serialize)]
pub struct SsoProvidersResponse {
    pub providers: Vec<SsoProviderInfo>,
}

/// Provider info for frontend
#[derive(Serialize)]
pub struct SsoProviderInfo {
    pub name: String,
    pub display_name: String,
    pub icon: String,
}

impl SsoProviderInfo {
    pub fn from_name(name: &str) -> Self {
        let (display_name, icon) = match name {
            "okta" => ("Okta", "okta"),
            "azure" => ("Microsoft", "microsoft"),
            "google" => ("Google", "google"),
            "keycloak" => ("Keycloak", "key"),
            _ => (name, "key"),
        };

        Self {
            name: name.to_string(),
            display_name: display_name.to_string(),
            icon: icon.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_provider_config_from_env() {
        std::env::set_var("OIDC_TEST_ISSUER_URL", "https://test.example.com");
        std::env::set_var("OIDC_TEST_CLIENT_ID", "test-client-id");
        std::env::set_var("OIDC_TEST_CLIENT_SECRET", "test-secret");

        let config = OidcProviderConfig::from_env("test").unwrap();
        assert_eq!(config.issuer_url, "https://test.example.com");
        assert_eq!(config.client_id, "test-client-id");
        assert_eq!(config.client_secret, Some("test-secret".to_string()));

        std::env::remove_var("OIDC_TEST_ISSUER_URL");
        std::env::remove_var("OIDC_TEST_CLIENT_ID");
        std::env::remove_var("OIDC_TEST_CLIENT_SECRET");
    }

    #[test]
    fn test_sso_provider_info() {
        let okta = SsoProviderInfo::from_name("okta");
        assert_eq!(okta.display_name, "Okta");

        let azure = SsoProviderInfo::from_name("azure");
        assert_eq!(azure.display_name, "Microsoft");
    }
}
