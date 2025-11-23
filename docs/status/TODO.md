# Todo / Development Tasks

## Current Sprint

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

### Migration Tasks
- [ ] Create legacy hash migration flow
  - Detect old SHA256 hash format
  - Re-hash on successful login
  - Gradual migration without forced resets

### Infrastructure
- [ ] Add health check endpoints for all services
- [ ] Implement proper error handling
- [ ] Set up CI/CD pipeline with security scanning
- [ ] Add integration tests for ML bridge
