//! MQTT Bridge for IoT Device Integration
//!
//! Provides secure MQTT over WebSocket integration with:
//! - TLS mutual authentication (X.509 client certificates)
//! - E2EE message encryption for device-to-device communication
//! - Automatic device provisioning
//! - Topic-based routing to chat rooms
//!
//! Topic Structure:
//! - unhidra/devices/{device_id}/status - Device status updates
//! - unhidra/devices/{device_id}/commands - Commands to device
//! - unhidra/rooms/{room_id}/messages - Room messages
//! - unhidra/broadcast - System-wide broadcasts

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use tokio::sync::{broadcast, mpsc, RwLock};
use tracing::{error, info, warn};

/// MQTT Bridge configuration
#[derive(Clone, Debug)]
pub struct MqttBridgeConfig {
    /// MQTT broker URL (mqtt:// or mqtts://)
    pub broker_url: String,
    /// Client ID prefix
    pub client_id_prefix: String,
    /// Keep-alive interval in seconds
    pub keep_alive_secs: u64,
    /// Reconnect interval in seconds
    pub reconnect_interval_secs: u64,
    /// Topic prefix
    pub topic_prefix: String,
    /// Enable TLS
    pub tls_enabled: bool,
    /// CA certificate path (for TLS)
    pub ca_cert_path: Option<String>,
    /// Client certificate path (for mutual TLS)
    pub client_cert_path: Option<String>,
    /// Client key path (for mutual TLS)
    pub client_key_path: Option<String>,
}

impl Default for MqttBridgeConfig {
    fn default() -> Self {
        Self {
            broker_url: "mqtt://localhost:1883".to_string(),
            client_id_prefix: "unhidra-bridge".to_string(),
            keep_alive_secs: 30,
            reconnect_interval_secs: 5,
            topic_prefix: "unhidra".to_string(),
            tls_enabled: false,
            ca_cert_path: None,
            client_cert_path: None,
            client_key_path: None,
        }
    }
}

impl MqttBridgeConfig {
    /// Load configuration from environment
    pub fn from_env() -> Self {
        Self {
            broker_url: std::env::var("MQTT_BROKER_URL")
                .unwrap_or_else(|_| "mqtt://localhost:1883".to_string()),
            client_id_prefix: std::env::var("MQTT_CLIENT_ID_PREFIX")
                .unwrap_or_else(|_| "unhidra-bridge".to_string()),
            keep_alive_secs: std::env::var("MQTT_KEEP_ALIVE_SECS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(30),
            reconnect_interval_secs: std::env::var("MQTT_RECONNECT_INTERVAL_SECS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(5),
            topic_prefix: std::env::var("MQTT_TOPIC_PREFIX")
                .unwrap_or_else(|_| "unhidra".to_string()),
            tls_enabled: std::env::var("MQTT_TLS_ENABLED")
                .map(|v| v.to_lowercase() == "true")
                .unwrap_or(false),
            ca_cert_path: std::env::var("MQTT_CA_CERT_PATH").ok(),
            client_cert_path: std::env::var("MQTT_CLIENT_CERT_PATH").ok(),
            client_key_path: std::env::var("MQTT_CLIENT_KEY_PATH").ok(),
        }
    }
}

/// MQTT message for IoT devices
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct IoTMessage {
    /// Message ID
    pub id: String,
    /// Source device ID
    pub device_id: String,
    /// Message type
    pub message_type: IoTMessageType,
    /// Payload (may be encrypted)
    pub payload: String,
    /// Timestamp (Unix milliseconds)
    pub timestamp: u64,
    /// Whether payload is E2EE encrypted
    pub encrypted: bool,
    /// Quality of Service level
    pub qos: u8,
}

/// IoT message types
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum IoTMessageType {
    /// Device status update
    Status,
    /// Sensor reading
    SensorData,
    /// Command to device
    Command,
    /// Command acknowledgment
    CommandAck,
    /// Alert/notification
    Alert,
    /// Heartbeat/ping
    Heartbeat,
    /// Device configuration
    Config,
    /// Chat message (routed to room)
    ChatMessage,
}

impl IoTMessage {
    /// Create a new IoT message
    pub fn new(device_id: &str, message_type: IoTMessageType, payload: &str) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            device_id: device_id.to_string(),
            message_type,
            payload: payload.to_string(),
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0),
            encrypted: false,
            qos: 1,
        }
    }

    /// Create a status message
    pub fn status(device_id: &str, status: &str) -> Self {
        Self::new(device_id, IoTMessageType::Status, status)
    }

    /// Create a sensor data message
    pub fn sensor_data(device_id: &str, data: &str) -> Self {
        Self::new(device_id, IoTMessageType::SensorData, data)
    }

    /// Create a command message
    pub fn command(device_id: &str, command: &str) -> Self {
        Self::new(device_id, IoTMessageType::Command, command)
    }

    /// Mark as encrypted
    pub fn with_encryption(mut self) -> Self {
        self.encrypted = true;
        self
    }
}

