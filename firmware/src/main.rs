//! Unhidra ESP32 Firmware - Secure WSS Client
//!
//! Phase 4: Secure ESP32 Firmware & WSS Integration (IoT Edge Hardening)
//!
//! This firmware implements:
//! - Secure WebSocket (WSS) connection to Unhidra backend
//! - TLS certificate verification using CA bundle
//! - Device authentication via Sec-WebSocket-Protocol header
//! - Automatic reconnection with exponential backoff and jitter
//! - Wi-Fi management via esp-idf-svc
//! - Keep-alive ping/pong handling

use esp_idf_svc::eventloop::EspSystemEventLoop;
use esp_idf_svc::hal::peripherals::Peripherals;
use esp_idf_svc::nvs::EspDefaultNvsPartition;
use esp_idf_svc::wifi::{BlockingWifi, ClientConfiguration, Configuration, EspWifi};
use esp_idf_svc::ws::client::{
    EspWebSocketClient, EspWebSocketClientConfig, WebSocketEvent, WebSocketEventType,
};
use esp_idf_svc::tls::X509;
use esp_idf_svc::log::EspLogger;

use log::*;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;

// ============================================================================
// Configuration Constants
// ============================================================================

/// Wi-Fi SSID - Should be set via sdkconfig or provisioning
const WIFI_SSID: &str = env!("WIFI_SSID");

/// Wi-Fi Password - Should be set via sdkconfig or provisioning
const WIFI_PASSWORD: &str = env!("WIFI_PASSWORD");

/// Device API Key for authentication - unique per device
/// In production, this should be stored in NVS or secure element
const DEVICE_API_KEY: &str = env!("DEVICE_API_KEY");

/// WebSocket server URL (WSS for TLS)
const WS_SERVER_URL: &str = "wss://api.unhidra.io/ws";

/// Device ID for identification (can be MAC address derived)
const DEVICE_ID: &str = env!("DEVICE_ID");

// Reconnection parameters
const INITIAL_BACKOFF_MS: u64 = 5000;      // 5 seconds initial backoff
const MAX_BACKOFF_MS: u64 = 60000;         // 1 minute max backoff
const BACKOFF_MULTIPLIER: f64 = 2.0;       // Exponential multiplier
const JITTER_FACTOR: f64 = 0.3;            // 30% jitter range

// Heartbeat interval (seconds)
const HEARTBEAT_INTERVAL_SECS: u64 = 60;

// Keep-alive ping interval (seconds)
const PING_INTERVAL_SECS: u32 = 30;

// ============================================================================
// Application State
// ============================================================================

struct DeviceState {
    /// Flag indicating if device is connected to WebSocket
    connected: AtomicBool,
    /// Count of consecutive connection failures (for backoff)
    failure_count: AtomicU32,
    /// Flag to signal shutdown
    shutdown: AtomicBool,
}

impl DeviceState {
    fn new() -> Self {
        Self {
            connected: AtomicBool::new(false),
            failure_count: AtomicU32::new(0),
            shutdown: AtomicBool::new(false),
        }
    }

    fn set_connected(&self, connected: bool) {
        self.connected.store(connected, Ordering::SeqCst);
        if connected {
            // Reset failure count on successful connection
            self.failure_count.store(0, Ordering::SeqCst);
        }
    }

    fn is_connected(&self) -> bool {
        self.connected.load(Ordering::SeqCst)
    }

    fn increment_failures(&self) -> u32 {
        self.failure_count.fetch_add(1, Ordering::SeqCst) + 1
    }

    fn get_failure_count(&self) -> u32 {
        self.failure_count.load(Ordering::SeqCst)
    }

    fn should_shutdown(&self) -> bool {
        self.shutdown.load(Ordering::SeqCst)
    }
}

// ============================================================================
// Wi-Fi Management
// ============================================================================

/// Initialize and connect to Wi-Fi using esp-idf-svc
fn init_wifi(
    peripherals: &mut Peripherals,
    sysloop: EspSystemEventLoop,
    nvs: EspDefaultNvsPartition,
) -> anyhow::Result<BlockingWifi<EspWifi<'static>>> {
    info!("Initializing Wi-Fi...");

    let wifi = EspWifi::new(
        peripherals.modem.take().unwrap(),
        sysloop.clone(),
        Some(nvs),
    )?;

    let mut wifi = BlockingWifi::wrap(wifi, sysloop)?;

    let wifi_config = Configuration::Client(ClientConfiguration {
        ssid: WIFI_SSID.try_into().expect("SSID too long"),
        password: WIFI_PASSWORD.try_into().expect("Password too long"),
        ..Default::default()
    });

    wifi.set_configuration(&wifi_config)?;

    info!("Starting Wi-Fi...");
    wifi.start()?;

    info!("Connecting to Wi-Fi SSID: {}...", WIFI_SSID);
    wifi.connect()?;

    info!("Waiting for DHCP...");
    wifi.wait_netif_up()?;

    let ip_info = wifi.wifi().sta_netif().get_ip_info()?;
    info!("Wi-Fi connected! IP: {:?}", ip_info.ip);

    Ok(wifi)
}

