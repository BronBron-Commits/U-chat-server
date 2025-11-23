//! Redis Streams Backend for Horizontal Scaling
//!
//! This module provides Redis Streams integration for:
//! - Message distribution across multiple service instances
//! - Reliable message delivery with consumer groups
//! - Message persistence and replay
//! - Room-based pub/sub

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use redis::aio::ConnectionManager;
use redis::{AsyncCommands, RedisError, RedisResult};
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;
use tracing::{error, info, warn};

/// Redis Streams configuration
#[derive(Clone)]
pub struct RedisConfig {
    /// Redis connection URL
    pub url: String,
    /// Consumer group name (unique per service instance type)
    pub consumer_group: String,
    /// Consumer name (unique per service instance)
    pub consumer_name: String,
    /// Stream key prefix
    pub stream_prefix: String,
    /// Maximum stream length (for trimming)
    pub max_stream_length: usize,
    /// Block timeout for reading streams
    pub block_timeout_ms: usize,
}

impl RedisConfig {
    /// Load configuration from environment
    pub fn from_env() -> Self {
        Self {
            url: std::env::var("REDIS_URL")
                .unwrap_or_else(|_| "redis://localhost:6379".to_string()),
            consumer_group: std::env::var("REDIS_CONSUMER_GROUP")
                .unwrap_or_else(|_| "chat-service".to_string()),
            consumer_name: std::env::var("REDIS_CONSUMER_NAME")
                .unwrap_or_else(|_| format!("instance-{}", uuid::Uuid::new_v4())),
            stream_prefix: std::env::var("REDIS_STREAM_PREFIX")
                .unwrap_or_else(|_| "unhidra:chat".to_string()),
            max_stream_length: std::env::var("REDIS_MAX_STREAM_LENGTH")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(10000),
            block_timeout_ms: std::env::var("REDIS_BLOCK_TIMEOUT_MS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(5000),
        }
    }
}

/// Chat message for Redis streams
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct StreamMessage {
    /// Message ID (UUID)
    pub id: String,
    /// Room/channel ID
    pub room_id: String,
    /// Sender user ID
    pub sender_id: String,
    /// Message content (encrypted if E2EE enabled)
    pub content: String,
    /// Message type (text, binary, system)
    pub message_type: String,
    /// Timestamp (Unix milliseconds)
    pub timestamp: u64,
    /// Additional metadata (JSON)
    pub metadata: Option<String>,
}

impl StreamMessage {
    /// Create a new text message
    pub fn new_text(room_id: &str, sender_id: &str, content: &str) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            room_id: room_id.to_string(),
            sender_id: sender_id.to_string(),
            content: content.to_string(),
            message_type: "text".to_string(),
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0),
            metadata: None,
        }
    }

    /// Create a system message
    pub fn new_system(room_id: &str, content: &str) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            room_id: room_id.to_string(),
            sender_id: "system".to_string(),
            content: content.to_string(),
            message_type: "system".to_string(),
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0),
            metadata: None,
        }
    }

    /// Convert to Redis hash fields
    fn to_redis_fields(&self) -> Vec<(&str, String)> {
        let mut fields = vec![
            ("id", self.id.clone()),
            ("room_id", self.room_id.clone()),
            ("sender_id", self.sender_id.clone()),
            ("content", self.content.clone()),
            ("message_type", self.message_type.clone()),
            ("timestamp", self.timestamp.to_string()),
        ];
        if let Some(ref meta) = self.metadata {
            fields.push(("metadata", meta.clone()));
        }
        fields
    }

    /// Parse from Redis stream entry
    fn from_redis_entry(data: &HashMap<String, redis::Value>) -> Option<Self> {
        let get_str = |key: &str| -> Option<String> {
            data.get(key).and_then(|v| match v {
                redis::Value::BulkString(bytes) => String::from_utf8(bytes.clone()).ok(),
                redis::Value::SimpleString(s) => Some(s.clone()),
                _ => None,
            })
        };

        Some(Self {
            id: get_str("id")?,
            room_id: get_str("room_id")?,
            sender_id: get_str("sender_id")?,
            content: get_str("content")?,
            message_type: get_str("message_type").unwrap_or_else(|| "text".to_string()),
            timestamp: get_str("timestamp")?.parse().ok()?,
            metadata: get_str("metadata"),
        })
    }
}

