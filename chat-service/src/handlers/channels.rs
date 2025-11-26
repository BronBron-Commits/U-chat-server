//! Channel handlers for multi-user chat rooms

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use unhidra_core::{
    audit::{self, AuditAction, AuditEvent},
    error::ApiError,
    models::Pagination,
};

/// Create channel request
#[derive(Debug, Deserialize)]
pub struct CreateChannelRequest {
    pub name: String,
    pub description: Option<String>,
    pub channel_type: ChannelType,
}

/// Channel type
#[derive(Debug, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "text")]
pub enum ChannelType {
    #[serde(rename = "public")]
    Public,
    #[serde(rename = "private")]
    Private,
    #[serde(rename = "direct")]
    Direct,
}

impl std::fmt::Display for ChannelType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ChannelType::Public => write!(f, "public"),
            ChannelType::Private => write!(f, "private"),
            ChannelType::Direct => write!(f, "direct"),
        }
    }
}

/// Channel response
#[derive(Debug, Serialize)]
pub struct ChannelResponse {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub channel_type: String,
    pub created_by: String,
    pub created_at: String,
    pub member_count: i64,
    pub unread_count: Option<i64>,
}

/// Create a new channel
pub async fn create_channel(
    State(pool): State<PgPool>,
    crate::auth::AuthUser(creator_id): crate::auth::AuthUser,
    Json(req): Json<CreateChannelRequest>,
) -> Result<Json<ChannelResponse>, ApiError> {
    let channel_id = Uuid::new_v4().to_string();

    let channel = sqlx::query!(
        r#"
        INSERT INTO channels (id, name, description, channel_type, created_by)
        VALUES ($1, $2, $3, $4, $5)
        RETURNING id, name, description, channel_type, created_by,
                  created_at::text as "created_at!"
        "#,
        channel_id,
        req.name,
        req.description,
        req.channel_type.to_string(),
        creator_id
    )
    .fetch_one(&pool)
    .await
    .map_err(|e| ApiError::Database(e.to_string()))?;

    // Add creator as admin member
    sqlx::query!(
        r#"
        INSERT INTO channel_members (channel_id, user_id, role)
        VALUES ($1, $2, 'admin')
        "#,
        channel_id,
        &creator_id
    )
    .execute(&pool)
    .await
    .map_err(|e| ApiError::Database(e.to_string()))?;

    // Audit log channel creation
    let audit_event = AuditEvent::new(&creator_id, AuditAction::RoomCreated)
        .with_service("chat-service")
        .with_resource("channel", &channel_id)
        .with_metadata("channel_name", &req.name)
        .with_metadata("channel_type", &req.channel_type.to_string());
    let _ = audit::log(audit_event).await;

    Ok(Json(ChannelResponse {
        id: channel.id,
        name: channel.name,
        description: channel.description,
        channel_type: channel.channel_type,
        created_by: channel.created_by,
        created_at: channel.created_at,
        member_count: 1,
        unread_count: Some(0),
    }))
}

/// List channels for current user
pub async fn list_channels(
    State(pool): State<PgPool>,
    crate::auth::AuthUser(user_id): crate::auth::AuthUser,
    Query(pagination): Query<Pagination>,
) -> Result<Json<Vec<ChannelResponse>>, ApiError> {
    let limit = pagination.per_page.min(100) as i64;
    let offset = pagination.offset() as i64;

    let channels = sqlx::query!(
        r#"
        SELECT
            c.id, c.name, c.description, c.channel_type, c.created_by,
            c.created_at::text as "created_at!",
            COUNT(DISTINCT cm.user_id) as "member_count!",
            COUNT(DISTINCT m.id) FILTER (
                WHERE m.created_at > COALESCE(rr.last_read_at, c.created_at)
                AND m.sender_id != $1
            ) as "unread_count!"
        FROM channels c
        LEFT JOIN channel_members cm ON c.id = cm.channel_id
        LEFT JOIN messages m ON c.id = m.channel_id AND m.deleted_at IS NULL
        LEFT JOIN read_receipts rr ON c.id = rr.channel_id AND rr.user_id = $1
        WHERE c.id IN (
            SELECT channel_id FROM channel_members WHERE user_id = $1
        )
        GROUP BY c.id, c.name, c.description, c.channel_type, c.created_by, c.created_at, rr.last_read_at
        ORDER BY c.created_at DESC
        LIMIT $2 OFFSET $3
        "#,
        &user_id,
        limit,
        offset
    )
    .fetch_all(&pool)
    .await
    .map_err(|e| ApiError::Database(e.to_string()))?;

    let response = channels
        .into_iter()
        .map(|c| ChannelResponse {
            id: c.id,
            name: c.name,
            description: c.description,
            channel_type: c.channel_type,
            created_by: c.created_by,
            created_at: c.created_at,
            member_count: c.member_count,
            unread_count: Some(c.unread_count),
        })
        .collect();

    Ok(Json(response))
}

