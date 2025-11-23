//! Auth API - Authentication service with Argon2id, JWT, and rate limiting
//!
//! Endpoints:
//! - POST /login - Authenticate user and receive JWT token
//! - POST /devices/register - Register a new device
//! - POST /devices/list - List user's devices
//! - POST /devices/revoke - Revoke a device
//! - GET /health - Health check endpoint
//! - GET /stats - Service statistics

mod handlers;
mod rate_limiter;
mod services;

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
        // Authentication
        .route("/login", post(login_handler))
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
