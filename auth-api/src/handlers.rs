//! HTTP handlers for authentication endpoints
//!
//! This module provides handlers with Argon2id password verification,
//! JWT token generation, device registration, and rate limiting.

use axum::{
    extract::{ConnectInfo, State},
    http::StatusCode,
    Json,
};
use jwt_common::{Claims, TokenService, DEFAULT_EXPIRATION_SECS};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use tracing::{info, warn};
use uuid::Uuid;

use crate::rate_limiter::AuthRateLimiter;
use crate::services::PasswordService;

use crate::oidc::OidcService;
use crate::webauthn_service::WebAuthnService;

/// Shared application state for auth-api handlers
pub struct AppState {
    /// Database connection (SQLite)
    pub db: Mutex<Connection>,
    /// Argon2id password hashing service
    pub password_service: PasswordService,
    /// JWT token generation service
    pub token_service: TokenService,
    /// Rate limiter
    pub rate_limiter: AuthRateLimiter,
    /// OIDC SSO service
    pub oidc_service: OidcService,
    /// WebAuthn (Passkey) service
    pub webauthn_service: WebAuthnService,
}

impl AppState {
    /// Create a new AppState with the given database connection
    pub fn new(db: Connection) -> Self {
        Self {
            db: Mutex::new(db),
            password_service: PasswordService::new(),
            token_service: TokenService::from_env(),
            rate_limiter: AuthRateLimiter::from_env(),
            oidc_service: OidcService::from_env(),
            webauthn_service: WebAuthnService::from_env()
                .unwrap_or_else(|e| {
                    tracing::warn!(error = %e, "WebAuthn initialization failed, using default config");
                    WebAuthnService::new(crate::webauthn_service::WebAuthnConfig::from_env())
                        .expect("WebAuthn service creation failed")
                }),
        }
    }

    /// Create AppState for development/testing with faster password hashing
    #[cfg(any(test, debug_assertions))]
    pub fn new_dev(db: Connection) -> Self {
        Self {
            db: Mutex::new(db),
            password_service: PasswordService::new_dev(),
            token_service: TokenService::from_env(),
            rate_limiter: AuthRateLimiter::new(),
            oidc_service: OidcService::new(),
            webauthn_service: WebAuthnService::new(crate::webauthn_service::WebAuthnConfig {
                rp_id: "localhost".to_string(),
                rp_name: "Unhidra Test".to_string(),
                rp_origin: "http://localhost:9200".to_string(),
            }).expect("WebAuthn service creation failed"),
        }
    }
}

/// Login request payload
#[derive(Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

/// Handle user login with rate limiting and Argon2id password verification
pub async fn login_handler(
    State(state): State<Arc<AppState>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Json(payload): Json<LoginRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let ip = addr.ip();

    // Rate limit check
    if !state.rate_limiter.check_login(ip) {
        return Err((
            StatusCode::TOO_MANY_REQUESTS,
            Json(json!({ "error": "Rate limit exceeded. Please try again later." })),
        ));
    }

    info!(username = %payload.username, ip = %ip, "Login request received");

    let username = payload.username.clone();
    let conn = state.db.lock().unwrap();

    let mut stmt = match conn.prepare(
        "SELECT password_hash, verified, display_name FROM users WHERE username = ?1",
    ) {
        Ok(s) => s,
        Err(e) => {
            warn!(error = %e, "Failed to prepare SQL statement");
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "Internal server error" })),
            ));
        }
    };

    let row = stmt.query_row(params![username.clone()], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, i64>(1)?,
            r.get::<_, String>(2)?,
        ))
    });

    let (stored_hash, verified, display_name) = match row {
        Ok(t) => t,
        Err(_) => {
            warn!(username = %username, ip = %ip, "User not found");
            return Err((
                StatusCode::UNAUTHORIZED,
                Json(json!({ "error": "Invalid credentials" })),
            ));
        }
    };

    if verified == 0 {
        warn!(username = %username, "User not verified");
        return Err((
            StatusCode::FORBIDDEN,
            Json(json!({ "error": "Account not verified" })),
        ));
    }

    match state.password_service.verify_password(&payload.password, &stored_hash) {
        Ok(true) => {}
        Ok(false) | Err(_) => {
            warn!(username = %username, ip = %ip, "Invalid password");
            return Err((
                StatusCode::UNAUTHORIZED,
                Json(json!({ "error": "Invalid credentials" })),
            ));
        }
    }

    let claims = Claims::new(&username, DEFAULT_EXPIRATION_SECS, None)
        .with_display_name(&display_name);

    let token = match state.token_service.generate(&claims) {
        Ok(t) => t,
        Err(e) => {
            warn!(username = %username, error = %e, "Token generation failed");
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "Token generation failed" })),
            ));
        }
    };

    info!(username = %username, "Login successful");

    Ok(Json(json!({
        "ok": true,
        "user": username,
        "display_name": display_name,
        "token": token
    })))
}

