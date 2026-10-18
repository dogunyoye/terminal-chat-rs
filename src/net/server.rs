use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::{Context, Result};
use chrono::Local;
use futures_util::{SinkExt, StreamExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{broadcast, mpsc, Mutex};
use tokio::task::JoinHandle;
use tokio_tungstenite::accept_async;
use tokio_tungstenite::tungstenite::Message;

use crate::protocol::{ClientMessage, ServerMessage};

/// A running chat server.
pub struct ServerHandle {
    pub addr: SocketAddr,
    shutdown: broadcast::Sender<()>,
    task: JoinHandle<()>,
}

impl ServerHandle {
    /// Stop the server and disconnect every participant.
    pub fn shutdown(&self) {
        let _ = self.shutdown.send(());
    }
}

impl Drop for ServerHandle {
    fn drop(&mut self) {
        self.shutdown();
        self.task.abort();
    }
}

struct Client {
    nickname: Option<String>,
    out: mpsc::UnboundedSender<ServerMessage>,
}

#[derive(Default)]
struct Room {
    next_id: u64,
    clients: HashMap<u64, Client>,
}

impl Room {
    fn assign_id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }
}

/// Bind a chat server to `0.0.0.0:port` and start accepting connections.
pub async fn listen(port: u16) -> Result<ServerHandle> {
    let listener = TcpListener::bind(("0.0.0.0", port))
        .await
        .with_context(|| format!("failed to bind to port {}", port))?;

    let addr = listener.local_addr()?;
    let room = Arc::new(Mutex::new(Room::default()));
    let (shutdown_tx, _) = broadcast::channel::<()>(1);
    let mut accept_shutdown = shutdown_tx.subscribe();
    let connection_shutdown = shutdown_tx.clone();

    let task = tokio::spawn(async move {
        loop {
            tokio::select! {
                accepted = listener.accept() => match accepted {
                    Ok((stream, peer)) => {
                        let room = Arc::clone(&room);
                        let shutdown = connection_shutdown.subscribe();
                        tokio::spawn(handle_connection(stream, peer, room, shutdown));
                    }
                    Err(error) => eprintln!("failed to accept connection: {}", error),
                },
                _ = accept_shutdown.recv() => break,
            }
        }
    });

    Ok(ServerHandle {
        addr,
        shutdown: shutdown_tx,
        task,
    })
}

async fn handle_connection(
    stream: TcpStream,
    peer: SocketAddr,
    room: Arc<Mutex<Room>>,
    mut shutdown_rx: broadcast::Receiver<()>,
) {
    let socket = match accept_async(stream).await {
        Ok(socket) => socket,
        Err(error) => {
            eprintln!("websocket handshake with {} failed: {}", peer, error);
            return;
        }
    };

    let (mut write, mut read) = socket.split();
    let (out_tx, mut out_rx) = mpsc::unbounded_channel::<ServerMessage>();

    let id = {
        let mut room = room.lock().await;
        let id = room.assign_id();
        room.clients.insert(
            id,
            Client {
                nickname: None,
                out: out_tx.clone(),
            },
        );
        id
    };

    let forwarder = tokio::spawn(async move {
        while let Some(message) = out_rx.recv().await {
            let text = match message.to_json() {
                Ok(text) => text,
                Err(_) => continue,
            };

            if write.send(Message::text(text)).await.is_err() {
                break;
            }
        }

        let _ = write.close().await;
    });

    loop {
        let item = tokio::select! {
            item = read.next() => item,
            _ = shutdown_rx.recv() => break,
        };

        let text = match item {
            Some(Ok(Message::Text(text))) => text,
            Some(Ok(Message::Close(_))) | None => break,
            Some(Ok(_)) => continue,
            Some(Err(_)) => break,
        };

        match ClientMessage::from_json(&text) {
            Ok(ClientMessage::Join { nickname }) => {
                let nickname = nickname.trim().to_string();

                if nickname.is_empty() {
                    let _ = out_tx.send(ServerMessage::Error {
                        message: "nickname cannot be empty".to_string(),
                    });
                    break;
                }

                let users = {
                    let mut room = room.lock().await;

                    let taken = room
                        .clients
                        .values()
                        .any(|client| client.nickname.as_deref() == Some(nickname.as_str()));

                    if taken {
                        let _ = out_tx.send(ServerMessage::Error {
                            message: format!("nickname '{}' is already taken", nickname),
                        });
                        break;
                    }

                    if let Some(client) = room.clients.get_mut(&id) {
                        client.nickname = Some(nickname.clone());
                    }

                    room.clients
                        .values()
                        .filter_map(|client| client.nickname.clone())
                        .collect::<Vec<_>>()
                };

                let _ = out_tx.send(ServerMessage::Welcome {
                    nickname: nickname.clone(),
                    users,
                });
                broadcast(&room, &ServerMessage::Joined { nickname }).await;
            }
            Ok(ClientMessage::Chat { content }) => {
                let content = content.trim().to_string();

                if content.is_empty() {
                    continue;
                }

                let nickname = {
                    let room = room.lock().await;
                    room.clients
                        .get(&id)
                        .and_then(|client| client.nickname.clone())
                };

                match nickname {
                    Some(nickname) => {
                        let message = ServerMessage::Chat {
                            nickname,
                            content,
                            timestamp: Local::now().format("%H:%M:%S").to_string(),
                        };
                        broadcast(&room, &message).await;
                    }
                    None => {
                        let _ = out_tx.send(ServerMessage::Error {
                            message: "you must join before sending messages".to_string(),
                        });
                    }
                }
            }
            Err(error) => {
                let _ = out_tx.send(ServerMessage::Error {
                    message: format!("malformed message: {}", error),
                });
            }
        }
    }

    let left = {
        let mut room = room.lock().await;
        room.clients.remove(&id).and_then(|client| client.nickname)
    };

    if let Some(nickname) = left {
        broadcast(&room, &ServerMessage::Left { nickname }).await;
    }

    drop(out_tx);
    let _ = forwarder.await;
}

async fn broadcast(room: &Arc<Mutex<Room>>, message: &ServerMessage) {
    let senders: Vec<mpsc::UnboundedSender<ServerMessage>> = {
        let room = room.lock().await;
        room.clients
            .values()
            .map(|client| client.out.clone())
            .collect()
    };

    for sender in senders {
        let _ = sender.send(message.clone());
    }
}
