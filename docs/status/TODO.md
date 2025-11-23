# Todo / Development Tasks

## Recently Completed

### Phase 6: Codebase Enhancement & CI/CD ✅
- [x] Enhanced core crate with shared models (User, Message, Group, Device, Presence)
- [x] Common error handling with `UnhidraError` type
- [x] Repository traits and service configuration utilities
- [x] GitHub Actions CI/CD pipeline (lint, test, security, coverage)
- [x] Release workflow with Docker image publishing
- [x] Workspace-wide linting (.rustfmt.toml, clippy.toml)
- [x] History service with SQLite database integration
- [x] Full-text search for message history
- [x] Chat service with group chat functionality
- [x] Group membership management (join, leave, roles)
- [x] Real-time message broadcasting

### Phase 5: Rate Limiting & Device Registration ✅
- [x] Gateway service rate limiting (per-IP, per-user, per-connection)
- [x] Auth API rate limiting (login, registration, device registration)
- [x] Device registration API (register, list, revoke)
- [x] Connection tracking with metadata
- [x] Prometheus metrics integration
- [x] Docker Compose setup with Prometheus/Grafana

### Phase 3: WSS Gateway Security ✅
- [x] Upgrade gateway to Sec-WebSocket-Protocol authentication
- [x] Connection tracking with DashMap
- [x] Graceful connection termination
- [x] Rate limiting for WebSocket connections
- [x] Origin validation (CSRF protection)

---

## Current Sprint

### Testing & Quality

- [ ] **TEST-01**: Run full integration test suite
  - Test gateway WebSocket connections
  - Test auth API endpoints
  - Test device registration flow
  - File: `gateway-service/tests/integration_tests.rs`

- [ ] **TEST-02**: Load testing with k6 or artillery
  - WebSocket connection capacity
  - Rate limiting validation
  - Memory usage under load

### Infrastructure

- [ ] **INFRA-01**: TLS termination setup
  - Configure nginx/traefik for WSS
  - SSL certificate provisioning
  - Document HTTPS setup

- [ ] **INFRA-02**: Kubernetes manifests
  - Deployment configurations
  - Service definitions
  - ConfigMaps and Secrets
  - Horizontal Pod Autoscaler

---

## Backlog

### Security Enhancements

- [ ] **SEC-01**: Account lockout after failed attempts
  - Track failed login attempts per account
  - Temporary lockout after N failures
  - Email notification on lockout

- [ ] **SEC-02**: Audit logging for security events
  - Log all authentication attempts
  - Log device registrations/revocations
  - Log rate limit violations
  - Export to SIEM

- [ ] **SEC-03**: Password change endpoint
  - Require current password
  - Invalidate existing tokens on change
  - Rate limit password changes

- [ ] **SEC-04**: Two-factor authentication (2FA)
  - TOTP support (Google Authenticator)
  - Recovery codes
  - Remember device option

### ESP32 Firmware Enhancements

- [ ] **IOT-01**: OTA firmware updates
  - Secure firmware signing
  - Staged rollout support
  - Rollback on boot failure
  - Version management

- [ ] **IOT-02**: Device provisioning flow
  - SmartConfig/BLE provisioning
  - QR code based setup
  - Factory reset functionality

- [ ] **IOT-03**: Sensor data collection
  - Generic sensor interface
  - Data buffering for offline mode
  - Batch upload optimization

- [ ] **IOT-04**: Secure boot and flash encryption
  - Enable ESP32 secure boot
  - Flash encryption for production
  - Key management documentation

### ML Infrastructure

- [ ] **ML-01**: Health-check endpoint for Python workers
  - Add `/internal/ml/health` route
  - Auto-restart unresponsive workers
  - Worker pool management

- [ ] **ML-02**: Model versioning and hot-reload
  - Support multiple model versions
  - A/B testing support
  - Zero-downtime model updates

- [ ] **ML-03**: Binary protocol for large payloads
  - MessagePack or Protocol Buffers
  - Streaming support
  - Compression

### Observability

- [ ] **OBS-01**: Distributed tracing
  - OpenTelemetry integration
  - Trace propagation across services
  - Jaeger/Zipkin support

- [ ] **OBS-02**: Alerting rules
  - High error rate alerts
  - Latency threshold alerts
  - Connection spike detection

- [ ] **OBS-03**: Custom Grafana dashboards
  - Real-time connection metrics
  - Rate limiting visualization
  - Device health overview

### API Enhancements

- [ ] **API-01**: User registration endpoint
  - Email verification flow
  - CAPTCHA integration
  - Profile management

- [ ] **API-02**: Room management API
  - Create/delete rooms
  - Room permissions
  - Member management

- [ ] **API-03**: Message history API
  - Paginated history retrieval
  - Search functionality
  - Message retention policies

---

## Task Priority Matrix

| Priority | Category | Task ID | Description |
|----------|----------|---------|-------------|
| 🔴 Critical | Testing | TEST-01 | Integration test suite |
| 🔴 Critical | Infra | INFRA-01 | TLS termination |
| 🟠 High | Security | SEC-01 | Account lockout |
| 🟠 High | IoT | IOT-01 | OTA updates |
| 🟡 Medium | Security | SEC-04 | Two-factor auth |
| 🟡 Medium | Observability | OBS-01 | Distributed tracing |
| 🟢 Low | ML | ML-03 | Binary protocol |
| 🟢 Low | API | API-02 | Room management |

---

## Environment Variables Reference

### Auth API
```bash
AUTH_BIND_ADDR=0.0.0.0:9200
AUTH_DB_PATH=/opt/unhidra/auth.db
JWT_SECRET=your-secret-key
RATE_LIMIT_LOGIN_PER_MINUTE=10
RATE_LIMIT_REGISTER_PER_HOUR=5
```

### Gateway Service
```bash
GATEWAY_PORT=9000
JWT_SECRET=your-secret-key
ALLOWED_ORIGINS=https://app.unhidra.io
RATE_LIMIT_IP_PER_MINUTE=60
RATE_LIMIT_USER_PER_MINUTE=30
RATE_LIMIT_MESSAGES_PER_SEC=50
```

---

## Quick Reference Commands

```bash
# Run all tests
cargo test --workspace

# Build release binaries
cargo build --release

# Start services with Docker
docker-compose up -d

# View logs
docker-compose logs -f gateway-service

# Run integration tests
cargo test --test integration_tests -- --ignored
```
