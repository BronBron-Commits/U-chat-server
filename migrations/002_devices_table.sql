-- Migration: 002_devices_table.sql
-- Description: Create devices table for IoT device management
-- Date: 2024-11

-- Create devices table for IoT device registration
CREATE TABLE IF NOT EXISTS devices (
    -- Unique device identifier (e.g., dev_abc12345)
    device_id TEXT PRIMARY KEY NOT NULL,

    -- Human-readable device name
    name TEXT NOT NULL,

    -- Device type (e.g., esp32, esp32-s3, rpi)
    device_type TEXT NOT NULL,

    -- Hashed API key (Argon2id PHC format)
    -- Never store plain API keys!
    api_key_hash TEXT NOT NULL,

    -- Owner's username (foreign key to users table)
    owner_username TEXT NOT NULL,

    -- JSON blob of device capabilities (optional)
    -- Example: {"sensors": ["temperature", "humidity"], "actuators": ["relay"]}
    capabilities TEXT,

    -- Timestamps
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    last_seen TEXT,  -- Updated when device connects

    -- Device status: 'active', 'revoked', 'suspended'
    status TEXT NOT NULL DEFAULT 'active',

    -- Foreign key constraint
    FOREIGN KEY (owner_username) REFERENCES users(username) ON DELETE CASCADE
);

-- Index for faster owner lookups
CREATE INDEX IF NOT EXISTS idx_devices_owner ON devices(owner_username);

-- Index for status filtering
CREATE INDEX IF NOT EXISTS idx_devices_status ON devices(status);

-- Index for device type queries
CREATE INDEX IF NOT EXISTS idx_devices_type ON devices(device_type);

-- Compound index for common query pattern (owner + status)
CREATE INDEX IF NOT EXISTS idx_devices_owner_status ON devices(owner_username, status);

-- Create audit log table for device events
CREATE TABLE IF NOT EXISTS device_audit_log (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    device_id TEXT NOT NULL,
    event_type TEXT NOT NULL,  -- 'registered', 'connected', 'disconnected', 'revoked', 'api_key_rotated'
    event_data TEXT,           -- JSON with additional event details
    ip_address TEXT,           -- Client IP address
    created_at TEXT NOT NULL DEFAULT (datetime('now')),

    FOREIGN KEY (device_id) REFERENCES devices(device_id) ON DELETE CASCADE
);

-- Index for device audit lookups
CREATE INDEX IF NOT EXISTS idx_device_audit_device ON device_audit_log(device_id);
CREATE INDEX IF NOT EXISTS idx_device_audit_type ON device_audit_log(event_type);

-- Comments:
-- 1. API keys are hashed using Argon2id (same as passwords)
-- 2. The api_key is only shown once during registration
-- 3. If a key is lost, the device must be re-registered
-- 4. Revoking a device sets status='revoked' (soft delete)
-- 5. audit_log provides security visibility into device lifecycle
