# Claude Code Instructions for Unhidra

## Documentation Maintenance

**IMPORTANT**: Keep the `docs/status/` folder updated on each significant change:

1. **Progress Tracking** (`docs/status/PROGRESS.md`) - Update with completed tasks and milestones
2. **Todo/Tasks** (`docs/status/TODO.md`) - Maintain current and future development tasks
3. **Research Findings** (`docs/status/RESEARCH.md`) - Document research, findings, and technical decisions
4. **Deployment** (`docs/status/DEPLOYMENT.md`) - Deployment guides and configuration notes

## Project Structure

- `auth-api/` - HTTP-based authentication API (Argon2id password hashing)
- `auth-service/` - WebSocket-based auth service
- `gateway-service/` - WebSocket gateway with token validation
- `chat-service/` - Chat functionality
- `presence-service/` - User presence tracking
- `history-service/` - Chat history
- `migrations/` - Database migration scripts

## Security Guidelines

- Use Argon2id for all password hashing (see `auth-api/src/services/auth_service.rs`)
- Never commit secrets or credentials
- Follow OWASP security best practices
- Use constant-time comparisons for sensitive data

## Development Notes

- Run tests before committing: `cargo test -p <package-name>`
- Apply database migrations from `migrations/` folder
- Use `PasswordService::new_dev()` for faster testing (dev parameters only)
