//! Thread handlers for threaded conversations
//!
//! Threads allow users to create reply chains to specific messages,
//! organizing conversations and reducing noise in main channels.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use unhidra_core::{
    error::ApiError,
    models::Pagination,
};

/// Create thread request
#[derive(Debug, Deserialize)]
pub struct CreateThreadRequest {
    pub channel_id: String,
    pub parent_message_id: String,
    pub content: String,
}

/// Thread response
#[derive(Debug, Serialize)]
pub struct ThreadResponse {
    pub id: String,
    pub channel_id: String,
    pub parent_message_id: String,
    pub reply_count: i64,
    pub participant_count: i64,
    pub last_reply_at: Option<String>,
    pub created_at: String,
}

/// Thread reply response
#[derive(Debug, Serialize)]
pub struct ThreadReplyResponse {
    pub id: String,
    pub thread_id: String,
    pub sender_id: String,
    pub content: String,
    pub created_at: String,
    pub edited_at: Option<String>,
}

/// Create a new thread (reply to a message)
pub async fn create_thread(
    State(pool): State<PgPool>,
    crate::auth::AuthUser(user_id): crate::auth::AuthUser,
    Json(req): Json<CreateThreadRequest>,
) -> Result<Json<ThreadResponse>, ApiError> {
    let thread_id = Uuid::new_v4().to_string();
    let message_id = Uuid::new_v4().to_string();

    // Verify user is a channel member
    let _membership = sqlx::query!(
        r#"
        SELECT user_id FROM channel_members
        WHERE channel_id = $1 AND user_id = $2
        "#,
        req.channel_id,
        &user_id
    )
    .fetch_optional(&pool)
    .await
    .map_err(|e| ApiError::Database(e.to_string()))?
    .ok_or(ApiError::Forbidden("Not a channel member".to_string()))?;

    // Verify parent message exists
    let parent_msg = sqlx::query!(
        r#"
        SELECT id, channel_id FROM messages
        WHERE id = $1 AND deleted_at IS NULL
        "#,
        req.parent_message_id
    )
    .fetch_optional(&pool)
    .await
    .map_err(|e| ApiError::Database(e.to_string()))?
    .ok_or(ApiError::NotFound("Parent message not found".to_string()))?;

    if parent_msg.channel_id != req.channel_id {
        return Err(ApiError::Validation(
            "Message not in specified channel".to_string(),
        ));
    }

    // Begin transaction
    let mut tx = pool.begin().await
        .map_err(|e| ApiError::Database(e.to_string()))?;

    // Create or get thread
    let thread = sqlx::query!(
        r#"
        INSERT INTO threads (id, channel_id, parent_message_id)
        VALUES ($1, $2, $3)
        ON CONFLICT (parent_message_id)
        DO UPDATE SET reply_count = threads.reply_count
        RETURNING id, channel_id, parent_message_id, reply_count,
                  participant_count, last_reply_at::text, created_at::text as "created_at!"
        "#,
        thread_id,
        req.channel_id,
        req.parent_message_id
    )
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| ApiError::Database(e.to_string()))?;

    // Insert reply message
    sqlx::query!(
        r#"
        INSERT INTO messages (id, channel_id, sender_id, thread_id, content, content_type)
        VALUES ($1, $2, $3, $4, $5, 'text')
        "#,
        message_id,
        req.channel_id,
        user_id,
        thread.id,
        req.content
    )
    .execute(&mut *tx)
    .await
    .map_err(|e| ApiError::Database(e.to_string()))?;

    // Update thread counts
    sqlx::query!(
        r#"
        UPDATE threads
        SET reply_count = reply_count + 1,
            last_reply_at = datetime('now', 'utc')
        WHERE id = $1
        "#,
        thread.id
    )
    .execute(&mut *tx)
    .await
    .map_err(|e| ApiError::Database(e.to_string()))?;

    // Add user as thread participant
    sqlx::query!(
        r#"
        INSERT INTO thread_participants (thread_id, user_id)
        VALUES ($1, $2)
        ON CONFLICT (thread_id, user_id) DO NOTHING
        "#,
        thread.id,
        user_id
    )
    .execute(&mut *tx)
    .await
    .map_err(|e| ApiError::Database(e.to_string()))?;

    // Update participant count
    sqlx::query!(
        r#"
        UPDATE threads
        SET participant_count = (
            SELECT COUNT(DISTINCT user_id)
            FROM thread_participants
            WHERE thread_id = $1
        )
        WHERE id = $1
        "#,
        thread.id
    )
    .execute(&mut *tx)
    .await
    .map_err(|e| ApiError::Database(e.to_string()))?;

    tx.commit().await
        .map_err(|e| ApiError::Database(e.to_string()))?;

    Ok(Json(ThreadResponse {
        id: thread.id,
        channel_id: thread.channel_id,
        parent_message_id: thread.parent_message_id,
        reply_count: thread.reply_count + 1,
        participant_count: thread.participant_count + 1,
        last_reply_at: thread.last_reply_at,
        created_at: thread.created_at,
    }))
}

