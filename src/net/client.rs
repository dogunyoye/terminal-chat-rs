use std::net::SocketAddr;

use anyhow::{Context, Result};
use futures_util::{SinkExt, StreamExt};
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{connect_async, MaybeTlsStream, WebSocketStream};

use crate::event::AppEvent;
use crate::protocol::{ClientMessage, ServerMessage};

type Socket = WebSocketStream<MaybeTlsStream<TcpStream>>;

/// A handle to a live connection, used to send messages and to tear it down.
pub struct ClientHandle {
    sender: mpsc::UnboundedSender<ClientMessage>,
    tasks: Vec<JoinHandle<()>>,
}

impl ClientHandle {
    pub fn send(&self, message: ClientMessage) -> Result<()> {
        self.sender
            .send(message)
            .map_err(|_| anyhow::anyhow!("connection is closed"))
    }

    /// Abort the background tasks associated with this connection.
    pub fn shutdown(&self) {
        for task in &self.tasks {
            task.abort();
        }
    }
}

impl Drop for ClientHandle {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// Connect to a chat server and join the room as `nickname`.
///
/// Received messages are delivered over `events`.
pub async fn connect(
    addr: SocketAddr,
    nickname: String,
    events: mpsc::UnboundedSender<AppEvent>,
) -> Result<ClientHandle> {
    let url = format!("ws://{}", addr);

    let (socket, _) = connect_async(&url)
        .await
        .with_context(|| format!("failed to connect to {}", url))?;

    Ok(spawn(socket, nickname, events))
}

fn spawn(
    socket: Socket,
    nickname: String,
    events: mpsc::UnboundedSender<AppEvent>,
) -> ClientHandle {
    let (mut write, mut read) = socket.split();
    let (out_tx, mut out_rx) = mpsc::unbounded_channel::<ClientMessage>();

    if let Err(error) = out_tx.send(ClientMessage::Join { nickname }) {
        let _ = events.send(AppEvent::Error(error.to_string()));
    }

    let writer_events = events.clone();
    let writer = tokio::spawn(async move {
        while let Some(message) = out_rx.recv().await {
            let text = match message.to_json() {
                Ok(text) => text,
                Err(error) => {
                    let _ = writer_events.send(AppEvent::Error(error.to_string()));
                    continue;
                }
            };

            if write.send(Message::text(text)).await.is_err() {
                break;
            }
        }

        let _ = write.close().await;
    });

    let reader_events = events.clone();
    let reader = tokio::spawn(async move {
        while let Some(item) = read.next().await {
            match item {
                Ok(Message::Text(text)) => match ServerMessage::from_json(&text) {
                    Ok(message) => {
                        if reader_events.send(AppEvent::Server(message)).is_err() {
                            break;
                        }
                    }
                    Err(error) => {
                        let _ = reader_events
                            .send(AppEvent::Error(format!("invalid message: {}", error)));
                    }
                },
                Ok(Message::Close(_)) => break,
                Ok(_) => {}
                Err(error) => {
                    let _ = reader_events.send(AppEvent::Error(error.to_string()));
                    break;
                }
            }
        }

        let _ = reader_events.send(AppEvent::Disconnected {
            reason: "the server closed the connection".to_string(),
        });
    });

    ClientHandle {
        sender: out_tx,
        tasks: vec![writer, reader],
    }
}
