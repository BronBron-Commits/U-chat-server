//! Database operations for history service

use crate::models::{MessageQuery, PaginatedMessages, SearchQuery, StoredMessage, StoreMessageRequest};
use chrono::Utc;
use sqlx::{sqlite::SqlitePool, Row};
use uuid::Uuid;

/// Database wrapper with query methods
#[derive(Clone)]
pub struct Database {
    pool: SqlitePool,
}

impl Database {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Store a new message
    pub async fn store_message(&self, req: &StoreMessageRequest) -> anyhow::Result<StoredMessage> {
        let id = Uuid::new_v4();
        let now = Utc::now();

        sqlx::query(
            r#"
            INSERT INTO messages (id, content, sender_id, sender_name, room_id, group_id, message_type, created_at)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(id.to_string())
        .bind(&req.content)
        .bind(&req.sender_id)
        .bind(&req.sender_name)
        .bind(&req.room_id)
        .bind(req.group_id.map(|g| g.to_string()))
        .bind(&req.message_type)
        .bind(now)
        .execute(&self.pool)
        .await?;

        Ok(StoredMessage {
            id,
            content: req.content.clone(),
            sender_id: req.sender_id.clone(),
            sender_name: req.sender_name.clone(),
            room_id: req.room_id.clone(),
            group_id: req.group_id,
            message_type: req.message_type.clone(),
            created_at: now,
            updated_at: None,
        })
    }

    /// Get messages with pagination
    pub async fn get_messages(&self, query: &MessageQuery) -> anyhow::Result<PaginatedMessages> {
        let mut sql = String::from("SELECT * FROM messages WHERE 1=1");
        let mut count_sql = String::from("SELECT COUNT(*) as count FROM messages WHERE 1=1");

        if let Some(before) = &query.before {
            sql.push_str(&format!(" AND created_at < '{}'", before.to_rfc3339()));
            count_sql.push_str(&format!(" AND created_at < '{}'", before.to_rfc3339()));
        }
        if let Some(after) = &query.after {
            sql.push_str(&format!(" AND created_at > '{}'", after.to_rfc3339()));
            count_sql.push_str(&format!(" AND created_at > '{}'", after.to_rfc3339()));
        }

        sql.push_str(" ORDER BY created_at DESC");
        sql.push_str(&format!(" LIMIT {} OFFSET {}", query.limit, query.offset));

        let messages = self.query_messages(&sql).await?;
        let total: i64 = sqlx::query(&count_sql)
            .fetch_one(&self.pool)
            .await?
            .get("count");

        Ok(PaginatedMessages {
            messages,
            total,
            limit: query.limit,
            offset: query.offset,
            has_more: query.offset + query.limit < total,
        })
    }

    /// Get messages for a specific room
    pub async fn get_room_messages(
        &self,
        room_id: &str,
        query: &MessageQuery,
    ) -> anyhow::Result<PaginatedMessages> {
        let messages = sqlx::query(
            r#"
            SELECT * FROM messages
            WHERE room_id = ?
            ORDER BY created_at DESC
            LIMIT ? OFFSET ?
            "#,
        )
        .bind(room_id)
        .bind(query.limit)
        .bind(query.offset)
        .fetch_all(&self.pool)
        .await?;

        let total: i64 = sqlx::query("SELECT COUNT(*) as count FROM messages WHERE room_id = ?")
            .bind(room_id)
            .fetch_one(&self.pool)
            .await?
            .get("count");

        Ok(PaginatedMessages {
            messages: messages.into_iter().map(|r| self.row_to_message(&r)).collect(),
            total,
            limit: query.limit,
            offset: query.offset,
            has_more: query.offset + query.limit < total,
        })
    }

    /// Get messages from a specific user
    pub async fn get_user_messages(
        &self,
        user_id: &str,
        query: &MessageQuery,
    ) -> anyhow::Result<PaginatedMessages> {
        let messages = sqlx::query(
            r#"
            SELECT * FROM messages
            WHERE sender_id = ?
            ORDER BY created_at DESC
            LIMIT ? OFFSET ?
            "#,
        )
        .bind(user_id)
        .bind(query.limit)
        .bind(query.offset)
        .fetch_all(&self.pool)
        .await?;

        let total: i64 = sqlx::query("SELECT COUNT(*) as count FROM messages WHERE sender_id = ?")
            .bind(user_id)
            .fetch_one(&self.pool)
            .await?
            .get("count");

        Ok(PaginatedMessages {
            messages: messages.into_iter().map(|r| self.row_to_message(&r)).collect(),
            total,
            limit: query.limit,
            offset: query.offset,
            has_more: query.offset + query.limit < total,
        })
    }

    /// Search messages by content
    pub async fn search_messages(&self, query: &SearchQuery) -> anyhow::Result<PaginatedMessages> {
        let search_pattern = format!("%{}%", query.q);

        let (messages, total) = if let Some(room_id) = &query.room_id {
            let messages = sqlx::query(
                r#"
                SELECT * FROM messages
                WHERE content LIKE ? AND room_id = ?
                ORDER BY created_at DESC
                LIMIT ? OFFSET ?
                "#,
            )
            .bind(&search_pattern)
            .bind(room_id)
            .bind(query.limit)
            .bind(query.offset)
            .fetch_all(&self.pool)
            .await?;

            let total: i64 = sqlx::query(
                "SELECT COUNT(*) as count FROM messages WHERE content LIKE ? AND room_id = ?",
            )
            .bind(&search_pattern)
            .bind(room_id)
            .fetch_one(&self.pool)
            .await?
            .get("count");

            (messages, total)
        } else {
            let messages = sqlx::query(
                r#"
                SELECT * FROM messages
                WHERE content LIKE ?
                ORDER BY created_at DESC
                LIMIT ? OFFSET ?
                "#,
            )
            .bind(&search_pattern)
            .bind(query.limit)
            .bind(query.offset)
            .fetch_all(&self.pool)
            .await?;

            let total: i64 =
                sqlx::query("SELECT COUNT(*) as count FROM messages WHERE content LIKE ?")
                    .bind(&search_pattern)
                    .fetch_one(&self.pool)
                    .await?
                    .get("count");

            (messages, total)
        };

        Ok(PaginatedMessages {
            messages: messages.into_iter().map(|r| self.row_to_message(&r)).collect(),
            total,
            limit: query.limit,
            offset: query.offset,
            has_more: query.offset + query.limit < total,
        })
    }

    /// Helper to run arbitrary message queries
    async fn query_messages(&self, sql: &str) -> anyhow::Result<Vec<StoredMessage>> {
        let rows = sqlx::query(sql).fetch_all(&self.pool).await?;
        Ok(rows.iter().map(|r| self.row_to_message(r)).collect())
    }

    /// Convert a database row to a StoredMessage
    fn row_to_message(&self, row: &sqlx::sqlite::SqliteRow) -> StoredMessage {
        let id_str: String = row.get("id");
        let group_id_str: Option<String> = row.get("group_id");

        StoredMessage {
            id: Uuid::parse_str(&id_str).unwrap_or_else(|_| Uuid::new_v4()),
            content: row.get("content"),
            sender_id: row.get("sender_id"),
            sender_name: row.get("sender_name"),
            room_id: row.get("room_id"),
            group_id: group_id_str.and_then(|s| Uuid::parse_str(&s).ok()),
            message_type: row.get("message_type"),
            created_at: row.get("created_at"),
            updated_at: row.get("updated_at"),
        }
    }

    /// Check database health
    pub async fn health_check(&self) -> bool {
        sqlx::query("SELECT 1")
            .fetch_one(&self.pool)
            .await
            .is_ok()
    }
}

/// Run database migrations
pub async fn run_migrations(pool: &SqlitePool) -> anyhow::Result<()> {
    tracing::info!("Running database migrations...");

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS messages (
            id TEXT PRIMARY KEY,
            content TEXT NOT NULL,
            sender_id TEXT NOT NULL,
            sender_name TEXT NOT NULL,
            room_id TEXT,
            group_id TEXT,
            message_type TEXT NOT NULL DEFAULT 'text',
            created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
            updated_at DATETIME
        )
        "#,
    )
    .execute(pool)
    .await?;

