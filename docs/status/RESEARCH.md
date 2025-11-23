# Research Findings

This document captures technical research, architectural decisions, and security considerations discovered during Unhidra development.

---

## ESP32 Secure WebSocket Integration (Phase 4)

### Problem Statement

Connecting IoT devices (ESP32) to a cloud backend securely requires:
1. **Encrypted Transport**: Prevent eavesdropping on device-cloud communication
2. **Server Authentication**: Ensure devices connect to legitimate servers (not MITM)
3. **Device Authentication**: Verify device identity before accepting connections
4. **Resilience**: Handle network instability gracefully
5. **Resource Efficiency**: Minimize memory/CPU on constrained devices

### Solution: WSS with Subprotocol Authentication

We chose **WebSocket Secure (WSS)** with device authentication via the `Sec-WebSocket-Protocol` header:

```
Device                                    Gateway
   │                                         │
   │ ──── TLS Handshake ──────────────────► │
   │ ◄─── Server Certificate ───────────── │
   │ ──── Verify Certificate ──────────────►│
   │                                         │
   │ ──── HTTP Upgrade Request ───────────► │
   │      Sec-WebSocket-Protocol: <API_KEY> │
   │                                         │
   │ ◄─── 101 Switching Protocols ──────── │
   │      (or 401 if auth fails)            │
   │                                         │
   │ ◄═══ Encrypted WebSocket Channel ═══► │
```

### Why Not Query Parameters for Auth?

| Method | Security Issue |
|--------|---------------|
| `wss://server/ws?token=xxx` | Token visible in server logs, browser history, referrer headers |
| `Sec-WebSocket-Protocol: xxx` | Token only in memory during handshake, not logged by default |

The WebSocket RFC allows using the subprotocol header for authentication tokens. This is the same approach used by AWS IoT, Azure IoT Hub, and other enterprise IoT platforms.

### esp-idf-svc Ecosystem Analysis

We evaluated several approaches for ESP32 WebSocket implementation:

| Approach | Pros | Cons | Decision |
|----------|------|------|----------|
| Raw ESP-IDF C bindings | Maximum control | Complex, unsafe, manual memory | ❌ |
| esp-idf-sys only | Low-level access | Still requires unsafe, no abstractions | ❌ |
| **esp-idf-svc** | Safe abstractions, maintained, features | Slightly larger binary | ✅ |
| embassy-rs | Pure async Rust | Less mature for ESP32 WebSocket | ❌ |

**Key esp-idf-svc benefits:**
- `EspWebSocketClient`: High-level, event-driven WebSocket client
- `EspWifi`: Managed Wi-Fi with auto-reconnect capabilities
- `binstart` feature: Handles ESP-IDF startup glue automatically
- Active maintenance: Follows ESP-IDF releases

### TLS Certificate Verification

**Why verification is mandatory:**

Without certificate verification, a MITM attack is trivial:
1. Attacker intercepts device traffic (e.g., rogue AP)
2. Attacker presents self-signed cert
3. Device accepts and sends credentials
4. Attacker has full access to device communication

**Our implementation:**

```rust
// Use ESP-IDF's built-in CA certificate bundle
crt_bundle_attach: Some(esp_idf_sys::esp_crt_bundle_attach),
```

This attaches Mozilla's CA root store (bundled with ESP-IDF) and verifies:
- Certificate chain validity
- Certificate expiration
- Common name / SAN matching

**For private CA (enterprise):**
```rust
// Use custom CA certificate
server_cert: Some(X509::pem_until_nul(include_bytes!("../certs/ca.pem"))),
```

### Reconnection Strategy Analysis

IoT devices must handle frequent disconnections (Wi-Fi roaming, server restarts, network issues). We implemented exponential backoff with jitter:

**Algorithm:**
```
backoff = min(initial * multiplier^(failures-1), max_backoff)
jitter = random(-30%, +30%) * backoff
wait_time = backoff + jitter
```

