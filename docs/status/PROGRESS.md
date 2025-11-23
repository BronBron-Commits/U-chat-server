# Development Progress

## Project Evolution Timeline

| Date | Phase | Description | PR/Commit |
|------|-------|-------------|-----------|
| 2024-11 | Initial | Initial commit with base services | `fef3210` |
| 2024-11 | Phase 1 | Argon2id password hashing | `e358a05` |
| 2024-11 | Phase 2 | ML IPC sidecar isolation | `9d8d0df` |
| 2024-11 | Phase 3 | WSS Gateway Security | In Progress |
| 2024-11 | Phase 4 | ESP32 Firmware & WSS Integration | Current |

---

## Phase 4: ESP32 Firmware & WSS Integration (IoT Edge Hardening)

**Status**: ✅ Completed

### Overview

Implemented secure ESP32 firmware using the modern `esp-idf-svc` ecosystem for IoT edge devices. The firmware establishes encrypted WebSocket connections to the Unhidra backend with device authentication and automatic reconnection.

### Completed Tasks

- [x] **Created firmware directory structure**
  - `firmware/src/main.rs` - Main application entry point
  - `firmware/Cargo.toml` - Dependencies and build configuration
  - `firmware/.cargo/config.toml` - Target architecture settings
  - `firmware/sdkconfig.defaults` - ESP-IDF SDK configuration
  - `firmware/build.rs` - Build script for environment variables
  - `firmware/.env.example` - Configuration template

- [x] **Implemented Wi-Fi management using EspWifi**
  - Uses `BlockingWifi` for synchronous connection handling
  - Automatic DHCP configuration
  - Credentials loaded from environment variables
  - Supports all ESP32 variants (ESP32, S2, S3, C3, C6)

- [x] **Implemented secure WebSocket client**
  - Uses `EspWebSocketClient` from esp-idf-svc
  - WSS (WebSocket Secure) over TLS
  - Full event-driven architecture
  - Binary and text message support

- [x] **TLS certificate verification**
  - Integrated `esp_crt_bundle_attach` for CA bundle
  - Server identity verification during TLS handshake
  - Prevents man-in-the-middle attacks
  - Support for custom CA certificates (documented)

- [x] **Device authentication via Sec-WebSocket-Protocol**
  - Device API key transmitted as WebSocket subprotocol
  - Server validates during WebSocket handshake
  - Credentials never exposed in URLs or query params
  - Unique per-device authentication tokens

- [x] **Automatic reconnection with exponential backoff**
  - Initial backoff: 5 seconds
  - Maximum backoff: 60 seconds
  - Exponential multiplier: 2.0x
  - Jitter: ±30% (prevents thundering herd)
  - Automatic recovery from Wi-Fi/server outages

- [x] **Application heartbeat mechanism**
  - 60-second heartbeat interval
  - JSON payload with device_id, timestamp, free_heap
  - Server-side device health monitoring support
  - Dead connection detection

- [x] **Keep-alive ping/pong**
  - 30-second ping interval (configurable)
  - Maintains NAT mappings
  - Prompt dead connection detection

### Dependencies Added

```toml
esp-idf-svc = "0.49"      # High-level ESP-IDF abstractions
esp-idf-sys = "0.35"      # ESP-IDF system bindings
esp-idf-hal = "0.44"      # Hardware abstraction layer
embedded-svc = "0.28"     # Embedded services traits
log = "0.4"               # Logging facade
anyhow = "1.0"            # Error handling
serde = "1.0"             # Serialization
serde_json = "1.0"        # JSON support
```

### Security Improvements (Phase 4)

| Improvement | Description |
|-------------|-------------|
| End-to-end encryption | All device-cloud traffic over TLS |
| Certificate pinning ready | CA bundle with custom cert support |
| Authentication isolation | API keys in protocol header, not URL |
| Reconnect resilience | Automatic recovery with backoff |
| Memory safety | Rust ownership model, no raw pointers |
| Secure config | Credentials in .env (gitignored) |

### Architecture Benefits

