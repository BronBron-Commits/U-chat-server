//! Prometheus metrics for gateway service observability
//!
//! Exposes metrics at /metrics endpoint for scraping by Prometheus.

use metrics::{counter, gauge, histogram, describe_counter, describe_gauge, describe_histogram};
use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};
use once_cell::sync::OnceCell;
use std::time::Duration;

/// Global Prometheus handle
static METRICS_HANDLE: OnceCell<PrometheusHandle> = OnceCell::new();

/// Metric names
pub const CONNECTIONS_TOTAL: &str = "gateway_connections_total";
pub const CONNECTIONS_ACTIVE: &str = "gateway_connections_active";
pub const MESSAGES_TOTAL: &str = "gateway_messages_total";
pub const MESSAGE_LATENCY: &str = "gateway_message_latency_seconds";
pub const AUTH_ATTEMPTS: &str = "gateway_auth_attempts_total";
pub const AUTH_FAILURES: &str = "gateway_auth_failures_total";
pub const RATE_LIMIT_HITS: &str = "gateway_rate_limit_hits_total";
pub const ROOMS_ACTIVE: &str = "gateway_rooms_active";
pub const ROOM_SUBSCRIBERS: &str = "gateway_room_subscribers";

/// Initialize the metrics system
pub fn init_metrics() {
    let handle = PrometheusBuilder::new()
        .install_recorder()
        .expect("Failed to install Prometheus recorder");

    METRICS_HANDLE.set(handle).expect("Metrics already initialized");

    // Describe metrics for Prometheus
    describe_counter!(CONNECTIONS_TOTAL, "Total number of WebSocket connections");
    describe_gauge!(CONNECTIONS_ACTIVE, "Number of currently active connections");
    describe_counter!(MESSAGES_TOTAL, "Total number of messages processed");
    describe_histogram!(MESSAGE_LATENCY, "Message processing latency in seconds");
    describe_counter!(AUTH_ATTEMPTS, "Total authentication attempts");
    describe_counter!(AUTH_FAILURES, "Failed authentication attempts");
    describe_counter!(RATE_LIMIT_HITS, "Rate limit violations");
    describe_gauge!(ROOMS_ACTIVE, "Number of active rooms");
    describe_gauge!(ROOM_SUBSCRIBERS, "Number of subscribers per room");

    tracing::info!("Metrics system initialized");
}

/// Get the Prometheus metrics handle
fn get_handle() -> &'static PrometheusHandle {
    METRICS_HANDLE.get().expect("Metrics not initialized")
}

/// Handler for /metrics endpoint
pub async fn metrics_handler() -> String {
    get_handle().render()
}

// ============================================================================
// Connection Metrics
// ============================================================================

/// Record a new connection
pub fn record_connection() {
    counter!(CONNECTIONS_TOTAL).increment(1);
    gauge!(CONNECTIONS_ACTIVE).increment(1.0);
}

/// Record a connection closed
pub fn record_disconnection() {
    gauge!(CONNECTIONS_ACTIVE).decrement(1.0);
}

/// Set the total number of active connections
pub fn set_active_connections(count: usize) {
    gauge!(CONNECTIONS_ACTIVE).set(count as f64);
}

// ============================================================================
// Message Metrics
// ============================================================================

/// Record a message sent
pub fn record_message_sent() {
    counter!(MESSAGES_TOTAL, "direction" => "sent").increment(1);
}

/// Record a message received
pub fn record_message_received() {
    counter!(MESSAGES_TOTAL, "direction" => "received").increment(1);
}

/// Record message processing latency
pub fn record_message_latency(duration: Duration) {
    histogram!(MESSAGE_LATENCY).record(duration.as_secs_f64());
}

// ============================================================================
// Authentication Metrics
// ============================================================================

/// Record an authentication attempt
pub fn record_auth_attempt() {
    counter!(AUTH_ATTEMPTS).increment(1);
}

/// Record a successful authentication
pub fn record_auth_success() {
    counter!(AUTH_ATTEMPTS, "result" => "success").increment(1);
}

/// Record a failed authentication
pub fn record_auth_failure(reason: &str) {
    counter!(AUTH_FAILURES, "reason" => reason.to_string()).increment(1);
}

// ============================================================================
// Rate Limiting Metrics
// ============================================================================

/// Record a rate limit hit
pub fn record_rate_limit_hit(limit_type: &str) {
    counter!(RATE_LIMIT_HITS, "type" => limit_type.to_string()).increment(1);
}

// ============================================================================
// Room Metrics
// ============================================================================

/// Set the number of active rooms
pub fn set_active_rooms(count: usize) {
    gauge!(ROOMS_ACTIVE).set(count as f64);
}

/// Record room subscriber count
pub fn set_room_subscribers(room_id: &str, count: usize) {
    gauge!(ROOM_SUBSCRIBERS, "room" => room_id.to_string()).set(count as f64);
}

/// Record room created
pub fn record_room_created() {
    counter!("gateway_rooms_created_total").increment(1);
}

/// Record room destroyed
pub fn record_room_destroyed() {
    counter!("gateway_rooms_destroyed_total").increment(1);
}

#[cfg(test)]
mod tests {
    use super::*;

    // Note: Metrics tests require careful handling due to global state
    // These tests verify the API compiles and basic operations work

    #[test]
    fn test_metric_functions_exist() {
        // Just verify the functions compile
        // Actual metrics testing should be done via integration tests
        let _ = CONNECTIONS_TOTAL;
        let _ = CONNECTIONS_ACTIVE;
        let _ = MESSAGES_TOTAL;
    }
}
