//! Database operations for chat service

use crate::models::{
    ChatMessage, CreateGroupRequest, Group, GroupMember, GroupWithStats, MemberRole,
    MessageQuery, MessageType, SendMessageRequest, UpdateGroupRequest,
};
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

    // ========================================================================
    // Group Operations
    // ========================================================================

    /// Create a new group
    pub async fn create_group(&self, req: &CreateGroupRequest) -> anyhow::Result<Group> {
        let id = Uuid::new_v4();
        let now = Utc::now();

        sqlx::query(
            r#"
            INSERT INTO groups (id, name, description, owner_id, is_private, created_at)
            VALUES (?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(id.to_string())
        .bind(&req.name)
        .bind(&req.description)
        .bind(&req.owner_id)
        .bind(req.is_private)
        .bind(now)
        .execute(&self.pool)
        .await?;

        // Add owner as a member
        self.add_member_internal(id, &req.owner_id, MemberRole::Owner)
            .await?;

        Ok(Group {
            id,
            name: req.name.clone(),
            description: req.description.clone(),
            owner_id: req.owner_id.clone(),
            is_private: req.is_private,
            created_at: now,
            updated_at: None,
        })
    }

    /// Get a group by ID
    pub async fn get_group(&self, group_id: Uuid) -> anyhow::Result<Option<Group>> {
        let row = sqlx::query("SELECT * FROM groups WHERE id = ?")
            .bind(group_id.to_string())
            .fetch_optional(&self.pool)
            .await?;

        Ok(row.map(|r| self.row_to_group(&r)))
    }

    /// Get a group with stats
    pub async fn get_group_with_stats(
        &self,
        group_id: Uuid,
    ) -> anyhow::Result<Option<GroupWithStats>> {
        let group = match self.get_group(group_id).await? {
            Some(g) => g,
            None => return Ok(None),
        };

        let member_count: i64 =
            sqlx::query("SELECT COUNT(*) as count FROM group_members WHERE group_id = ?")
                .bind(group_id.to_string())
                .fetch_one(&self.pool)
                .await?
                .get("count");

        let message_count: i64 =
            sqlx::query("SELECT COUNT(*) as count FROM messages WHERE group_id = ?")
                .bind(group_id.to_string())
                .fetch_one(&self.pool)
                .await?
                .get("count");

        Ok(Some(GroupWithStats {
            group,
            member_count,
            message_count,
        }))
    }

    /// List all public groups
    pub async fn list_groups(&self, limit: i64, offset: i64) -> anyhow::Result<Vec<Group>> {
        let rows = sqlx::query(
            r#"
            SELECT * FROM groups
            WHERE is_private = 0
            ORDER BY created_at DESC
            LIMIT ? OFFSET ?
            "#,
        )
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.iter().map(|r| self.row_to_group(r)).collect())
    }

    /// Update a group
    pub async fn update_group(
        &self,
        group_id: Uuid,
        req: &UpdateGroupRequest,
    ) -> anyhow::Result<Option<Group>> {
        let now = Utc::now();

        let existing = match self.get_group(group_id).await? {
            Some(g) => g,
            None => return Ok(None),
        };

        let name = req.name.as_ref().unwrap_or(&existing.name);
        let description = req.description.as_ref().or(existing.description.as_ref());
        let is_private = req.is_private.unwrap_or(existing.is_private);

        sqlx::query(
            r#"
            UPDATE groups
            SET name = ?, description = ?, is_private = ?, updated_at = ?
            WHERE id = ?
            "#,
        )
        .bind(name)
        .bind(description)
        .bind(is_private)
        .bind(now)
        .bind(group_id.to_string())
        .execute(&self.pool)
        .await?;

        self.get_group(group_id).await
    }

    /// Delete a group
    pub async fn delete_group(&self, group_id: Uuid) -> anyhow::Result<bool> {
        // Delete messages first (foreign key constraint)
        sqlx::query("DELETE FROM messages WHERE group_id = ?")
            .bind(group_id.to_string())
            .execute(&self.pool)
            .await?;

        // Delete members
        sqlx::query("DELETE FROM group_members WHERE group_id = ?")
            .bind(group_id.to_string())
            .execute(&self.pool)
            .await?;

        // Delete group
        let result = sqlx::query("DELETE FROM groups WHERE id = ?")
            .bind(group_id.to_string())
            .execute(&self.pool)
            .await?;

        Ok(result.rows_affected() > 0)
    }

    // ========================================================================
    // Member Operations
    // ========================================================================

    /// Add a member to a group (internal)
    async fn add_member_internal(
        &self,
        group_id: Uuid,
        user_id: &str,
        role: MemberRole,
    ) -> anyhow::Result<GroupMember> {
        let now = Utc::now();

        sqlx::query(
            r#"
            INSERT OR REPLACE INTO group_members (group_id, user_id, role, joined_at)
            VALUES (?, ?, ?, ?)
            "#,
        )
        .bind(group_id.to_string())
        .bind(user_id)
        .bind(role.as_str())
        .bind(now)
        .execute(&self.pool)
        .await?;

        Ok(GroupMember {
            group_id,
            user_id: user_id.to_string(),
            role,
            joined_at: now,
        })
    }

    /// Add a member to a group
    pub async fn add_member(
        &self,
        group_id: Uuid,
        user_id: &str,
        role: MemberRole,
    ) -> anyhow::Result<GroupMember> {
        self.add_member_internal(group_id, user_id, role).await
    }

    /// Remove a member from a group
    pub async fn remove_member(&self, group_id: Uuid, user_id: &str) -> anyhow::Result<bool> {
        let result = sqlx::query("DELETE FROM group_members WHERE group_id = ? AND user_id = ?")
            .bind(group_id.to_string())
            .bind(user_id)
            .execute(&self.pool)
            .await?;

        Ok(result.rows_affected() > 0)
    }

    /// List members of a group
    pub async fn list_members(&self, group_id: Uuid) -> anyhow::Result<Vec<GroupMember>> {
        let rows = sqlx::query(
            r#"
            SELECT * FROM group_members
            WHERE group_id = ?
            ORDER BY joined_at ASC
            "#,
        )
        .bind(group_id.to_string())
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.iter().map(|r| self.row_to_member(r)).collect())
    }

    /// Check if a user is a member of a group
    pub async fn is_member(&self, group_id: Uuid, user_id: &str) -> anyhow::Result<bool> {
        let count: i64 = sqlx::query(
            "SELECT COUNT(*) as count FROM group_members WHERE group_id = ? AND user_id = ?",
        )
        .bind(group_id.to_string())
        .bind(user_id)
        .fetch_one(&self.pool)
        .await?
        .get("count");

        Ok(count > 0)
    }

    /// Get a user's groups
    pub async fn get_user_groups(&self, user_id: &str) -> anyhow::Result<Vec<Group>> {
        let rows = sqlx::query(
            r#"
            SELECT g.* FROM groups g
            INNER JOIN group_members m ON g.id = m.group_id
            WHERE m.user_id = ?
            ORDER BY g.created_at DESC
            "#,
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.iter().map(|r| self.row_to_group(r)).collect())
    }

    // ========================================================================
    // Message Operations
    // ========================================================================

    /// Send a message to a group
    pub async fn send_message(
        &self,
        group_id: Uuid,
        req: &SendMessageRequest,
    ) -> anyhow::Result<ChatMessage> {
        let id = Uuid::new_v4();
        let now = Utc::now();

        sqlx::query(
            r#"
            INSERT INTO messages (id, group_id, sender_id, sender_name, content, message_type, created_at)
            VALUES (?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(id.to_string())
        .bind(group_id.to_string())
        .bind(&req.sender_id)
        .bind(&req.sender_name)
        .bind(&req.content)
        .bind(req.message_type.as_str())
        .bind(now)
        .execute(&self.pool)
        .await?;

        Ok(ChatMessage {
            id,
            group_id,
            sender_id: req.sender_id.clone(),
            sender_name: req.sender_name.clone(),
            content: req.content.clone(),
            message_type: req.message_type,
            created_at: now,
            edited_at: None,
        })
    }

    /// Get messages from a group
    pub async fn get_messages(
        &self,
        group_id: Uuid,
        query: &MessageQuery,
    ) -> anyhow::Result<Vec<ChatMessage>> {
        let rows = sqlx::query(
            r#"
            SELECT * FROM messages
            WHERE group_id = ?
            ORDER BY created_at DESC
            LIMIT ? OFFSET ?
            "#,
        )
        .bind(group_id.to_string())
        .bind(query.limit)
        .bind(query.offset)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.iter().map(|r| self.row_to_message(r)).collect())
    }

    // ========================================================================
    // Health Check
    // ========================================================================

    /// Check database health
    pub async fn health_check(&self) -> bool {
        sqlx::query("SELECT 1")
            .fetch_one(&self.pool)
            .await
            .is_ok()
    }

    // ========================================================================
    // Helper Methods
    // ========================================================================

    fn row_to_group(&self, row: &sqlx::sqlite::SqliteRow) -> Group {
        let id_str: String = row.get("id");
        Group {
            id: Uuid::parse_str(&id_str).unwrap_or_else(|_| Uuid::new_v4()),
            name: row.get("name"),
            description: row.get("description"),
            owner_id: row.get("owner_id"),
            is_private: row.get("is_private"),
            created_at: row.get("created_at"),
            updated_at: row.get("updated_at"),
        }
    }

    fn row_to_member(&self, row: &sqlx::sqlite::SqliteRow) -> GroupMember {
        let group_id_str: String = row.get("group_id");
        let role_str: String = row.get("role");
        GroupMember {
            group_id: Uuid::parse_str(&group_id_str).unwrap_or_else(|_| Uuid::new_v4()),
            user_id: row.get("user_id"),
            role: MemberRole::from_str(&role_str),
            joined_at: row.get("joined_at"),
        }
    }

    fn row_to_message(&self, row: &sqlx::sqlite::SqliteRow) -> ChatMessage {
        let id_str: String = row.get("id");
        let group_id_str: String = row.get("group_id");
        let message_type_str: String = row.get("message_type");
        ChatMessage {
            id: Uuid::parse_str(&id_str).unwrap_or_else(|_| Uuid::new_v4()),
            group_id: Uuid::parse_str(&group_id_str).unwrap_or_else(|_| Uuid::new_v4()),
            sender_id: row.get("sender_id"),
            sender_name: row.get("sender_name"),
            content: row.get("content"),
            message_type: MessageType::from_str(&message_type_str),
            created_at: row.get("created_at"),
            edited_at: row.get("edited_at"),
        }
    }
}

