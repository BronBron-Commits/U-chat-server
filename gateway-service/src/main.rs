use axum::{
    extract::{
        ws::{Message, WebSocket},
        Query, State, WebSocketUpgrade,
    },
    response::Html,
    routing::get,
    Router,
};
use futures_util::{SinkExt, StreamExt};
use jsonwebtoken::{decode, DecodingKey, Validation};
use serde::Deserialize;
use std::sync::Arc;
use tokio::sync::broadcast;

#[derive(Clone)]
struct AppState {
    tx: broadcast::Sender<String>,
}

#[derive(Deserialize)]
struct WsQuery {
    token: String,
}

async fn ws_handler(
    ws: WebSocketUpgrade,
    Query(q): Query<WsQuery>,
    State(state): State<Arc<AppState>>,
) -> Result<impl axum::response::IntoResponse, Html<&'static str>> {
    let secret = std::env::var("JWT_SECRET").unwrap_or_else(|_| "supersecret".into());

    let verify = decode::<serde_json::Value>(
        &q.token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &Validation::default(),
    );

    if verify.is_err() {
        return Err(Html("INVALID TOKEN"));
    }

    Ok(ws.on_upgrade(move |socket| handle_socket(socket, state)))
}

async fn handle_socket(socket: WebSocket, state: Arc<AppState>) {
    let (mut sender, mut receiver) = socket.split();
    let mut rx = state.tx.subscribe();
    let tx = state.tx.clone();

    let send_task = tokio::spawn(async move {
        while let Ok(msg) = rx.recv().await {
            if sender.send(Message::Text(msg)).await.is_err() {
                break;
            }
        }
    });

    let recv_task = tokio::spawn(async move {
        while let Some(Ok(Message::Text(text))) = receiver.next().await {
            let _ = tx.send(text);
        }
    });

    let _ = tokio::join!(send_task, recv_task);
}

#[tokio::main]
async fn main() {
    let (tx, _) = broadcast::channel(100);
    let state = Arc::new(AppState { tx });

    let app = Router::new()
        .route("/ws", get(ws_handler))
        .with_state(state);

    println!("Gateway running on 0.0.0.0:9000");

    let listener = tokio::net::TcpListener::bind("0.0.0.0:9000")
        .await
        .expect("Failed to bind port");

    axum::serve(listener, app)
        .await
        .expect("Server crashed");
}
