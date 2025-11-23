# Todo / Development Tasks

## Current Sprint

### Optional Fast-Track Enhancements

- [ ] **EF-SEC-01**: Rate limiting on login endpoint
  - Add Tower rate limiter or governor crate
  - Limit login attempts per IP/account per minute
  - Prevents DoS via expensive hash computations

- [ ] **EF-DEVX-01**: Environment-based password cost selection
  - Implement config-based parameter selection
  - Use reduced params in development, full params in production
  - Environment variable or feature flag driven

## Backlog

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
- [ ] Add health check endpoints
- [ ] Implement proper error handling
- [ ] Add structured logging (tracing crate)
- [ ] Set up CI/CD pipeline with security scanning
