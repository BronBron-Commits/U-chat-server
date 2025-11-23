# Research Findings

## ML IPC Sidecar Architecture

### Problem Statement

Running Python ML inference in-process with Rust (via PyO3 or similar FFI) causes:
1. **Tokio Event Loop Blocking**: CPU-intensive ML tasks monopolize async worker threads
2. **GIL Contention**: Python's Global Interpreter Lock prevents true parallelism
3. **Fault Coupling**: Python crashes or memory issues bring down the Rust server
4. **Resource Competition**: ML and web serving compete for the same process resources

### Solution: Process Isolation via IPC

We chose a **sidecar process** architecture where:
- Python ML runs in a separate process, spawned by Rust
- Communication occurs via Unix Domain Sockets (UDS)
- All I/O is fully async (Tokio on Rust side, asyncio on Python side)

### Why Unix Domain Sockets?

| Option | Latency | Security | Complexity |
|--------|---------|----------|------------|
| **UDS** | ~10μs | Local only | Low |
| TCP/IP | ~100μs | Network exposed | Medium |
| Shared Memory | ~1μs | Complex sync | High |
| Named Pipes | ~10μs | Platform-specific | Medium |

UDS provides:
- Near-memory-speed performance for local IPC
- No network exposure (inherently secure)
- Simple file-based permissions (chmod 0600)
- Native support in both Rust (tokio) and Python (asyncio)

### Protocol Design

We use **length-prefixed JSON** messages:

```
┌─────────────────┬──────────────────────────┐
│ 4 bytes (BE)   │ JSON payload             │
│ message length │ (UTF-8 encoded)          │
└─────────────────┴──────────────────────────┘
```

**Why JSON over binary formats?**
- Human-readable for debugging
- No schema compilation needed
- Python/Rust native support
- Acceptable overhead for moderate message sizes

**When to consider binary protocols:**
- Payloads > 1MB consistently
- Latency requirements < 1ms
- High-frequency requests (>10k/sec)
- Options: MessagePack, Protocol Buffers, FlatBuffers

### Async I/O Integration

**Rust side (Tokio):**
```rust
// Write is non-blocking - yields to scheduler
socket.write_all(&payload).await?;
// Read is non-blocking - yields while waiting
socket.read_exact(&mut buffer).await?;
```

**Python side (asyncio):**
```python
# Async read - yields during I/O wait
data = await reader.readexactly(length)
# Async write - yields during flush
await writer.drain()
```

This ensures:
- No Tokio worker threads are blocked
- Python can handle I/O while waiting on ML
- True cooperative multitasking on both sides

### Fault Isolation Benefits

| Failure Mode | In-Process (PyO3) | IPC Sidecar |
|--------------|-------------------|-------------|
| Python crash | Server crashes | Worker restarts |
| Memory leak | Server OOM | Worker OOM, server ok |
| Deadlock | Server frozen | Worker timeout, restart |
| Long inference | Event loop blocked | Other requests continue |

### Performance Considerations

**IPC Overhead:**
- Socket round-trip: ~50-100μs
- JSON serialization: ~10-50μs (depending on payload)
- Total overhead: ~100-200μs per request

**For 500ms ML inference:**
- IPC overhead is 0.02-0.04% of total time
- Negligible impact on user-perceived latency

**Scaling Options:**
- Single worker: Sequential processing (current)
- Worker pool: Round-robin distribution
- Queue-based: Redis/RabbitMQ for persistence

### Security Compliance

| Requirement | Implementation |
|-------------|----------------|
| Local-only access | UDS (no network binding) |
| File permissions | Socket chmod 0600 |
| Input validation | JSON schema validation |
| Process isolation | Separate memory spaces |
| Resource limits | Can apply cgroups to Python process |

### References

- [Tokio Unix Domain Sockets](https://docs.rs/tokio/latest/tokio/net/struct.UnixStream.html)
- [Python asyncio Streams](https://docs.python.org/3/library/asyncio-stream.html)
- [Sidecar Pattern - Microsoft](https://learn.microsoft.com/en-us/azure/architecture/patterns/sidecar)
- [GIL and Multiprocessing](https://docs.python.org/3/library/multiprocessing.html)

---

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
