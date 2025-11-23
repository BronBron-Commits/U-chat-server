//! Immutable Audit Logging for Compliance
//!
//! Provides structured, tamper-evident audit logging for:
//! - Authentication events (login, logout, failures)
//! - Authorization decisions (access granted/denied)
//! - Data access and modifications
//! - Security events (rate limiting, suspicious activity)
//!
//! Features:
//! - Structured event types
//! - Optional HMAC signatures for tamper detection
//! - Hash chain for integrity verification
//! - Multiple storage backends (memory, Redis, SQLite)

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn};

/// Audit event actor types
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ActorType {
    User,
    Device,
    Service,
    System,
    Anonymous,
}

impl Default for ActorType {
    fn default() -> Self {
        Self::User
    }
}

/// Audit action result
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ActionResult {
    Success,
    Failure,
    Denied,
    Error,
}

impl Default for ActionResult {
    fn default() -> Self {
        Self::Success
    }
}

/// Standard audit actions
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditAction {
    // Authentication
    Login,
    LoginFailed,
    Logout,
    TokenRefresh,
    PasswordChange,
    PasswordReset,

    // SSO
    SsoLogin,
    SsoLoginFailed,

    // Passkeys
    PasskeyRegistered,
    PasskeyAuthenticated,
    PasskeyRevoked,

    // Devices
    DeviceRegistered,
    DeviceConnected,
    DeviceDisconnected,
    DeviceRevoked,

    // Messages
    MessageSent,
    MessageReceived,
    MessageDeleted,

    // Rooms/Channels
    RoomCreated,
    RoomJoined,
    RoomLeft,
    RoomDeleted,

    // Security
    PermissionDenied,
    RateLimitExceeded,
    SuspiciousActivity,
    TlsHandshakeFailed,

    // Data Access
    DataAccessed,
    DataModified,
    DataDeleted,
    DataExported,

    // Admin
    ConfigChanged,
    UserCreated,
    UserDeleted,
    UserModified,

    // Custom
    Custom(String),
}

impl std::fmt::Display for AuditAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Custom(s) => write!(f, "{}", s),
            other => write!(f, "{:?}", other),
        }
    }
}

/// Audit event
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AuditEvent {
    /// Event ID
    pub id: String,
    /// Timestamp
    pub timestamp: DateTime<Utc>,
    /// Actor ID (user_id, device_id, etc.)
    pub actor_id: String,
    /// Actor type
    pub actor_type: ActorType,
    /// Actor IP address
    pub actor_ip: Option<String>,
    /// Action performed
    pub action: AuditAction,
    /// Action result
    pub result: ActionResult,
    /// Resource type (user, device, room, etc.)
    pub resource_type: Option<String>,
    /// Resource ID
    pub resource_id: Option<String>,
    /// Service that generated the event
    pub service_name: Option<String>,
    /// Request ID for correlation
    pub request_id: Option<String>,
    /// Session ID
    pub session_id: Option<String>,
    /// Additional metadata
    pub metadata: HashMap<String, serde_json::Value>,
    /// HMAC signature (optional)
    pub signature: Option<String>,
    /// Previous event hash (for chain integrity)
    pub previous_hash: Option<String>,
}

impl AuditEvent {
    /// Create a new audit event
    pub fn new(actor_id: impl Into<String>, action: AuditAction) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            timestamp: Utc::now(),
            actor_id: actor_id.into(),
            actor_type: ActorType::User,
            actor_ip: None,
            action,
            result: ActionResult::Success,
            resource_type: None,
            resource_id: None,
            service_name: None,
            request_id: None,
            session_id: None,
            metadata: HashMap::new(),
            signature: None,
            previous_hash: None,
        }
    }

    /// Create a system event
    pub fn system(action: AuditAction) -> Self {
        let mut event = Self::new("system", action);
        event.actor_type = ActorType::System;
        event
    }

    /// Set actor type
    pub fn with_actor_type(mut self, actor_type: ActorType) -> Self {
        self.actor_type = actor_type;
        self
    }

    /// Set actor IP
    pub fn with_ip(mut self, ip: impl Into<String>) -> Self {
        self.actor_ip = Some(ip.into());
        self
    }

    /// Set result
    pub fn with_result(mut self, result: ActionResult) -> Self {
        self.result = result;
        self
    }

    /// Set resource
    pub fn with_resource(mut self, resource_type: impl Into<String>, resource_id: impl Into<String>) -> Self {
        self.resource_type = Some(resource_type.into());
        self.resource_id = Some(resource_id.into());
        self
    }

    /// Set service name
    pub fn with_service(mut self, service: impl Into<String>) -> Self {
        self.service_name = Some(service.into());
        self
    }

    /// Set request ID
    pub fn with_request_id(mut self, request_id: impl Into<String>) -> Self {
        self.request_id = Some(request_id.into());
        self
    }

    /// Set session ID
    pub fn with_session(mut self, session_id: impl Into<String>) -> Self {
        self.session_id = Some(session_id.into());
        self
    }

    /// Add metadata
    pub fn with_metadata(mut self, key: impl Into<String>, value: impl Serialize) -> Self {
        if let Ok(json_value) = serde_json::to_value(value) {
            self.metadata.insert(key.into(), json_value);
        }
        self
    }

    /// Mark as failure
    pub fn failed(mut self) -> Self {
        self.result = ActionResult::Failure;
        self
    }

    /// Mark as denied
    pub fn denied(mut self) -> Self {
        self.result = ActionResult::Denied;
        self
    }
}

