# Deployment Guide

## Prerequisites

- Rust 1.70+ (for async features)
- SQLite 3.x
- Network access for JWT secret management

## Database Migration

### Apply Argon2id Migration

```bash
# From project root
sqlite3 /opt/unhidra/auth.db < migrations/001_argon2id_password_hash.sql
```

**Important**: After migration, existing users with SHA256 hashes will need password resets.

### Verify Migration

```sql
-- Check table schema
.schema users

-- Expected output:
-- CREATE TABLE users (
--     username TEXT PRIMARY KEY NOT NULL,
--     password_hash TEXT NOT NULL,
--     verified INTEGER NOT NULL DEFAULT 0,
--     display_name TEXT NOT NULL DEFAULT ''
-- );
```

## Environment Variables

| Variable    | Description                    | Default      |
|-------------|--------------------------------|--------------|
| JWT_SECRET  | JWT signing secret             | supersecret  |

**Production**: Always set a strong, random JWT_SECRET

```bash
export JWT_SECRET=$(openssl rand -base64 32)
```

## Building for Production

```bash
# Release build with optimizations
cargo build --release -p auth-api

# Binary location
./target/release/auth-api
```

## Running the Service

```bash
# Start auth-api
./target/release/auth-api

# Verify running
curl -X POST http://localhost:9200/login \
  -H "Content-Type: application/json" \
  -d '{"username":"test","password":"test"}'
```

## Security Checklist

- [ ] Set strong JWT_SECRET in production
- [ ] Run database migration before first deployment
- [ ] Verify TLS/HTTPS in front of auth-api
- [ ] Configure rate limiting (recommended)
- [ ] Set up monitoring and alerting
- [ ] Review log output for sensitive data leakage
