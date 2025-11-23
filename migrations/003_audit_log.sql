-- Immutable Audit Log Table for Compliance
-- This table is designed to be append-only for compliance requirements
-- fillfactor=100 optimizes for append-only workloads

-- For SQLite (current database)
CREATE TABLE IF NOT EXISTS audit_log (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    timestamp TEXT NOT NULL DEFAULT (datetime('now', 'utc')),
    -- Actor information
    actor_id TEXT NOT NULL,
    actor_type TEXT NOT NULL DEFAULT 'user',  -- 'user', 'device', 'system', 'service'
    actor_ip TEXT,
    -- Action details
    action TEXT NOT NULL,  -- 'login', 'logout', 'message_sent', 'device_registered', etc.
    action_result TEXT NOT NULL DEFAULT 'success',  -- 'success', 'failure', 'denied'
    -- Resource information
    resource_type TEXT,  -- 'user', 'device', 'room', 'message', etc.
    resource_id TEXT,
    -- Context and metadata
    service_name TEXT,  -- Service that generated the log
    request_id TEXT,  -- For correlating related events
    session_id TEXT,
    metadata TEXT,  -- JSON blob for additional context
    -- Integrity
    signature TEXT,  -- Optional: HMAC signature for tamper detection
    previous_hash TEXT  -- Optional: Hash chain for integrity
);

-- Indexes for common query patterns
CREATE INDEX IF NOT EXISTS idx_audit_timestamp ON audit_log(timestamp);
CREATE INDEX IF NOT EXISTS idx_audit_actor ON audit_log(actor_id, timestamp);
CREATE INDEX IF NOT EXISTS idx_audit_action ON audit_log(action, timestamp);
CREATE INDEX IF NOT EXISTS idx_audit_resource ON audit_log(resource_type, resource_id, timestamp);
CREATE INDEX IF NOT EXISTS idx_audit_result ON audit_log(action_result, timestamp);
CREATE INDEX IF NOT EXISTS idx_audit_session ON audit_log(session_id);

-- View for recent security events
CREATE VIEW IF NOT EXISTS security_events AS
SELECT
    id,
    timestamp,
    actor_id,
    actor_type,
    actor_ip,
    action,
    action_result,
    resource_type,
    resource_id,
    metadata
FROM audit_log
WHERE action IN (
    'login',
    'login_failed',
    'logout',
    'password_change',
    'passkey_registered',
    'passkey_revoked',
    'device_registered',
    'device_revoked',
    'permission_denied',
    'rate_limit_exceeded',
    'suspicious_activity'
)
ORDER BY timestamp DESC;

-- View for compliance reporting
CREATE VIEW IF NOT EXISTS compliance_summary AS
SELECT
    date(timestamp) as log_date,
    action,
    action_result,
    COUNT(*) as event_count
FROM audit_log
GROUP BY date(timestamp), action, action_result
ORDER BY log_date DESC, event_count DESC;

-- PostgreSQL version (for future migration)
-- CREATE TABLE audit_log (
--     id BIGSERIAL PRIMARY KEY,
--     timestamp TIMESTAMPTZ NOT NULL DEFAULT NOW(),
--     actor_id UUID NOT NULL,
--     actor_type TEXT NOT NULL DEFAULT 'user',
--     actor_ip INET,
--     action TEXT NOT NULL,
--     action_result TEXT NOT NULL DEFAULT 'success',
--     resource_type TEXT,
--     resource_id UUID,
--     service_name TEXT,
--     request_id UUID,
--     session_id UUID,
--     metadata JSONB,
--     signature BYTEA,
--     previous_hash BYTEA
-- ) WITH (fillfactor=100);
--
-- CREATE INDEX idx_audit_timestamp ON audit_log(timestamp);
-- CREATE INDEX idx_audit_actor ON audit_log(actor_id, timestamp);
-- CREATE INDEX idx_audit_action ON audit_log(action, timestamp);
-- CREATE INDEX idx_audit_resource ON audit_log(resource_type, resource_id, timestamp);
-- CREATE INDEX idx_audit_result ON audit_log(action_result, timestamp);
-- CREATE INDEX idx_audit_session ON audit_log(session_id);
