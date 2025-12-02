use tokio::net::TcpListener;
use tokio::sync::{broadcast, mpsc};

use tokio_tungstenite::accept_async;
use futures_util::{SinkExt, StreamExt};
use tungstenite::protocol::Message;

use axum::{
    routing::post,
    Router,
    extract::{Multipart},
    response::Html,
};

use chrono::Utc;
use anyhow::Result;

use uchat_proto::events::{ClientEvent, ServerEvent};
use uchat_proto::jwt::create_token;

//
// ENTRYPOINT
//
#[tokio::main]
async fn main() -> Result<()> {
    //
    // 1. WS server
    //
    let ws_listener = TcpListener::bind("0.0.0.0:9000").await?;
    println!("WS gateway on ws://0.0.0.0:9000/ws");

    let tx = broadcast::channel::<String>(1024).0;

    tokio::spawn({
        let tx = tx.clone();
        async move {
            loop {
                let (stream, _) = ws_listener.accept().await.unwrap();
                let tx = tx.clone();
                let mut rx = tx.subscribe();

                tokio::spawn(async move {
                    let _ = handle_ws(stream, tx, &mut rx).await;
                });
            }
        }
    });

    //
    // 2. Upload server (Axum)
    //
    let app = Router::new()
        .route("/upload", post(upload_handler));

    let http_listener = TcpListener::bind("0.0.0.0:7000").await?;
    println!("Upload server on http://0.0.0.0:7000/upload");

    axum::serve(http_listener, app).await?;

    Ok(())
}

//
// WS HANDLER
//
async fn handle_ws(
    stream: tokio::net::TcpStream,
    tx: broadcast::Sender<String>,
    rx: &mut broadcast::Receiver<String>,
) -> Result<()> {
    let ws = accept_async(stream).await?;
    let (mut ws_write, mut ws_read) = ws.split();

    let secret = "MY_SECRET_KEY";

    let (msg_tx, mut msg_rx) = mpsc::unbounded_channel::<Message>();
    let writer = tokio::spawn(async move {
        while let Some(msg) = msg_rx.recv().await {
            let _ = ws_write.send(msg).await;
        }
    });

    let mut rx2 = rx.resubscribe();
    let msg_tx_clone = msg_tx.clone();
    tokio::spawn(async move {
        while let Ok(content) = rx2.recv().await {
            let event = ServerEvent::MessageBroadcast {
                from: "user".into(),
                content,
            };
            let json = serde_json::to_string(&event).unwrap();
            let _ = msg_tx_clone.send(Message::Text(json));
        }
    });

    while let Some(msg) = ws_read.next().await {
        if let Ok(Message::Text(text)) = msg {
            if let Ok(event) = serde_json::from_str::<ClientEvent>(&text) {
                match event {
                    ClientEvent::Login { username, .. } => {
                        let token = create_token(secret, &username);
                        let reply = ServerEvent::LoginOk { token };
                        let json = serde_json::to_string(&reply)?;
                        let _ = msg_tx.send(Message::Text(json));
                    }

                    ClientEvent::SendMessage { content } => {
                        let _ = tx.send(content);
                    }

                    ClientEvent::SendMedia { .. } => {
                        // Placeholder for future media events
                        let _ = tx.send("[media message]".into());
                    }
                }
            }
        }
    }

    writer.abort();
    Ok(())
}

//
// FILE UPLOAD HANDLER (AXUM)
//
async fn upload_handler(mut multipart: Multipart) -> Html<String> {
    let mut saved_files = vec![];

    while let Some(field) = multipart.next_field().await.unwrap() {
        let name = field.name().unwrap_or("file").to_string();
        let data = field.bytes().await.unwrap();

        let filename = format!("upload_{}_{}.bin",
            name,
            Utc::now().timestamp_nanos_opt().unwrap()
        );

        tokio::fs::write(&filename, &data).await.unwrap();
        saved_files.push(filename);
    }

    Html(format!("Uploaded files: {:?}", saved_files))
}