/// Get channel by ID
pub async fn get_channel(
    State(pool): State<PgPool>,
    crate::auth::AuthUser(user_id): crate::auth::AuthUser,
    Path(channel_id): Path<String>,
) -> Result<Json<ChannelResponse>, ApiError> {
    // Verify user is a member
    let _membership = sqlx::query!(
        r#"
        SELECT user_id FROM channel_members
        WHERE channel_id = $1 AND user_id = $2
        "#,
        channel_id,
        &user_id
    )
    .fetch_optional(&pool)
    .await
    .map_err(|e| ApiError::Database(e.to_string()))?
    .ok_or(ApiError::Forbidden("Not a channel member".to_string()))?;

    let channel = sqlx::query!(
        r#"
        SELECT
            c.id, c.name, c.description, c.channel_type, c.created_by,
            c.created_at::text as "created_at!",
            COUNT(DISTINCT cm.user_id) as "member_count!"
        FROM channels c
        LEFT JOIN channel_members cm ON c.id = cm.channel_id
        WHERE c.id = $1
        GROUP BY c.id
        "#,
        channel_id
    )
    .fetch_optional(&pool)
    .await
    .map_err(|e| ApiError::Database(e.to_string()))?
    .ok_or(ApiError::NotFound("Channel not found".to_string()))?;

    Ok(Json(ChannelResponse {
        id: channel.id,
        name: channel.name,
        description: channel.description,
        channel_type: channel.channel_type,
        created_by: channel.created_by,
        created_at: channel.created_at,
        member_count: channel.member_count,
        unread_count: None,
    }))
}

/// Add member request
#[derive(Debug, Deserialize)]
pub struct AddMemberRequest {
    pub user_id: String,
    pub role: Option<String>,
}

/// Add member to channel
pub async fn add_member(
    State(pool): State<PgPool>,
    crate::auth::AuthUser(current_user): crate::auth::AuthUser,
    Path(channel_id): Path<String>,
    Json(req): Json<AddMemberRequest>,
) -> Result<StatusCode, ApiError> {
    // Verify current user is admin
    let membership = sqlx::query!(
        r#"
        SELECT role FROM channel_members
        WHERE channel_id = $1 AND user_id = $2
        "#,
        channel_id,
        &current_user
    )
    .fetch_optional(&pool)
    .await
    .map_err(|e| ApiError::Database(e.to_string()))?
    .ok_or(ApiError::Forbidden("Not a channel member".to_string()))?;

    if membership.role != "admin" && membership.role != "owner" {
        return Err(ApiError::Forbidden(
            "Only admins can add members".to_string(),
        ));
    }

    // Add new member
    sqlx::query!(
        r#"
        INSERT INTO channel_members (channel_id, user_id, role)
        VALUES ($1, $2, $3)
        ON CONFLICT (channel_id, user_id) DO NOTHING
        "#,
        channel_id,
        req.user_id,
        req.role.unwrap_or_else(|| "member".to_string())
    )
    .execute(&pool)
    .await
    .map_err(|e| ApiError::Database(e.to_string()))?;

    Ok(StatusCode::CREATED)
}

/// Mark channel as read
pub async fn mark_as_read(
    State(pool): State<PgPool>,
    crate::auth::AuthUser(user_id): crate::auth::AuthUser,
    Path(channel_id): Path<String>,
) -> Result<StatusCode, ApiError> {
    // Get latest message ID
    let latest_msg = sqlx::query!(
        r#"
        SELECT id FROM messages
        WHERE channel_id = $1 AND deleted_at IS NULL
        ORDER BY created_at DESC
        LIMIT 1
        "#,
        channel_id
    )
    .fetch_optional(&pool)
    .await
    .map_err(|e| ApiError::Database(e.to_string()))?;

    if let Some(msg) = latest_msg {
        sqlx::query!(
            r#"
            INSERT INTO read_receipts (channel_id, user_id, last_read_message_id)
            VALUES ($1, $2, $3)
            ON CONFLICT (channel_id, user_id)
            DO UPDATE SET
                last_read_message_id = $3,
                last_read_at = datetime('now', 'utc')
            "#,
            channel_id,
            &user_id,
            msg.id
        )
        .execute(&pool)
        .await
        .map_err(|e| ApiError::Database(e.to_string()))?;
    }

    Ok(StatusCode::NO_CONTENT)
}
