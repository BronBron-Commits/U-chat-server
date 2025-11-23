use tokio::net::TcpListener;
use tokio::sync::broadcast;
use tokio_tungstenite::accept_async;

use futures_util::stream::StreamExt;
use futures_util::SinkExt;

use tungstenite::protocol::Message;

use uchat_proto::events::{ClientEvent, ServerEvent};

use serde_json;
use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    let listener = TcpListener::bind("0.0.0.0:9300").await.unwrap();

    let (tx, _rx) = broadcast::channel::<String>(1024);

    println!("chat-service running on ws://0.0.0.0:9300/ws");

    loop {
        let (stream, _) = listener.accept().await.unwrap();
        let tx = tx.clone();
        let mut rx = tx.subscribe();

        tokio::spawn(async move {
            if let Err(e) = handle_chat(stream, tx, &mut rx).await {
                eprintln!("chat-service error: {:?}", e);
            }
        });
    }
}

async fn handle_chat(
    stream: tokio::net::TcpStream,
    tx: broadcast::Sender<String>,
    rx: &mut broadcast::Receiver<String>,
) -> Result<()> {
    let ws_stream = accept_async(stream).await?;
    let (ws_write, mut ws_read) = ws_stream.split();

    let (msg_tx, mut msg_rx) = tokio::sync::mpsc::unbounded_channel::<Message>();

    let mut writer = ws_write;
    let writer_task = tokio::spawn(async move {
        while let Some(msg) = msg_rx.recv().await {
            if writer.send(msg).await.is_err() {
                break;
            }
        }
    });

    let msg_tx_clone = msg_tx.clone();
    let mut rx2 = rx.resubscribe();
    let broadcast_task = tokio::spawn(async move {
        while let Ok(content) = rx2.recv().await {
            let evt = ServerEvent::MessageBroadcast {
                from: "chat-service".into(),
                content,
            };
            let _ = msg_tx_clone.send(Message::Text(serde_json::to_string(&evt).unwrap()));
        }
    });

    while let Some(msg) = ws_read.next().await {
        if let Ok(Message::Text(text)) = msg {
            match serde_json::from_str::<ClientEvent>(&text) {
                Ok(ClientEvent::SendMessage { content }) => {
                    let _ = tx.send(content);
                }
                Ok(_) => {}
                Err(_) => {
                    let err = ServerEvent::Error {
                        details: "Invalid event".into(),
                    };
                    let _ = msg_tx.send(Message::Text(serde_json::to_string(&err).unwrap()));
                }
            }
        }
    }

    writer_task.abort();
    broadcast_task.abort();
    Ok(())
}
