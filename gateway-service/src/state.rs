//! Shared application state for the WebSocket gateway.
//!
//! This module defines the centralized state management for the gateway-service,
//! including room-based broadcast channels for real-time messaging and
//! connection tracking for monitoring and targeted delivery.

use dashmap::DashMap;
use jwt_common::TokenService;
use std::sync::Arc;
use tokio::sync::broadcast;

use crate::connection::{ConnectionId, ConnectionInfo};

/// Type alias for room ID to broadcast channel sender mapping.
///
/// Uses DashMap for lock-free concurrent access across multiple connections.
/// Each room has its own broadcast channel for efficient fan-out messaging.
pub type RoomsMap = DashMap<String, broadcast::Sender<String>>;

/// Type alias for connection ID to connection info mapping.
pub type ConnectionsMap = DashMap<ConnectionId, ConnectionInfo>;

/// Broadcast channel capacity per room.
/// If clients can't keep up, oldest messages are dropped.
pub const CHANNEL_CAPACITY: usize = 100;

/// Shared application state for the WebSocket gateway.
#[derive(Clone)]
pub struct AppState {
    /// Thread-safe map of room IDs to their broadcast channels.
    ///
    /// - Key: Room ID (derived from user/device token claims)
    /// - Value: Broadcast channel sender for that room
    pub rooms: Arc<RoomsMap>,

    /// Thread-safe map of connection IDs to their metadata.
    ///
    /// Used for monitoring, debugging, and targeted message delivery.
    pub connections: Arc<ConnectionsMap>,

    /// JWT token service for validation (from jwt-common crate).
    pub token_service: TokenService,

    /// Allowed origins for WebSocket connections (CSRF protection).
    pub allowed_origins: Vec<String>,

    /// Service start time for uptime calculations
    pub started_at: chrono::DateTime<chrono::Utc>,
}

impl AppState {
    /// Creates a new AppState with the given configuration.
    pub fn new(token_service: TokenService, allowed_origins: Vec<String>) -> Self {
        Self {
            rooms: Arc::new(DashMap::new()),
            connections: Arc::new(DashMap::new()),
            token_service,
            allowed_origins,
            started_at: chrono::Utc::now(),
        }
    }

    /// Creates AppState from environment variables.
    ///
    /// Reads JWT_SECRET for token validation.
    /// WARNING: In production, always set JWT_SECRET to a strong random value.
    pub fn from_env(allowed_origins: Vec<String>) -> Self {
        Self::new(TokenService::from_env(), allowed_origins)
    }

    /// Creates AppState with default development configuration.
    ///
    /// WARNING: Only use in development. Uses weak JWT secret.
    #[allow(dead_code)]
    pub fn new_dev() -> Self {
        Self::from_env(vec![
            "http://localhost:3000".to_string(),
            "http://127.0.0.1:3000".to_string(),
        ])
    }

    /// Checks if an origin is allowed for WebSocket connections.
    pub fn is_origin_allowed(&self, origin: &str) -> bool {
        // In development mode with empty origins, allow all for testing
        if self.allowed_origins.is_empty() {
            return true;
        }
        self.allowed_origins.iter().any(|o| o == origin)
    }

    /// Get or create a broadcast channel for a room.
    pub fn get_or_create_room(&self, room_id: &str) -> broadcast::Sender<String> {
        self.rooms
            .entry(room_id.to_string())
            .or_insert_with(|| {
                tracing::info!(room = room_id, "Creating new room broadcast channel");
                crate::metrics::record_room_created();
                broadcast::channel::<String>(CHANNEL_CAPACITY).0
            })
            .clone()
    }

    /// Register a new connection.
    pub fn register_connection(&self, info: ConnectionInfo) {
        let id = info.id.clone();
        self.connections.insert(id.clone(), info);
        crate::metrics::record_connection();
        tracing::debug!(connection_id = %id, "Connection registered");
    }

    /// Unregister a connection.
    pub fn unregister_connection(&self, connection_id: &str) -> Option<ConnectionInfo> {
        let removed = self.connections.remove(connection_id);
        if removed.is_some() {
            crate::metrics::record_disconnection();
            tracing::debug!(connection_id = %connection_id, "Connection unregistered");
        }
        removed.map(|(_, v)| v)
    }

    /// Update connection info.
    pub fn update_connection<F>(&self, connection_id: &str, update_fn: F)
    where
        F: FnOnce(&mut ConnectionInfo),
    {
        if let Some(mut entry) = self.connections.get_mut(connection_id) {
            update_fn(&mut entry);
        }
    }

    /// Clean up empty rooms.
    pub fn cleanup_empty_rooms(&self) {
        let rooms_to_remove: Vec<String> = self
            .rooms
            .iter()
            .filter(|entry| entry.value().receiver_count() == 0)
            .map(|entry| entry.key().clone())
            .collect();

        for room_id in rooms_to_remove {
            self.rooms.remove(&room_id);
            crate::metrics::record_room_destroyed();
            tracing::info!(room = room_id, "Room removed (no subscribers)");
        }

        // Update metrics
        crate::metrics::set_active_rooms(self.rooms.len());
    }

    /// Get service uptime in seconds.
    pub fn uptime_secs(&self) -> i64 {
        (chrono::Utc::now() - self.started_at).num_seconds()
    }

    /// Get total number of connected clients.
    pub fn connection_count(&self) -> usize {
        self.connections.len()
    }

    /// Get total number of active rooms.
    pub fn room_count(&self) -> usize {
        self.rooms.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_origin_check() {
        let state = AppState::new(
            TokenService::new("secret"),
            vec!["https://example.com".to_string()],
        );

        assert!(state.is_origin_allowed("https://example.com"));
        assert!(!state.is_origin_allowed("https://evil.com"));
    }

    #[test]
    fn test_empty_origins_allows_all() {
        let state = AppState::new(TokenService::new("secret"), vec![]);
        assert!(state.is_origin_allowed("https://any-origin.com"));
    }

    #[test]
    fn test_room_creation() {
        let state = AppState::new(TokenService::new("secret"), vec![]);

        let sender1 = state.get_or_create_room("test_room");
        let sender2 = state.get_or_create_room("test_room");

        // Same room should return same sender
        assert_eq!(sender1.receiver_count(), sender2.receiver_count());
    }

    #[test]
    fn test_connection_tracking() {
        let state = AppState::new(TokenService::new("secret"), vec![]);

        let conn = ConnectionInfo::new(
            "conn_1".to_string(),
            "user_1".to_string(),
            "room_1".to_string(),
            None,
        );

        state.register_connection(conn);
        assert_eq!(state.connection_count(), 1);

        let removed = state.unregister_connection("conn_1");
        assert!(removed.is_some());
        assert_eq!(state.connection_count(), 0);
    }

    #[test]
    fn test_connection_update() {
        let state = AppState::new(TokenService::new("secret"), vec![]);

        let conn = ConnectionInfo::new(
            "conn_2".to_string(),
            "user_2".to_string(),
            "room_2".to_string(),
            None,
        );

        state.register_connection(conn);

        state.update_connection("conn_2", |c| {
            c.increment_sent();
            c.increment_sent();
        });

        if let Some(entry) = state.connections.get("conn_2") {
            assert_eq!(entry.messages_sent, 2);
        }
    }
}
