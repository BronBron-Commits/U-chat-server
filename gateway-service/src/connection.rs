//! Connection tracking and metadata management
//!
//! Tracks active WebSocket connections with detailed metadata for
//! monitoring, debugging, and targeted message delivery.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::net::IpAddr;
use std::sync::atomic::{AtomicU64, Ordering};

/// Unique connection identifier
pub type ConnectionId = String;

/// Connection metadata for tracking and monitoring
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionInfo {
    /// Unique connection identifier (UUID)
    pub id: ConnectionId,

    /// User ID from JWT claims
    pub user_id: String,

    /// Optional device ID (for IoT devices)
    pub device_id: Option<String>,

    /// Room the connection is subscribed to
    pub room_id: String,

    /// Display name from JWT claims
    pub display_name: Option<String>,

    /// Client IP address
    pub ip_address: Option<IpAddr>,

    /// User agent string (if available)
    pub user_agent: Option<String>,

    /// Timestamp when connection was established
    pub connected_at: DateTime<Utc>,

    /// Last activity timestamp
    pub last_activity: DateTime<Utc>,

    /// Number of messages sent by this connection
    pub messages_sent: u64,

    /// Number of messages received by this connection
    pub messages_received: u64,

    /// Connection state
    pub state: ConnectionState,
}

/// Connection state enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConnectionState {
    /// Connection is being established
    Connecting,
    /// Connection is active and authenticated
    Active,
    /// Connection is closing gracefully
    Closing,
    /// Connection is closed
    Closed,
}

impl ConnectionInfo {
    /// Create a new connection info
    pub fn new(
        id: ConnectionId,
        user_id: String,
        room_id: String,
        ip_address: Option<IpAddr>,
    ) -> Self {
        let now = Utc::now();
        Self {
            id,
            user_id,
            device_id: None,
            room_id,
            display_name: None,
            ip_address,
            user_agent: None,
            connected_at: now,
            last_activity: now,
            messages_sent: 0,
            messages_received: 0,
            state: ConnectionState::Active,
        }
    }

    /// Set the device ID
    pub fn with_device_id(mut self, device_id: impl Into<String>) -> Self {
        self.device_id = Some(device_id.into());
        self
    }

    /// Set the display name
    pub fn with_display_name(mut self, display_name: impl Into<String>) -> Self {
        self.display_name = Some(display_name.into());
        self
    }

    /// Set the user agent
    pub fn with_user_agent(mut self, user_agent: impl Into<String>) -> Self {
        self.user_agent = Some(user_agent.into());
        self
    }

    /// Update last activity timestamp
    pub fn touch(&mut self) {
        self.last_activity = Utc::now();
    }

    /// Increment messages sent counter
    pub fn increment_sent(&mut self) {
        self.messages_sent += 1;
        self.touch();
    }

    /// Increment messages received counter
    pub fn increment_received(&mut self) {
        self.messages_received += 1;
        self.touch();
    }

    /// Set connection state
    pub fn set_state(&mut self, state: ConnectionState) {
        self.state = state;
    }

    /// Check if connection has been idle for too long
    pub fn is_idle(&self, max_idle_secs: i64) -> bool {
        let idle_time = Utc::now() - self.last_activity;
        idle_time.num_seconds() > max_idle_secs
    }

    /// Get connection duration
    pub fn duration(&self) -> chrono::Duration {
        Utc::now() - self.connected_at
    }
}

/// Global connection counter for generating unique IDs
static CONNECTION_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Generate a unique connection ID
pub fn generate_connection_id() -> ConnectionId {
    let counter = CONNECTION_COUNTER.fetch_add(1, Ordering::SeqCst);
    let timestamp = Utc::now().timestamp_millis();
    format!("conn_{:x}_{:x}", timestamp, counter)
}

/// Connection statistics summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionStats {
    pub total_connections: usize,
    pub active_connections: usize,
    pub total_messages_sent: u64,
    pub total_messages_received: u64,
    pub unique_users: usize,
    pub unique_rooms: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;

    #[test]
    fn test_connection_info_creation() {
        let conn = ConnectionInfo::new(
            "test_conn_1".to_string(),
            "user123".to_string(),
            "room:general".to_string(),
            Some(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1))),
        );

        assert_eq!(conn.user_id, "user123");
        assert_eq!(conn.room_id, "room:general");
        assert_eq!(conn.state, ConnectionState::Active);
        assert_eq!(conn.messages_sent, 0);
    }

    #[test]
    fn test_connection_info_with_builders() {
        let conn = ConnectionInfo::new(
            "test_conn_2".to_string(),
            "user456".to_string(),
            "room:chat".to_string(),
            None,
        )
        .with_device_id("esp32-001")
        .with_display_name("Test User")
        .with_user_agent("ESP32/1.0");

        assert_eq!(conn.device_id, Some("esp32-001".to_string()));
        assert_eq!(conn.display_name, Some("Test User".to_string()));
        assert_eq!(conn.user_agent, Some("ESP32/1.0".to_string()));
    }

    #[test]
    fn test_connection_counters() {
        let mut conn = ConnectionInfo::new(
            "test_conn_3".to_string(),
            "user789".to_string(),
            "room:test".to_string(),
            None,
        );

        conn.increment_sent();
        conn.increment_sent();
        conn.increment_received();

        assert_eq!(conn.messages_sent, 2);
        assert_eq!(conn.messages_received, 1);
    }

    #[test]
    fn test_generate_connection_id() {
        let id1 = generate_connection_id();
        let id2 = generate_connection_id();

        assert!(id1.starts_with("conn_"));
        assert!(id2.starts_with("conn_"));
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_idle_check() {
        let conn = ConnectionInfo::new(
            "test_conn_4".to_string(),
            "user".to_string(),
            "room".to_string(),
            None,
        );

        // Just created, should not be idle
        assert!(!conn.is_idle(60));
    }
}
