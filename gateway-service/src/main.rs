use tokio::net::TcpListener;
use tokio::sync::{broadcast, mpsc};
use tokio_tungstenite::accept_async;

use futures_util::stream::StreamExt;
use futures_util::SinkExt;

use tungstenite::protocol::Message;

use uchat_proto::events::{ClientEvent, ServerEvent};
use uchat_proto::jwt::{create_token};

use serde_json;
use anyhow::Result;

#[tokio::main]
async fn main() {
    let listener = TcpListener::bind("0.0.0.0:9000").await.unwrap();

    let (tx, _rx) = broadcast::channel::<String>(1024);

    println!("gateway-service listening on ws://0.0.0.0:9000/ws");

    loop {
        let (stream, _) = listener.accept().await.unwrap();
        let tx = tx.clone();
        let mut rx = tx.subscribe();

        tokio::spawn(async move {
            if let Err(e) = handle_connection(stream, tx, &mut rx).await {
                eprintln!("connection error: {:?}", e);
            }
        });
    }
}

async fn handle_connection(
    stream: tokio::net::TcpStream,
    tx: broadcast::Sender<String>,
    rx: &mut broadcast::Receiver<String>,
) -> Result<()> {
    let ws_stream = accept_async(stream).await?;
    let (ws_write, mut ws_read) = ws_stream.split();

    let secret = "MY_SECRET_KEY";

    // Writer channel
    let (msg_tx, mut msg_rx) = mpsc::unbounded_channel::<Message>();

    // Writer task (the ONLY task that touches ws_write)
    let mut ws_write = ws_write;
    let writer = tokio::spawn(async move {
        while let Some(msg) = msg_rx.recv().await {
            if ws_write.send(msg).await.is_err() {
                break;
            }
        }
    });

    // Broadcast listener task
    let msg_tx_clone = msg_tx.clone();
    let mut rx2 = rx.resubscribe();
    let broadcaster = tokio::spawn(async move {
        while let Ok(msg) = rx2.recv().await {
            let event = ServerEvent::MessageBroadcast {
                from: "user".into(),
                content: msg,
            };
            if let Ok(json) = serde_json::to_string(&event) {
                let _ = msg_tx_clone.send(Message::Text(json));
            }
        }
    });

    // Reader loop
    while let Some(msg) = ws_read.next().await {
        match msg {
            Ok(Message::Text(text)) => {
                match serde_json::from_str::<ClientEvent>(&text) {
                    Ok(event) => match event {
                        ClientEvent::Login { username, password: _ } => {
                            let token = create_token(secret, &username);
                            let reply = ServerEvent::LoginOk { token };
                            let json = serde_json::to_string(&reply)?;
                            let _ = msg_tx.send(Message::Text(json));
                        }

                        ClientEvent::SendMessage { content } => {
                            let _ = tx.send(content);
                        }
                    },

                    Err(_) => {
                        let err = ServerEvent::Error {
                            details: "Invalid event".into(),
                        };
                        let json = serde_json::to_string(&err)?;
                        let _ = msg_tx.send(Message::Text(json));
                    }
                }
            }

            Ok(Message::Close(_)) => break,
            _ => {}
        }
    }

    writer.abort();
    broadcaster.abort();
    Ok(())
}
