//! Shared data models used across Unhidra services

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// User model representing an authenticated user in the system
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct User {
    pub id: Uuid,
    pub username: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    pub created_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<DateTime<Utc>>,
    pub verified: bool,
}

impl User {
    /// Create a new user with the given username
    pub fn new(username: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            username: username.into(),
            display_name: None,
            email: None,
            created_at: Utc::now(),
            updated_at: None,
            verified: false,
        }
    }

    /// Get the display name or fall back to username
    pub fn display(&self) -> &str {
        self.display_name.as_deref().unwrap_or(&self.username)
    }
}

/// Chat message model
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Message {
    pub id: Uuid,
    pub content: String,
    pub sender_id: Uuid,
    pub sender_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub room_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group_id: Option<Uuid>,
    pub timestamp: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub edited_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub attachments: Vec<Attachment>,
    pub message_type: MessageType,
}

impl Message {
    /// Create a new text message
    pub fn new(content: impl Into<String>, sender_id: Uuid, sender_name: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            content: content.into(),
            sender_id,
            sender_name: sender_name.into(),
            room_id: None,
            group_id: None,
            timestamp: Utc::now(),
            edited_at: None,
            attachments: Vec::new(),
            message_type: MessageType::Text,
        }
    }

    /// Set the room ID for this message
    pub fn in_room(mut self, room_id: impl Into<String>) -> Self {
        self.room_id = Some(room_id.into());
        self
    }

    /// Set the group ID for this message
    pub fn in_group(mut self, group_id: Uuid) -> Self {
        self.group_id = Some(group_id);
        self
    }

    /// Add an attachment to this message
    pub fn with_attachment(mut self, attachment: Attachment) -> Self {
        self.attachments.push(attachment);
        self
    }
}

/// Message type enumeration
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum MessageType {
    #[default]
    Text,
    Image,
    File,
    System,
    Notification,
}

/// File attachment model
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Attachment {
    pub id: Uuid,
    pub filename: String,
    pub mime_type: String,
    pub size_bytes: u64,
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thumbnail_url: Option<String>,
    pub uploaded_at: DateTime<Utc>,
}

impl Attachment {
    /// Create a new attachment
    pub fn new(
        filename: impl Into<String>,
        mime_type: impl Into<String>,
        size_bytes: u64,
        url: impl Into<String>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            filename: filename.into(),
            mime_type: mime_type.into(),
            size_bytes,
            url: url.into(),
            thumbnail_url: None,
            uploaded_at: Utc::now(),
        }
    }
}

/// Group chat model
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Group {
    pub id: Uuid,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub owner_id: Uuid,
    pub members: Vec<GroupMember>,
    pub created_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<DateTime<Utc>>,
    pub is_private: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avatar_url: Option<String>,
}

impl Group {
    /// Create a new group with the given name and owner
    pub fn new(name: impl Into<String>, owner_id: Uuid) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            description: None,
            owner_id,
            members: vec![GroupMember::new(owner_id, GroupRole::Owner)],
            created_at: Utc::now(),
            updated_at: None,
            is_private: false,
            avatar_url: None,
        }
    }

    /// Add a member to the group
    pub fn add_member(&mut self, user_id: Uuid, role: GroupRole) {
        if !self.members.iter().any(|m| m.user_id == user_id) {
            self.members.push(GroupMember::new(user_id, role));
        }
    }

    /// Check if a user is a member of this group
    pub fn is_member(&self, user_id: Uuid) -> bool {
        self.members.iter().any(|m| m.user_id == user_id)
    }

    /// Check if a user is an admin or owner
    pub fn is_admin(&self, user_id: Uuid) -> bool {
        self.members
            .iter()
            .any(|m| m.user_id == user_id && matches!(m.role, GroupRole::Owner | GroupRole::Admin))
    }
}

/// Group member with role
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GroupMember {
    pub user_id: Uuid,
    pub role: GroupRole,
    pub joined_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nickname: Option<String>,
}

impl GroupMember {
    pub fn new(user_id: Uuid, role: GroupRole) -> Self {
        Self {
            user_id,
            role,
            joined_at: Utc::now(),
            nickname: None,
        }
    }
}

/// Role within a group
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum GroupRole {
    Owner,
    Admin,
    Moderator,
    #[default]
    Member,
}

/// IoT Device model
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Device {
    pub id: Uuid,
    pub device_id: String,
    pub name: String,
    pub device_type: DeviceType,
    pub owner_id: Uuid,
    pub status: DeviceStatus,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub capabilities: Vec<String>,
    pub registered_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_seen: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub firmware_version: Option<String>,
}

