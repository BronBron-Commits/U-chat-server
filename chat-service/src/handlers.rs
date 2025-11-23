//! HTTP handlers for chat service

use crate::{
    models::{
        AddMemberRequest, CreateGroupRequest, HealthResponse, MemberRole, MessageQuery,
        SendMessageRequest, UpdateGroupRequest,
    },
    state::AppState,
};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use std::sync::Arc;
use uuid::Uuid;

/// Pagination query parameters
#[derive(Debug, Clone, serde::Deserialize, Default)]
pub struct PaginationQuery {
    #[serde(default = "default_limit")]
    pub limit: i64,
    #[serde(default)]
    pub offset: i64,
}

fn default_limit() -> i64 {
    50
}

/// User context for join/leave operations
#[derive(Debug, Clone, serde::Deserialize)]
pub struct UserContext {
    pub user_id: String,
}

// ============================================================================
// Group Handlers
// ============================================================================

/// Create a new group
pub async fn create_group(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateGroupRequest>,
) -> impl IntoResponse {
    match state.db.create_group(&req).await {
        Ok(group) => (StatusCode::CREATED, Json(group)).into_response(),
        Err(e) => {
            tracing::error!("Failed to create group: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Failed to create group" })),
            )
                .into_response()
        }
    }
}

/// List all public groups
pub async fn list_groups(
    State(state): State<Arc<AppState>>,
    Query(query): Query<PaginationQuery>,
) -> impl IntoResponse {
    match state.db.list_groups(query.limit, query.offset).await {
        Ok(groups) => Json(groups).into_response(),
        Err(e) => {
            tracing::error!("Failed to list groups: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Failed to list groups" })),
            )
                .into_response()
        }
    }
}

/// Get a specific group
pub async fn get_group(
    State(state): State<Arc<AppState>>,
    Path(group_id): Path<Uuid>,
) -> impl IntoResponse {
    match state.db.get_group_with_stats(group_id).await {
        Ok(Some(group)) => Json(group).into_response(),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "Group not found" })),
        )
            .into_response(),
        Err(e) => {
            tracing::error!("Failed to get group: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Failed to get group" })),
            )
                .into_response()
        }
    }
}

/// Update a group
pub async fn update_group(
    State(state): State<Arc<AppState>>,
    Path(group_id): Path<Uuid>,
    Json(req): Json<UpdateGroupRequest>,
) -> impl IntoResponse {
    match state.db.update_group(group_id, &req).await {
        Ok(Some(group)) => Json(group).into_response(),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "Group not found" })),
        )
            .into_response(),
        Err(e) => {
            tracing::error!("Failed to update group: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Failed to update group" })),
            )
                .into_response()
        }
    }
}

/// Delete a group
pub async fn delete_group(
    State(state): State<Arc<AppState>>,
    Path(group_id): Path<Uuid>,
) -> impl IntoResponse {
    match state.db.delete_group(group_id).await {
        Ok(true) => {
            state.remove_group_channel(&group_id);
            (StatusCode::NO_CONTENT, ()).into_response()
        }
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "Group not found" })),
        )
            .into_response(),
        Err(e) => {
            tracing::error!("Failed to delete group: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Failed to delete group" })),
            )
                .into_response()
        }
    }
}

// ============================================================================
// Member Handlers
// ============================================================================

/// List group members
pub async fn list_members(
    State(state): State<Arc<AppState>>,
    Path(group_id): Path<Uuid>,
) -> impl IntoResponse {
    match state.db.list_members(group_id).await {
        Ok(members) => Json(members).into_response(),
        Err(e) => {
            tracing::error!("Failed to list members: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Failed to list members" })),
            )
                .into_response()
        }
    }
}

/// Add a member to a group
pub async fn add_member(
    State(state): State<Arc<AppState>>,
    Path(group_id): Path<Uuid>,
    Json(req): Json<AddMemberRequest>,
) -> impl IntoResponse {
    match state.db.add_member(group_id, &req.user_id, req.role).await {
        Ok(member) => (StatusCode::CREATED, Json(member)).into_response(),
        Err(e) => {
            tracing::error!("Failed to add member: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Failed to add member" })),
            )
                .into_response()
        }
    }
}

/// Remove a member from a group
pub async fn remove_member(
    State(state): State<Arc<AppState>>,
    Path((group_id, user_id)): Path<(Uuid, String)>,
) -> impl IntoResponse {
    match state.db.remove_member(group_id, &user_id).await {
        Ok(true) => (StatusCode::NO_CONTENT, ()).into_response(),
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "Member not found" })),
        )
            .into_response(),
        Err(e) => {
            tracing::error!("Failed to remove member: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Failed to remove member" })),
            )
                .into_response()
        }
    }
}