**Parameters chosen:**
- Initial backoff: 5 seconds (quick recovery for transient issues)
- Maximum backoff: 60 seconds (don't wait too long)
- Multiplier: 2.0x (standard exponential growth)
- Jitter: ±30% (prevents thundering herd)

**Why jitter matters:**

Without jitter, if 1000 devices lose connection simultaneously (server restart), they all reconnect at t=5s, t=10s, t=20s... causing load spikes. With jitter, reconnections are distributed:

```
Without jitter:    ||||||||||||  (all at once)
With jitter:       | | || |  | | ||  |  (spread over time)
```

### Memory Considerations

ESP32 has limited RAM (320KB on basic variant). Our choices:

| Decision | Memory Impact | Rationale |
|----------|---------------|-----------|
| Stack size 32KB | Required for TLS + JSON | Default 8KB insufficient |
| Buffer size 2KB | Per-message limit | Balance between memory and payload |
| No heap fragmentation | Use stack where possible | Embedded best practice |
| Release mode LTO | ~20% smaller binary | Important for flash-constrained devices |

### Security Compliance Summary

| OWASP IoT Guideline | Implementation |
|--------------------|----------------|
| Encrypt all data in transit | WSS (TLS 1.2/1.3) |
| Verify server identity | CA certificate bundle |
| Authenticate devices | API key via subprotocol |
| Handle disconnections | Auto-reconnect with backoff |
| Protect credentials | .env files (gitignored), NVS storage |
| Use memory-safe language | Rust (ownership model) |

### References

- [ESP-IDF WebSocket Client](https://docs.espressif.com/projects/esp-idf/en/latest/esp32/api-reference/protocols/esp_websocket_client.html)
- [esp-idf-svc crate](https://github.com/esp-rs/esp-idf-svc)
- [RFC 6455: WebSocket Protocol](https://tools.ietf.org/html/rfc6455)
- [OWASP IoT Security Guidelines](https://owasp.org/www-project-internet-of-things/)

---

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

---

## WebSocket Security Rationale (Phase 3)

### Token Transmission via Sec-WebSocket-Protocol Header

**Problem**: Browser WebSocket API does not support custom Authorization headers.

**Rejected Alternatives**:

1. **URL Query Parameter** (`wss://server/ws?token=XYZ`)
   - Exposes tokens in server logs, browser history, referrer headers
   - Violates security best practices
   - Potential credential leakage

2. **Cookies**
   - Subject to CSRF attacks
   - Complex cross-origin handling
   - Not suitable for non-browser clients (IoT)

**Chosen Solution**: `Sec-WebSocket-Protocol` header

- Browser clients: `new WebSocket(url, ["bearer", token])`
- Server extracts token from header before upgrade
- Token encrypted in transit (wss/TLS)
- Not typically logged by intermediaries
- Works for both browser and non-browser clients

### Room-Based Pub/Sub Architecture

**Problem**: Scaling message broadcast to many clients.

**Naive Approach** (Rejected):
- Global list of WebSocket connections
- Loop through all connections per message
- O(N) broadcast, blocking under lock
- Doesn't scale

**Chosen Solution**: DashMap + Tokio Broadcast Channels

| Component | Purpose |
|-----------|---------|
| DashMap | Lock-free concurrent map, sharded storage |
| broadcast::channel | Efficient fan-out, internal buffering |
| Room isolation | Different rooms broadcast independently |

Benefits:
- Concurrent broadcasts without contention
- Bounded buffer (100 messages) for backpressure
- Memory freed when rooms empty
- No explicit locking per message

### Origin Validation (CSRF Protection)

**Threat**: Cross-Site WebSocket Hijacking (CSWSH)
- Attacker's web page initiates WebSocket to our server
- Browser sends victim's cookies automatically
- Attacker can send/receive messages as victim

**Mitigation**:
- Server validates `Origin` header on handshake
- Only allow known frontend origins
- Reject unexpected origins with HTTP 403

Configuration: `ALLOWED_ORIGINS` environment variable

### Resource Cleanup Strategy

**Problem**: Memory leaks from abandoned rooms.

**Solution**:
1. Track subscriber count per room (`sender.receiver_count()`)
2. On disconnect, check if room is empty
3. Remove empty rooms from DashMap
4. Dropping Sender closes all Receivers

This ensures:
- No unbounded memory growth
- Clean disconnection handling
- Proper channel cleanup

### Bounded Channel Capacity

Channel capacity: 100 messages

**Rationale**:
- Prevents memory exhaustion from slow consumers
- Oldest messages dropped if buffer full
- Acceptable for transient real-time data
- Clients should handle message loss gracefully

---

## References

- [OWASP Password Storage Cheat Sheet](https://cheatsheetseries.owasp.org/cheatsheets/Password_Storage_Cheat_Sheet.html)
- [Argon2 RFC (RFC 9106)](https://datatracker.ietf.org/doc/html/rfc9106)
- [RustCrypto argon2 crate](https://docs.rs/argon2)

---

## WebSocket Authentication Patterns

### Comparison of Authentication Methods

| Method | Pros | Cons | Use Case |
|--------|------|------|----------|
| Query Parameter | Simple to implement | Logged, visible in URLs | Legacy systems |
| **Sec-WebSocket-Protocol** | Hidden from logs, standard | Single value only | IoT devices, SPAs |
| HTTP Headers (custom) | Flexible | Not supported by all clients | Internal services |
| First Message Auth | Maximum flexibility | Connection already open | Chat applications |
| Cookie-based | Automatic on same domain | CSRF concerns, cross-domain issues | Web browsers |

### Our Choice: Sec-WebSocket-Protocol

For Unhidra, we use `Sec-WebSocket-Protocol` because:
1. ESP32 firmware can set it easily via esp-idf-svc
2. Web browsers can set it via JavaScript WebSocket API
3. Token never appears in server access logs
4. Standard HTTP handshake - works through all proxies

### Server-Side Validation Flow

```rust
// Extract subprotocol from upgrade request
let protocol = request.headers()
    .get("sec-websocket-protocol")
    .and_then(|h| h.to_str().ok());

// Validate as JWT or API key
match validate_token(protocol) {
    Ok(claims) => {
        // Accept connection, echo subprotocol back
        response.headers_mut().insert(
            "sec-websocket-protocol",
            protocol.parse().unwrap()
        );
    }
    Err(_) => {
        return StatusCode::UNAUTHORIZED;
    }
}
```

---

## Versioning and Compatibility Notes

### Crate Version Compatibility Matrix

| Crate | Version | ESP-IDF Version | Notes |
|-------|---------|-----------------|-------|
| esp-idf-svc | 0.49.x | v5.2 | Current stable |
| esp-idf-sys | 0.35.x | v5.2 | Matches svc |
| esp-idf-hal | 0.44.x | v5.2 | Hardware layer |
| embedded-svc | 0.28.x | N/A | Traits only |

### Breaking Changes Encountered

1. **esp-idf-svc 0.48 → 0.49**: WebSocket API changed to event-based
2. **ESP-IDF 5.0 → 5.2**: WebSocket component moved to esp-protocols
3. **Rust nightly requirements**: Some ESP32 targets require nightly (Xtensa)

### Future Compatibility

- Monitor esp-rs/esp-idf-svc for updates
- Pin versions in Cargo.toml for reproducibility
- Test firmware with each ESP-IDF major release
