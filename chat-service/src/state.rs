//! Application state for chat service

use crate::db::Database;
use dashmap::DashMap;
use sqlx::sqlite::SqlitePool;
use tokio::sync::broadcast;
use uuid::Uuid;

/// Broadcast message for real-time updates
#[derive(Debug, Clone)]
pub struct BroadcastMessage {
    pub group_id: Uuid,
    pub message: String,
}

/// Application state
pub struct AppState {
    pub db: Database,
    /// Group ID -> Broadcast sender for real-time messaging
    pub group_channels: DashMap<Uuid, broadcast::Sender<BroadcastMessage>>,
}

impl AppState {
    pub fn new(pool: SqlitePool) -> Self {
        Self {
            db: Database::new(pool),
            group_channels: DashMap::new(),
        }
    }

    /// Get or create a broadcast channel for a group
    pub fn get_group_channel(&self, group_id: Uuid) -> broadcast::Sender<BroadcastMessage> {
        self.group_channels
            .entry(group_id)
            .or_insert_with(|| broadcast::channel(100).0)
            .clone()
    }

    /// Broadcast a message to all subscribers in a group
    pub fn broadcast_to_group(&self, group_id: Uuid, message: String) {
        if let Some(sender) = self.group_channels.get(&group_id) {
            let _ = sender.send(BroadcastMessage { group_id, message });
        }
    }

    /// Remove a group channel when the group is deleted
    pub fn remove_group_channel(&self, group_id: &Uuid) {
        self.group_channels.remove(group_id);
    }
}