/// List replies in a thread
pub async fn list_thread_replies(
    State(pool): State<PgPool>,
    crate::auth::AuthUser(user_id): crate::auth::AuthUser,
    Path(thread_id): Path<String>,
    Query(pagination): Query<Pagination>,
) -> Result<Json<Vec<ThreadReplyResponse>>, ApiError> {

    let limit = pagination.per_page.min(100) as i64;
    let offset = pagination.offset() as i64;

    // Verify thread exists and user has access
    let thread = sqlx::query!(
        r#"
        SELECT t.channel_id
        FROM threads t
        JOIN channel_members cm ON t.channel_id = cm.channel_id
        WHERE t.id = $1 AND cm.user_id = $2
        "#,
        thread_id,
        &user_id
    )
    .fetch_optional(&pool)
    .await
    .map_err(|e| ApiError::Database(e.to_string()))?
    .ok_or(ApiError::NotFound("Thread not found or access denied".to_string()))?;

    // Fetch replies
    let replies = sqlx::query!(
        r#"
        SELECT id, sender_id, content,
               created_at::text as "created_at!",
               edited_at::text
        FROM messages
        WHERE thread_id = $1 AND deleted_at IS NULL
        ORDER BY created_at ASC
        LIMIT $2 OFFSET $3
        "#,
        thread_id,
        limit,
        offset
    )
    .fetch_all(&pool)
    .await
    .map_err(|e| ApiError::Database(e.to_string()))?;

    let response = replies
        .into_iter()
        .map(|r| ThreadReplyResponse {
            id: r.id,
            thread_id: thread_id.clone(),
            sender_id: r.sender_id,
            content: r.content,
            created_at: r.created_at,
            edited_at: r.edited_at,
        })
        .collect();

    Ok(Json(response))
}

/// Get thread details
pub async fn get_thread(
    State(pool): State<PgPool>,
    crate::auth::AuthUser(user_id): crate::auth::AuthUser,
    Path(thread_id): Path<String>,
) -> Result<Json<ThreadResponse>, ApiError> {

    let thread = sqlx::query!(
        r#"
        SELECT
            t.id, t.channel_id, t.parent_message_id,
            t.reply_count, t.participant_count,
            t.last_reply_at::text, t.created_at::text as "created_at!"
        FROM threads t
        JOIN channel_members cm ON t.channel_id = cm.channel_id
        WHERE t.id = $1 AND cm.user_id = $2
        "#,
        thread_id,
        &user_id
    )
    .fetch_optional(&pool)
    .await
    .map_err(|e| ApiError::Database(e.to_string()))?
    .ok_or(ApiError::NotFound("Thread not found or access denied".to_string()))?;

    Ok(Json(ThreadResponse {
        id: thread.id,
        channel_id: thread.channel_id,
        parent_message_id: thread.parent_message_id,
        reply_count: thread.reply_count,
        participant_count: thread.participant_count,
        last_reply_at: thread.last_reply_at,
        created_at: thread.created_at,
    }))
}

/// Add participant to thread
pub async fn add_thread_participant(
    State(pool): State<PgPool>,
    crate::auth::AuthUser(user_id): crate::auth::AuthUser,
    Path(thread_id): Path<String>,
    Json(req): Json<AddParticipantRequest>,
) -> Result<StatusCode, ApiError> {

    // Verify thread exists and current user has access
    let thread = sqlx::query!(
        r#"
        SELECT t.channel_id
        FROM threads t
        JOIN channel_members cm ON t.channel_id = cm.channel_id
        WHERE t.id = $1 AND cm.user_id = $2
        "#,
        thread_id,
        &user_id
    )
    .fetch_optional(&pool)
    .await
    .map_err(|e| ApiError::Database(e.to_string()))?
    .ok_or(ApiError::NotFound("Thread not found or access denied".to_string()))?;

    // Verify new participant is a channel member
    let _member = sqlx::query!(
        r#"
        SELECT user_id FROM channel_members
        WHERE channel_id = $1 AND user_id = $2
        "#,
        thread.channel_id,
        req.user_id
    )
    .fetch_optional(&pool)
    .await
    .map_err(|e| ApiError::Database(e.to_string()))?
    .ok_or(ApiError::Validation(
        "User is not a channel member".to_string(),
    ))?;

    // Add participant
    sqlx::query!(
        r#"
        INSERT INTO thread_participants (thread_id, user_id)
        VALUES ($1, $2)
        ON CONFLICT (thread_id, user_id) DO NOTHING
        "#,
        thread_id,
        req.user_id
    )
    .execute(&pool)
    .await
    .map_err(|e| ApiError::Database(e.to_string()))?;

    // Update participant count
    sqlx::query!(
        r#"
        UPDATE threads
        SET participant_count = (
            SELECT COUNT(DISTINCT user_id)
            FROM thread_participants
            WHERE thread_id = $1
        )
        WHERE id = $1
        "#,
        thread_id
    )
    .execute(&pool)
    .await
    .map_err(|e| ApiError::Database(e.to_string()))?;

    Ok(StatusCode::CREATED)
}

/// Mark thread as read
pub async fn mark_thread_read(
    State(pool): State<PgPool>,
    crate::auth::AuthUser(user_id): crate::auth::AuthUser,
    Path(thread_id): Path<String>,
) -> Result<StatusCode, ApiError> {

    // Update last_read_at for participant
    sqlx::query!(
        r#"
        UPDATE thread_participants
        SET last_read_at = datetime('now', 'utc')
        WHERE thread_id = $1 AND user_id = $2
        "#,
        thread_id,
        &user_id
    )
    .execute(&pool)
    .await
    .map_err(|e| ApiError::Database(e.to_string()))?;

    Ok(StatusCode::NO_CONTENT)
}

/// Add participant request
#[derive(Debug, Deserialize)]
pub struct AddParticipantRequest {
    pub user_id: String,
}
