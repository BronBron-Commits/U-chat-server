use tokio::net::TcpListener;
use tokio::net::TcpStream;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::broadcast;
use uuid::Uuid;
use anyhow::Result;
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<()> {
    let listener = TcpListener::bind("127.0.0.1:9700").await?;
    let (tx, _) = broadcast::channel::<(Uuid, Vec<u8>)>(100);
    let connections = Arc::new(tokio::sync::Mutex::new(std::collections::HashMap::<Uuid, tokio::sync::Mutex<TcpStream>>::new()));

    loop {
        let (stream, _) = listener.accept().await?;
        let id = Uuid::new_v4();

        let tx = tx.clone();
        let connections = connections.clone();

        tokio::spawn(async move {
            let mut read_half = stream;
            let mut write_lock = tokio::sync::Mutex::new(read_half.try_clone().unwrap());

            connections.lock().await.insert(id, write_lock);

            let mut buf = [0u8; 1024];

            loop {
                let n = read_half.read(&mut buf).await.unwrap_or(0);
                if n == 0 {
                    connections.lock().await.remove(&id);
                    break;
                }

                let msg = buf[..n].to_vec();
                let _ = tx.send((id, msg));
            }
        });

        let connections = connections.clone();
        tokio::spawn(async move {
            let mut rx = tx.subscribe();

            while let Ok((sender_id, data)) = rx.recv().await {
                let conns = connections.lock().await;

                for (other_id, other_stream_lock) in conns.iter() {
                    if *other_id == sender_id {
                        continue;
                    }

                    let mut other_stream = other_stream_lock.lock().await;
                    let _ = other_stream.write_all(&data).await;
                }
            }
        });
    }
}
