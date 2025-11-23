-- Channels and Threads Migration
-- Supports enterprise chat features: channels, threads, file uploads, reactions

-- ============================================================================
-- Channels
-- ============================================================================
CREATE TABLE IF NOT EXISTS channels (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    description TEXT,
    channel_type TEXT NOT NULL DEFAULT 'public',  -- 'public', 'private', 'direct'
    created_by TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now', 'utc')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now', 'utc')),
    archived_at TEXT,
    metadata TEXT  -- JSON for custom attributes
);

CREATE INDEX IF NOT EXISTS idx_channels_type ON channels(channel_type);
CREATE INDEX IF NOT EXISTS idx_channels_created_by ON channels(created_by);

-- Channel Members
CREATE TABLE IF NOT EXISTS channel_members (
    channel_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    role TEXT NOT NULL DEFAULT 'member',  -- 'owner', 'admin', 'member', 'guest'
    joined_at TEXT NOT NULL DEFAULT (datetime('now', 'utc')),
    last_read_at TEXT,
    notifications TEXT NOT NULL DEFAULT 'all',  -- 'all', 'mentions', 'none'
    PRIMARY KEY (channel_id, user_id),
    FOREIGN KEY (channel_id) REFERENCES channels(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_channel_members_user ON channel_members(user_id);

-- ============================================================================
-- Messages (enhanced)
-- ============================================================================
CREATE TABLE IF NOT EXISTS messages (
    id TEXT PRIMARY KEY NOT NULL,
    channel_id TEXT NOT NULL,
    sender_id TEXT NOT NULL,
    thread_id TEXT,  -- NULL for top-level messages, parent message ID for replies
    content TEXT NOT NULL,  -- Encrypted content if E2EE enabled
    content_type TEXT NOT NULL DEFAULT 'text',  -- 'text', 'file', 'system', 'e2ee'
    edited_at TEXT,  -- Last edit timestamp
    deleted_at TEXT,  -- Soft delete
    created_at TEXT NOT NULL DEFAULT (datetime('now', 'utc')),
    metadata TEXT,  -- JSON: mentions, formatting, etc.
    FOREIGN KEY (channel_id) REFERENCES channels(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_messages_channel ON messages(channel_id, created_at);
CREATE INDEX IF NOT EXISTS idx_messages_sender ON messages(sender_id);
CREATE INDEX IF NOT EXISTS idx_messages_thread ON messages(thread_id);

-- ============================================================================
-- Threads
-- ============================================================================
CREATE TABLE IF NOT EXISTS threads (
    id TEXT PRIMARY KEY NOT NULL,
    channel_id TEXT NOT NULL,
    parent_message_id TEXT NOT NULL,
    reply_count INTEGER NOT NULL DEFAULT 0,
    participant_count INTEGER NOT NULL DEFAULT 0,
    last_reply_at TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now', 'utc')),
    FOREIGN KEY (channel_id) REFERENCES channels(id) ON DELETE CASCADE,
    FOREIGN KEY (parent_message_id) REFERENCES messages(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_threads_channel ON threads(channel_id);
CREATE INDEX IF NOT EXISTS idx_threads_parent ON threads(parent_message_id);

-- Thread Participants
CREATE TABLE IF NOT EXISTS thread_participants (
    thread_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    joined_at TEXT NOT NULL DEFAULT (datetime('now', 'utc')),
    last_read_at TEXT,
    PRIMARY KEY (thread_id, user_id),
    FOREIGN KEY (thread_id) REFERENCES threads(id) ON DELETE CASCADE
);

-- ============================================================================
-- File Uploads
-- ============================================================================
CREATE TABLE IF NOT EXISTS file_uploads (
    id TEXT PRIMARY KEY NOT NULL,
    message_id TEXT,
    channel_id TEXT NOT NULL,
    uploader_id TEXT NOT NULL,
    filename TEXT NOT NULL,
    original_filename TEXT NOT NULL,
    file_size INTEGER NOT NULL,
    mime_type TEXT NOT NULL,
    storage_path TEXT NOT NULL,  -- S3 key or local path
    storage_backend TEXT NOT NULL DEFAULT 'local',  -- 'local', 's3', 'minio'
    checksum TEXT NOT NULL,  -- SHA-256 hash
    encrypted INTEGER NOT NULL DEFAULT 0,  -- Client-side E2EE
    encryption_key_id TEXT,  -- Reference to key used for encryption
    thumbnail_path TEXT,  -- For images/videos
    created_at TEXT NOT NULL DEFAULT (datetime('now', 'utc')),
    deleted_at TEXT,
    metadata TEXT,  -- JSON: dimensions, duration, etc.
    FOREIGN KEY (message_id) REFERENCES messages(id) ON DELETE SET NULL,
    FOREIGN KEY (channel_id) REFERENCES channels(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_files_channel ON file_uploads(channel_id);
CREATE INDEX IF NOT EXISTS idx_files_uploader ON file_uploads(uploader_id);
CREATE INDEX IF NOT EXISTS idx_files_message ON file_uploads(message_id);

-- ============================================================================
-- Reactions
-- ============================================================================
CREATE TABLE IF NOT EXISTS reactions (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    message_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    emoji TEXT NOT NULL,  -- Unicode emoji or custom emoji ID
    created_at TEXT NOT NULL DEFAULT (datetime('now', 'utc')),
    UNIQUE(message_id, user_id, emoji),
    FOREIGN KEY (message_id) REFERENCES messages(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_reactions_message ON reactions(message_id);
CREATE INDEX IF NOT EXISTS idx_reactions_user ON reactions(user_id);

-- ============================================================================
-- Read Receipts
-- ============================================================================
CREATE TABLE IF NOT EXISTS read_receipts (
    channel_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    last_read_message_id TEXT NOT NULL,
    last_read_at TEXT NOT NULL DEFAULT (datetime('now', 'utc')),
    PRIMARY KEY (channel_id, user_id),
    FOREIGN KEY (channel_id) REFERENCES channels(id) ON DELETE CASCADE,
    FOREIGN KEY (last_read_message_id) REFERENCES messages(id) ON DELETE CASCADE
);

-- ============================================================================
-- Typing Indicators (transient, typically in Redis)
-- ============================================================================
CREATE TABLE IF NOT EXISTS typing_indicators (
    channel_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    started_at TEXT NOT NULL DEFAULT (datetime('now', 'utc')),
    PRIMARY KEY (channel_id, user_id)
);

-- ============================================================================
-- User Preferences
-- ============================================================================
CREATE TABLE IF NOT EXISTS user_preferences (
    user_id TEXT PRIMARY KEY NOT NULL,
    theme TEXT DEFAULT 'system',  -- 'light', 'dark', 'system'
    language TEXT DEFAULT 'en',
    timezone TEXT DEFAULT 'UTC',
    notification_sound INTEGER DEFAULT 1,
    desktop_notifications INTEGER DEFAULT 1,
    email_notifications INTEGER DEFAULT 0,
    message_preview INTEGER DEFAULT 1,
    compact_mode INTEGER DEFAULT 0,
    metadata TEXT,  -- JSON for additional preferences
    updated_at TEXT NOT NULL DEFAULT (datetime('now', 'utc'))
);

-- ============================================================================
-- Views for Common Queries
-- ============================================================================

-- Unread message count per channel per user
CREATE VIEW IF NOT EXISTS unread_counts AS
SELECT
    cm.channel_id,
    cm.user_id,
    COUNT(m.id) as unread_count
FROM channel_members cm
LEFT JOIN read_receipts rr ON cm.channel_id = rr.channel_id AND cm.user_id = rr.user_id
LEFT JOIN messages m ON m.channel_id = cm.channel_id
    AND m.deleted_at IS NULL
    AND (rr.last_read_message_id IS NULL OR m.created_at > (
        SELECT created_at FROM messages WHERE id = rr.last_read_message_id
    ))
GROUP BY cm.channel_id, cm.user_id;

-- Active threads with reply counts
CREATE VIEW IF NOT EXISTS active_threads AS
SELECT
    t.id,
    t.channel_id,
    t.parent_message_id,
    t.reply_count,
    t.participant_count,
    t.last_reply_at,
    m.content as parent_content,
    m.sender_id as parent_sender
FROM threads t
JOIN messages m ON t.parent_message_id = m.id
WHERE t.last_reply_at > datetime('now', '-7 days')
ORDER BY t.last_reply_at DESC;
