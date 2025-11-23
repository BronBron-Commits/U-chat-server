# Research Findings

## Argon2id Selection Rationale

### Why Argon2id?

1. **Password Hashing Competition Winner** (2015)
   - Designed specifically for password hashing
   - Peer-reviewed and extensively analyzed

2. **Memory-Hard Algorithm**
   - Requires significant memory per hash computation
   - Dramatically increases cost for GPU/ASIC attackers
   - Time-memory tradeoff resistance

3. **Argon2id Variant**
   - Hybrid of Argon2i (side-channel resistant) and Argon2d (GPU resistant)
   - Best of both worlds for password hashing
   - Recommended by OWASP and IETF

### Parameter Selection

| Parameter | Our Value | OWASP Minimum | Justification |
|-----------|-----------|---------------|---------------|
| Memory    | 48 MiB    | ~19 MiB       | Future-proofing against hardware advances |
| Iterations| 3         | 2             | Additional security margin |
| Parallelism| 1        | 1             | Prevents async runtime thread starvation |

### Parallelism = 1 Decision

In async web servers (Axum/Tokio), setting parallelism > 1 would:
- Spawn multiple threads per login request
- Potentially starve the async runtime
- Create unfair scheduling under load

Single-threaded hashing allows Tokio to schedule other requests fairly.

### PHC String Format

Format: `$argon2id$v=19$m=49152,t=3,p=1$<salt>$<hash>`

Benefits:
- Self-documenting (includes all parameters)
- Forward-compatible (new params auto-parsed)
- Standard format (interoperable)
- Salt embedded (no separate column needed)

## References

- [OWASP Password Storage Cheat Sheet](https://cheatsheetseries.owasp.org/cheatsheets/Password_Storage_Cheat_Sheet.html)
- [Argon2 RFC (RFC 9106)](https://datatracker.ietf.org/doc/html/rfc9106)
- [RustCrypto argon2 crate](https://docs.rs/argon2)
