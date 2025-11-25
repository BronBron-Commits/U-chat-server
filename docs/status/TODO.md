# Todo / Development Tasks

> **Enterprise Roadmap**: See [ENTERPRISE_ROADMAP.md](./ENTERPRISE_ROADMAP.md) for the complete enterprise readiness plan.

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

## Recently Completed (Phase 14) ✅

### Integration Completion (2025-11-25)
- [x] **Thread handlers** - Following channel pattern (`chat-service/src/handlers/threads.rs`)
- [x] **File upload/download with E2EE** - MinIO + local storage (`chat-service/src/handlers/files.rs`)
- [x] **PostgreSQL audit logger integration** - Feature flag enabled (`core/Cargo.toml`, `core/src/audit.rs`)
- [x] **MQTT bridge main integration** - Background tasks and device management (`gateway-service/src/main.rs`)
- [x] **Comprehensive integration tests** - 7 test suites (`tests/integration_tests.rs`)
- [x] **Documentation updates** - PROGRESS.md, TODO.md, INTEGRATION_COMPLETION.md

---

## Current Sprint

### High Priority - Immediate Next Steps

- [ ] **INTEGRATE-01**: Wire up JWT claims in handlers
  - Extract `user_id` from JWT tokens
  - Replace hardcoded `"system"` values
  - Files: `chat-service/src/handlers/{channels,threads,files}.rs`
  - Estimate: 30 minutes

- [ ] **INTEGRATE-02**: Mount API routes in chat-service
  - Create router module
  - Mount thread handlers (`/api/threads/*`)
  - Mount file handlers (`/api/files/*`)
  - Update channel routes (`/api/channels/*`)
  - File: `chat-service/src/main.rs` or new `routes.rs`
  - Estimate: 1 hour

- [ ] **INTEGRATE-03**: Test with PostgreSQL
  - Run migrations on PostgreSQL instance
  - Test audit logger with `postgres` feature
  - Verify all handlers with real database
  - Estimate: 1 hour

- [ ] **INTEGRATE-04**: Deploy MinIO for file testing
  - Add MinIO to docker-compose.yml
  - Configure S3 backend
  - Test file upload/download with encryption
  - Estimate: 1 hour

### Testing & Quality

- [x] **TEST-01**: ~~Run full integration test suite~~ ✅ Complete
  - ✅ E2EE message flow
  - ✅ Channel management
  - ✅ Thread creation
  - ✅ Audit logging
  - ✅ MQTT bridge
  - ✅ Rate limiting
  - ✅ Password hashing
  - File: `tests/integration_tests.rs`

- [ ] **TEST-02**: Load testing with k6 or artillery
  - WebSocket connection capacity
  - Rate limiting validation
  - Memory usage under load
  - File upload throughput

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

### MQTT & IoT

- [ ] **MQTT-01**: Complete rumqttc client integration
  - Wire up `mqtt_bridge_impl.rs` to main event loop
  - Test with actual MQTT broker (Mosquitto)
  - ESP32 firmware integration test
  - Estimate: 2 hours

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

## Enterprise Backlog (NEW)

> Priority items for enterprise readiness. See [ENTERPRISE_ROADMAP.md](./ENTERPRISE_ROADMAP.md) for detailed specs.

### Phase 7: End-to-End Encryption (Critical - 3-4 weeks)

- [ ] **E2EE-01**: X25519 key pair generation for each device
  - Crate: `x25519-dalek` v2.0
  - Generate identity keys on device registration
  - Store private keys in secure enclave/keychain

- [ ] **E2EE-02**: X3DH key exchange protocol
  - PreKey bundles stored on server (encrypted)
  - One-time prekeys for forward secrecy

- [ ] **E2EE-03**: Double Ratchet implementation
  - Crate: `chacha20poly1305` for AEAD
  - Symmetric ratchet with HKDF
  - DH ratchet on each message exchange

- [ ] **E2EE-04**: Message encryption with ChaCha20-Poly1305
  - Associated data: sender_id, timestamp, message_id
  - Server sees only encrypted blobs

- [ ] **E2EE-05**: MLS group encryption stub
  - Crate: `openmls` v0.5 (future-proof)
  - Efficient for large groups

### Phase 8: Enterprise Identity & Access (High - 2-3 weeks)

- [ ] **IAM-01**: Upgrade JWT to RS256 asymmetric signing
  - RSA-2048 or Ed25519 keys
  - JWKS endpoint for key rotation

- [ ] **IAM-02**: OpenID Connect provider integration
  - Crate: `openidconnect` v3.4
  - Authorization code + PKCE
  - Okta/Azure AD/Keycloak adapters

- [ ] **IAM-03**: SAML 2.0 SP implementation
  - Crate: `samael` v0.0.14
  - SP metadata and assertion validation

- [ ] **IAM-04**: SCIM 2.0 user provisioning
  - JIT provisioning, auto-deprovisioning

- [ ] **IAM-05**: Role-Based Access Control (RBAC)
  - Roles: Super Admin > Org Admin > Space Admin > Member > Guest
  - Permission matrix for all operations

- [ ] **IAM-06**: Attribute-Based Access Control (ABAC)
  - OPA/Rego policy engine integration
  - Context-aware permissions

- [ ] **IAM-07**: WebAuthn/Passkey authentication
  - Crate: `webauthn-rs` v0.4
  - Passwordless login

### Phase 9: Compliance & Audit (High - 2-3 weeks)

- [ ] **AUDIT-01**: Immutable audit log table
  - Append-only Postgres with hash chaining
  - Cryptographic integrity verification

