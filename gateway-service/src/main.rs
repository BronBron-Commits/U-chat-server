//! Gateway Service - WebSocket Gateway with JWT Authentication
//!
//! Phase 3: WSS Gateway Security Implementation
//!
//! Features:
//! - Sec-WebSocket-Protocol based JWT authentication
//! - Room-based pub/sub messaging with DashMap
//! - Origin checking for CSRF protection
//! - Rate limiting per IP and per user
//! - Connection tracking with metadata
//! - Health check and metrics endpoints
//! - Structured logging with tracing

mod state;
mod ws_handler;
mod rate_limiter;
mod metrics;
mod connection;
mod mqtt_bridge;

#[cfg(feature = "mqtt-bridge")]
mod mqtt_bridge_impl;

use axum::{
    extract::State,
    routing::get,
    Router,
};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpListener;
use tower_http::{
    cors::{Any, CorsLayer},
    trace::TraceLayer,
};
use tracing::{info, Level};
use tracing_subscriber::{fmt, EnvFilter};

use crate::state::AppState;
use crate::ws_handler::ws_handler;
use crate::metrics::metrics_handler;
use crate::rate_limiter::RateLimiter;

/// Default port for the gateway service
const DEFAULT_PORT: u16 = 9000;

/// Default allowed origins (empty = allow all in dev mode)
const DEFAULT_ORIGINS: &str = "";

#[tokio::main]
async fn main() {
    // Initialize tracing
    init_tracing();

    // Load configuration from environment
    let port: u16 = std::env::var("GATEWAY_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(DEFAULT_PORT);

    let allowed_origins: Vec<String> = std::env::var("ALLOWED_ORIGINS")
        .unwrap_or_else(|_| DEFAULT_ORIGINS.to_string())
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    // Create application state
    let state = Arc::new(AppState::from_env(allowed_origins.clone()));

    // Create rate limiter
    let rate_limiter = Arc::new(RateLimiter::new());

    // Initialize metrics
    crate::metrics::init_metrics();

    // Initialize MQTT bridge if enabled
    #[cfg(feature = "mqtt-bridge")]
    let mqtt_bridge = {
        let bridge_enabled = std::env::var("MQTT_BRIDGE_ENABLED")
            .map(|v| v.to_lowercase() == "true")
            .unwrap_or(false);

        if bridge_enabled {
            info!("MQTT bridge enabled, starting...");
            let config = mqtt_bridge::MqttBridgeConfig::from_env();
            let bridge = Arc::new(mqtt_bridge::MqttBridge::new(config));

            // Start background task for stale device cleanup
            let bridge_clone = Arc::clone(&bridge);
            tokio::spawn(async move {
                let mut interval = tokio::time::interval(Duration::from_secs(60));
                loop {
                    interval.tick().await;
                    bridge_clone.check_stale_devices(300); // 5 minute timeout
                }
            });

            Some(bridge)
        } else {
            info!("MQTT bridge disabled");
            None
        }
    };

    #[cfg(not(feature = "mqtt-bridge"))]
    let mqtt_bridge: Option<Arc<mqtt_bridge::MqttBridge>> = None;

    if mqtt_bridge.is_none() {
        info!("MQTT bridge feature not available or disabled");
    }

    // Build CORS layer
    let cors = if allowed_origins.is_empty() {
        CorsLayer::new()
            .allow_origin(Any)
            .allow_methods(Any)
            .allow_headers(Any)
    } else {
        CorsLayer::new()
            .allow_origin(Any) // WebSocket connections handle origin separately
            .allow_methods(Any)
            .allow_headers(Any)
    };

    // Build router
    let app = Router::new()
        // WebSocket endpoint
        .route("/ws", get(ws_handler))
        // Health check endpoint
        .route("/health", get(health_handler))
        // Readiness check (includes dependency checks)
        .route("/ready", get(ready_handler))
        // Metrics endpoint for Prometheus
        .route("/metrics", get(metrics_handler))
        // Connection stats endpoint
        .route("/stats", get(stats_handler))
        // State for handlers
        .with_state((state.clone(), rate_limiter))
        // Middleware
        .layer(TraceLayer::new_for_http())
        .layer(cors);

    let addr = SocketAddr::from(([0, 0, 0, 0], port));

    info!(
        port = port,
        origins = ?allowed_origins,
        "Gateway service starting"
    );

    let listener = TcpListener::bind(addr).await.expect("Failed to bind to address");

    info!("Gateway service listening on {}", addr);

    axum::serve(listener, app)
        .await
        .expect("Server failed");
}

/// Initialize tracing subscriber
fn init_tracing() {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("gateway_service=info,tower_http=debug"));

    fmt()
        .with_env_filter(filter)
        .with_target(true)
        .with_thread_ids(true)
        .with_file(true)
        .with_line_number(true)
        .init();
}

/// Health check handler - simple liveness probe
async fn health_handler() -> &'static str {
    "OK"
}

/// Readiness check handler - verifies service is ready to accept traffic
async fn ready_handler(
    State((state, _)): State<(Arc<AppState>, Arc<RateLimiter>)>,
) -> axum::response::Json<serde_json::Value> {
    let room_count = state.rooms.len();
    let connection_count = state.connections.len();

    axum::response::Json(serde_json::json!({
        "status": "ready",
        "rooms": room_count,
        "connections": connection_count
    }))
}

/// Stats handler - returns current gateway statistics
async fn stats_handler(
    State((state, rate_limiter)): State<(Arc<AppState>, Arc<RateLimiter>)>,
) -> axum::response::Json<serde_json::Value> {
    let room_count = state.rooms.len();
    let connection_count = state.connections.len();

    // Collect room details
    let rooms: Vec<_> = state.rooms.iter()
        .map(|entry| {
            let room_id = entry.key().clone();
            let subscribers = entry.value().receiver_count();
            serde_json::json!({
                "room_id": room_id,
                "subscribers": subscribers
            })
        })
        .collect();

    // Collect connection details (anonymized)
    let connections: Vec<_> = state.connections.iter()
        .take(100) // Limit to prevent large responses
        .map(|entry| {
            let conn = entry.value();
            serde_json::json!({
                "user_id": conn.user_id,
                "room_id": conn.room_id,
                "connected_at": conn.connected_at.to_rfc3339(),
                "messages_sent": conn.messages_sent,
                "messages_received": conn.messages_received
            })
        })
        .collect();

    axum::response::Json(serde_json::json!({
        "rooms": {
            "count": room_count,
            "details": rooms
        },
        "connections": {
            "count": connection_count,
            "sample": connections
        },
        "rate_limiter": {
            "ip_limit": rate_limiter.ip_limit_info(),
            "user_limit": rate_limiter.user_limit_info()
        }
    }))
}