// ============================================================================
// Device Registration
// ============================================================================

#[derive(Deserialize)]
pub struct DeviceRegistrationRequest {
    pub name: String,
    pub device_type: String,
    #[serde(default)]
    pub capabilities: Option<serde_json::Value>,
    pub owner_token: String,
}

#[derive(Serialize)]
pub struct DeviceRegistrationResponse {
    pub ok: bool,
    pub device_id: String,
    pub api_key: String,
    pub name: String,
    pub device_type: String,
}

/// Handle device registration
pub async fn register_device_handler(
    State(state): State<Arc<AppState>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Json(payload): Json<DeviceRegistrationRequest>,
) -> Result<Json<DeviceRegistrationResponse>, (StatusCode, Json<serde_json::Value>)> {
    let ip = addr.ip();

    if !state.rate_limiter.check_device_registration(ip) {
        return Err((
            StatusCode::TOO_MANY_REQUESTS,
            Json(json!({ "error": "Rate limit exceeded" })),
        ));
    }

    let owner_claims = match state.token_service.validate(&payload.owner_token) {
        Ok(claims) => claims,
        Err(e) => {
            warn!(ip = %ip, error = %e, "Invalid owner token");
            return Err((
                StatusCode::UNAUTHORIZED,
                Json(json!({ "error": "Invalid or expired token" })),
            ));
        }
    };

    let device_id = format!("dev_{}", &Uuid::new_v4().to_string()[..8]);
    let api_key = format!("unhidra_dk_{}", Uuid::new_v4().to_string().replace("-", ""));

    info!(device_id = device_id, owner = owner_claims.sub, "Registering device");

    let conn = state.db.lock().unwrap();

    conn.execute(
        "CREATE TABLE IF NOT EXISTS devices (
            device_id TEXT PRIMARY KEY NOT NULL,
            name TEXT NOT NULL,
            device_type TEXT NOT NULL,
            api_key_hash TEXT NOT NULL,
            owner_username TEXT NOT NULL,
            capabilities TEXT,
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            last_seen TEXT,
            status TEXT NOT NULL DEFAULT 'active'
        )",
        [],
    ).map_err(|e| {
        warn!(error = %e, "Failed to create devices table");
        (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": "Database error" })))
    })?;

    let api_key_hash = state.password_service.hash_password(&api_key).map_err(|e| {
        warn!(error = %e, "Failed to hash API key");
        (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": "Failed to generate API key" })))
    })?;

    let capabilities_json = payload.capabilities.map(|c| serde_json::to_string(&c).unwrap_or_default());

    conn.execute(
        "INSERT INTO devices (device_id, name, device_type, api_key_hash, owner_username, capabilities)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![device_id, payload.name, payload.device_type, api_key_hash, owner_claims.sub, capabilities_json],
    ).map_err(|e| {
        warn!(error = %e, "Failed to insert device");
        (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": "Failed to register device" })))
    })?;

    info!(device_id = device_id, "Device registered successfully");

    Ok(Json(DeviceRegistrationResponse {
        ok: true,
        device_id,
        api_key,
        name: payload.name,
        device_type: payload.device_type,
    }))
}

#[derive(Deserialize)]
pub struct ListDevicesRequest {
    pub token: String,
}

#[derive(Serialize)]
pub struct DeviceInfo {
    pub device_id: String,
    pub name: String,
    pub device_type: String,
    pub created_at: String,
    pub last_seen: Option<String>,
    pub status: String,
}

pub async fn list_devices_handler(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<ListDevicesRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let claims = match state.token_service.validate(&payload.token) {
        Ok(claims) => claims,
        Err(_) => return Err((StatusCode::UNAUTHORIZED, Json(json!({ "error": "Invalid token" })))),
    };

    let conn = state.db.lock().unwrap();

    let devices: Vec<DeviceInfo> = match conn.prepare(
        "SELECT device_id, name, device_type, created_at, last_seen, status FROM devices WHERE owner_username = ?1 AND status = 'active'",
    ) {
        Ok(mut stmt) => stmt.query_map(params![claims.sub], |row| {
            Ok(DeviceInfo {
                device_id: row.get(0)?,
                name: row.get(1)?,
                device_type: row.get(2)?,
                created_at: row.get(3)?,
                last_seen: row.get(4)?,
                status: row.get(5)?,
            })
        }).map(|iter| iter.filter_map(|r| r.ok()).collect()).unwrap_or_default(),
        Err(_) => vec![],
    };

    Ok(Json(json!({ "ok": true, "devices": devices })))
}

#[derive(Deserialize)]
pub struct RevokeDeviceRequest {
    pub token: String,
    pub device_id: String,
}

pub async fn revoke_device_handler(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<RevokeDeviceRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let claims = match state.token_service.validate(&payload.token) {
        Ok(claims) => claims,
        Err(_) => return Err((StatusCode::UNAUTHORIZED, Json(json!({ "error": "Invalid token" })))),
    };

    let conn = state.db.lock().unwrap();

    match conn.execute(
        "UPDATE devices SET status = 'revoked' WHERE device_id = ?1 AND owner_username = ?2",
        params![payload.device_id, claims.sub],
    ) {
        Ok(rows) if rows > 0 => {
            info!(device_id = payload.device_id, "Device revoked");
            Ok(Json(json!({ "ok": true, "message": "Device revoked" })))
        }
        _ => Err((StatusCode::NOT_FOUND, Json(json!({ "error": "Device not found" })))),
    }
}

// ============================================================================
// Health and Stats
// ============================================================================

pub async fn health_handler() -> &'static str {
    "OK"
}

pub async fn stats_handler(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    Json(json!({
        "status": "healthy",
        "rate_limits": state.rate_limiter.get_info()
    }))
}

// ============================================================================
// SSO (OpenID Connect) Handlers
// ============================================================================

use axum::extract::Path;
use axum::response::Redirect;

use crate::oidc::{OidcService, SsoProviderInfo, SsoProvidersResponse};
use crate::webauthn_service::{WebAuthnService, CredentialInfo};

/// List available SSO providers
pub async fn sso_providers_handler(
    State(state): State<Arc<AppState>>,
) -> Json<SsoProvidersResponse> {
    let providers: Vec<SsoProviderInfo> = state
        .oidc_service
        .enabled_providers()
        .iter()
        .map(|name| SsoProviderInfo::from_name(name))
        .collect();

    Json(SsoProvidersResponse { providers })
}

/// Start SSO flow for a provider
pub async fn sso_start_handler(
    State(state): State<Arc<AppState>>,
    Path(provider): Path<String>,
) -> Result<Redirect, (StatusCode, Json<serde_json::Value>)> {
    match state.oidc_service.start_auth(&provider).await {
        Ok((auth_url, _state)) => Ok(Redirect::temporary(&auth_url)),
        Err(e) => {
            warn!(provider = provider, error = %e, "SSO start failed");
            Err((
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": format!("SSO initialization failed: {}", e) })),
            ))
        }
    }
}

/// SSO callback handler
#[derive(Deserialize)]
pub struct SsoCallbackParams {
    pub code: String,
    pub state: String,
}

pub async fn sso_callback_handler(
    State(state): State<Arc<AppState>>,
    Path(provider): Path<String>,
    axum::extract::Query(params): axum::extract::Query<SsoCallbackParams>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let user_info = state
        .oidc_service
        .complete_auth(&params.state, &params.code)
        .await
        .map_err(|e| {
            warn!(provider = provider, error = %e, "SSO callback failed");
            (
                StatusCode::UNAUTHORIZED,
                Json(json!({ "error": format!("SSO authentication failed: {}", e) })),
            )
        })?;

    // Create or update user in database
    let username = user_info.email.clone().unwrap_or_else(|| user_info.sub.clone());
    let display_name = user_info.name.clone().unwrap_or_else(|| username.clone());

    {
        let conn = state.db.lock().unwrap();

        // Ensure SSO users table exists
        conn.execute(
            "CREATE TABLE IF NOT EXISTS sso_users (
                username TEXT PRIMARY KEY NOT NULL,
                provider TEXT NOT NULL,
                provider_sub TEXT NOT NULL,
                email TEXT,
                display_name TEXT NOT NULL,
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                last_login TEXT
            )",
            [],
        ).map_err(|e| {
            (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": format!("Database error: {}", e) })))
        })?;

        // Insert or update user
        conn.execute(
            "INSERT INTO sso_users (username, provider, provider_sub, email, display_name, last_login)
             VALUES (?1, ?2, ?3, ?4, ?5, datetime('now'))
             ON CONFLICT(username) DO UPDATE SET last_login = datetime('now')",
            rusqlite::params![username, user_info.provider, user_info.sub, user_info.email, display_name],
        ).map_err(|e| {
            (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": format!("Database error: {}", e) })))
        })?;
    }

    // Generate JWT token
    let claims = Claims::new(&username, DEFAULT_EXPIRATION_SECS, None)
        .with_display_name(&display_name);

    let token = state.token_service.generate(&claims).map_err(|e| {
        (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": format!("Token generation failed: {}", e) })))
    })?;

    info!(username = username, provider = user_info.provider, "SSO login successful");

    Ok(Json(json!({
        "ok": true,
        "user": username,
        "display_name": display_name,
        "provider": user_info.provider,
        "token": token
    })))
}

// ============================================================================
// WebAuthn (Passkey) Handlers
// ============================================================================

#[derive(Deserialize)]
pub struct PasskeyRegisterStartRequest {
    pub token: String,
    pub device_name: Option<String>,
}

/// Start passkey registration
pub async fn passkey_register_start_handler(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<PasskeyRegisterStartRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    // Validate token
    let claims = state.token_service.validate(&payload.token).map_err(|_| {
        (StatusCode::UNAUTHORIZED, Json(json!({ "error": "Invalid token" })))
    })?;

    let display_name = claims.display_name.clone().unwrap_or_else(|| claims.sub.clone());

    match state.webauthn_service.start_registration(&claims.sub, &claims.sub, &display_name) {
        Ok((options, challenge)) => {
            Ok(Json(json!({
                "ok": true,
                "challenge": challenge,
                "options": options
            })))
        }
        Err(e) => {
            warn!(user = claims.sub, error = %e, "Passkey registration start failed");
            Err((
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": format!("Registration failed: {}", e) })),
            ))
        }
    }
}

#[derive(Deserialize)]
pub struct PasskeyRegisterFinishRequest {
    pub challenge: String,
    pub credential: serde_json::Value,
    pub device_name: String,
}

/// Complete passkey registration
pub async fn passkey_register_finish_handler(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<PasskeyRegisterFinishRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let credential: webauthn_rs::prelude::RegisterPublicKeyCredential =
        serde_json::from_value(payload.credential).map_err(|e| {
            (StatusCode::BAD_REQUEST, Json(json!({ "error": format!("Invalid credential: {}", e) })))
        })?;

    match state.webauthn_service.complete_registration(&payload.challenge, credential, &payload.device_name) {
        Ok(stored) => {
            info!(credential_id = stored.credential_id, "Passkey registered successfully");
            Ok(Json(json!({
                "ok": true,
                "credential_id": stored.credential_id,
                "name": stored.name
            })))
        }
        Err(e) => {
            warn!(error = %e, "Passkey registration finish failed");
            Err((
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": format!("Registration failed: {}", e) })),
            ))
        }
    }
}

#[derive(Deserialize)]
pub struct PasskeyLoginStartRequest {
    pub username: Option<String>,
}

/// Start passkey authentication
pub async fn passkey_login_start_handler(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<PasskeyLoginStartRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    match state.webauthn_service.start_authentication(payload.username.as_deref()) {
        Ok((options, challenge)) => {
            Ok(Json(json!({
                "ok": true,
                "challenge": challenge,
                "options": options
            })))
        }
        Err(e) => {
            warn!(username = ?payload.username, error = %e, "Passkey login start failed");
            Err((
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": format!("Authentication failed: {}", e) })),
            ))
        }
    }
}

#[derive(Deserialize)]
pub struct PasskeyLoginFinishRequest {
    pub challenge: String,
    pub credential: serde_json::Value,
}

/// Complete passkey authentication
pub async fn passkey_login_finish_handler(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<PasskeyLoginFinishRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let credential: webauthn_rs::prelude::PublicKeyCredential =
        serde_json::from_value(payload.credential).map_err(|e| {
            (StatusCode::BAD_REQUEST, Json(json!({ "error": format!("Invalid credential: {}", e) })))
        })?;

    match state.webauthn_service.complete_authentication(&payload.challenge, credential) {
        Ok(user_id) => {
            // Generate JWT token
            let claims = Claims::new(&user_id, DEFAULT_EXPIRATION_SECS, None);
            let token = state.token_service.generate(&claims).map_err(|e| {
                (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": format!("Token generation failed: {}", e) })))
            })?;

            info!(user = user_id, "Passkey login successful");

            Ok(Json(json!({
                "ok": true,
                "user": user_id,
                "token": token
            })))
        }
        Err(e) => {
            warn!(error = %e, "Passkey login finish failed");
            Err((
                StatusCode::UNAUTHORIZED,
                Json(json!({ "error": format!("Authentication failed: {}", e) })),
            ))
        }
    }
}

#[derive(Deserialize)]
pub struct PasskeyListRequest {
    pub token: String,
}

/// List user's passkeys
pub async fn passkey_list_handler(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<PasskeyListRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let claims = state.token_service.validate(&payload.token).map_err(|_| {
        (StatusCode::UNAUTHORIZED, Json(json!({ "error": "Invalid token" })))
    })?;

    let credentials = state.webauthn_service.list_credentials(&claims.sub);

    Ok(Json(json!({
        "ok": true,
        "credentials": credentials
    })))
}

#[derive(Deserialize)]
pub struct PasskeyRevokeRequest {
    pub token: String,
    pub credential_id: String,
}

/// Revoke a passkey
pub async fn passkey_revoke_handler(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<PasskeyRevokeRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let claims = state.token_service.validate(&payload.token).map_err(|_| {
        (StatusCode::UNAUTHORIZED, Json(json!({ "error": "Invalid token" })))
    })?;

    match state.webauthn_service.revoke_credential(&claims.sub, &payload.credential_id) {
        Ok(()) => {
            info!(user = claims.sub, credential_id = payload.credential_id, "Passkey revoked");
            Ok(Json(json!({ "ok": true, "message": "Passkey revoked" })))
        }
        Err(e) => {
            Err((StatusCode::NOT_FOUND, Json(json!({ "error": format!("Revocation failed: {}", e) }))))
        }
    }
}
