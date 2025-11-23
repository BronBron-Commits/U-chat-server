//! Chat Service with Redis Streams Backend
//!
//! Provides horizontally scalable chat functionality with:
//! - Redis Streams for message distribution
//! - Consumer groups for reliable delivery
//! - Message history and persistence
//! - E2EE support (client-side encryption)
//!
//! Endpoints:
//! - POST /send - Send a message to a room
//! - GET /messages/:room_id - Get message history
//! - GET /rooms/:room_id/info - Get room stream info
//! - GET /health - Health check
//! - GET /ready - Readiness check (includes Redis status)

mod redis_streams;

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::RwLock;
use tower_http::{cors::CorsLayer, trace::TraceLayer};
use tracing::{error, info, warn};

use redis_streams::{RedisConfig, RedisStreams, StreamMessage};

/// Application state
struct AppState {
    /// Redis Streams client
    redis: RwLock<Option<RedisStreams>>,
    /// Service instance ID
    instance_id: String,
}

impl AppState {
    async fn new() -> Self {
        let instance_id = std::env::var("INSTANCE_ID")
            .unwrap_or_else(|_| uuid::Uuid::new_v4().to_string()[..8].to_string());

        // Try to connect to Redis
        let redis = match RedisStreams::from_env().await {
            Ok(r) => {
                info!("Connected to Redis");
                Some(r)
            }
            Err(e) => {
                warn!(error = %e, "Redis not available, running in standalone mode");
                None
            }
        };

        Self {
            redis: RwLock::new(redis),
            instance_id,
        }
    }
}

// ============================================================================
// Request/Response Types
// ============================================================================

#[derive(Deserialize)]
struct SendMessageRequest {
    room_id: String,
    sender_id: String,
    content: String,
    #[serde(default)]
    message_type: Option<String>,
    #[serde(default)]
    metadata: Option<String>,
}

#[derive(Serialize)]
struct SendMessageResponse {
    ok: bool,
    message_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream_id: Option<String>,
}

#[derive(Deserialize)]
struct HistoryQuery {
    #[serde(default = "default_count")]
    count: usize,
    #[serde(default)]
    before: Option<String>,
}

fn default_count() -> usize {
    50
}

#[derive(Serialize)]
struct HistoryResponse {
    ok: bool,
    room_id: String,
    messages: Vec<MessageInfo>,
    count: usize,
}

#[derive(Serialize)]
struct MessageInfo {
    id: String,
    sender_id: String,
    content: String,
    message_type: String,
    timestamp: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    metadata: Option<String>,
}

impl From<StreamMessage> for MessageInfo {
    fn from(msg: StreamMessage) -> Self {
        Self {
            id: msg.id,
            sender_id: msg.sender_id,
            content: msg.content,
            message_type: msg.message_type,
            timestamp: msg.timestamp,
            metadata: msg.metadata,
        }
    }
}

#[derive(Serialize)]
struct RoomInfoResponse {
    ok: bool,
    room_id: String,
    message_count: usize,
    consumer_group: String,
    redis_available: bool,
}

// ============================================================================
// Handlers
// ============================================================================

/// Send a message to a room
async fn send_message_handler(
    State(state): State<Arc<AppState>>,
    Json(req): Json<SendMessageRequest>,
) -> Result<Json<SendMessageResponse>, (StatusCode, Json<serde_json::Value>)> {
    let mut message = StreamMessage::new_text(&req.room_id, &req.sender_id, &req.content);

    if let Some(msg_type) = req.message_type {
        message.message_type = msg_type;
    }
    message.metadata = req.metadata;

    let mut redis_guard = state.redis.write().await;

    if let Some(ref mut redis) = *redis_guard {
        match redis.publish(&message).await {
            Ok(stream_id) => {
                info!(
                    room_id = req.room_id,
                    message_id = message.id,
                    "Message sent via Redis"
                );
                Ok(Json(SendMessageResponse {
                    ok: true,
                    message_id: message.id,
                    stream_id: Some(stream_id),
                }))
            }
            Err(e) => {
                error!(error = %e, "Failed to publish message to Redis");
                Err((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({ "error": "Failed to send message" })),
                ))
            }
        }
    } else {
        // Fallback: log message locally (for testing without Redis)
        info!(
            room_id = req.room_id,
            message_id = message.id,
            "Message logged locally (Redis not available)"
        );
        Ok(Json(SendMessageResponse {
            ok: true,
            message_id: message.id,
            stream_id: None,
        }))
    }
}

