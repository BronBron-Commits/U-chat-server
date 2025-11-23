//! Data models for history service

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Stored message record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredMessage {
    pub id: Uuid,
    pub content: String,
    pub sender_id: String,
    pub sender_name: String,
    pub room_id: Option<String>,
    pub group_id: Option<Uuid>,
    pub message_type: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: Option<DateTime<Utc>>,
}

/// Request to store a new message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoreMessageRequest {
    pub content: String,
    pub sender_id: String,
    pub sender_name: String,
    #[serde(default)]
    pub room_id: Option<String>,
    #[serde(default)]
    pub group_id: Option<Uuid>,
    #[serde(default = "default_message_type")]
    pub message_type: String,
}

fn default_message_type() -> String {
    "text".to_string()
}

/// Query parameters for message retrieval
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MessageQuery {
    #[serde(default = "default_limit")]
    pub limit: i64,
    #[serde(default)]
    pub offset: i64,
    #[serde(default)]
    pub before: Option<DateTime<Utc>>,
    #[serde(default)]
    pub after: Option<DateTime<Utc>>,
}

fn default_limit() -> i64 {
    50
}

/// Search query parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchQuery {
    pub q: String,
    #[serde(default = "default_limit")]
    pub limit: i64,
    #[serde(default)]
    pub offset: i64,
    #[serde(default)]
    pub room_id: Option<String>,
}

/// Paginated response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaginatedMessages {
    pub messages: Vec<StoredMessage>,
    pub total: i64,
    pub limit: i64,
    pub offset: i64,
    pub has_more: bool,
}

/// Health check response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthResponse {
    pub status: String,
    pub service: String,
    pub version: String,
    pub database: String,
}
