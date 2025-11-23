# Claude Code Instructions for Unhidra

## Documentation Maintenance

**IMPORTANT**: Keep the `docs/status/` folder updated on each significant change:

1. **Progress Tracking** (`docs/status/PROGRESS.md`) - Update with completed tasks and milestones
2. **Todo/Tasks** (`docs/status/TODO.md`) - Maintain current and future development tasks
3. **Research Findings** (`docs/status/RESEARCH.md`) - Document research, findings, and technical decisions
4. **Deployment** (`docs/status/DEPLOYMENT.md`) - Deployment guides and configuration notes

## Project Structure

### Backend Services
- `auth-api/` - HTTP-based authentication API (Argon2id password hashing)
- `auth-service/` - WebSocket-based auth service
- `gateway-service/` - WebSocket gateway with JWT token validation (WSS)
- `chat-service/` - Chat functionality
- `presence-service/` - User presence tracking
- `history-service/` - Chat history
- `ml-bridge/` - ML IPC sidecar for Python inference isolation

### IoT/Embedded
- `firmware/` - ESP32 firmware with secure WSS client (Phase 4)
  - `src/main.rs` - Main application with Wi-Fi and WebSocket client
  - `Cargo.toml` - Dependencies (esp-idf-svc, esp-idf-sys)
  - `sdkconfig.defaults` - ESP-IDF SDK configuration
  - `.cargo/config.toml` - Target and build configuration

### Infrastructure
- `migrations/` - Database migration scripts
- `scripts/` - Utility scripts (inference_worker.py)

## Security Guidelines

- Use Argon2id for all password hashing (see `auth-api/src/services/auth_service.rs`)
- Never commit secrets or credentials (use .env files, excluded from git)
- Follow OWASP security best practices
- Use constant-time comparisons for sensitive data
- **WSS Required**: All WebSocket connections must use TLS (wss://)
- **Certificate Verification**: ESP32 firmware uses CA bundle for server verification
- **Device Authentication**: Devices authenticate via Sec-WebSocket-Protocol header

## Development Notes

### Backend
- Run tests before committing: `cargo test -p <package-name>`
- Apply database migrations from `migrations/` folder
- Use `PasswordService::new_dev()` for faster testing (dev parameters only)

### ESP32 Firmware
- Requires ESP-IDF v5.2+ and Rust toolchain for Xtensa
- Configure device credentials in `firmware/.env` (copy from `.env.example`)
- Build: `cd firmware && cargo build --release`
- Flash: `cd firmware && cargo run --release`
- Target architectures: ESP32, ESP32-S2, ESP32-S3, ESP32-C3, ESP32-C6

## Security Phases Implemented

| Phase | Description | Status |
|-------|-------------|--------|
| 1 | Argon2id Password Hashing | ✅ Complete |
| 2 | ML IPC Sidecar Isolation | ✅ Complete |
| 3 | WSS Gateway Security | 🔄 In Progress |
| 4 | ESP32 Firmware & WSS Integration | ✅ Complete |

## Quick Reference

```bash
# Run all backend tests
cargo test --workspace

# Build release binaries
cargo build --release

# Start auth-api
./target/release/auth-api

# Build ESP32 firmware
cd firmware && cargo build --release

# Flash ESP32 (with monitor)
cd firmware && cargo run --release
```
