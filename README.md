U-Chat
Lightweight modular Rust chat stack built from small independent services.

Architecture  
    auth-api  
        Issues HS256 JWT tokens

    gateway-service  
        Validates JWTs  
        Manages all WebSocket connections

    chat-service  
        Handles broadcast messaging  
        Handles direct messaging

    presence-service  
        Tracks online users  
        Tracks offline users

    history-service  
        Stores message history  
        Retrieves message history

    bot-service  
        Runs automated internal tasks  
        Handles system-generated events

    client  
        Demonstrates authentication flow  
        Connects to gateway WebSocket

Features  
    HS256 authentication  
    Central WebSocket gateway  
    Asynchronous tokio runtime  
    Broadcast system using tokio sync broadcast  
    Services run independently or together  
    Verified on Linux and Termux

Build  
    git clone git@github.com:BronBron-Commits/U-chat.git  
    cd U-chat  
    cargo build --release

Run  
    chmod +x run-all.sh  
    ./run-all.sh  
        Starts all services in background  
        Writes logs into the logs directory

Client  
    cargo build --release --bin client  
    ./target/release/client  
        Logs in  
        Receives JWT  
        Connects to gateway WebSocket

Status  
    v0.1.3  
        First fully verified end-to-end flow  
        Stable authentication  
        Stable WebSocket messaging

License  
    MIT
