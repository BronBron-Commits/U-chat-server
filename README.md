# U-Chat  
A lightweight, modular chat stack written in Rust.  
Focused on clarity, correctness, and small, composable microservices.

## Overview  
U-Chat consists of six independent Rust services that communicate over WebSockets and HTTP.

1. auth-api  
   Issues HS256 JWT tokens for login.  
2. gateway-service  
   Validates JWTs and manages all WebSocket connections.  
3. chat-service  
   Handles broadcast and direct messaging logic.  
4. presence-service  
   Tracks online and offline state.  
5. history-service  
   Stores and retrieves message history.  
6. bot-service  
   Internal automation and system events.

A Rust CLI client is provided as an example of using the gateway and auth endpoints.

## Features  
• JWT authentication using HS256  
• Central WebSocket gateway  
• Fully asynchronous services built on tokio  
• Broadcast system based on tokio::sync::broadcast  
• Modular services that can run together or independently  
• Verified working on desktop Linux and Termux on Android

## Building  
Clone the repository:

\`\`\`
git clone git@github.com:BronBron-Commits/U-chat.git
cd U-chat
\`\`\`

Build everything:

\`\`\`
cargo build --release
\`\`\`

## Running the stack  
Use the included run-all.sh script.

\`\`\`
chmod +x run-all.sh
./run-all.sh
\`\`\`

This launches all microservices in the background and writes logs to the logs/ directory.

## Example client  
Build:

\`\`\`
cargo build --release --bin client
\`\`\`

Run:

\`\`\`
./target/release/client
\`\`\`

The client performs a login request, receives a JWT, and connects to the WebSocket gateway.

## Project status  
The current version (v0.1.3) represents the first fully verified end-to-end chat flow, including stable authentication and WebSocket communication.

Future work includes message persistence, channel support, user accounts, and a full GUI client.

## License  
MIT License
