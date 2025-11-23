U-Chat
A small, modular Rust-based chat system.

Services:
• auth-api handles login and token issuing
• gateway-service WebSocket entrypoint and event router
• chat-service message broadcasting
• presence-service online status tracking
• history-service basic message storage

Shared:
• uchat-proto common event and token types

Goal:
Provide a lightweight Rust chat backend with typed events and clean WebSocket communication.