/// Get message history for a room
async fn get_history_handler(
    State(state): State<Arc<AppState>>,
    Path(room_id): Path<String>,
    Query(query): Query<HistoryQuery>,
) -> Result<Json<HistoryResponse>, (StatusCode, Json<serde_json::Value>)> {
    let mut redis_guard = state.redis.write().await;

    if let Some(ref mut redis) = *redis_guard {
        match redis.get_history(&room_id, query.count, query.before.as_deref()).await {
            Ok(messages) => {
                let count = messages.len();
                let messages: Vec<MessageInfo> = messages.into_iter().map(Into::into).collect();

                Ok(Json(HistoryResponse {
                    ok: true,
                    room_id,
                    messages,
                    count,
                }))
            }
            Err(e) => {
                error!(room_id = room_id, error = %e, "Failed to get history");
                Err((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({ "error": "Failed to get message history" })),
                ))
            }
        }
    } else {
        Ok(Json(HistoryResponse {
            ok: true,
            room_id,
            messages: vec![],
            count: 0,
        }))
    }
}

/// Get room stream info
async fn room_info_handler(
    State(state): State<Arc<AppState>>,
    Path(room_id): Path<String>,
) -> Json<RoomInfoResponse> {
    let mut redis_guard = state.redis.write().await;

    if let Some(ref mut redis) = *redis_guard {
        match redis.stream_info(&room_id).await {
            Ok(info) => Json(RoomInfoResponse {
                ok: true,
                room_id: info.room_id,
                message_count: info.length,
                consumer_group: info.consumer_group,
                redis_available: true,
            }),
            Err(_) => Json(RoomInfoResponse {
                ok: true,
                room_id,
                message_count: 0,
                consumer_group: String::new(),
                redis_available: true,
            }),
        }
    } else {
        Json(RoomInfoResponse {
            ok: true,
            room_id,
            message_count: 0,
            consumer_group: String::new(),
            redis_available: false,
        })
    }
}

/// Health check
async fn health_handler() -> &'static str {
    "OK"
}

/// Readiness check (includes Redis status)
async fn ready_handler(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    let redis_guard = state.redis.read().await;
    let redis_ready = redis_guard.is_some();

    Json(serde_json::json!({
        "ok": true,
        "instance_id": state.instance_id,
        "redis_connected": redis_ready,
    }))
}

// ============================================================================
// Main
// ============================================================================

#[tokio::main]
async fn main() {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "chat_service=info,tower_http=debug".into()),
        )
        .init();

    let bind_addr = std::env::var("CHAT_BIND_ADDR")
        .unwrap_or_else(|_| "0.0.0.0:3001".to_string());

    // Create application state
    let state = Arc::new(AppState::new().await);

    // Build router
    let app = Router::new()
        .route("/send", post(send_message_handler))
        .route("/messages/{room_id}", get(get_history_handler))
        .route("/rooms/{room_id}/info", get(room_info_handler))
        .route("/health", get(health_handler))
        .route("/ready", get(ready_handler))
        .with_state(state)
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive());

    let listener = TcpListener::bind(&bind_addr)
        .await
        .expect("Failed to bind to address");

    info!(bind_addr = bind_addr, "Chat service starting");

    axum::serve(listener, app).await.expect("Server failed");
}