- [ ] **AUDIT-02**: Security event logging
  - Auth attempts, permission changes, device events
  - SIEM export capability

- [ ] **AUDIT-03**: OpenSearch/ELK integration
  - Structured JSON log streaming
  - Retention policy enforcement

- [ ] **AUDIT-04**: WORM storage integration
  - AWS S3 Object Lock / Azure Immutable Blob
  - Compliance hold support

- [ ] **AUDIT-05**: GDPR data subject rights
  - Data export and deletion APIs
  - Consent management

### Phase 10: Scalability & High Availability (Medium - 3-4 weeks)

- [ ] **SCALE-01**: NATS JetStream for messaging
  - Crate: `async-nats` v0.33
  - Cross-region message routing

- [ ] **SCALE-02**: Redis Cluster for presence/caching
  - Crate: `redis` v0.24 with cluster
  - Distributed rate limiting

- [ ] **SCALE-03**: CockroachDB/Citus migration
  - Horizontal sharding
  - Multi-region replication

- [ ] **SCALE-04**: Kubernetes Helm chart
  - Deployment, HPA, PDB, NetworkPolicy
  - ArtifactHub publication

- [ ] **SCALE-05**: Health checks & graceful shutdown
  - Liveness/readiness probes
  - Connection draining

- [ ] **SCALE-06**: Service mesh (Linkerd/Istio)
  - mTLS between services
  - Traffic management

### Phase 11: Enterprise Chat Features (Partially Complete)

- [x] **CHAT-01**: ~~Channel and Space management~~ ✅ Complete
  - ✅ Public/private/direct channels
  - ✅ Channel permissions (RBAC)
  - ✅ Member management
  - File: `chat-service/src/handlers/channels.rs`

- [x] **CHAT-02**: ~~Threaded conversations~~ ✅ Complete
  - ✅ Reply-to-message with parent_id
  - ✅ Thread participants
  - ✅ Reply count tracking
  - File: `chat-service/src/handlers/threads.rs`

- [ ] **CHAT-03**: Message reactions and editing
  - Emoji reactions, edit window
  - Edit history for compliance
  - Schema exists in `004_channels_threads.sql`

- [ ] **CHAT-04**: Read receipts and typing indicators
  - Delivery/read status
  - Privacy-respecting settings
  - Schema exists in `004_channels_threads.sql`

- [x] **CHAT-05**: ~~E2EE file sharing~~ ✅ Complete
  - ✅ Client-side encryption
  - ✅ MinIO/S3 backend
  - ✅ Local filesystem backend
  - ✅ SHA-256 checksums
  - File: `chat-service/src/handlers/files.rs`

- [ ] **CHAT-06**: WebRTC voice/video signaling
  - Separate signaling microservice
  - LiveKit/SFU compatible

- [ ] **CHAT-07**: Bot and app platform
  - Webhook integrations
  - OAuth2 app framework

### Phase 12: Secure Automation Mode (Medium - 4-6 weeks)

- [ ] **AUTO-01**: Device-bound X.509 certificates
  - EST/CMPv2 enrollment
  - Crate: `rcgen`, `x509-parser`

- [ ] **AUTO-02**: MQTT over WebSocket bridge
  - Topic-based routing
  - QoS level support

- [ ] **AUTO-03**: OPA policy engine
  - Rego policy language
  - Per-topic permissions

- [ ] **AUTO-04**: Offline message queue
  - Durable storage for offline devices
  - Message expiration policies

- [ ] **AUTO-05**: Voice command integration
  - Whisper.cpp STT
  - Intent → action dispatcher

### Phase 13: Go-to-Market (Low - 2-4 weeks)

- [ ] **GTM-01**: One-click enterprise installer
  - Terraform/Ansible/Helm
  - Air-gapped OVA appliance

- [ ] **GTM-02**: FIPS mode profile
  - FIPS-validated crypto (ring/aws-lc)
  - Zero-telemetry build

- [ ] **GTM-03**: Commercial support tier
  - SLA documentation
  - LTS branches

- [ ] **GTM-04**: Documentation portal
  - OpenAPI reference
  - Security whitepaper

---

## Task Priority Matrix

| Priority | Category | Task ID | Description |
|----------|----------|---------|-------------|
| 🔴 Critical | E2EE | E2EE-01 | X25519 key generation |
| 🔴 Critical | E2EE | E2EE-03 | Double Ratchet |
| 🔴 Critical | IAM | IAM-01 | RS256 JWT upgrade |
| 🔴 Critical | IAM | IAM-02 | OIDC integration |
| 🔴 Critical | Testing | TEST-01 | Integration test suite |
| 🔴 Critical | Infra | INFRA-01 | TLS termination |
| 🟠 High | E2EE | E2EE-04 | Message encryption |
| 🟠 High | IAM | IAM-05 | RBAC engine |
| 🟠 High | Audit | AUDIT-01 | Immutable audit log |
| 🟠 High | Security | SEC-01 | Account lockout |
| 🟠 High | Scale | SCALE-01 | NATS JetStream |
| 🟠 High | IoT | IOT-01 | OTA updates |
| 🟡 Medium | IAM | IAM-07 | WebAuthn/Passkeys |
| 🟡 Medium | Chat | CHAT-01 | Channels/Spaces |
| 🟡 Medium | Chat | CHAT-05 | E2EE file sharing |
| 🟡 Medium | Security | SEC-04 | Two-factor auth |
| 🟡 Medium | Observability | OBS-01 | Distributed tracing |
| 🟢 Low | Chat | CHAT-06 | Voice/Video |
| 🟢 Low | Auto | AUTO-02 | MQTT bridge |
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