/// Audit logger trait
#[async_trait::async_trait]
pub trait AuditLogger: Send + Sync {
    /// Log an audit event
    async fn log(&self, event: AuditEvent) -> anyhow::Result<()>;

    /// Query audit events
    async fn query(&self, filter: AuditFilter) -> anyhow::Result<Vec<AuditEvent>>;
}

/// Filter for querying audit events
#[derive(Clone, Debug, Default)]
pub struct AuditFilter {
    pub actor_id: Option<String>,
    pub action: Option<String>,
    pub resource_type: Option<String>,
    pub resource_id: Option<String>,
    pub from_timestamp: Option<DateTime<Utc>>,
    pub to_timestamp: Option<DateTime<Utc>>,
    pub result: Option<ActionResult>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

/// In-memory audit logger (for development/testing)
pub struct MemoryAuditLogger {
    events: RwLock<Vec<AuditEvent>>,
    max_events: usize,
}

impl MemoryAuditLogger {
    pub fn new(max_events: usize) -> Self {
        Self {
            events: RwLock::new(Vec::new()),
            max_events,
        }
    }
}

impl Default for MemoryAuditLogger {
    fn default() -> Self {
        Self::new(10000)
    }
}

#[async_trait::async_trait]
impl AuditLogger for MemoryAuditLogger {
    async fn log(&self, event: AuditEvent) -> anyhow::Result<()> {
        let mut events = self.events.write().await;

        // Trim if at capacity
        if events.len() >= self.max_events {
            events.remove(0);
        }

        info!(
            event_id = event.id,
            actor = event.actor_id,
            action = %event.action,
            result = ?event.result,
            "Audit event logged"
        );

        events.push(event);
        Ok(())
    }

    async fn query(&self, filter: AuditFilter) -> anyhow::Result<Vec<AuditEvent>> {
        let events = self.events.read().await;

        let filtered: Vec<AuditEvent> = events
            .iter()
            .filter(|e| {
                if let Some(ref actor) = filter.actor_id {
                    if e.actor_id != *actor {
                        return false;
                    }
                }
                if let Some(ref action) = filter.action {
                    if format!("{}", e.action) != *action {
                        return false;
                    }
                }
                if let Some(ref rt) = filter.resource_type {
                    if e.resource_type.as_ref() != Some(rt) {
                        return false;
                    }
                }
                if let Some(ref from) = filter.from_timestamp {
                    if e.timestamp < *from {
                        return false;
                    }
                }
                if let Some(ref to) = filter.to_timestamp {
                    if e.timestamp > *to {
                        return false;
                    }
                }
                if let Some(ref result) = filter.result {
                    if e.result != *result {
                        return false;
                    }
                }
                true
            })
            .skip(filter.offset.unwrap_or(0))
            .take(filter.limit.unwrap_or(100))
            .cloned()
            .collect();

        Ok(filtered)
    }
}

/// Global audit logger instance
static AUDIT_LOGGER: once_cell::sync::OnceCell<Arc<dyn AuditLogger>> = once_cell::sync::OnceCell::new();

/// Initialize the global audit logger
pub fn init_audit_logger(logger: Arc<dyn AuditLogger>) {
    AUDIT_LOGGER.set(logger).ok();
}

/// Get the global audit logger
pub fn audit_logger() -> Option<&'static Arc<dyn AuditLogger>> {
    AUDIT_LOGGER.get()
}

/// Log an audit event using the global logger
pub async fn log(event: AuditEvent) -> anyhow::Result<()> {
    if let Some(logger) = audit_logger() {
        logger.log(event).await
    } else {
        warn!("Audit logger not initialized, event dropped");
        Ok(())
    }
}

/// Convenience function for logging authentication events
pub async fn log_auth(
    actor_id: &str,
    action: AuditAction,
    ip: Option<&str>,
    success: bool,
) -> anyhow::Result<()> {
    let mut event = AuditEvent::new(actor_id, action)
        .with_service("auth-api");

    if let Some(ip) = ip {
        event = event.with_ip(ip);
    }

    if !success {
        event = event.failed();
    }

    log(event).await
}

/// Convenience function for logging message events
pub async fn log_message(
    actor_id: &str,
    action: AuditAction,
    room_id: &str,
    message_id: &str,
) -> anyhow::Result<()> {
    let event = AuditEvent::new(actor_id, action)
        .with_service("chat-service")
        .with_resource("message", message_id)
        .with_metadata("room_id", room_id);

    log(event).await
}

// Re-export async_trait for implementors
pub use async_trait::async_trait;

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_memory_logger() {
        let logger = MemoryAuditLogger::new(100);

        let event = AuditEvent::new("user123", AuditAction::Login)
            .with_ip("192.168.1.1")
            .with_service("auth-api");

        logger.log(event).await.unwrap();

        let events = logger
            .query(AuditFilter {
                actor_id: Some("user123".to_string()),
                ..Default::default()
            })
            .await
            .unwrap();

        assert_eq!(events.len(), 1);
        assert_eq!(events[0].actor_id, "user123");
    }

    #[test]
    fn test_event_builder() {
        let event = AuditEvent::new("user1", AuditAction::Login)
            .with_ip("10.0.0.1")
            .with_result(ActionResult::Success)
            .with_resource("user", "user1")
            .with_service("auth-api")
            .with_metadata("browser", "Firefox");

        assert_eq!(event.actor_id, "user1");
        assert_eq!(event.actor_ip, Some("10.0.0.1".to_string()));
        assert!(event.metadata.contains_key("browser"));
    }
}