// ============================================================================
// WebSocket Client
// ============================================================================

/// Calculate backoff duration with exponential increase and jitter
fn calculate_backoff(failure_count: u32) -> Duration {
    let base_backoff = INITIAL_BACKOFF_MS as f64 * BACKOFF_MULTIPLIER.powi(failure_count as i32 - 1);
    let capped_backoff = base_backoff.min(MAX_BACKOFF_MS as f64);

    // Add jitter: random value between -JITTER_FACTOR and +JITTER_FACTOR
    // Using a simple pseudo-random based on current tick count
    let tick = unsafe { esp_idf_svc::sys::esp_timer_get_time() } as u64;
    let jitter_range = capped_backoff * JITTER_FACTOR;
    let jitter = ((tick % 1000) as f64 / 500.0 - 1.0) * jitter_range;

    let final_backoff = (capped_backoff + jitter).max(INITIAL_BACKOFF_MS as f64) as u64;

    info!("Calculated backoff: {}ms (failures: {})", final_backoff, failure_count);
    Duration::from_millis(final_backoff)
}

/// Create WebSocket client configuration with TLS and authentication
fn create_ws_config<'a>() -> EspWebSocketClientConfig<'a> {
    EspWebSocketClientConfig {
        // Server URL
        server_uri: WS_SERVER_URL,

        // TLS Configuration: Use the default CA certificate bundle
        // This enables server certificate verification
        crt_bundle_attach: Some(esp_idf_svc::sys::esp_crt_bundle_attach),

        // Alternative: Use custom CA certificate (uncomment if needed)
        // server_cert: Some(X509::pem_until_nul(include_bytes!("../certs/ca.pem"))),

        // Authentication: Device API key as subprotocol
        // Server validates this during WebSocket handshake
        subprotocol: Some(DEVICE_API_KEY),

        // Ping/Pong keep-alive
        ping_interval_sec: Some(PING_INTERVAL_SECS),

        // Network timeout (in seconds)
        network_timeout_ms: 10000,

        // Buffer sizes
        buffer_size: 2048,

        ..Default::default()
    }
}

/// Handle incoming WebSocket events
fn handle_ws_event(event: &WebSocketEvent, state: &Arc<DeviceState>) {
    match event.event_type {
        WebSocketEventType::Connected => {
            info!("WebSocket connected to server!");
            state.set_connected(true);

            // Send initial registration message
            // Note: Actual send would be done via client.send()
            info!("Device {} registered with server", DEVICE_ID);
        }

        WebSocketEventType::Disconnected => {
            warn!("WebSocket disconnected from server");
            state.set_connected(false);
        }

        WebSocketEventType::Close(close_status) => {
            warn!("WebSocket connection closed: {:?}", close_status);
            state.set_connected(false);
        }

        WebSocketEventType::Closed => {
            info!("WebSocket connection fully closed");
            state.set_connected(false);
        }

        WebSocketEventType::Text(text) => {
            info!("Received text message: {}", text);
            handle_server_message(text, state);
        }

        WebSocketEventType::Binary(data) => {
            info!("Received binary message: {} bytes", data.len());
            // Handle binary data (e.g., firmware updates, sensor commands)
        }

        WebSocketEventType::Ping(data) => {
            debug!("Received ping: {} bytes", data.len());
            // Pong is automatically sent by the client
        }

        WebSocketEventType::Pong(data) => {
            debug!("Received pong: {} bytes", data.len());
        }

        WebSocketEventType::Error(err) => {
            error!("WebSocket error: {:?}", err);
            state.set_connected(false);
        }

        _ => {
            debug!("Unhandled WebSocket event type");
        }
    }
}

/// Process messages received from the server
fn handle_server_message(message: &str, _state: &Arc<DeviceState>) {
    // Parse JSON message
    // In production, use serde_json for proper parsing

    if message.contains("\"type\":\"command\"") {
        info!("Received command from server");
        // Handle device commands (e.g., restart, config update)
    } else if message.contains("\"type\":\"config\"") {
        info!("Received configuration update");
        // Handle configuration updates
    } else if message.contains("\"type\":\"ota\"") {
        info!("Received OTA update notification");
        // Trigger OTA update process
    } else if message.contains("\"type\":\"ack\"") {
        debug!("Received acknowledgment from server");
    } else {
        debug!("Received unknown message type: {}", message);
    }
}

/// Build heartbeat message JSON
fn build_heartbeat_message() -> String {
    let timestamp = unsafe { esp_idf_svc::sys::esp_timer_get_time() } / 1_000_000;

    // Get free heap memory for health reporting
    let free_heap = unsafe { esp_idf_svc::sys::esp_get_free_heap_size() };

    format!(
        r#"{{"type":"heartbeat","device_id":"{}","ts":{},"free_heap":{}}}"#,
        DEVICE_ID, timestamp, free_heap
    )
}

