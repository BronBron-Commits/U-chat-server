# Unhidra User Manual

## Overview

Unhidra is an enterprise-grade secure chat platform built with Rust, featuring end-to-end encryption (E2EE), single sign-on (SSO), and IoT device support.

## Setup

### Prerequisites

- Docker and Docker Compose
- PostgreSQL 15+
- Redis 7+
- Rust 1.75+ (for development)

### Quick Start with Docker

1. **Start all services**:
   ```bash
   docker-compose up -d
   ```

   This starts:
   - PostgreSQL database
   - Redis for caching and event streaming
   - MinIO for file storage
   - Eclipse Mosquitto MQTT broker
   - All Unhidra microservices

2. **Run database migrations**:
   ```bash
   psql -U unhidra -d unhidra -f migrations/001_argon2id_password_hash.sql
   psql -U unhidra -d unhidra -f migrations/002_devices_table.sql
   psql -U unhidra -d unhidra -f migrations/003_audit_log.sql
   psql -U unhidra -d unhidra -f migrations/004_channels_threads.sql
   psql -U unhidra -d unhidra -f migrations/005_postgres_audit_log.sql
   ```

   Or use SQLx CLI:
   ```bash
   sqlx migrate run
   ```

3. **Configure environment**:
   ```bash
   cp .env.example .env
   # Edit .env with your configuration
   ```

### Development Setup

1. **Clone the repository**:
   ```bash
   git clone https://github.com/Matthewtgordon/Unhidra.git
   cd Unhidra
   ```

2. **Build all services**:
   ```bash
   cargo build --release
   ```

3. **Run services manually** (optional, for development):
   ```bash
   ./run-all.sh
   ```

## Usage

### Authentication

#### Traditional Login

1. **Register a new account**:
   ```bash
   curl -X POST http://localhost:9200/api/register \
     -H "Content-Type: application/json" \
     -d '{"username": "alice", "password": "secure-password-123"}'
   ```

2. **Login to get JWT token**:
   ```bash
   curl -X POST http://localhost:9200/api/login \
     -H "Content-Type: application/json" \
     -d '{"username": "alice", "password": "secure-password-123"}'
   ```

   Response:
   ```json
   {
     "token": "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9...",
     "expires_in": 3600
   }
   ```

#### SSO Login (OIDC)

Navigate to the SSO endpoint:
```bash
curl http://localhost:9200/auth/sso?provider=okta
```

Supported providers:
- Okta
- Azure AD
- Google Workspace
- Keycloak
- Custom OIDC providers

#### WebAuthn/Passkeys

1. **Register a passkey**:
   ```bash
   curl -X POST http://localhost:9200/api/webauthn/register/start \
     -H "Authorization: Bearer <your-jwt-token>"
   ```

2. **Authenticate with passkey**:
   ```bash
   curl -X POST http://localhost:9200/api/webauthn/authenticate/start \
     -H "Content-Type: application/json" \
     -d '{"username": "alice"}'
   ```

### Chat Operations

#### Create a Channel

```bash
curl -X POST http://localhost:3002/api/channels \
  -H "Authorization: Bearer <your-jwt-token>" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "general",
    "description": "General discussion",
    "channel_type": "public"
  }'
```

Channel types:
- `public`: Open to all users
- `private`: Invite-only
- `direct`: One-on-one messaging

#### List Channels

```bash
curl -X GET http://localhost:3002/api/channels \
  -H "Authorization: Bearer <your-jwt-token>"
```

#### Send a Message with Thread

1. **Create a thread**:
   ```bash
   curl -X POST http://localhost:3002/api/threads \
     -H "Authorization: Bearer <your-jwt-token>" \
     -H "Content-Type: application/json" \
     -d '{
       "channel_id": "channel-uuid",
       "title": "Project Discussion"
     }'
   ```

2. **Send a message to the thread**:
   ```bash
   curl -X POST http://localhost:3002/api/threads/<thread-id>/messages \
     -H "Authorization: Bearer <your-jwt-token>" \
     -H "Content-Type: application/json" \
     -d '{
       "content": "Hello everyone!",
       "encrypted": false
     }'
   ```

### File Operations

#### Upload a File (with E2EE)

```bash
curl -X POST http://localhost:3002/api/files \
  -H "Authorization: Bearer <your-jwt-token>" \
  -F "file=@document.pdf" \
  -F "channel_id=channel-uuid" \
  -F "encrypt=true"
```

Files are:
- Stored in MinIO object storage
- Encrypted end-to-end using ChaCha20Poly1305
- Accessible only to channel members

#### Download a File

```bash
curl -X GET http://localhost:3002/api/files/<file-id> \
  -H "Authorization: Bearer <your-jwt-token>" \
  -o downloaded-file.pdf
```

The client automatically decrypts E2EE files using the Double Ratchet session keys.

### IoT Devices (MQTT)

#### Connect an ESP32 Device

