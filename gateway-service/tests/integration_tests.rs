//! Integration tests for gateway-service
//!
//! Tests the WebSocket gateway with JWT authentication, rate limiting,
//! and room-based messaging.

use std::time::Duration;

/// Test helper to generate a valid JWT token for testing
fn generate_test_token(secret: &str, username: &str) -> String {
    use jwt_common::{Claims, TokenService};
    let service = TokenService::new(secret);
    let claims = Claims::new(username, 3600, None);
    service.generate(&claims).unwrap()
}

#[cfg(test)]
mod unit_tests {
    use super::*;

    #[test]
    fn test_generate_test_token() {
        let token = generate_test_token("test-secret", "testuser");
        assert!(!token.is_empty());
        assert!(token.contains('.'));
    }
}

/// Integration tests require a running server
/// Run with: cargo test --test integration_tests -- --ignored
#[cfg(test)]
mod integration_tests {
    use super::*;

    /// Test health endpoint
    #[tokio::test]
    #[ignore = "Requires running server"]
    async fn test_health_endpoint() {
        let client = reqwest::Client::new();
        let response = client
            .get("http://localhost:9000/health")
            .send()
            .await
            .expect("Failed to send request");

        assert_eq!(response.status(), 200);
        let body = response.text().await.unwrap();
        assert_eq!(body, "OK");
    }

    /// Test ready endpoint
    #[tokio::test]
    #[ignore = "Requires running server"]
    async fn test_ready_endpoint() {
        let client = reqwest::Client::new();
        let response = client
            .get("http://localhost:9000/ready")
            .send()
            .await
            .expect("Failed to send request");

        assert_eq!(response.status(), 200);
        let body: serde_json::Value = response.json().await.unwrap();
        assert_eq!(body["status"], "ready");
    }

    /// Test metrics endpoint
    #[tokio::test]
    #[ignore = "Requires running server"]
    async fn test_metrics_endpoint() {
        let client = reqwest::Client::new();
        let response = client
            .get("http://localhost:9000/metrics")
            .send()
            .await
            .expect("Failed to send request");

        assert_eq!(response.status(), 200);
        let body = response.text().await.unwrap();
        assert!(body.contains("gateway_"));
    }

    /// Test WebSocket connection without token (should fail)
    #[tokio::test]
    #[ignore = "Requires running server"]
    async fn test_websocket_no_auth() {
        use tokio_tungstenite::connect_async;

        let url = "ws://localhost:9000/ws";
        let result = connect_async(url).await;

        // Should fail without authentication
        assert!(result.is_err() || {
            let (_, response) = result.unwrap();
            response.status().as_u16() == 403
        });
    }

    /// Test WebSocket connection with valid token
    #[tokio::test]
    #[ignore = "Requires running server with matching JWT_SECRET"]
    async fn test_websocket_with_auth() {
        use futures_util::{SinkExt, StreamExt};
        use tokio_tungstenite::{connect_async, tungstenite::http::Request};

        let secret = std::env::var("JWT_SECRET").unwrap_or_else(|_| "supersecret".to_string());
        let token = generate_test_token(&secret, "testuser");

        let request = Request::builder()
            .uri("ws://localhost:9000/ws")
            .header("Sec-WebSocket-Protocol", format!("bearer, {}", token))
            .header("Host", "localhost:9000")
            .header("Connection", "Upgrade")
            .header("Upgrade", "websocket")
            .header("Sec-WebSocket-Version", "13")
            .header("Sec-WebSocket-Key", "dGhlIHNhbXBsZSBub25jZQ==")
            .body(())
            .unwrap();

        let result = connect_async(request).await;
        assert!(result.is_ok(), "WebSocket connection should succeed with valid token");

        let (mut ws_stream, _) = result.unwrap();

        // Send a test message
        ws_stream
            .send(tokio_tungstenite::tungstenite::Message::Text("Hello, World!".into()))
            .await
            .expect("Failed to send message");

        // Should receive the message back (broadcast to self)
        let msg = tokio::time::timeout(Duration::from_secs(5), ws_stream.next())
            .await
            .expect("Timeout waiting for message")
            .expect("Stream ended")
            .expect("Error receiving message");

        if let tokio_tungstenite::tungstenite::Message::Text(text) = msg {
            assert!(text.contains("Hello, World!"));
        }

        // Close connection
        ws_stream.close(None).await.ok();
    }

    /// Test rate limiting on WebSocket connections
    #[tokio::test]
    #[ignore = "Requires running server"]
    async fn test_websocket_rate_limiting() {
        use tokio_tungstenite::{connect_async, tungstenite::http::Request};

        let secret = std::env::var("JWT_SECRET").unwrap_or_else(|_| "supersecret".to_string());
        let token = generate_test_token(&secret, "ratelimituser");

        let mut connections = vec![];

        // Try to open many connections quickly
        for i in 0..100 {
            let request = Request::builder()
                .uri("ws://localhost:9000/ws")
                .header("Sec-WebSocket-Protocol", format!("bearer, {}", token))
                .header("Host", "localhost:9000")
                .header("Connection", "Upgrade")
                .header("Upgrade", "websocket")
                .header("Sec-WebSocket-Version", "13")
                .header("Sec-WebSocket-Key", format!("test-key-{}", i))
                .body(())
                .unwrap();

            match connect_async(request).await {
                Ok((ws, _)) => connections.push(ws),
                Err(_) => {
                    // Rate limiting kicked in
                    println!("Rate limited after {} connections", connections.len());
                    break;
                }
            }
        }

        // Should be rate limited before 100 connections
        assert!(
            connections.len() < 100,
            "Rate limiting should prevent opening too many connections"
        );

        // Clean up
        for mut conn in connections {
            conn.close(None).await.ok();
        }
    }
}