/// Main WebSocket connection loop with auto-reconnection
fn run_websocket_loop(state: Arc<DeviceState>) -> anyhow::Result<()> {
    loop {
        if state.should_shutdown() {
            info!("Shutdown requested, exiting WebSocket loop");
            break;
        }

        info!("Establishing WebSocket connection to {}...", WS_SERVER_URL);

        let config = create_ws_config();
        let state_clone = state.clone();

        // Create WebSocket client with event callback
        let result = EspWebSocketClient::new(
            &config,
            Duration::from_secs(30),
            move |event| {
                handle_ws_event(event, &state_clone);
            },
        );

        match result {
            Ok(mut client) => {
                info!("WebSocket client created successfully");

                // Reset failure count on successful connection
                state.failure_count.store(0, Ordering::SeqCst);

                let mut last_heartbeat = 0u64;

                // Main loop while connected
                while state.is_connected() && !state.should_shutdown() {
                    // Send periodic heartbeat
                    let now = unsafe { esp_idf_svc::sys::esp_timer_get_time() } as u64 / 1_000_000;

                    if now - last_heartbeat >= HEARTBEAT_INTERVAL_SECS {
                        let heartbeat = build_heartbeat_message();
                        match client.send(WebSocketEventType::Text(&heartbeat)) {
                            Ok(_) => {
                                debug!("Heartbeat sent");
                                last_heartbeat = now;
                            }
                            Err(e) => {
                                error!("Failed to send heartbeat: {:?}", e);
                                state.set_connected(false);
                                break;
                            }
                        }
                    }

                    // Small delay to prevent busy loop
                    std::thread::sleep(Duration::from_millis(100));
                }

                // Clean disconnect
                info!("Disconnecting WebSocket client...");
                drop(client);
            }
            Err(e) => {
                error!("Failed to create WebSocket client: {:?}", e);
            }
        }

        // Calculate backoff before retry
        let failures = state.increment_failures();
        let backoff = calculate_backoff(failures);

        warn!(
            "WebSocket disconnected. Reconnecting in {:?} (attempt {})",
            backoff, failures
        );

        std::thread::sleep(backoff);
    }

    Ok(())
}

// ============================================================================
// Main Entry Point
// ============================================================================

fn main() -> anyhow::Result<()> {
    // Link ESP-IDF patches (required for binstart feature)
    esp_idf_svc::sys::link_patches();

    // Initialize logging
    EspLogger::initialize_default();

    info!("==============================================");
    info!("  Unhidra ESP32 Firmware - Phase 4");
    info!("  Secure WSS Integration");
    info!("==============================================");
    info!("Device ID: {}", DEVICE_ID);
    info!("Server URL: {}", WS_SERVER_URL);

    // Initialize peripherals
    let mut peripherals = Peripherals::take()?;
    let sysloop = EspSystemEventLoop::take()?;
    let nvs = EspDefaultNvsPartition::take()?;

    // Initialize and connect to Wi-Fi
    let _wifi = init_wifi(&mut peripherals, sysloop, nvs)?;

    // Create application state
    let state = Arc::new(DeviceState::new());

    // Start WebSocket connection loop (runs indefinitely with auto-reconnect)
    info!("Starting WebSocket connection loop...");
    run_websocket_loop(state)?;

    info!("Firmware main loop ended");
    Ok(())
}

// ============================================================================
// Optional: OTA Update Support (Future Enhancement)
// ============================================================================

#[allow(dead_code)]
mod ota {
    use log::*;

    /// Initiate OTA update from given URL
    /// This would be triggered by a server command
    pub fn start_ota_update(_url: &str) -> anyhow::Result<()> {
        info!("OTA update functionality - placeholder");
        // Implementation would use esp_ota_ops
        // 1. Download firmware image
        // 2. Verify signature
        // 3. Write to OTA partition
        // 4. Set boot partition
        // 5. Restart
        Ok(())
    }

    /// Check if device has pending OTA update
    pub fn has_pending_update() -> bool {
        false
    }
}

// ============================================================================
// Tests (run on host with mock)
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_backoff_calculation() {
        // First failure: base backoff
        let d1 = calculate_backoff(1);
        assert!(d1.as_millis() >= INITIAL_BACKOFF_MS as u128 - (INITIAL_BACKOFF_MS as f64 * JITTER_FACTOR) as u128);

        // Multiple failures: increases
        let d5 = calculate_backoff(5);
        assert!(d5 > d1);

        // High failure count: capped at max
        let d100 = calculate_backoff(100);
        assert!(d100.as_millis() <= MAX_BACKOFF_MS as u128 + (MAX_BACKOFF_MS as f64 * JITTER_FACTOR) as u128);
    }

    #[test]
    fn test_heartbeat_message_format() {
        let msg = build_heartbeat_message();
        assert!(msg.contains("\"type\":\"heartbeat\""));
        assert!(msg.contains(&format!("\"device_id\":\"{}\"", DEVICE_ID)));
    }
}