    // Create indexes for common queries
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_messages_room ON messages(room_id)")
        .execute(pool)
        .await?;

    sqlx::query("CREATE INDEX IF NOT EXISTS idx_messages_sender ON messages(sender_id)")
        .execute(pool)
        .await?;

    sqlx::query("CREATE INDEX IF NOT EXISTS idx_messages_created ON messages(created_at DESC)")
        .execute(pool)
        .await?;

    tracing::info!("Database migrations completed");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn setup_test_db() -> Database {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();

        run_migrations(&pool).await.unwrap();
        Database::new(pool)
    }

    #[tokio::test]
    async fn test_store_and_retrieve_message() {
        let db = setup_test_db().await;

        let req = StoreMessageRequest {
            content: "Hello, world!".to_string(),
            sender_id: "user1".to_string(),
            sender_name: "Alice".to_string(),
            room_id: Some("general".to_string()),
            group_id: None,
            message_type: "text".to_string(),
        };

        let stored = db.store_message(&req).await.unwrap();
        assert_eq!(stored.content, "Hello, world!");
        assert_eq!(stored.sender_name, "Alice");

        let query = MessageQuery::default();
        let result = db.get_messages(&query).await.unwrap();
        assert_eq!(result.messages.len(), 1);
        assert_eq!(result.total, 1);
    }

    #[tokio::test]
    async fn test_get_room_messages() {
        let db = setup_test_db().await;

        // Store messages in different rooms
        for i in 0..5 {
            let room = if i % 2 == 0 { "room-a" } else { "room-b" };
            let req = StoreMessageRequest {
                content: format!("Message {}", i),
                sender_id: "user1".to_string(),
                sender_name: "Alice".to_string(),
                room_id: Some(room.to_string()),
                group_id: None,
                message_type: "text".to_string(),
            };
            db.store_message(&req).await.unwrap();
        }

        let query = MessageQuery::default();
        let result = db.get_room_messages("room-a", &query).await.unwrap();
        assert_eq!(result.total, 3);
    }

    #[tokio::test]
    async fn test_search_messages() {
        let db = setup_test_db().await;

        let messages = vec!["Hello world", "Goodbye world", "Hello there", "Test message"];
        for content in messages {
            let req = StoreMessageRequest {
                content: content.to_string(),
                sender_id: "user1".to_string(),
                sender_name: "Alice".to_string(),
                room_id: None,
                group_id: None,
                message_type: "text".to_string(),
            };
            db.store_message(&req).await.unwrap();
        }

        let query = SearchQuery {
            q: "Hello".to_string(),
            limit: 50,
            offset: 0,
            room_id: None,
        };
        let result = db.search_messages(&query).await.unwrap();
        assert_eq!(result.total, 2);
    }
}
