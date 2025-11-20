# Changelog

## [0.1.2] – Gateway WebSocket Update
### Added
- Implemented working WebSocket broadcast pipeline in `gateway-service`.
- Added `Utf8Bytes` handling for Axum 0.8 WebSocket compatibility.
- Confirmed real client echo-test functionality over `ws://127.0.0.1:9000/ws`.

### Fixed
- Resolved mismatched type errors between `String` and `Utf8Bytes`.
- Corrected broadcast channel types to ensure inbound/outbound WS messages no longer panic.

### Notes
- WebSocket gateway is now verified functional using `websocat`.
- Event-hub integration will be added in the next iteration.
- All core services compile and run cleanly under `tmux` session `unhidra-ms`.

