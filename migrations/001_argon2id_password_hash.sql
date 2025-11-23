-- Migration: Argon2id Password Hash Support
-- Date: 2025-01-XX
-- Description: Expand password_hash column to accommodate PHC-formatted Argon2id hashes
--              and remove legacy salt column (salt is now embedded in hash)
--
-- Argon2id PHC format example: $argon2id$v=19$m=49152,t=3,p=1$<salt>$<hash>
-- These hashes can exceed 180 characters, so we expand to VARCHAR(255)

-- SQLite doesn't support ALTER COLUMN directly, so we need to recreate the table
-- This migration assumes a users table with the following structure:
--   username TEXT PRIMARY KEY
--   password_hash TEXT
--   salt TEXT (to be removed)
--   verified INTEGER
--   display_name TEXT

-- Step 1: Create new table with updated schema
CREATE TABLE IF NOT EXISTS users_new (
    username TEXT PRIMARY KEY NOT NULL,
    password_hash TEXT NOT NULL,  -- Now stores PHC-formatted Argon2id hash (up to 255 chars)
    verified INTEGER NOT NULL DEFAULT 0,
    display_name TEXT NOT NULL DEFAULT ''
);

-- Step 2: Migrate existing data (if any)
-- Note: Existing SHA256 hashes will NOT be compatible with Argon2id verification
-- Users with old hashes will need password resets
INSERT OR IGNORE INTO users_new (username, password_hash, verified, display_name)
SELECT username, password_hash, verified, display_name FROM users;

-- Step 3: Drop old table and rename new one
DROP TABLE IF EXISTS users;
ALTER TABLE users_new RENAME TO users;

-- Step 4: Create index for faster lookups
CREATE INDEX IF NOT EXISTS idx_users_username ON users(username);

-- Migration complete
--
-- IMPORTANT: After running this migration, all existing users will need to reset
-- their passwords since SHA256 hashes are not compatible with Argon2id verification.
-- Consider implementing a password migration flow that:
-- 1. Detects old hash format (no $ prefix)
-- 2. Verifies with legacy SHA256
-- 3. Re-hashes with Argon2id on successful login
