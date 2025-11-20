# Changelog

## [0.1.1] – Client Service Update  
### Added
- Implemented new client-side health checks for core services.
- Added HTTP test against Auth API at `http://127.0.0.1:9200`.
- Added Gateway WebSocket reachability placeholder for `ws://127.0.0.1:9000/ws`.

### Improved
- Output logs now clearly show which services are being tested.
- Client boot process is cleaner and easier to read for developers.

### Notes
- Auth API fully reachable (HTTP 200 OK)
- Gateway reachable; WebSocket placeholder working
- Preparing WebSocket auth and message protocol for next release
