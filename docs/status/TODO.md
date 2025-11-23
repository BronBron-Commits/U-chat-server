# Todo / Development Tasks

## Critical - Phase 3 Completion (Gateway Integration)

### Phase 3: WSS Gateway Security (BLOCKING for ESP32 integration)

The ESP32 firmware (Phase 4) is ready and waiting for Phase 3 gateway updates. The firmware sends authentication via `Sec-WebSocket-Protocol` header, but the gateway currently reads from query params.

- [ ] **CRITICAL**: Upgrade gateway to Sec-WebSocket-Protocol authentication
  - File: `gateway-service/src/main.rs`
  - Extract token from `Sec-WebSocket-Protocol` header instead of query param
  - Return the validated subprotocol in response headers
  - This enables ESP32 firmware connection

- [ ] Add connection tracking with DashMap
  - Store connected client info (user_id, device_id, connect_time)
  - Enable targeted message delivery
  - Support for presence tracking

- [ ] Implement graceful connection termination
  - Send close frame with reason code
  - Clean up connection state
  - Log disconnection events

- [ ] Add rate limiting for WebSocket connections
  - Limit connections per IP
  - Limit connections per user/device
  - Prevent resource exhaustion

---

## Current Sprint

### Phase 4 Integration Tasks (Waiting on Phase 3)

- [ ] **EF-IOT-03**: Test ESP32 firmware with updated gateway
  - Verify WSS connection establishment
  - Test authentication flow end-to-end
  - Validate heartbeat reception on server
  - Test reconnection after server restart

- [ ] **EF-IOT-04**: Device registration endpoint
  - Backend endpoint to register new devices
  - Generate device API keys
  - Store device metadata (type, location, capabilities)

### Phase 4 Optional Enhancements

- [ ] **EF-IOT-01**: Enhanced reconnection backoff
  - Already implemented basic exponential backoff with jitter
  - Consider adding: connection quality scoring
  - Consider adding: adaptive backoff based on failure patterns

- [ ] **EF-IOT-02**: Device heartbeat processing
  - Server-side handler for heartbeat messages
  - Store last_seen timestamp per device
  - Alerting for devices that go silent
  - Dashboard for device fleet health

- [ ] **EF-IOT-05**: OTA firmware updates
  - Implement OTA update trigger via WebSocket command
  - Secure firmware signing and verification
  - Rollback support on boot failure
  - File: `firmware/src/main.rs` (ota module placeholder exists)

- [ ] **EF-IOT-06**: Custom CA certificate support
  - For private/enterprise deployments
  - Flash custom CA to device
  - Documentation for cert generation

### Phase 2 Optional Enhancements (ML IPC Sidecar)

- [ ] **EF-ML-01**: Health-check endpoint for Python workers
  - Add `/internal/ml/health` route in gateway-service
  - Call `PythonWorker::health_check()` with timeout
  - Return worker status, PID, and uptime
  - Auto-restart unresponsive workers

- [ ] **EF-ML-02**: Timeouts for ML IPC calls
  - Wrap all `infer()` calls with `tokio::time::timeout`
  - Configure timeout via environment variable (default 2s)
  - Return error to client on timeout
  - Consider killing/restarting stuck Python process

- [ ] **EF-OBS-01**: Structured logging for key flows
  - Add correlation IDs to ML requests
  - Log request sent/response received events
  - Log Python worker startup/shutdown
  - Use tracing with JSON output format

- [ ] **EF-OBS-02**: Basic metrics for WebSocket and ML
  - Add Prometheus metrics (via `metrics` crate)
  - Track: request count, latency histogram, error count
  - Expose `/metrics` endpoint
  - Alert on high error rate or latency

### Phase 1 Optional Enhancements (Authentication)

- [ ] **EF-SEC-01**: Rate limiting on login endpoint
  - Add Tower rate limiter or governor crate
  - Limit login attempts per IP/account per minute
  - Prevents DoS via expensive hash computations

- [ ] **EF-DEVX-01**: Environment-based password cost selection
  - Implement config-based parameter selection
  - Use reduced params in development, full params in production
  - Environment variable or feature flag driven

- [ ] **EF-DEVX-02**: Spec export command
  - CLI command or protected endpoint
  - Output current configuration and capabilities
  - List active features, Argon2 params, build info

---

## Backlog

### ML Infrastructure
- [ ] Implement actual ML model loading in Python worker
- [ ] Add model versioning and hot-reload capability
- [ ] Support multiple concurrent Python workers (round-robin)
- [ ] Add worker pool management with auto-scaling
- [ ] Implement binary protocol (MessagePack/protobuf) for large payloads

### Security Enhancements

- [ ] Implement password change endpoint
- [ ] Add password reset flow with secure tokens
- [ ] Implement account lockout after failed attempts
- [ ] Add audit logging for authentication events
- [ ] Implement secure boot for ESP32 (production)
- [ ] Enable flash encryption for ESP32 (production)

### Migration Tasks

- [ ] Create legacy hash migration flow
  - Detect old SHA256 hash format
  - Re-hash on successful login
  - Gradual migration without forced resets

### Infrastructure
- [ ] Add health check endpoints for all services
- [ ] Implement proper error handling with custom error types
- [ ] Set up CI/CD pipeline with security scanning
- [ ] Add integration tests for ML bridge
- [ ] Add integration tests for ESP32 firmware (HIL testing)
- [ ] Docker compose for local development
- [ ] Kubernetes manifests for production deployment

### ESP32 Firmware Roadmap
- [ ] Support for ESP32 provisioning (SmartConfig/BLE)
- [ ] Local configuration via BLE before Wi-Fi setup
- [ ] Support for multiple Wi-Fi networks (fallback)
- [ ] Deep sleep mode for battery operation
- [ ] Sensor data collection and transmission
- [ ] Local command execution from server messages

---

## Dependency Tracking

### Phase 3 → Phase 4 Integration

```
Phase 3 (Gateway WSS)          Phase 4 (ESP32 Firmware)
         │                              │
         │  Sec-WebSocket-Protocol      │
         │◄─────────────────────────────┤
         │                              │
         │  Authentication validated    │
         ├─────────────────────────────►│
         │                              │
         │  WebSocket connection open   │
         │◄────────────────────────────►│
         │                              │
```

**Current Status**: Phase 4 firmware is complete and ready. Waiting for Phase 3 to implement subprotocol-based authentication.

---

## Task Priority Matrix

| Priority | Category | Task ID | Description |
|----------|----------|---------|-------------|
| 🔴 Critical | Gateway | Phase 3 | Sec-WebSocket-Protocol auth |
| 🟠 High | IoT | EF-IOT-03 | End-to-end testing |
| 🟠 High | IoT | EF-IOT-04 | Device registration |
| 🟡 Medium | Security | EF-SEC-01 | Rate limiting |
| 🟡 Medium | Observability | EF-OBS-01 | Structured logging |
| 🟢 Low | ML | EF-ML-01 | Health checks |
| 🟢 Low | IoT | EF-IOT-05 | OTA updates |
