//! Data models for chat service

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Group chat model
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Group {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub owner_id: String,
    pub is_private: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: Option<DateTime<Utc>>,
}

/// Request to create a new group
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateGroupRequest {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    pub owner_id: String,
    #[serde(default)]
    pub is_private: bool,
}

/// Request to update a group
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateGroupRequest {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub is_private: Option<bool>,
}

/// Group member model
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupMember {
    pub group_id: Uuid,
    pub user_id: String,
    pub role: MemberRole,
    pub joined_at: DateTime<Utc>,
}

/// Member role in a group
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MemberRole {
    Owner,
    Admin,
    Moderator,
    Member,
}

impl Default for MemberRole {
    fn default() -> Self {
        Self::Member
    }
}

impl MemberRole {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "owner" => Self::Owner,
            "admin" => Self::Admin,
            "moderator" => Self::Moderator,
            _ => Self::Member,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Owner => "owner",
            Self::Admin => "admin",
            Self::Moderator => "moderator",
            Self::Member => "member",
        }
    }
}

/// Request to add a member to a group
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddMemberRequest {
    pub user_id: String,
    #[serde(default)]
    pub role: MemberRole,
}

/// Chat message model
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub id: Uuid,
    pub group_id: Uuid,
    pub sender_id: String,
    pub sender_name: String,
    pub content: String,
    pub message_type: MessageType,
    pub created_at: DateTime<Utc>,
    pub edited_at: Option<DateTime<Utc>>,
}

/// Message type
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum MessageType {
    #[default]
    Text,
    Image,
    File,
    System,
}

impl MessageType {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "image" => Self::Image,
            "file" => Self::File,
            "system" => Self::System,
            _ => Self::Text,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Image => "image",
            Self::File => "file",
            Self::System => "system",
        }
    }
}

/// Request to send a message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendMessageRequest {
    pub sender_id: String,
    pub sender_name: String,
    pub content: String,
    #[serde(default)]
    pub message_type: MessageType,
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

/// Group with member count
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupWithStats {
    #[serde(flatten)]
    pub group: Group,
    pub member_count: i64,
    pub message_count: i64,
}

/// Health check response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthResponse {
    pub status: String,
    pub service: String,
    pub version: String,
    pub database: String,
}
