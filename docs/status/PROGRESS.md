# Development Progress

## Phase 2: Architectural Decoupling (ML IPC Sidecar Isolation)

**Status**: Completed

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

**Status**: Completed

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
