//! HTTP handlers for history service

use crate::{
    models::{HealthResponse, MessageQuery, SearchQuery, StoreMessageRequest},
    AppState,
};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};

/// Store a new message
pub async fn store_message(
    State(state): State<AppState>,
    Json(req): Json<StoreMessageRequest>,
) -> impl IntoResponse {
    match state.db.store_message(&req).await {
        Ok(message) => (StatusCode::CREATED, Json(message)).into_response(),
        Err(e) => {
            tracing::error!("Failed to store message: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Failed to store message" })),
            )
                .into_response()
        }
    }
}

/// Get messages with pagination
pub async fn get_messages(
    State(state): State<AppState>,
    Query(query): Query<MessageQuery>,
) -> impl IntoResponse {
    match state.db.get_messages(&query).await {
        Ok(result) => Json(result).into_response(),
        Err(e) => {
            tracing::error!("Failed to get messages: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Failed to retrieve messages" })),
            )
                .into_response()
        }
    }
}

/// Get messages for a specific room
pub async fn get_room_messages(
    State(state): State<AppState>,
    Path(room_id): Path<String>,
    Query(query): Query<MessageQuery>,
) -> impl IntoResponse {
    match state.db.get_room_messages(&room_id, &query).await {
        Ok(result) => Json(result).into_response(),
        Err(e) => {
            tracing::error!("Failed to get room messages: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Failed to retrieve room messages" })),
            )
                .into_response()
        }
    }
}

/// Get messages from a specific user
pub async fn get_user_messages(
    State(state): State<AppState>,
    Path(user_id): Path<String>,
    Query(query): Query<MessageQuery>,
) -> impl IntoResponse {
    match state.db.get_user_messages(&user_id, &query).await {
        Ok(result) => Json(result).into_response(),
        Err(e) => {
            tracing::error!("Failed to get user messages: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Failed to retrieve user messages" })),
            )
                .into_response()
        }
    }
}

/// Search messages by content
pub async fn search_messages(
    State(state): State<AppState>,
    Query(query): Query<SearchQuery>,
) -> impl IntoResponse {
    match state.db.search_messages(&query).await {
        Ok(result) => Json(result).into_response(),
        Err(e) => {
            tracing::error!("Failed to search messages: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Failed to search messages" })),
            )
                .into_response()
        }
    }
}

/// Health check endpoint
pub async fn health_check(State(state): State<AppState>) -> impl IntoResponse {
    let db_healthy = state.db.health_check().await;

    let response = HealthResponse {
        status: if db_healthy { "healthy" } else { "unhealthy" }.to_string(),
        service: "history-service".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        database: if db_healthy { "connected" } else { "disconnected" }.to_string(),
    };

    if db_healthy {
        (StatusCode::OK, Json(response))
    } else {
        (StatusCode::SERVICE_UNAVAILABLE, Json(response))
    }
}
