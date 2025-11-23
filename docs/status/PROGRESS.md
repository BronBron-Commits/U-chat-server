# Development Progress

## Project Evolution Timeline

| Date | Phase | Description | PR/Commit |
|------|-------|-------------|-----------|
| 2024-11 | Initial | Initial commit with base services | `fef3210` |
| 2024-11 | Phase 1 | Argon2id password hashing | `e358a05` |
| 2024-11 | Phase 2 | ML IPC sidecar isolation | `9d8d0df` |
| 2024-11 | Phase 3 | WSS Gateway Security | ✅ Complete |
| 2024-11 | Phase 4 | ESP32 Firmware & WSS Integration | ✅ Complete |
| 2024-11 | Phase 5 | Rate Limiting & Device Registration | ✅ Complete |

---

## Phase 5: Rate Limiting & Device Registration

**Status**: ✅ Completed

### Overview

Implemented comprehensive rate limiting across all services and added device registration functionality to support IoT device management.

### Completed Tasks

- [x] **Gateway Service Rate Limiting**
  - Per-IP connection rate limiting (60/min default)
  - Per-user connection rate limiting (30/min default)
  - Per-connection message rate limiting (50/sec default)
  - Configurable via environment variables
  - File: `gateway-service/src/rate_limiter.rs`

- [x] **Auth API Rate Limiting**
  - Per-IP login attempt limiting (10/min default)
  - Per-IP registration limiting (5/hour default)
  - Per-IP device registration limiting (10/hour default)
  - File: `auth-api/src/rate_limiter.rs`

- [x] **Device Registration API**
  - POST `/devices/register` - Register new IoT device
  - POST `/devices/list` - List user's devices
  - POST `/devices/revoke` - Revoke device access
  - API key generation with Argon2id hashing
  - Device metadata storage in SQLite

- [x] **Connection Tracking**
  - Real-time connection metadata tracking
  - Messages sent/received counters
  - Connection duration tracking
  - IP address and user agent logging
  - File: `gateway-service/src/connection.rs`

- [x] **Prometheus Metrics**
  - `gateway_connections_total` - Total connections
  - `gateway_connections_active` - Active connections
  - `gateway_messages_total` - Messages processed
  - `gateway_message_latency_seconds` - Latency histogram
  - `gateway_rate_limit_hits_total` - Rate limit violations
  - File: `gateway-service/src/metrics.rs`

- [x] **Docker Compose Setup**
  - Multi-service orchestration
  - Prometheus metrics collection
  - Grafana dashboards
  - Volume persistence
  - Health checks

### Security Improvements (Phase 5)

| Improvement | Description |
|-------------|-------------|
| Rate limiting | Prevents brute force and DoS attacks |
| Device API keys | Secure device authentication |
| Connection tracking | Audit trail for connections |
| Metrics observability | Real-time security monitoring |

---

## Phase 4: ESP32 Firmware & WSS Integration (IoT Edge Hardening)

**Status**: ✅ Completed

### Overview

Implemented secure ESP32 firmware using the modern `esp-idf-svc` ecosystem for IoT edge devices. The firmware establishes encrypted WebSocket connections to the Unhidra backend with device authentication and automatic reconnection.

### Completed Tasks

- [x] **Created firmware directory structure**
- [x] **Implemented Wi-Fi management using EspWifi**
- [x] **Implemented secure WebSocket client**
- [x] **TLS certificate verification**
- [x] **Device authentication via Sec-WebSocket-Protocol**
- [x] **Automatic reconnection with exponential backoff**
- [x] **Application heartbeat mechanism**
- [x] **Keep-alive ping/pong**

### Security Improvements (Phase 4)

| Improvement | Description |
|-------------|-------------|
| End-to-end encryption | All device-cloud traffic over TLS |
| Certificate pinning ready | CA bundle with custom cert support |
| Authentication isolation | API keys in protocol header, not URL |
| Reconnect resilience | Automatic recovery with backoff |
| Memory safety | Rust ownership model, no raw pointers |
| Secure config | Credentials in .env (gitignored) |

---

## Phase 3: WSS Gateway Security

**Status**: ✅ Completed

### Completed Tasks

- [x] **Upgraded gateway to Axum framework**
- [x] **Sec-WebSocket-Protocol authentication**
  - Extract token from subprotocol header
  - Validate JWT during handshake
  - Return validated subprotocol in response

- [x] **Connection tracking with DashMap**
  - Store connected client info (user_id, device_id, connect_time)
  - Enable targeted message delivery
  - Support for presence tracking

- [x] **Graceful connection termination**
  - Send close frame with reason code
  - Clean up connection state
  - Log disconnection events

- [x] **Rate limiting for WebSocket connections**
  - Limit connections per IP
  - Limit connections per user/device
  - Prevent resource exhaustion

- [x] **Origin validation (CSRF protection)**
  - Configurable allowed origins
  - Reject unauthorized origins

---

## Phase 2: Architectural Decoupling (ML IPC Sidecar Isolation)

**Status**: ✅ Completed

### Completed Tasks

- [x] Created `ml-bridge` crate with PythonWorker implementation
- [x] Implemented length-prefixed JSON protocol
- [x] Created Python inference worker daemon
- [x] Added comprehensive error handling
- [x] Integrated into workspace

### Architecture Benefits

- **Event Loop Protection**: Python ML runs in separate process
- **GIL Bypass**: Separate process means no Python GIL contention
- **Fault Isolation**: Python crash doesn't bring down Rust server
- **Independent Scaling**: Can spawn multiple Python workers if needed
- **Security**: UDS is local-only, socket permissions set to 0600

---

## Phase 1: Cryptographic Hardening (Argon2id Password Hashing)

**Status**: ✅ Completed

### Completed Tasks

- [x] Created `PasswordService` with Argon2id implementation
- [x] Added argon2 crate dependency
- [x] Updated handlers to use Argon2id verification
- [x] Created database migration
- [x] Comprehensive test suite

### Security Improvements

- Memory-hard password hashing (resists GPU/ASIC attacks)
- 128-bit random salt per password (CSPRNG)
- Constant-time verification (timing attack protection)
- PHC-formatted strings (self-documenting hash format)

---

## Summary Statistics

| Metric | Value |
|--------|-------|
| Phases Completed | 5 |
| New Crates Added | 3 (ml-bridge, jwt-common enhanced, firmware) |
| Security Improvements | 20+ |
| Test Coverage | Unit tests for auth, ML bridge, gateway |
| Supported Platforms | Linux (backend), ESP32 family (firmware) |
| Docker Support | Full compose with Prometheus/Grafana |