/// Redis Streams client for chat service
pub struct RedisStreams {
    /// Connection manager (auto-reconnects)
    conn: ConnectionManager,
    /// Configuration
    config: RedisConfig,
    /// Local broadcast channels for room subscriptions
    local_channels: dashmap::DashMap<String, broadcast::Sender<StreamMessage>>,
}

impl RedisStreams {
    /// Create a new Redis Streams client
    pub async fn new(config: RedisConfig) -> RedisResult<Self> {
        let client = redis::Client::open(config.url.as_str())?;
        let conn = ConnectionManager::new(client).await?;

        Ok(Self {
            conn,
            config,
            local_channels: dashmap::DashMap::new(),
        })
    }

    /// Create from environment configuration
    pub async fn from_env() -> RedisResult<Self> {
        Self::new(RedisConfig::from_env()).await
    }

    /// Get stream key for a room
    fn stream_key(&self, room_id: &str) -> String {
        format!("{}:room:{}", self.config.stream_prefix, room_id)
    }

    /// Get global stream key (for all messages)
    fn global_stream_key(&self) -> String {
        format!("{}:global", self.config.stream_prefix)
    }

    /// Initialize consumer group for a stream
    pub async fn init_consumer_group(&mut self, room_id: &str) -> RedisResult<()> {
        let stream_key = self.stream_key(room_id);

        // Try to create the consumer group (ignore error if already exists)
        let result: RedisResult<()> = redis::cmd("XGROUP")
            .arg("CREATE")
            .arg(&stream_key)
            .arg(&self.config.consumer_group)
            .arg("$")
            .arg("MKSTREAM")
            .query_async(&mut self.conn)
            .await;

        match result {
            Ok(_) => {
                info!(room_id = room_id, "Created consumer group for room");
            }
            Err(e) if e.to_string().contains("BUSYGROUP") => {
                // Group already exists, that's fine
            }
            Err(e) => {
                warn!(room_id = room_id, error = %e, "Failed to create consumer group");
                return Err(e);
            }
        }

        Ok(())
    }

    /// Publish a message to a room stream
    pub async fn publish(&mut self, message: &StreamMessage) -> RedisResult<String> {
        let stream_key = self.stream_key(&message.room_id);
        let fields = message.to_redis_fields();

        // Add to room stream with auto-generated ID
        let entry_id: String = redis::cmd("XADD")
            .arg(&stream_key)
            .arg("MAXLEN")
            .arg("~")
            .arg(self.config.max_stream_length)
            .arg("*")
            .arg(fields.as_slice())
            .query_async(&mut self.conn)
            .await?;

        // Also publish to pub/sub for real-time delivery
        let json = serde_json::to_string(message).unwrap_or_default();
        let _: () = self.conn.publish(&stream_key, &json).await?;

        info!(
            room_id = message.room_id,
            message_id = message.id,
            entry_id = entry_id,
            "Published message to stream"
        );

        // Notify local subscribers
        if let Some(sender) = self.local_channels.get(&message.room_id) {
            let _ = sender.send(message.clone());
        }

        Ok(entry_id)
    }

    /// Subscribe to a room (returns a broadcast receiver)
    pub fn subscribe(&self, room_id: &str) -> broadcast::Receiver<StreamMessage> {
        let sender = self.local_channels
            .entry(room_id.to_string())
            .or_insert_with(|| broadcast::channel(1000).0);
        sender.subscribe()
    }

    /// Read messages from stream (for a consumer group)
    pub async fn read_messages(
        &mut self,
        room_id: &str,
        count: usize,
    ) -> RedisResult<Vec<StreamMessage>> {
        let stream_key = self.stream_key(room_id);

        // Read pending messages first, then new ones
        let result: redis::Value = redis::cmd("XREADGROUP")
            .arg("GROUP")
            .arg(&self.config.consumer_group)
            .arg(&self.config.consumer_name)
            .arg("COUNT")
            .arg(count)
            .arg("BLOCK")
            .arg(self.config.block_timeout_ms)
            .arg("STREAMS")
            .arg(&stream_key)
            .arg(">")
            .query_async(&mut self.conn)
            .await?;

        // Parse the response
        let messages = Self::parse_stream_response(result);
        Ok(messages)
    }

    /// Get message history from stream
    pub async fn get_history(
        &mut self,
        room_id: &str,
        count: usize,
        before_id: Option<&str>,
    ) -> RedisResult<Vec<StreamMessage>> {
        let stream_key = self.stream_key(room_id);
        let end = before_id.unwrap_or("+");

        let result: redis::Value = redis::cmd("XREVRANGE")
            .arg(&stream_key)
            .arg(end)
            .arg("-")
            .arg("COUNT")
            .arg(count)
            .query_async(&mut self.conn)
            .await?;

        let messages = Self::parse_xrange_response(result);
        Ok(messages)
    }

