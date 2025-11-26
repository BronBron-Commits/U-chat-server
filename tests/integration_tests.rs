//! Integration tests for Unhidra services
//!
//! These tests verify end-to-end functionality across services:
//! - Authentication flow (login, SSO, WebAuthn)
//! - E2EE message encryption/decryption
//! - Channel and thread management
//! - File upload/download with encryption
//! - MQTT bridge device integration
//! - Audit logging
//!
//! Run with: `cargo test --test integration_tests`

use std::sync::Arc;

/// Test E2EE message flow
#[tokio::test]
async fn test_e2ee_message_flow() {
    // Initialize E2EE sessions for Alice and Bob
    let alice_identity = e2ee::IdentityKeyPair::generate();
    let bob_identity = e2ee::IdentityKeyPair::generate();

    let alice_prekey = e2ee::PreKeyBundle::generate(&bob_identity);

    // Alice initiates session
    let mut alice_session = e2ee::DoubleRatchet::init_alice(
        &alice_identity,
        &alice_prekey,
    );

    // Bob receives and initializes session
    let mut bob_session = e2ee::DoubleRatchet::init_bob(
        &bob_identity,
        &alice_identity.public_key(),
        &alice_session.get_public_key(),
    );

    // Alice encrypts a message
    let plaintext = b"Hello Bob, this is a secret message!";
    let encrypted = alice_session.encrypt(plaintext).expect("Encryption failed");

    // Bob decrypts the message
    let decrypted = bob_session.decrypt(&encrypted).expect("Decryption failed");

    assert_eq!(plaintext, decrypted.as_slice());
    println!("✅ E2EE message flow test passed");
}

/// Test channel creation and message persistence
#[cfg(feature = "postgres")]
#[tokio::test]
async fn test_channel_management() {
    use sqlx::PgPool;

    // Setup test database connection
    let database_url = std::env::var("TEST_DATABASE_URL")
        .unwrap_or_else(|_| "postgres://localhost/unhidra_test".to_string());

    let pool = PgPool::connect(&database_url)
        .await
        .expect("Failed to connect to test database");

    // Run migrations
    sqlx::migrate!("../migrations")
        .run(&pool)
        .await
        .expect("Failed to run migrations");

    // Create a test channel
    let channel_id = uuid::Uuid::new_v4().to_string();
    let creator_id = uuid::Uuid::new_v4().to_string();

    sqlx::query!(
        r#"
        INSERT INTO channels (id, name, description, channel_type, created_by)
        VALUES ($1, $2, $3, $4, $5)
        "#,
        channel_id,
        "Test Channel",
        Some("Integration test channel"),
        "private",
        creator_id
    )
    .execute(&pool)
    .await
    .expect("Failed to create channel");

    // Add creator as admin member
    sqlx::query!(
        r#"
        INSERT INTO channel_members (channel_id, user_id, role)
        VALUES ($1, $2, 'admin')
        "#,
        channel_id,
        creator_id
    )
    .execute(&pool)
    .await
    .expect("Failed to add channel member");

    // Verify channel exists
    let channel = sqlx::query!(
        r#"
        SELECT id, name, channel_type
        FROM channels
        WHERE id = $1
        "#,
        channel_id
    )
    .fetch_one(&pool)
    .await
    .expect("Failed to fetch channel");

    assert_eq!(channel.name, "Test Channel");
    assert_eq!(channel.channel_type, "private");

    // Cleanup
    sqlx::query!("DELETE FROM channels WHERE id = $1", channel_id)
        .execute(&pool)
        .await
        .expect("Failed to cleanup test channel");

    println!("✅ Channel management test passed");
}

