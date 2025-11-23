//! WebSocket handler for real-time bidirectional communication.
//!
//! This module implements Phase 3 WebSocket Fabric Hardening:
//! - Token authentication via Sec-WebSocket-Protocol header
//! - Room-based pub/sub with DashMap and tokio::broadcast
//! - Origin checking for CSRF protection
//! - Rate limiting per IP and per user
//! - Connection tracking with metadata
//! - Resource cleanup on disconnect

use axum::{
    extract::{
        ws::{Message, WebSocket},
        ConnectInfo, State, WebSocketUpgrade,
    },
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use futures_util::{SinkExt, StreamExt};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::broadcast;
use tracing::{error, info, warn, Span};

use crate::connection::{generate_connection_id, ConnectionInfo, ConnectionState};
use crate::rate_limiter::RateLimiter;
use crate::state::AppState;

/// WebSocket upgrade handler for GET /ws endpoint.
///
/// # Authentication Flow
/// 1. Check rate limits for IP address
/// 2. Extract token from Sec-WebSocket-Protocol header
/// 3. Validate JWT signature and expiration using jwt-common
/// 4. Check Origin header against allowed origins (CSRF protection)
/// 5. Check rate limits for user
/// 6. If valid, upgrade to WebSocket and join the appropriate room
///
/// # Security
/// - Rejects connections without valid tokens (HTTP 403)
/// - Validates Origin header to prevent Cross-Site WebSocket Hijacking
/// - Applies rate limiting per IP and per user
/// - Token is passed securely in header (not URL query) to avoid logging
pub async fn ws_handler(
    ws: WebSocketUpgrade,
    State((state, rate_limiter)): State<(Arc<AppState>, Arc<RateLimiter>)>,
    headers: HeaderMap,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
) -> Response {
    let ip = addr.ip();

    // Record authentication attempt
    crate::metrics::record_auth_attempt();

    // Rate limit check: IP address
    if !rate_limiter.check_ip(ip) {
        crate::metrics::record_auth_failure("rate_limit_ip");
        return (StatusCode::TOO_MANY_REQUESTS, "Rate limit exceeded").into_response();
    }

    // Extract and validate Origin header for CSRF protection
    if let Some(origin) = headers.get("origin").and_then(|v| v.to_str().ok()) {
        if !state.is_origin_allowed(origin) {
            warn!(origin = origin, ip = %ip, "WebSocket rejected: disallowed origin");
            crate::metrics::record_auth_failure("origin_not_allowed");
            return (StatusCode::FORBIDDEN, "Origin not allowed").into_response();
        }
    }
    // Note: Missing Origin header is allowed for non-browser clients (IoT devices)

    // Extract token from Sec-WebSocket-Protocol header
    let protocol_header = headers
        .get("sec-websocket-protocol")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    let token = extract_token_from_protocol(protocol_header);

    if token.is_empty() {
        warn!(ip = %ip, "WebSocket rejected: missing token in Sec-WebSocket-Protocol");
        crate::metrics::record_auth_failure("missing_token");
        return (StatusCode::FORBIDDEN, "Missing authentication token").into_response();
    }

    // Validate the JWT token using shared jwt-common TokenService
    let claims = match state.token_service.validate(&token) {
        Ok(claims) => claims,
        Err(e) => {
            warn!(ip = %ip, error = %e, "WebSocket rejected: invalid token");
            crate::metrics::record_auth_failure("invalid_token");
            return (StatusCode::FORBIDDEN, "Invalid or expired token").into_response();
        }
    };

    // Rate limit check: user
    if !rate_limiter.check_user(&claims.sub) {
        crate::metrics::record_auth_failure("rate_limit_user");
        return (StatusCode::TOO_MANY_REQUESTS, "Rate limit exceeded for user").into_response();
    }

    // Determine room ID from token claims
    let room_id = claims.room_id();

    // Extract user agent if available
    let user_agent = headers
        .get("user-agent")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    // Generate connection ID
    let connection_id = generate_connection_id();

    info!(
        user = claims.sub,
        room = room_id,
        ip = %ip,
        connection_id = connection_id,
        "WebSocket connection authenticated"
    );

    crate::metrics::record_auth_success();

    // Get or create broadcast channel for the room
    let sender = state.get_or_create_room(&room_id);

    // Create connection info
    let conn_info = ConnectionInfo::new(
        connection_id.clone(),
        claims.sub.clone(),
        room_id.clone(),
        Some(ip),
    )
    .with_user_agent(user_agent.unwrap_or_default());

    // Register the connection
    state.register_connection(conn_info);

    // Complete the WebSocket upgrade
    ws.protocols(["bearer"]).on_upgrade(move |socket| {
        handle_socket(
            socket,
            connection_id,
            room_id,
            sender,
            state,
            rate_limiter,
            claims.sub,
        )
    })
}

/// Extracts the token from Sec-WebSocket-Protocol header.
///
/// Supports multiple formats:
/// 1. "bearer, <token>" - Standard format from browser WebSocket API
/// 2. "<token>" - Direct token (for non-browser clients like ESP32)
fn extract_token_from_protocol(header: &str) -> String {
    let parts: Vec<&str> = header.split(',').map(|s| s.trim()).collect();

    // Format: "bearer, <token>"
    if parts.len() >= 2 && parts[0].eq_ignore_ascii_case("bearer") {
        return parts[1].to_string();
    }

    // Format: direct token (for testing/non-browser clients)
    if parts.len() == 1 && !parts[0].eq_ignore_ascii_case("bearer") && !parts[0].is_empty() {
        return parts[0].to_string();
    }

    String::new()
}

/// Handles an active WebSocket connection.
///
/// # Message Flow
/// 1. Subscribes to the room's broadcast channel
/// 2. Spawns a task to forward broadcast messages to this client
/// 3. Listens for incoming messages and broadcasts them to the room
/// 4. Applies message rate limiting
/// 5. Cleans up resources on disconnect
async fn handle_socket(
    socket: WebSocket,
    connection_id: String,
    room_id: String,
    sender: broadcast::Sender<String>,
    state: Arc<AppState>,
    rate_limiter: Arc<RateLimiter>,
    user_id: String,
) {
    let (mut ws_sender, mut ws_receiver) = socket.split();

    // Subscribe to room broadcasts
    let mut rx = sender.subscribe();

    info!(
        user = user_id,
        room = room_id,
        connection_id = connection_id,
        "Client joined room"
    );

    // Task: Forward broadcast messages to this WebSocket client
    let forward_room_id = room_id.clone();
    let forward_user_id = user_id.clone();
    let forward_conn_id = connection_id.clone();
    let forward_state = state.clone();
    let forward_task = tokio::spawn(async move {
        while let Ok(msg) = rx.recv().await {
            if ws_sender.send(Message::Text(msg)).await.is_err() {
                info!(
                    user = forward_user_id,
                    room = forward_room_id,
                    connection_id = forward_conn_id,
                    "Client disconnected (send failed)"
                );
                break;
            }
            // Update messages received count
            forward_state.update_connection(&forward_conn_id, |c| {
                c.increment_received();
            });
            crate::metrics::record_message_received();
        }
    });

    // Main loop: Receive messages from client and broadcast to room
    while let Some(result) = ws_receiver.next().await {
        let msg_start = Instant::now();

        match result {
            Ok(Message::Text(text)) => {
                // Check message rate limit
                if !rate_limiter.check_message(&connection_id) {
                    warn!(
                        connection_id = connection_id,
                        user = user_id,
                        "Message rate limit exceeded, dropping message"
                    );
                    continue;
                }

                // Update connection stats
                state.update_connection(&connection_id, |c| {
                    c.increment_sent();
                });

                // Broadcast to all room subscribers
                if let Err(e) = sender.send(text.to_string()) {
                    error!(error = %e, "Failed to broadcast message");
                }

                crate::metrics::record_message_sent();
                crate::metrics::record_message_latency(msg_start.elapsed());
            }
            Ok(Message::Binary(data)) => {
                // Check rate limit
                if !rate_limiter.check_message(&connection_id) {
                    continue;
                }

                state.update_connection(&connection_id, |c| {
                    c.increment_sent();
                });

                // Convert binary to hex for broadcast
                let encoded = hex_encode(&data);
                let _ = sender.send(format!(r#"{{"type":"binary","data":"{}"}}"#, encoded));

                crate::metrics::record_message_sent();
            }
            Ok(Message::Ping(data)) => {
                tracing::trace!(user = user_id, "Received ping");
                let _ = data; // Axum handles pong automatically
            }
            Ok(Message::Pong(_)) => {
                tracing::trace!(user = user_id, "Received pong");
                state.update_connection(&connection_id, |c| {
                    c.touch();
                });
            }
            Ok(Message::Close(_)) => {
                info!(
                    user = user_id,
                    room = room_id,
                    connection_id = connection_id,
                    "Client sent close frame"
                );
                break;
            }
            Err(e) => {
                warn!(
                    user = user_id,
                    connection_id = connection_id,
                    error = %e,
                    "WebSocket receive error"
                );
                break;
            }
        }
    }

    // Cleanup
    forward_task.abort();

    // Update connection state and unregister
    state.update_connection(&connection_id, |c| {
        c.set_state(ConnectionState::Closed);
    });

    let conn_info = state.unregister_connection(&connection_id);

    // Clean up rate limiter
    rate_limiter.remove_connection(&connection_id);

    if let Some(info) = conn_info {
        info!(
            user = user_id,
            room = room_id,
            connection_id = connection_id,
            messages_sent = info.messages_sent,
            messages_received = info.messages_received,
            duration_secs = info.duration().num_seconds(),
            "Client disconnected"
        );
    }

    // Clean up empty rooms
    if sender.receiver_count() == 0 {
        state.rooms.remove(&room_id);
        crate::metrics::record_room_destroyed();
        info!(room = room_id, "Room removed (no remaining subscribers)");
    }
}

/// Simple hex encoding for binary messages.
fn hex_encode(data: &[u8]) -> String {
    data.iter().map(|b| format!("{:02x}", b)).collect::<String>()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_token_bearer_format() {
        let header = "bearer, eyJhbGciOiJIUzI1NiJ9.test";
        let token = extract_token_from_protocol(header);
        assert_eq!(token, "eyJhbGciOiJIUzI1NiJ9.test");
    }

    #[test]
    fn test_extract_token_direct() {
        let header = "eyJhbGciOiJIUzI1NiJ9.test";
        let token = extract_token_from_protocol(header);
        assert_eq!(token, "eyJhbGciOiJIUzI1NiJ9.test");
    }

    #[test]
    fn test_extract_token_empty() {
        let token = extract_token_from_protocol("");
        assert_eq!(token, "");
    }

    #[test]
    fn test_extract_token_bearer_only() {
        let token = extract_token_from_protocol("bearer");
        assert_eq!(token, "");
    }

    #[test]
    fn test_hex_encode() {
        let data = vec![0xDE, 0xAD, 0xBE, 0xEF];
        assert_eq!(hex_encode(&data), "deadbeef");
    }
}