1. **Configure device credentials** in `firmware/.env`:
   ```env
   WIFI_SSID=your-wifi-name
   WIFI_PASSWORD=your-wifi-password
   WS_SERVER_URL=wss://your-server.com:9000
   DEVICE_TOKEN=your-device-jwt-token
   ```

2. **Flash the firmware**:
   ```bash
   cd firmware
   cargo run --release
   ```

3. **Device publishes to MQTT**:
   - Topic: `unhidra/devices/<device-id>/sensor`
   - Payload: JSON sensor data

4. **Messages are bridged to chat** via the MQTT bridge service.

#### Publish a Test Message

```bash
mosquitto_pub -h localhost -p 1883 \
  -t "unhidra/devices/esp32-001/sensor" \
  -m '{"temperature": 23.5, "humidity": 45.2}'
```

## Admin Operations

### Audit Logs

Query the immutable audit log:

```sql
SELECT
  actor_id,
  action,
  resource_type,
  resource_id,
  occurred_at,
  ip_address,
  user_agent
FROM audit_log
WHERE action = 'login'
  AND occurred_at > NOW() - INTERVAL '24 hours'
ORDER BY occurred_at DESC;
```

Common audit actions:
- `login`, `logout`, `login_failed`
- `channel_create`, `channel_delete`
- `message_send`, `message_edit`, `message_delete`
- `file_upload`, `file_download`
- `member_add`, `member_remove`

### Scaling with Kubernetes

1. **Install Helm chart**:
   ```bash
   cd helm/unhidra
   helm dependency update

   helm install unhidra . \
     --set postgresql.enabled=true \
     --set redis.enabled=true \
     --set replicaCount=3
   ```

2. **Use external managed services**:
   ```bash
   helm install unhidra . \
     --set postgresql.enabled=false \
     --set redis.enabled=false \
     --set externalDatabase.host=rds.amazonaws.com \
     --set externalDatabase.port=5432 \
     --set externalDatabase.database=unhidra \
     --set externalRedis.host=redis.cache.amazonaws.com
   ```

3. **Scale services**:
   ```bash
   kubectl scale deployment/chat-service --replicas=5
   kubectl scale deployment/gateway-service --replicas=10
   ```

### Monitoring

Access monitoring dashboards:

- **Prometheus**: http://localhost:9090
- **Grafana**: http://localhost:3001 (admin/admin)

Key metrics to monitor:
- `unhidra_messages_sent_total`
- `unhidra_connections_active`
- `unhidra_auth_attempts_total`
- `unhidra_e2ee_sessions_active`

## Security Best Practices

### Passwords

- Minimum 12 characters
- Mix of uppercase, lowercase, numbers, symbols
- Argon2id hashing (secure against GPU attacks)
- Rate limited: 10 attempts/minute

### JWT Tokens

- 1-hour expiration by default
- Store securely (HttpOnly cookies recommended)
- Never commit `JWT_SECRET` to version control
- Rotate secrets regularly

### E2EE Sessions

- Double Ratchet protocol (Signal-style)
- X3DH key agreement for initial session
- Forward secrecy: past messages remain secure if keys compromised
- Break-in recovery: future messages secure after key compromise

### TLS/WSS

- All WebSocket connections use TLS (wss://)
- ESP32 devices verify server certificates
- Minimum TLS 1.2, prefer TLS 1.3

## Troubleshooting

### Database Connection Failed

```bash
# Check PostgreSQL is running
docker-compose ps postgres

# Verify connection
psql -U unhidra -d unhidra -h localhost -c "SELECT 1;"
```

### Redis Connection Failed

```bash
# Check Redis is running
docker-compose ps redis

# Test connection
redis-cli -h localhost ping
```

### MinIO Access Denied

```bash
# Check MinIO is running
docker-compose ps minio

# Verify credentials in .env match docker-compose.yml
# Default: minioadmin / minioadmin
```

### MQTT Connection Failed

```bash
# Check Mosquitto is running
docker-compose ps mosquitto

# Test publish
mosquitto_pub -h localhost -p 1883 -t test -m "hello"

# Test subscribe
mosquitto_sub -h localhost -p 1883 -t test
```

### JWT Token Invalid

- Ensure `JWT_SECRET` is the same across all services
- Check token hasn't expired (default: 1 hour)
- Verify token format: `Authorization: Bearer <token>`

### E2EE Session Failed

- Both clients must have completed X3DH key exchange
- Check prekey bundles are published
- Verify ratchet state is synchronized

## API Reference

Full API documentation available at:
- **OpenAPI Spec**: `/api/openapi.json`
- **Rust Docs**: `cargo doc --open`
- **Online Docs**: https://docs.unhidra.io

## Support

- **Issues**: https://github.com/Matthewtgordon/Unhidra/issues
- **Discussions**: https://github.com/Matthewtgordon/Unhidra/discussions
- **Security**: security@unhidra.io

## License

MIT License - see [LICENSE](../LICENSE) file for details.