    /// Acknowledge message processing
    pub async fn ack(&mut self, room_id: &str, entry_ids: &[&str]) -> RedisResult<usize> {
        let stream_key = self.stream_key(room_id);

        let count: usize = redis::cmd("XACK")
            .arg(&stream_key)
            .arg(&self.config.consumer_group)
            .arg(entry_ids)
            .query_async(&mut self.conn)
            .await?;

        Ok(count)
    }

    /// Parse XREADGROUP response
    fn parse_stream_response(value: redis::Value) -> Vec<StreamMessage> {
        let mut messages = Vec::new();

        if let redis::Value::Array(streams) = value {
            for stream in streams {
                if let redis::Value::Array(parts) = stream {
                    if parts.len() >= 2 {
                        if let redis::Value::Array(entries) = &parts[1] {
                            for entry in entries {
                                if let redis::Value::Array(entry_parts) = entry {
                                    if entry_parts.len() >= 2 {
                                        if let redis::Value::Array(fields) = &entry_parts[1] {
                                            let map = Self::fields_to_map(fields);
                                            if let Some(msg) = StreamMessage::from_redis_entry(&map) {
                                                messages.push(msg);
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        messages
    }

    /// Parse XRANGE/XREVRANGE response
    fn parse_xrange_response(value: redis::Value) -> Vec<StreamMessage> {
        let mut messages = Vec::new();

        if let redis::Value::Array(entries) = value {
            for entry in entries {
                if let redis::Value::Array(parts) = entry {
                    if parts.len() >= 2 {
                        if let redis::Value::Array(fields) = &parts[1] {
                            let map = Self::fields_to_map(fields);
                            if let Some(msg) = StreamMessage::from_redis_entry(&map) {
                                messages.push(msg);
                            }
                        }
                    }
                }
            }
        }

        messages
    }

    /// Convert Redis field array to HashMap
    fn fields_to_map(fields: &[redis::Value]) -> HashMap<String, redis::Value> {
        let mut map = HashMap::new();
        let mut iter = fields.iter();

        while let (Some(key), Some(value)) = (iter.next(), iter.next()) {
            if let redis::Value::BulkString(k) = key {
                if let Ok(key_str) = String::from_utf8(k.clone()) {
                    map.insert(key_str, value.clone());
                }
            }
        }

        map
    }

    /// Start background consumer task
    pub fn start_consumer(
        mut self,
        room_ids: Vec<String>,
    ) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            loop {
                for room_id in &room_ids {
                    match self.read_messages(room_id, 100).await {
                        Ok(messages) => {
                            for msg in messages {
                                // Broadcast to local subscribers
                                if let Some(sender) = self.local_channels.get(room_id) {
                                    let _ = sender.send(msg);
                                }
                            }
                        }
                        Err(e) => {
                            error!(room_id = room_id, error = %e, "Failed to read messages");
                            tokio::time::sleep(Duration::from_secs(1)).await;
                        }
                    }
                }

                // Small delay between poll cycles
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        })
    }

    /// Get stream info
    pub async fn stream_info(&mut self, room_id: &str) -> RedisResult<StreamInfo> {
        let stream_key = self.stream_key(room_id);

        let len: usize = redis::cmd("XLEN")
            .arg(&stream_key)
            .query_async(&mut self.conn)
            .await
            .unwrap_or(0);

        Ok(StreamInfo {
            room_id: room_id.to_string(),
            length: len,
            consumer_group: self.config.consumer_group.clone(),
        })
    }
}

/// Stream information
#[derive(Clone, Serialize, Deserialize)]
pub struct StreamInfo {
    pub room_id: String,
    pub length: usize,
    pub consumer_group: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stream_message_creation() {
        let msg = StreamMessage::new_text("room1", "user1", "Hello, World!");
        assert_eq!(msg.room_id, "room1");
        assert_eq!(msg.sender_id, "user1");
        assert_eq!(msg.content, "Hello, World!");
        assert_eq!(msg.message_type, "text");
    }

    #[test]
    fn test_config_from_env() {
        let config = RedisConfig::from_env();
        assert!(!config.url.is_empty());
        assert!(!config.consumer_group.is_empty());
    }
}