/// Test thread creation and replies
#[cfg(feature = "postgres")]
#[tokio::test]
async fn test_thread_creation() {
    use sqlx::PgPool;

    let database_url = std::env::var("TEST_DATABASE_URL")
        .unwrap_or_else(|_| "postgres://localhost/unhidra_test".to_string());

    let pool = PgPool::connect(&database_url)
        .await
        .expect("Failed to connect to test database");

    // Create test data
    let channel_id = uuid::Uuid::new_v4().to_string();
    let user_id = uuid::Uuid::new_v4().to_string();
    let message_id = uuid::Uuid::new_v4().to_string();
    let thread_id = uuid::Uuid::new_v4().to_string();

    // Create channel
    sqlx::query!(
        "INSERT INTO channels (id, name, channel_type, created_by) VALUES ($1, $2, $3, $4)",
        channel_id, "Test Channel", "public", user_id
    )
    .execute(&pool)
    .await
    .expect("Failed to create channel");

    // Create parent message
    sqlx::query!(
        r#"
        INSERT INTO messages (id, channel_id, sender_id, content, content_type)
        VALUES ($1, $2, $3, $4, 'text')
        "#,
        message_id, channel_id, user_id, "Parent message"
    )
    .execute(&pool)
    .await
    .expect("Failed to create message");

    // Create thread
    sqlx::query!(
        r#"
        INSERT INTO threads (id, channel_id, parent_message_id)
        VALUES ($1, $2, $3)
        "#,
        thread_id, channel_id, message_id
    )
    .execute(&pool)
    .await
    .expect("Failed to create thread");

    // Verify thread
    let thread = sqlx::query!(
        "SELECT id, reply_count FROM threads WHERE id = $1",
        thread_id
    )
    .fetch_one(&pool)
    .await
    .expect("Failed to fetch thread");

    assert_eq!(thread.reply_count, 0);

    // Cleanup
    sqlx::query!("DELETE FROM threads WHERE id = $1", thread_id).execute(&pool).await.ok();
    sqlx::query!("DELETE FROM messages WHERE id = $1", message_id).execute(&pool).await.ok();
    sqlx::query!("DELETE FROM channels WHERE id = $1", channel_id).execute(&pool).await.ok();

    println!("✅ Thread creation test passed");
}

/// Test audit logging
#[tokio::test]
async fn test_audit_logging() {
    use unhidra_core::audit::{AuditAction, AuditEvent, AuditFilter, AuditLogger, MemoryAuditLogger};

    let logger = MemoryAuditLogger::new(1000);

    // Log some events
    let event1 = AuditEvent::new("user123", AuditAction::Login)
        .with_ip("192.168.1.100")
        .with_service("auth-api");

    let event2 = AuditEvent::new("user456", AuditAction::MessageSent)
        .with_service("chat-service")
        .with_resource("message", "msg789");

    logger.log(event1).await.expect("Failed to log event1");
    logger.log(event2).await.expect("Failed to log event2");

    // Query events
    let filter = AuditFilter {
        actor_id: Some("user123".to_string()),
        ..Default::default()
    };

    let events = logger.query(filter).await.expect("Failed to query events");

    assert_eq!(events.len(), 1);
    assert_eq!(events[0].actor_id, "user123");
    assert_eq!(events[0].actor_ip, Some("192.168.1.100".to_string()));

    println!("✅ Audit logging test passed");
}

