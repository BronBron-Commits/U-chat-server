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
}

impl AppState {
    /// Create a new AppState with the given database connection
    pub fn new(db: Connection) -> Self {
        Self {
            db: Mutex::new(db),
            password_service: PasswordService::new(),
            token_service: TokenService::from_env(),
            rate_limiter: AuthRateLimiter::from_env(),
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
