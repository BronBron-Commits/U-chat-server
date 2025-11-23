//! Auth API - Authentication service with Argon2id, JWT, SSO, WebAuthn, and rate limiting
//!
//! Endpoints:
//! - POST /login - Authenticate user and receive JWT token
//! - POST /devices/register - Register a new device
//! - POST /devices/list - List user's devices
//! - POST /devices/revoke - Revoke a device
//! - GET /health - Health check endpoint
//! - GET /stats - Service statistics
//!
//! SSO Endpoints:
//! - GET /auth/sso/providers - List available SSO providers
//! - GET /auth/sso/:provider - Start SSO flow
//! - GET /auth/callback/:provider - SSO callback
//!
//! WebAuthn Endpoints:
//! - POST /auth/passkey/register/start - Start passkey registration
//! - POST /auth/passkey/register/finish - Complete passkey registration
//! - POST /auth/passkey/login/start - Start passkey authentication
//! - POST /auth/passkey/login/finish - Complete passkey authentication
//! - POST /auth/passkey/list - List user's passkeys
//! - POST /auth/passkey/revoke - Revoke a passkey

mod handlers;
pub mod oidc;
mod rate_limiter;
mod services;
pub mod webauthn_service;

use axum::{
    routing::{get, post},
    Router,
};
use rusqlite::Connection;
use std::net::SocketAddr;
use std::sync::Arc;
use tower_http::{cors::CorsLayer, trace::TraceLayer};
use tracing::info;

use handlers::{
    health_handler, list_devices_handler, login_handler, register_device_handler,
    revoke_device_handler, stats_handler, AppState,
    // SSO handlers
    sso_providers_handler, sso_start_handler, sso_callback_handler,
    // WebAuthn handlers
    passkey_register_start_handler, passkey_register_finish_handler,
    passkey_login_start_handler, passkey_login_finish_handler,
    passkey_list_handler, passkey_revoke_handler,
};

#[tokio::main]
async fn main() {
    // Initialize tracing
    tracing_subscriber_init();

    let bind_addr = std::env::var("AUTH_BIND_ADDR")
        .unwrap_or_else(|_| "0.0.0.0:9200".to_string());

    let db_path = std::env::var("AUTH_DB_PATH")
        .unwrap_or_else(|_| "/opt/unhidra/auth.db".to_string());

    // Open SQLite database connection
    let conn = Connection::open(&db_path)
        .unwrap_or_else(|_| panic!("Failed to open database at {}", db_path));

    // Create application state
    let state = Arc::new(AppState::new(conn));

    // Build router
    let app = Router::new()
        // Traditional authentication
        .route("/login", post(login_handler))
        // SSO authentication
        .route("/auth/sso/providers", get(sso_providers_handler))
        .route("/auth/sso/{provider}", get(sso_start_handler))
        .route("/auth/callback/{provider}", get(sso_callback_handler))
        // WebAuthn (Passkey) authentication
        .route("/auth/passkey/register/start", post(passkey_register_start_handler))
        .route("/auth/passkey/register/finish", post(passkey_register_finish_handler))
        .route("/auth/passkey/login/start", post(passkey_login_start_handler))
        .route("/auth/passkey/login/finish", post(passkey_login_finish_handler))
        .route("/auth/passkey/list", post(passkey_list_handler))
        .route("/auth/passkey/revoke", post(passkey_revoke_handler))
        // Device management
        .route("/devices/register", post(register_device_handler))
        .route("/devices/list", post(list_devices_handler))
        .route("/devices/revoke", post(revoke_device_handler))
        // Health and stats
        .route("/health", get(health_handler))
        .route("/stats", get(stats_handler))
        .with_state(state)
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive());

    let addr: SocketAddr = bind_addr.parse().expect("Invalid bind address");

    info!(bind_addr = %addr, db_path = %db_path, "Auth API starting");

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .expect("Failed to bind to address");

    info!("Auth API running on {}", addr);

    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await
    .expect("Server failed");
}

fn tracing_subscriber_init() {
    use tracing_subscriber::{fmt, EnvFilter};

    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("auth_api=info,tower_http=debug"));

    fmt().with_env_filter(filter).init();
}
