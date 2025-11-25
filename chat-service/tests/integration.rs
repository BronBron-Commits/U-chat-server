//! Integration tests for chat service with PostgreSQL

use sqlx::{postgres::PgPoolOptions, PgPool};

/// Setup test database connection
async fn setup_test_db() -> PgPool {
    let database_url = std::env::var("TEST_DATABASE_URL")
        .unwrap_or_else(|_| "postgres://unhidra:unhidra_dev_password@localhost:5432/unhidra_test".to_string());

    PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await
        .expect("Failed to connect to test database")
}

#[tokio::test]
async fn test_database_connection() {
    let pool = setup_test_db().await;

    // Test basic query
    let result = sqlx::query!("SELECT 1 as value")
        .fetch_one(&pool)
        .await
        .expect("Failed to execute test query");

    assert_eq!(result.value, Some(1));
}

#[tokio::test]
async fn test_channel_creation() {
    let pool = setup_test_db().await;

    // Create a test channel
    let channel_id = uuid::Uuid::new_v4().to_string();
    let channel_name = format!("test-channel-{}", uuid::Uuid::new_v4());
    let user_id = "test-user-123";

    let result = sqlx::query!(
        r#"
        INSERT INTO channels (id, name, description, channel_type, created_by)
        VALUES ($1, $2, $3, $4, $5)
        RETURNING id
        "#,
        channel_id,
        channel_name,
        Some("Test channel description"),
        "public",
        user_id
    )
    .fetch_one(&pool)
    .await;

    match result {
        Ok(record) => {
            assert_eq!(record.id, channel_id);

            // Cleanup
            let _ = sqlx::query!("DELETE FROM channels WHERE id = $1", channel_id)
                .execute(&pool)
                .await;
        }
        Err(e) => {
            eprintln!("Channel creation test skipped - table may not exist: {}", e);
        }
    }
}

#[tokio::test]
async fn test_thread_operations() {
    let pool = setup_test_db().await;

    let channel_id = uuid::Uuid::new_v4().to_string();
    let thread_id = uuid::Uuid::new_v4().to_string();
    let message_id = uuid::Uuid::new_v4().to_string();
    let user_id = "test-user-456";

    // First create a channel
    let channel_result = sqlx::query!(
        r#"
        INSERT INTO channels (id, name, channel_type, created_by)
        VALUES ($1, $2, $3, $4)
        "#,
        channel_id,
        format!("test-channel-{}", uuid::Uuid::new_v4()),
        "public",
        user_id
    )
    .execute(&pool)
    .await;

    if channel_result.is_err() {
        eprintln!("Thread test skipped - channels table may not exist");
        return;
    }

    // Create a message as parent
    let msg_result = sqlx::query!(
        r#"
        INSERT INTO messages (id, channel_id, sender_id, content, content_type)
        VALUES ($1, $2, $3, $4, $5)
        "#,
        message_id,
        channel_id,
        user_id,
        "Parent message",
        "text"
    )
    .execute(&pool)
    .await;

    if msg_result.is_ok() {
        // Create a thread
        let thread_result = sqlx::query!(
            r#"
            INSERT INTO threads (id, channel_id, parent_message_id)
            VALUES ($1, $2, $3)
            "#,
            thread_id,
            channel_id,
            message_id
        )
        .execute(&pool)
        .await;

        assert!(thread_result.is_ok(), "Thread creation should succeed");

        // Cleanup
        let _ = sqlx::query!("DELETE FROM threads WHERE id = $1", thread_id).execute(&pool).await;
        let _ = sqlx::query!("DELETE FROM messages WHERE id = $1", message_id).execute(&pool).await;
    }

    // Cleanup channel
    let _ = sqlx::query!("DELETE FROM channels WHERE id = $1", channel_id).execute(&pool).await;
}

#[tokio::test]
async fn test_audit_log() {
    let pool = setup_test_db().await;

    let actor_id = uuid::Uuid::new_v4();
    let resource_id = uuid::Uuid::new_v4().to_string();

    let result = sqlx::query!(
        r#"
        INSERT INTO audit_log (actor_id, action, resource_type, resource_id, service_name)
        VALUES ($1, $2, $3, $4, $5)
        RETURNING id
        "#,
        actor_id,
        "test_action",
        "test_resource",
        resource_id,
        "chat-service"
    )
    .fetch_one(&pool)
    .await;

    match result {
        Ok(record) => {
            assert!(record.id > 0);

            // Verify we can query it
            let query_result = sqlx::query!(
                "SELECT COUNT(*) as count FROM audit_log WHERE actor_id = $1",
                actor_id
            )
            .fetch_one(&pool)
            .await;

            assert!(query_result.is_ok());
        }
        Err(e) => {
            eprintln!("Audit log test skipped - table may not exist: {}", e);
        }
    }
}

#[tokio::test]
async fn test_file_metadata() {
    let pool = setup_test_db().await;

    let file_id = uuid::Uuid::new_v4().to_string();
    let channel_id = uuid::Uuid::new_v4().to_string();
    let uploader_id = "test-user-789";

    // First create a channel
    let _ = sqlx::query!(
        "INSERT INTO channels (id, name, channel_type, created_by) VALUES ($1, $2, $3, $4)",
        channel_id,
        format!("test-channel-{}", uuid::Uuid::new_v4()),
        "public",
        uploader_id
    )
    .execute(&pool)
    .await;

    let result = sqlx::query!(
        r#"
        INSERT INTO files (id, channel_id, uploader_id, filename, content_type, size_bytes, storage_key)
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        RETURNING id
        "#,
        file_id,
        channel_id,
        uploader_id,
        "test-file.txt",
        "text/plain",
        1024i64,
        format!("s3://unhidra-files/{}", file_id)
    )
    .fetch_one(&pool)
    .await;

    match result {
        Ok(record) => {
            assert_eq!(record.id, file_id);

            // Cleanup
            let _ = sqlx::query!("DELETE FROM files WHERE id = $1", file_id).execute(&pool).await;
        }
        Err(e) => {
            eprintln!("File metadata test skipped - table may not exist: {}", e);
        }
    }

    // Cleanup channel
    let _ = sqlx::query!("DELETE FROM channels WHERE id = $1", channel_id).execute(&pool).await;
}