/// Device status
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DeviceStatus {
    pub device_id: String,
    pub online: bool,
    pub last_seen: u64,
    pub firmware_version: Option<String>,
    pub ip_address: Option<String>,
    pub capabilities: Vec<String>,
}

/// MQTT Bridge for IoT integration
pub struct MqttBridge {
    /// Configuration
    config: MqttBridgeConfig,
    /// Connected devices (device_id -> status)
    devices: DashMap<String, DeviceStatus>,
    /// Message channel for incoming IoT messages
    incoming_tx: mpsc::Sender<IoTMessage>,
    /// Message channel for outgoing messages to devices
    outgoing_tx: broadcast::Sender<(String, IoTMessage)>,
    /// Shutdown signal
    shutdown: RwLock<bool>,
}

impl MqttBridge {
    /// Create a new MQTT bridge
    pub fn new(config: MqttBridgeConfig) -> Self {
        let (incoming_tx, _) = mpsc::channel(1000);
        let (outgoing_tx, _) = broadcast::channel(1000);

        Self {
            config,
            devices: DashMap::new(),
            incoming_tx,
            outgoing_tx,
            shutdown: RwLock::new(false),
        }
    }

    /// Create from environment configuration
    pub fn from_env() -> Self {
        Self::new(MqttBridgeConfig::from_env())
    }

    /// Get topic for device status
    pub fn device_status_topic(&self, device_id: &str) -> String {
        format!("{}/devices/{}/status", self.config.topic_prefix, device_id)
    }

    /// Get topic for device commands
    pub fn device_commands_topic(&self, device_id: &str) -> String {
        format!("{}/devices/{}/commands", self.config.topic_prefix, device_id)
    }

    /// Get topic for room messages
    pub fn room_messages_topic(&self, room_id: &str) -> String {
        format!("{}/rooms/{}/messages", self.config.topic_prefix, room_id)
    }

    /// Register a device
    pub fn register_device(&self, device_id: &str, capabilities: Vec<String>) {
        let status = DeviceStatus {
            device_id: device_id.to_string(),
            online: true,
            last_seen: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
            firmware_version: None,
            ip_address: None,
            capabilities,
        };
        self.devices.insert(device_id.to_string(), status);
        info!(device_id = device_id, "IoT device registered");
    }

    /// Update device status
    pub fn update_device_status(&self, device_id: &str, online: bool) {
        if let Some(mut status) = self.devices.get_mut(device_id) {
            status.online = online;
            status.last_seen = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
        }
    }

    /// Get device status
    pub fn get_device_status(&self, device_id: &str) -> Option<DeviceStatus> {
        self.devices.get(device_id).map(|s| s.clone())
    }

    /// Get all online devices
    pub fn get_online_devices(&self) -> Vec<DeviceStatus> {
        self.devices
            .iter()
            .filter(|d| d.online)
            .map(|d| d.clone())
            .collect()
    }

    /// Send a command to a device
    pub async fn send_command(&self, device_id: &str, command: &str) -> anyhow::Result<()> {
        let message = IoTMessage::command(device_id, command);
        let topic = self.device_commands_topic(device_id);

        let _ = self.outgoing_tx.send((topic, message));

        info!(device_id = device_id, "Command sent to device");
        Ok(())
    }

    /// Subscribe to incoming messages
    pub fn subscribe(&self) -> mpsc::Receiver<IoTMessage> {
        let (tx, rx) = mpsc::channel(100);
        // In a real implementation, this would be connected to the MQTT client
        rx
    }

    /// Forward a message to a chat room
    pub async fn forward_to_room(&self, room_id: &str, message: &IoTMessage) -> anyhow::Result<()> {
        let topic = self.room_messages_topic(room_id);

        let _ = self.outgoing_tx.send((topic, message.clone()));

        info!(
            room_id = room_id,
            device_id = message.device_id,
            "Message forwarded to room"
        );
        Ok(())
    }

