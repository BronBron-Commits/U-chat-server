# Unhidra – Distributed Messaging System

Unhidra is a multi-service Rust messaging platform composed of:

• gateway-service – WebSocket entrypoint  
• auth-api – authentication and token issuance  
• chat-service – message routing  
• presence-service – online/offline tracking  
• history-service – message history retrieval  
• event-hub-service – internal pub/sub bridge  
• client – CLI test client  
• startup-check – health diagnostics tool  

==================================================
CURRENT STATUS (v0.1.2)
==================================================

GATEWAY SERVICE
• Running on: ws://127.0.0.1:9000/ws  
• WebSocket echo verified using:  
  websocat ws://127.0.0.1:9000/ws  
  hello → hello  
• Updated to support Utf8Bytes (Axum 0.8)  
• Message broadcasting and receiving confirmed working  

AUTH API
• Running on http://127.0.0.1:9200  
• /login endpoint available (POST only)  
• Client verifies health successfully  

CLIENT
• Tests Auth API  
• Tests Gateway WebSocket connection  
• Provides output confirming operational status  

STARTUP CHECK
• Validates service ports:  
  9000 gateway  
  9200 auth  
  9300 chat  
  9400 presence  
  9500 history  

==================================================
RECENT UPDATES (v0.1.2)
==================================================

GATEWAY IMPROVEMENTS
• Fixed String→Utf8Bytes type errors  
• Updated WebSocket message handling  
• Correct broadcast routing  
• More stable concurrent session handling  

CLIENT IMPROVEMENTS
• Health-test for Auth API  
• Gateway WebSocket connection test  
• Cleaner terminal logging  

SYSTEM-WIDE
• Full workspace build stability  
• Updated README and documentation  
• Preparing for token authentication and real messaging  

==================================================
BUILDING
==================================================

Clone and compile everything:

git clone git@github.com:BronBron-Commits/Unhidra.git
cd Unhidra
cargo build --workspace --release

==================================================
NEXT STEPS
==================================================

• Implement authenticated WebSocket sessions  
• Finish message protocol between services  
• GUI front-end  
• Windows binary support  
• Federation and extended presence tracking  