/// Join a group (self-serve)
pub async fn join_group(
    State(state): State<Arc<AppState>>,
    Path(group_id): Path<Uuid>,
    Json(ctx): Json<UserContext>,
) -> impl IntoResponse {
    // Check if group exists and is public
    match state.db.get_group(group_id).await {
        Ok(Some(group)) => {
            if group.is_private {
                return (
                    StatusCode::FORBIDDEN,
                    Json(serde_json::json!({ "error": "Cannot join private group" })),
                )
                    .into_response();
            }
        }
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({ "error": "Group not found" })),
            )
                .into_response();
        }
        Err(e) => {
            tracing::error!("Failed to check group: {}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Failed to join group" })),
            )
                .into_response();
        }
    }

    match state
        .db
        .add_member(group_id, &ctx.user_id, MemberRole::Member)
        .await
    {
        Ok(member) => (StatusCode::OK, Json(member)).into_response(),
        Err(e) => {
            tracing::error!("Failed to join group: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Failed to join group" })),
            )
                .into_response()
        }
    }
}

/// Leave a group
pub async fn leave_group(
    State(state): State<Arc<AppState>>,
    Path(group_id): Path<Uuid>,
    Json(ctx): Json<UserContext>,
) -> impl IntoResponse {
    match state.db.remove_member(group_id, &ctx.user_id).await {
        Ok(_) => (StatusCode::NO_CONTENT, ()).into_response(),
        Err(e) => {
            tracing::error!("Failed to leave group: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Failed to leave group" })),
            )
                .into_response()
        }
    }
}

// ============================================================================
// Message Handlers
// ============================================================================

/// Send a message to a group
pub async fn send_message(
    State(state): State<Arc<AppState>>,
    Path(group_id): Path<Uuid>,
    Json(req): Json<SendMessageRequest>,
) -> impl IntoResponse {
    // Check if user is a member
    match state.db.is_member(group_id, &req.sender_id).await {
        Ok(true) => {}
        Ok(false) => {
            return (
                StatusCode::FORBIDDEN,
                Json(serde_json::json!({ "error": "Not a member of this group" })),
            )
                .into_response();
        }
        Err(e) => {
            tracing::error!("Failed to check membership: {}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Failed to send message" })),
            )
                .into_response();
        }
    }

    match state.db.send_message(group_id, &req).await {
        Ok(message) => {
            // Broadcast to real-time subscribers
            if let Ok(json) = serde_json::to_string(&message) {
                state.broadcast_to_group(group_id, json);
            }
            (StatusCode::CREATED, Json(message)).into_response()
        }
        Err(e) => {
            tracing::error!("Failed to send message: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Failed to send message" })),
            )
                .into_response()
        }
    }
}

/// Get messages from a group
pub async fn get_messages(
    State(state): State<Arc<AppState>>,
    Path(group_id): Path<Uuid>,
    Query(query): Query<MessageQuery>,
) -> impl IntoResponse {
    match state.db.get_messages(group_id, &query).await {
        Ok(messages) => Json(messages).into_response(),
        Err(e) => {
            tracing::error!("Failed to get messages: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Failed to get messages" })),
            )
                .into_response()
        }
    }
}

// ============================================================================
// User Handlers
// ============================================================================

/// Get a user's groups
pub async fn get_user_groups(
    State(state): State<Arc<AppState>>,
    Path(user_id): Path<String>,
) -> impl IntoResponse {
    match state.db.get_user_groups(&user_id).await {
        Ok(groups) => Json(groups).into_response(),
        Err(e) => {
            tracing::error!("Failed to get user groups: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Failed to get user groups" })),
            )
                .into_response()
        }
    }
}

// ============================================================================
// Health Handler
// ============================================================================

/// Health check endpoint
pub async fn health_check(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let db_healthy = state.db.health_check().await;

    let response = HealthResponse {
        status: if db_healthy { "healthy" } else { "unhealthy" }.to_string(),
        service: "chat-service".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        database: if db_healthy {
            "connected"
        } else {
            "disconnected"
        }
        .to_string(),
    };

    if db_healthy {
        (StatusCode::OK, Json(response))
    } else {
        (StatusCode::SERVICE_UNAVAILABLE, Json(response))
    }
}