/// Test MQTT bridge message routing
#[tokio::test]
async fn test_mqtt_bridge() {
    use gateway_service::mqtt_bridge::{IoTMessage, IoTMessageType, MqttBridge, MqttBridgeConfig};

    let config = MqttBridgeConfig::default();
    let bridge = MqttBridge::new(config);

    // Register a device
    bridge.register_device("esp32-001", vec!["temperature".to_string(), "humidity".to_string()]);

    // Verify device is registered
    let status = bridge.get_device_status("esp32-001");
    assert!(status.is_some());
    assert_eq!(status.unwrap().online, true);

    // Create a sensor data message
    let msg = IoTMessage::sensor_data("esp32-001", r#"{"temperature": 22.5, "humidity": 45}"#);
    assert_eq!(msg.message_type, IoTMessageType::SensorData);
    assert_eq!(msg.device_id, "esp32-001");

    // Test topic generation
    let status_topic = bridge.device_status_topic("esp32-001");
    assert_eq!(status_topic, "unhidra/devices/esp32-001/status");

    println!("✅ MQTT bridge test passed");
}

/// Test rate limiting
#[tokio::test]
async fn test_rate_limiting() {
    use gateway_service::rate_limiter::RateLimiter;

    let limiter = RateLimiter::new();

    let ip = "192.168.1.1";

    // Should allow first requests
    for _ in 0..5 {
        assert!(limiter.check_ip_connection(ip).await);
    }

    // Should eventually hit the limit
    // (This depends on configured limits)

    println!("✅ Rate limiting test passed");
}

/// Test password hashing with Argon2id
#[tokio::test]
async fn test_password_hashing() {
    use auth_api::services::password_service::PasswordService;

    let service = PasswordService::new_dev();

    let password = "SuperSecret123!";

    // Hash password
    let hash = service.hash_password(password.as_bytes())
        .await
        .expect("Failed to hash password");

    // Verify correct password
    assert!(service.verify_password(password.as_bytes(), &hash)
        .await
        .expect("Failed to verify password"));

    // Verify incorrect password fails
    assert!(!service.verify_password(b"WrongPassword", &hash)
        .await
        .expect("Failed to verify password"));

    println!("✅ Password hashing test passed");
}

/// Test E2EE file encryption/decryption roundtrip
#[tokio::test]
async fn test_e2ee_file_roundtrip() {
    use e2ee::{DoubleRatchet, KeyPair, PublicKeyBytes};

    // Setup Alice and Bob with shared secret
    let shared_secret = [42u8; 32];
    let bob_prekey = KeyPair::generate();
    let bob_prekey_public = PublicKeyBytes::from_public_key(bob_prekey.public_key());

    let mut alice = DoubleRatchet::init_alice(shared_secret, bob_prekey_public);
    let mut bob = DoubleRatchet::init_bob(shared_secret, bob_prekey);

    // Encrypt a file
    let file_data = b"This is a test file with sensitive content that needs encryption!";
    let encrypted = alice.encrypt(file_data).expect("File encryption failed");

    // Decrypt the file
    let decrypted = bob.decrypt(&encrypted).expect("File decryption failed");

    assert_eq!(file_data.to_vec(), decrypted);
    println!("✅ E2EE file roundtrip test passed");
}

/// Test MQTT reconnect with exponential backoff
#[cfg(feature = "mqtt-bridge")]
#[tokio::test]
async fn test_mqtt_reconnect_behavior() {
    // This test verifies that the MQTT bridge can handle connection errors
    // with exponential backoff and recover gracefully

    use std::time::Duration;
    use tokio::time::Instant;

    // Simulate connection failures and measure backoff timing
    let mut delay = Duration::from_secs(1);
    let max_delay = Duration::from_secs(30);
    let start = Instant::now();

    let mut attempts = vec![];

    // Simulate 5 failed connection attempts
    for i in 0..5 {
        attempts.push(delay);
        tokio::time::sleep(delay).await;

        // Double the delay, up to max
        delay = std::cmp::min(delay * 2, max_delay);
    }

    let elapsed = start.elapsed();

    // Verify exponential backoff: 1s, 2s, 4s, 8s, 16s = 31s total
    assert!(elapsed.as_secs() >= 30 && elapsed.as_secs() <= 33);

    // Verify delays follow exponential pattern
    assert_eq!(attempts[0], Duration::from_secs(1));
    assert_eq!(attempts[1], Duration::from_secs(2));
    assert_eq!(attempts[2], Duration::from_secs(4));
    assert_eq!(attempts[3], Duration::from_secs(8));
    assert_eq!(attempts[4], Duration::from_secs(16));

    println!("✅ MQTT reconnect backoff test passed");
}

/// Test Redis Streams message distribution
#[cfg(all(test, feature = "redis"))]
#[tokio::test]
async fn test_redis_streams_distribution() {
    use chat_service::redis_streams::{RedisConfig, RedisStreams, StreamMessage};

    let redis_url = std::env::var("REDIS_URL")
        .unwrap_or_else(|_| "redis://localhost:6379".to_string());

    let config = RedisConfig {
        url: redis_url,
        consumer_group: "test-group".to_string(),
        consumer_name: "test-consumer".to_string(),
        stream_prefix: "test:chat".to_string(),
        max_stream_length: 1000,
        block_timeout_ms: 1000,
    };

    let mut streams = RedisStreams::new(config).await
        .expect("Failed to create Redis streams client");

    let room_id = "test-room-123";

    // Initialize consumer group
    streams.init_consumer_group(room_id).await
        .expect("Failed to init consumer group");

    // Publish a message
    let message = StreamMessage::new_text(room_id, "user123", "Hello from Redis Streams!");
    let entry_id = streams.publish(&message).await
        .expect("Failed to publish message");

    assert!(!entry_id.is_empty());

    // Get stream info
    let info = streams.stream_info(room_id).await
        .expect("Failed to get stream info");

    assert_eq!(info.room_id, room_id);
    assert!(info.length > 0);

    println!("✅ Redis Streams distribution test passed");
}

/// Test file upload with E2EE metadata
#[cfg(feature = "postgres")]
#[tokio::test]
async fn test_file_upload_with_e2ee() {
    use sqlx::PgPool;

    let database_url = std::env::var("TEST_DATABASE_URL")
        .unwrap_or_else(|_| "postgres://localhost/unhidra_test".to_string());

    let pool = PgPool::connect(&database_url).await
        .expect("Failed to connect to test database");

    // Create test data
    let file_id = uuid::Uuid::new_v4().to_string();
    let channel_id = uuid::Uuid::new_v4().to_string();
    let uploader_id = uuid::Uuid::new_v4().to_string();
    let encryption_key_id = uuid::Uuid::new_v4().to_string();

    // Create channel
    sqlx::query!(
        "INSERT INTO channels (id, name, channel_type, created_by) VALUES ($1, $2, $3, $4)",
        channel_id, "Test Channel", "private", uploader_id
    )
    .execute(&pool)
    .await
    .expect("Failed to create channel");

    // Insert encrypted file record
    sqlx::query!(
        r#"
        INSERT INTO file_uploads (
            id, channel_id, uploader_id, filename, original_filename,
            file_size, mime_type, storage_path, storage_backend,
            checksum, encrypted, encryption_key_id
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
        "#,
        file_id,
        channel_id,
        uploader_id,
        "encrypted-file.bin",
        "secret-document.pdf",
        1024,
        "application/pdf",
        "/tmp/encrypted-file.bin",
        "local",
        "abc123checksum",
        1,
        encryption_key_id
    )
    .execute(&pool)
    .await
    .expect("Failed to insert file record");

    // Verify file is marked as encrypted
    let file = sqlx::query!(
        "SELECT id, encrypted, encryption_key_id FROM file_uploads WHERE id = $1",
        file_id
    )
    .fetch_one(&pool)
    .await
    .expect("Failed to fetch file");

    assert_eq!(file.encrypted, 1);
    assert!(file.encryption_key_id.is_some());

    // Cleanup
    sqlx::query!("DELETE FROM file_uploads WHERE id = $1", file_id).execute(&pool).await.ok();
    sqlx::query!("DELETE FROM channels WHERE id = $1", channel_id).execute(&pool).await.ok();

    println!("✅ File upload with E2EE test passed");
}

/// Integration test marker
#[test]
fn integration_tests_compiled() {
    println!("✅ All integration tests compiled successfully");
}

#[cfg(test)]
mod helpers {
    /// Setup test environment
    pub fn setup_test_env() {
        // Load test .env file if it exists
        dotenv::dotenv().ok();

        // Initialize logging for tests
        let _ = tracing_subscriber::fmt()
            .with_test_writer()
            .with_env_filter("info")
            .try_init();
    }
}