/// Run database migrations
pub async fn run_migrations(pool: &SqlitePool) -> anyhow::Result<()> {
    tracing::info!("Running database migrations...");

    // Groups table
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS groups (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            description TEXT,
            owner_id TEXT NOT NULL,
            is_private INTEGER NOT NULL DEFAULT 0,
            created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
            updated_at DATETIME
        )
        "#,
    )
    .execute(pool)
    .await?;

    // Group members table
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS group_members (
            group_id TEXT NOT NULL,
            user_id TEXT NOT NULL,
            role TEXT NOT NULL DEFAULT 'member',
            joined_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
            PRIMARY KEY (group_id, user_id),
            FOREIGN KEY (group_id) REFERENCES groups(id)
        )
        "#,
    )
    .execute(pool)
    .await?;

    // Messages table
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS messages (
            id TEXT PRIMARY KEY,
            group_id TEXT NOT NULL,
            sender_id TEXT NOT NULL,
            sender_name TEXT NOT NULL,
            content TEXT NOT NULL,
            message_type TEXT NOT NULL DEFAULT 'text',
            created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
            edited_at DATETIME,
            FOREIGN KEY (group_id) REFERENCES groups(id)
        )
        "#,
    )
    .execute(pool)
    .await?;

    // Create indexes
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_messages_group ON messages(group_id)")
        .execute(pool)
        .await?;

    sqlx::query("CREATE INDEX IF NOT EXISTS idx_messages_created ON messages(created_at DESC)")
        .execute(pool)
        .await?;

    sqlx::query("CREATE INDEX IF NOT EXISTS idx_members_user ON group_members(user_id)")
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
    async fn test_create_group() {
        let db = setup_test_db().await;

        let req = CreateGroupRequest {
            name: "Test Group".to_string(),
            description: Some("A test group".to_string()),
            owner_id: "user1".to_string(),
            is_private: false,
        };

        let group = db.create_group(&req).await.unwrap();
        assert_eq!(group.name, "Test Group");
        assert_eq!(group.owner_id, "user1");
    }

    #[tokio::test]
    async fn test_group_membership() {
        let db = setup_test_db().await;

        let group = db
            .create_group(&CreateGroupRequest {
                name: "Test".to_string(),
                description: None,
                owner_id: "owner".to_string(),
                is_private: false,
            })
            .await
            .unwrap();

        // Owner should be a member
        assert!(db.is_member(group.id, "owner").await.unwrap());

        // Add another member
        db.add_member(group.id, "user2", MemberRole::Member)
            .await
            .unwrap();
        assert!(db.is_member(group.id, "user2").await.unwrap());

        // List members
        let members = db.list_members(group.id).await.unwrap();
        assert_eq!(members.len(), 2);
    }

    #[tokio::test]
    async fn test_send_and_get_messages() {
        let db = setup_test_db().await;

        let group = db
            .create_group(&CreateGroupRequest {
                name: "Chat".to_string(),
                description: None,
                owner_id: "owner".to_string(),
                is_private: false,
            })
            .await
            .unwrap();

        // Send messages
        for i in 0..5 {
            db.send_message(
                group.id,
                &SendMessageRequest {
                    sender_id: "owner".to_string(),
                    sender_name: "Owner".to_string(),
                    content: format!("Message {}", i),
                    message_type: MessageType::Text,
                },
            )
            .await
            .unwrap();
        }

        // Get messages
        let messages = db
            .get_messages(group.id, &MessageQuery::default())
            .await
            .unwrap();
        assert_eq!(messages.len(), 5);
    }
}