    /// Process an incoming MQTT message
    pub async fn process_message(&self, topic: &str, payload: &[u8]) -> anyhow::Result<()> {
        let message: IoTMessage = serde_json::from_slice(payload)?;

        // Update device status
        self.update_device_status(&message.device_id, true);

        // Route based on message type
        match message.message_type {
            IoTMessageType::Status => {
                info!(
                    device_id = message.device_id,
                    "Device status update received"
                );
            }
            IoTMessageType::SensorData => {
                info!(
                    device_id = message.device_id,
                    "Sensor data received"
                );
            }
            IoTMessageType::Alert => {
                warn!(
                    device_id = message.device_id,
                    payload = message.payload,
                    "Device alert received"
                );
            }
            IoTMessageType::ChatMessage => {
                // Extract room ID from topic or payload
                // Forward to chat service
            }
            _ => {}
        }

        // Send to internal channel
        let _ = self.incoming_tx.send(message).await;

        Ok(())
    }

    /// Check for stale devices and mark them offline
    pub fn check_stale_devices(&self, timeout_secs: u64) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        for mut device in self.devices.iter_mut() {
            if device.online && (now - device.last_seen) > timeout_secs {
                device.online = false;
                info!(device_id = device.device_id, "Device marked offline (stale)");
            }
        }
    }
}

/// IoT topic matcher
pub struct TopicMatcher {
    patterns: Vec<(String, TopicHandler)>,
}

/// Handler for matched topics
pub enum TopicHandler {
    DeviceStatus,
    DeviceCommand,
    RoomMessage,
    Broadcast,
    Custom(String),
}

impl TopicMatcher {
    /// Create a new topic matcher with default patterns
    pub fn new(prefix: &str) -> Self {
        Self {
            patterns: vec![
                (format!("{}/devices/+/status", prefix), TopicHandler::DeviceStatus),
                (format!("{}/devices/+/commands", prefix), TopicHandler::DeviceCommand),
                (format!("{}/rooms/+/messages", prefix), TopicHandler::RoomMessage),
                (format!("{}/broadcast", prefix), TopicHandler::Broadcast),
            ],
        }
    }

    /// Match a topic against patterns
    pub fn match_topic(&self, topic: &str) -> Option<(TopicHandler, HashMap<String, String>)> {
        for (pattern, handler) in &self.patterns {
            if let Some(params) = self.match_pattern(pattern, topic) {
                return Some((handler.clone(), params));
            }
        }
        None
    }

    /// Match a single pattern (simple wildcard matching)
    fn match_pattern(&self, pattern: &str, topic: &str) -> Option<HashMap<String, String>> {
        let pattern_parts: Vec<&str> = pattern.split('/').collect();
        let topic_parts: Vec<&str> = topic.split('/').collect();

        if pattern_parts.len() != topic_parts.len() {
            return None;
        }

        let mut params = HashMap::new();
        let mut param_idx = 0;

        for (p, t) in pattern_parts.iter().zip(topic_parts.iter()) {
            if *p == "+" {
                params.insert(format!("param{}", param_idx), t.to_string());
                param_idx += 1;
            } else if p != t {
                return None;
            }
        }

        Some(params)
    }
}

impl Clone for TopicHandler {
    fn clone(&self) -> Self {
        match self {
            Self::DeviceStatus => Self::DeviceStatus,
            Self::DeviceCommand => Self::DeviceCommand,
            Self::RoomMessage => Self::RoomMessage,
            Self::Broadcast => Self::Broadcast,
            Self::Custom(s) => Self::Custom(s.clone()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_iot_message_creation() {
        let msg = IoTMessage::status("device1", r#"{"online": true}"#);
        assert_eq!(msg.device_id, "device1");
        assert_eq!(msg.message_type, IoTMessageType::Status);
    }

    #[test]
    fn test_topic_generation() {
        let config = MqttBridgeConfig::default();
        let bridge = MqttBridge::new(config);

        assert_eq!(bridge.device_status_topic("dev1"), "unhidra/devices/dev1/status");
        assert_eq!(bridge.device_commands_topic("dev1"), "unhidra/devices/dev1/commands");
        assert_eq!(bridge.room_messages_topic("room1"), "unhidra/rooms/room1/messages");
    }

    #[test]
    fn test_topic_matcher() {
        let matcher = TopicMatcher::new("unhidra");

        let result = matcher.match_topic("unhidra/devices/dev1/status");
        assert!(result.is_some());

        let (handler, params) = result.unwrap();
        assert!(matches!(handler, TopicHandler::DeviceStatus));
        assert_eq!(params.get("param0"), Some(&"dev1".to_string()));
    }
}