```
┌─────────────────┐     WSS/TLS      ┌──────────────────┐
│   ESP32 Device  │◄────────────────►│  Gateway Service │
│                 │   Authenticated   │                  │
│ - Wi-Fi (STA)   │   Encrypted      │ - JWT Validation │
│ - WebSocket     │                  │ - Message Routing│
│ - TLS (mbedTLS) │                  │ - Connection Mgmt│
│ - Heartbeat     │                  │                  │
└─────────────────┘                  └──────────────────┘
        │                                     │
        │                                     │
        └──────────── Same Auth Flow ─────────┘
              (Sec-WebSocket-Protocol)
```

---

## Phase 3: WSS Gateway Security (In Progress)

**Status**: 🔄 In Progress (background)

### Planned Tasks

- [ ] Upgrade gateway to Sec-WebSocket-Protocol authentication
- [ ] Remove token from query parameters
- [ ] Add connection tracking with DashMap
- [ ] Implement graceful connection termination
- [ ] Add rate limiting for WebSocket connections

### Current State

The gateway-service currently validates JWT tokens via query parameter (`?token=...`). Phase 3 will migrate to using the `Sec-WebSocket-Protocol` header for authentication, matching the ESP32 firmware implementation.

**Integration Note**: Phase 4 firmware is designed to work with the Phase 3 gateway once completed. The device sends its API key via the subprotocol header, which the gateway will validate.

---

## Phase 2: Architectural Decoupling (ML IPC Sidecar Isolation)

**Status**: ✅ Completed

### Completed Tasks

- [x] Created `ml-bridge` crate with PythonWorker implementation
  - File: `ml-bridge/src/workers/ml_bridge.rs`
  - Manages Python subprocess lifecycle
  - Unix Domain Socket (UDS) for IPC communication
  - Full async I/O integration with Tokio

- [x] Implemented length-prefixed JSON protocol
  - 4-byte big-endian length prefix
  - JSON payload for request/response
  - Supports request correlation IDs

- [x] Created Python inference worker daemon
  - File: `scripts/inference_worker.py`
  - Asyncio-based Unix socket server
  - Mock ML inference with 500ms simulated delay
  - Graceful shutdown handling

- [x] Added comprehensive error handling
  - `PythonWorkerError` enum with specific error types
  - Timeout support for inference calls
  - Health check endpoint for monitoring

- [x] Integrated into workspace
  - Added `ml-bridge` to workspace members
  - Dependencies: tokio, serde, serde_json, anyhow, thiserror, tracing

### Architecture Benefits

- **Event Loop Protection**: Python ML runs in separate process, cannot block Tokio
- **GIL Bypass**: Separate process means no Python GIL contention
- **Fault Isolation**: Python crash doesn't bring down Rust server
- **Independent Scaling**: Can spawn multiple Python workers if needed
- **Security**: UDS is local-only, socket permissions set to 0600

---

## Phase 1: Cryptographic Hardening (Argon2id Password Hashing)

**Status**: ✅ Completed

### Completed Tasks

- [x] Created `PasswordService` with Argon2id implementation
  - File: `auth-api/src/services/auth_service.rs`
  - Parameters: 48 MiB memory, 3 iterations, parallelism = 1
  - Exceeds OWASP 2024+ minimums

- [x] Added argon2 crate dependency
  - Replaced legacy sha2 crate with argon2 v0.5
  - Added rand_core for secure salt generation

- [x] Updated handlers to use Argon2id verification
  - File: `auth-api/src/handlers.rs`
  - Constant-time password verification
  - PHC-formatted hash storage

- [x] Created database migration
  - File: `migrations/001_argon2id_password_hash.sql`
  - Expands password_hash to VARCHAR(255)
  - Removes legacy salt column (embedded in PHC format)

- [x] Comprehensive test suite
  - 7 tests covering roundtrip, unique salts, unicode, edge cases
  - Development profile for faster testing

### Security Improvements

- Memory-hard password hashing (resists GPU/ASIC attacks)
- 128-bit random salt per password (CSPRNG)
- Constant-time verification (timing attack protection)
- PHC-formatted strings (self-documenting hash format)

---

## Summary Statistics

| Metric | Value |
|--------|-------|
| Phases Completed | 3 (Phase 1, 2, 4) |
| Phases In Progress | 1 (Phase 3) |
| New Crates Added | 2 (ml-bridge, firmware) |
| Security Improvements | 12+ |
| Test Coverage | Unit tests for auth, ML bridge |
| Supported Platforms | Linux (backend), ESP32 family (firmware) |
