use std::sync::Arc;
use axum::{
    extract::ws::{WebSocket, Message},
    extract::{Query, WebSocketUpgrade},
    response::IntoResponse,
    routing::get,
    Router,
};
use futures_util::{StreamExt, SinkExt};
use jsonwebtoken::{decode, DecodingKey, Validation};
use serde::Deserialize;
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

    let app = Router::new()
        .route("/ws", get(ws_handler))
        .with_state(Arc::new(AppState { tx }));

    println!("Gateway running on 0.0.0.0:9000");
    axum::serve(
        tokio::net::TcpListener::bind("0.0.0.0:9000").await.unwrap(),
        app,
    )
    .await
    .unwrap();
}

async fn ws_handler(
    ws: WebSocketUpgrade,
    Query(query): Query<WsQuery>,
    app: Arc<AppState>,
) -> impl IntoResponse {
    let token_valid = decode::<serde_json::Value>(
        &query.token,
        &DecodingKey::from_secret(b"secret"),
        &Validation::default(),
    )
    .is_ok();

    if !token_valid {
        return "INVALID TOKEN";
    }

    ws.on_upgrade(|socket| async move {
        handle_socket(socket, app).await;
    })
}

async fn handle_socket(mut socket: WebSocket, app: Arc<AppState>) {
    let mut rx = app.tx.subscribe();

    let mut socket_write = socket.clone();

    let send_task = tokio::spawn(async move {
        while let Ok(msg) = rx.recv().await {
            let _ = socket_write.send(Message::Text(msg)).await;
        }
    });

    let app_tx = app.tx.clone();
    let recv_task = tokio::spawn(async move {
        while let Some(Ok(Message::Text(text))) = socket.recv().await {
            let _ = app_tx.send(text);
        }
    });

    let _ = tokio::join!(send_task, recv_task);
}
