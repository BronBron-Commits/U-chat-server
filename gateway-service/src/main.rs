use axum::{
    extract::{
        ws::{Message, WebSocket},
        Query, State, WebSocketUpgrade,
    },
    response::{Html, IntoResponse},
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

#[tokio::main]
async fn main() {
    let (tx, _) = broadcast::channel::<String>(100);
    let state = Arc::new(AppState { tx });

    let app = Router::new()
        .route("/ws", get(ws_handler))
        .with_state(state);

    println!("Gateway running on 0.0.0.0:9000");

    axum::serve(
        tokio::net::TcpListener::bind("0.0.0.0:9000")
            .await
            .unwrap(),
        app,
    )
    .await
    .unwrap();
}

async fn ws_handler(
    Query(query): Query<WsQuery>,
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    let secret = std::env::var("JWT_SECRET").unwrap_or("supersecret".into());
    let key = DecodingKey::from_secret(secret.as_bytes());
    let validation = Validation::default();

    if decode::<serde_json::Value>(&query.token, &key, &validation).is_err() {
        return Html("INVALID TOKEN");
    }

    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

async fn handle_socket(mut socket: WebSocket, state: Arc<AppState>) {
    let mut rx = state.tx.subscribe();
    let mut socket_send = socket.clone();

    let send_task = tokio::spawn(async move {
        while let Ok(msg) = rx.recv().await {
            if socket_send.send(Message::Text(msg)).await.is_err() {
                break;
            }
        }
    });

    let recv_task = tokio::spawn(async move {
        while let Some(Ok(Message::Text(text))) = socket.recv().await {
            let _ = state.tx.send(text);
        }
    });

    let _ = tokio::join!(send_task, recv_task);
}