impl Device {
    pub fn new(
        device_id: impl Into<String>,
        name: impl Into<String>,
        device_type: DeviceType,
        owner_id: Uuid,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            device_id: device_id.into(),
            name: name.into(),
            device_type,
            owner_id,
            status: DeviceStatus::Offline,
            capabilities: Vec::new(),
            registered_at: Utc::now(),
            last_seen: None,
            firmware_version: None,
        }
    }
}

/// Device type enumeration
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum DeviceType {
    #[default]
    Sensor,
    Actuator,
    Controller,
    Gateway,
    Other,
}

/// Device status
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum DeviceStatus {
    Online,
    #[default]
    Offline,
    Maintenance,
    Error,
}

/// User presence status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Presence {
    pub user_id: Uuid,
    pub status: PresenceStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub custom_status: Option<String>,
    pub last_active: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_room: Option<String>,
}

impl Presence {
    pub fn new(user_id: Uuid, status: PresenceStatus) -> Self {
        Self {
            user_id,
            status,
            custom_status: None,
            last_active: Utc::now(),
            current_room: None,
        }
    }
}

/// Presence status enumeration
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum PresenceStatus {
    Online,
    Away,
    DoNotDisturb,
    Invisible,
    #[default]
    Offline,
}

/// Typing indicator
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TypingIndicator {
    pub user_id: Uuid,
    pub username: String,
    pub room_id: String,
    pub started_at: DateTime<Utc>,
}

/// Pagination parameters for list requests
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Pagination {
    #[serde(default = "default_page")]
    pub page: u32,
    #[serde(default = "default_per_page")]
    pub per_page: u32,
}

fn default_page() -> u32 {
    1
}

fn default_per_page() -> u32 {
    20
}

impl Pagination {
    pub fn new(page: u32, per_page: u32) -> Self {
        Self { page, per_page }
    }

    /// Calculate the offset for database queries
    pub fn offset(&self) -> u32 {
        (self.page.saturating_sub(1)) * self.per_page
    }
}

/// Paginated response wrapper
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaginatedResponse<T> {
    pub data: Vec<T>,
    pub pagination: PaginationInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaginationInfo {
    pub current_page: u32,
    pub per_page: u32,
    pub total_items: u64,
    pub total_pages: u32,
}

impl PaginationInfo {
    pub fn new(current_page: u32, per_page: u32, total_items: u64) -> Self {
        let total_pages = ((total_items as f64) / (per_page as f64)).ceil() as u32;
        Self {
            current_page,
            per_page,
            total_items,
            total_pages,
        }
    }
}

/// API response wrapper for consistent response format
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status")]
pub enum ApiResponse<T> {
    #[serde(rename = "success")]
    Success { data: T },
    #[serde(rename = "error")]
    Error { error: ApiErrorInfo },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiErrorInfo {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

impl<T> ApiResponse<T> {
    pub fn success(data: T) -> Self {
        Self::Success { data }
    }

    pub fn error(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self::Error {
            error: ApiErrorInfo {
                code: code.into(),
                message: message.into(),
                details: None,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_user_creation() {
        let user = User::new("alice");
        assert_eq!(user.username, "alice");
        assert!(!user.verified);
        assert_eq!(user.display(), "alice");
    }

    #[test]
    fn test_user_display_name() {
        let mut user = User::new("bob");
        user.display_name = Some("Bob Smith".to_string());
        assert_eq!(user.display(), "Bob Smith");
    }

    #[test]
    fn test_message_creation() {
        let sender_id = Uuid::new_v4();
        let msg = Message::new("Hello, world!", sender_id, "alice")
            .in_room("general");

        assert_eq!(msg.content, "Hello, world!");
        assert_eq!(msg.sender_id, sender_id);
        assert_eq!(msg.room_id, Some("general".to_string()));
        assert_eq!(msg.message_type, MessageType::Text);
    }

    #[test]
    fn test_group_membership() {
        let owner_id = Uuid::new_v4();
        let member_id = Uuid::new_v4();

        let mut group = Group::new("Test Group", owner_id);
        assert!(group.is_member(owner_id));
        assert!(group.is_admin(owner_id));

        group.add_member(member_id, GroupRole::Member);
        assert!(group.is_member(member_id));
        assert!(!group.is_admin(member_id));
    }

    #[test]
    fn test_pagination() {
        let pagination = Pagination::new(2, 20);
        assert_eq!(pagination.offset(), 20);

        let pagination_first = Pagination::new(1, 10);
        assert_eq!(pagination_first.offset(), 0);
    }

    #[test]
    fn test_api_response_serialization() {
        let response: ApiResponse<String> = ApiResponse::success("test".to_string());
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("\"status\":\"success\""));

        let error_response: ApiResponse<()> = ApiResponse::error("not_found", "Resource not found");
        let error_json = serde_json::to_string(&error_response).unwrap();
        assert!(error_json.contains("\"status\":\"error\""));
    }
}
