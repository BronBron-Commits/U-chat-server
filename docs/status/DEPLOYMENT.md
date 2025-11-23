# Deployment Guide

This guide covers deployment of Unhidra backend services and ESP32 firmware.

---

## Table of Contents

1. [System Overview](#system-overview)
2. [Features & Capabilities](#features--capabilities)
3. [Prerequisites](#prerequisites)
4. [Backend Deployment](#backend-deployment)
5. [ESP32 Firmware Deployment](#esp32-firmware-deployment)
6. [Environment Configuration](#environment-configuration)
7. [Security Checklist](#security-checklist)
8. [Monitoring & Operations](#monitoring--operations)
9. [Troubleshooting](#troubleshooting)

---

## System Overview

Unhidra is a secure IoT communication platform with the following components:

```
┌─────────────────────────────────────────────────────────────────────┐
│                         Unhidra Architecture                         │
├─────────────────────────────────────────────────────────────────────┤
│                                                                      │
│   ┌─────────────┐     ┌─────────────┐     ┌─────────────┐          │
│   │  ESP32      │     │  Web/Mobile │     │  Bot        │          │
│   │  Devices    │     │  Clients    │     │  Services   │          │
│   └──────┬──────┘     └──────┬──────┘     └──────┬──────┘          │
│          │                   │                   │                  │
│          │ WSS/TLS           │ WSS/TLS           │                  │
│          │                   │                   │                  │
│          └───────────────────┼───────────────────┘                  │
│                              │                                      │
│                    ┌─────────▼─────────┐                           │
│                    │  Gateway Service  │ ◄── JWT Validation        │
│                    │  (Port 9000)      │     Connection Mgmt       │
│                    └─────────┬─────────┘                           │
│                              │                                      │
│         ┌────────────────────┼────────────────────┐                │
│         │                    │                    │                 │
│  ┌──────▼──────┐     ┌───────▼───────┐   ┌───────▼───────┐        │
│  │ Chat Service│     │ Presence Svc  │   │ History Svc   │        │
│  └─────────────┘     └───────────────┘   └───────────────┘        │
│                                                                      │
│  ┌─────────────┐     ┌───────────────┐   ┌───────────────┐        │
│  │ Auth API    │     │  ML Bridge    │   │   Storage     │        │
│  │ (Port 9200) │     │  (UDS IPC)    │   │  (SQLite)     │        │
│  └─────────────┘     └───────┬───────┘   └───────────────┘        │
│        │                     │                                      │
│        │ Argon2id            │ Python Sidecar                       │
│        │                     │                                      │
│  ┌─────▼─────┐        ┌──────▼──────┐                              │
│  │ SQLite DB │        │ Python ML   │                              │
│  │           │        │ Worker      │                              │
│  └───────────┘        └─────────────┘                              │
│                                                                      │
└─────────────────────────────────────────────────────────────────────┘
```

---

## Features & Capabilities

### Security Features (Implemented)

| Feature | Description | Phase |
|---------|-------------|-------|
| **Argon2id Password Hashing** | Memory-hard algorithm (48 MiB, 3 iterations) | 1 |
| **Constant-Time Verification** | Timing attack protection | 1 |
| **PHC Hash Format** | Self-documenting, forward-compatible | 1 |
| **Process Isolation** | ML runs in separate sidecar process | 2 |
| **Unix Domain Sockets** | Local-only, secure IPC | 2 |
| **JWT Authentication** | Stateless token validation | 3 |
| **WSS Encryption** | TLS 1.2/1.3 for all connections | 3/4 |
| **Certificate Verification** | CA bundle validation on ESP32 | 4 |
| **Subprotocol Auth** | Credentials hidden from logs | 4 |
| **Auto-Reconnect** | Exponential backoff with jitter | 4 |

### Backend Services

| Service | Port | Description |
|---------|------|-------------|
| `auth-api` | 9200 | HTTP authentication API |
| `gateway-service` | 9000 | WebSocket gateway |
| `chat-service` | - | Message routing |
| `presence-service` | - | Online status tracking |
| `history-service` | - | Message persistence |
| `ml-bridge` | UDS | ML inference sidecar |

### ESP32 Firmware Features

| Feature | Description |
|---------|-------------|
| **Wi-Fi STA Mode** | Station mode with DHCP |
| **WSS Client** | Secure WebSocket over TLS |
| **Event-Driven** | Non-blocking message handling |
| **Heartbeat** | 60-second application heartbeat |
| **Keep-Alive** | 30-second ping/pong |
| **Auto-Reconnect** | Survives network disruptions |
| **Multi-Chip Support** | ESP32, S2, S3, C3, C6 |

---

## Prerequisites

### Backend Requirements

- **Rust**: 1.70+ (stable)
- **SQLite**: 3.x
- **Python**: 3.8+ (for ML worker)
- **OS**: Linux recommended (UDS support)

### ESP32 Firmware Requirements

- **Rust Toolchain**: Nightly (for Xtensa) or Stable (for RISC-V)
- **ESP-IDF**: v5.2
- **espup**: For toolchain management
- **espflash**: For flashing firmware
- **Hardware**: ESP32, ESP32-S2, ESP32-S3, ESP32-C3, or ESP32-C6

### Install ESP32 Toolchain

```bash
# Install espup (toolchain manager)
cargo install espup

# Install ESP32 toolchain
espup install

# Source the environment (add to .bashrc/.zshrc)
source ~/export-esp.sh

# Install flashing tool
cargo install espflash
```

---

## Backend Deployment

### 1. Database Migration

```bash
# Create database directory
mkdir -p /opt/unhidra

# Apply Argon2id migration
sqlite3 /opt/unhidra/auth.db < migrations/001_argon2id_password_hash.sql

# Verify schema
sqlite3 /opt/unhidra/auth.db ".schema users"
```

**Expected schema:**
```sql
CREATE TABLE users (
    username TEXT PRIMARY KEY NOT NULL,
    password_hash TEXT NOT NULL,
    verified INTEGER NOT NULL DEFAULT 0,
    display_name TEXT NOT NULL DEFAULT ''
);
```

### 2. Build Services

```bash
# Build all services in release mode
cargo build --release

# Or build specific services
cargo build --release -p auth-api
cargo build --release -p gateway-service
cargo build --release -p ml-bridge
```

### 3. Configure Environment

Create `/etc/unhidra/config.env`:

```bash
# Authentication
JWT_SECRET=$(openssl rand -base64 32)

# Database
DATABASE_URL=/opt/unhidra/auth.db

# ML Worker (optional)
ML_SOCKET_PATH=/tmp/unhidra-ml.sock
ML_TIMEOUT_MS=2000

# Logging
RUST_LOG=info
```

### 4. Start Services

```bash
# Source configuration
source /etc/unhidra/config.env

# Start auth-api
./target/release/auth-api &

# Start gateway-service
./target/release/gateway-service &

# Start ML worker (optional)
python3 scripts/inference_worker.py &
```

### 5. Verify Deployment

```bash
# Test auth-api
curl -X POST http://localhost:9200/login \
  -H "Content-Type: application/json" \
  -d '{"username":"test","password":"test"}'

# Test gateway (requires valid JWT)
# Use wscat or similar WebSocket client
wscat -c "ws://localhost:9000/ws?token=<JWT>"
```

---

## ESP32 Firmware Deployment

### 1. Configure Device Credentials

```bash
cd firmware

# Copy example configuration
cp .env.example .env

# Edit with your values
cat > .env << 'EOF'
WIFI_SSID="YourWiFiNetwork"
WIFI_PASSWORD="YourWiFiPassword"
DEVICE_API_KEY="device-key-from-backend"
DEVICE_ID="esp32-001"
EOF
```

### 2. Select Target Architecture

Edit `firmware/.cargo/config.toml`:

```toml
[build]
# Choose your chip:
target = "xtensa-esp32-espidf"      # ESP32
# target = "xtensa-esp32s2-espidf"  # ESP32-S2
# target = "xtensa-esp32s3-espidf"  # ESP32-S3
# target = "riscv32imc-esp-espidf"  # ESP32-C3
# target = "riscv32imac-esp-espidf" # ESP32-C6
```

### 3. Build Firmware

```bash
cd firmware

# Build release firmware
cargo build --release

# Check binary size
cargo size --release
```

### 4. Flash Device

```bash
# Flash and monitor (auto-detects USB port)
cargo run --release

# Or manually specify port
espflash flash --monitor target/xtensa-esp32-espidf/release/unhidra-firmware -p /dev/ttyUSB0
```

### 5. Verify Connection

Monitor output should show:
```
INFO - Wi-Fi connected! IP: 192.168.1.x
INFO - Connecting to WebSocket at wss://api.unhidra.io/ws...
INFO - WebSocket connected!
INFO - Device esp32-001 registered with server
```

---

## Environment Configuration

### Backend Environment Variables

| Variable | Description | Default | Required |
|----------|-------------|---------|----------|
| `JWT_SECRET` | JWT signing secret | `supersecret` | **Yes (prod)** |
| `DATABASE_URL` | SQLite database path | `/opt/unhidra/auth.db` | Yes |
| `GATEWAY_PORT` | Gateway listen port | `9000` | No |
| `AUTH_PORT` | Auth API listen port | `9200` | No |
| `RUST_LOG` | Log level | `info` | No |
| `ML_SOCKET_PATH` | ML worker socket | `/tmp/unhidra-ml.sock` | No |

### ESP32 Environment Variables

| Variable | Description | Required |
|----------|-------------|----------|
| `WIFI_SSID` | Wi-Fi network name | **Yes** |
| `WIFI_PASSWORD` | Wi-Fi password | **Yes** |
| `DEVICE_API_KEY` | Backend authentication key | **Yes** |
| `DEVICE_ID` | Unique device identifier | **Yes** |

---

## Security Checklist

### Pre-Production Checklist

#### Backend
- [ ] Generate strong JWT_SECRET: `openssl rand -base64 32`
- [ ] Set proper file permissions on database
- [ ] Configure firewall (only expose necessary ports)
- [ ] Enable TLS termination (nginx/traefik/haproxy)
- [ ] Set up log rotation
- [ ] Configure rate limiting (see TODO.md)
- [ ] Review CORS settings for gateway

#### ESP32 Firmware
- [ ] Replace default credentials in .env
- [ ] Verify TLS certificate validation is enabled
- [ ] Test with production server
- [ ] Consider enabling secure boot (production)
- [ ] Consider enabling flash encryption (production)
- [ ] Disable JTAG in production builds

### Security Configuration Verification

```bash
# Check auth-api Argon2 parameters
curl http://localhost:9200/debug/password-params  # (if endpoint exists)

# Verify TLS on gateway
openssl s_client -connect api.unhidra.io:443 -servername api.unhidra.io

# Check ESP32 certificate bundle
# (Monitor serial output during connection)
```

---

## Monitoring & Operations

### Health Checks

```bash
# Auth API health
curl http://localhost:9200/health

# Gateway WebSocket ping
# (send ping frame, expect pong)

# ML Worker health (if implemented)
curl http://localhost:9200/internal/ml/health
```

### Log Locations

| Component | Log Output |
|-----------|------------|
| Backend services | stdout (redirect to file or journald) |
| ESP32 firmware | Serial monitor (USB) |
| Python ML worker | stdout/stderr |

### Common Operations

```bash
# Restart services
systemctl restart unhidra-auth
systemctl restart unhidra-gateway

# View logs
journalctl -u unhidra-auth -f
journalctl -u unhidra-gateway -f

# Check database
sqlite3 /opt/unhidra/auth.db "SELECT COUNT(*) FROM users;"

# Monitor ESP32 (after flashing)
espflash monitor -p /dev/ttyUSB0
```

---

## Troubleshooting

### Backend Issues

| Issue | Cause | Solution |
|-------|-------|----------|
| "INVALID TOKEN" on WebSocket | JWT expired or wrong secret | Check JWT_SECRET matches, verify token expiry |
| Database locked | Multiple processes accessing | Ensure single writer, use WAL mode |
| ML timeout | Python worker not running | Start `inference_worker.py` |
| High memory on auth | Many concurrent logins | Argon2id is memory-intensive by design |

### ESP32 Issues

| Issue | Cause | Solution |
|-------|-------|----------|
| Wi-Fi won't connect | Wrong credentials | Verify WIFI_SSID/PASSWORD |
| "TLS handshake failed" | Certificate issue | Check server cert is valid, not self-signed |
| "WebSocket disconnected" (immediate) | Auth failure | Verify DEVICE_API_KEY |
| Frequent reconnects | Server restarts, network | Check server stability, Wi-Fi signal |
| Stack overflow | Insufficient stack size | Increase `CONFIG_ESP_MAIN_TASK_STACK_SIZE` |
| Build fails (missing component) | ESP-IDF config | Ensure `esp_websocket_client` in sdkconfig |

### Build Issues

```bash
# ESP32 toolchain not found
source ~/export-esp.sh  # Re-source environment

# Wrong target architecture
# Check .cargo/config.toml matches your chip

# Cargo can't find esp-idf
# Run: espup install
# Then: source ~/export-esp.sh

# Out of flash space
# Use release mode: cargo build --release
# Enable LTO in Cargo.toml
```

---

## Upgrade Notes

### From Phase 2 to Phase 4

No database changes required. Firmware is additive.

### Future Migration: Phase 3 Gateway Auth

When Phase 3 completes (Sec-WebSocket-Protocol auth):
1. Update gateway-service
2. Clients must send auth in subprotocol header, not query param
3. ESP32 firmware already uses subprotocol (no changes needed)

---

## Appendix: Service Files (systemd)

### auth-api.service

```ini
[Unit]
Description=Unhidra Auth API
After=network.target

[Service]
Type=simple
User=unhidra
EnvironmentFile=/etc/unhidra/config.env
ExecStart=/opt/unhidra/bin/auth-api
Restart=always
RestartSec=5

[Install]
WantedBy=multi-user.target
```

### gateway-service.service

```ini
[Unit]
Description=Unhidra Gateway Service
After=network.target auth-api.service

[Service]
Type=simple
User=unhidra
EnvironmentFile=/etc/unhidra/config.env
ExecStart=/opt/unhidra/bin/gateway-service
Restart=always
RestartSec=5

[Install]
WantedBy=multi-user.target
```

### ml-worker.service

```ini
[Unit]
Description=Unhidra ML Worker
After=network.target

[Service]
Type=simple
User=unhidra
WorkingDirectory=/opt/unhidra
ExecStart=/usr/bin/python3 scripts/inference_worker.py
Restart=always
RestartSec=5

[Install]
WantedBy=multi-user.target
```

---

## Version History

| Version | Date | Changes |
|---------|------|---------|
| 0.4.0 | 2024-11 | Phase 4: ESP32 firmware with WSS |
| 0.3.0 | 2024-11 | Phase 3: Gateway WSS (in progress) |
| 0.2.0 | 2024-11 | Phase 2: ML IPC sidecar |
| 0.1.0 | 2024-11 | Phase 1: Argon2id password hashing |
| 0.0.1 | 2024-11 | Initial release |
