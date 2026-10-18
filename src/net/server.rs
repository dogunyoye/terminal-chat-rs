use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{anyhow, bail, Context, Result};
use chrono::Local;
use futures_util::{SinkExt, StreamExt};
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::TcpListener;
use tokio::sync::{broadcast, mpsc, Mutex};
use tokio::task::JoinHandle;
use tokio::time::{interval_at, Instant, MissedTickBehavior};
use tokio_rustls::TlsAcceptor;
use tokio_tungstenite::accept_async;
use tokio_tungstenite::tungstenite::Message;

use crate::net::{IDLE_TIMEOUT, PING_INTERVAL};
use crate::protocol::{ClientMessage, ServerMessage};
use crate::sanitize;

/// How many queued messages a single slow client may accumulate before it is
/// considered too far behind and disconnected.
const CLIENT_QUEUE_CAPACITY: usize = 256;

/// TLS material used to serve `wss://` connections.
pub struct TlsConfig {
    pub cert_path: PathBuf,
    pub key_path: PathBuf,
}

impl TlsConfig {
    pub fn new(cert_path: impl Into<PathBuf>, key_path: impl Into<PathBuf>) -> Self {
        TlsConfig {
            cert_path: cert_path.into(),
            key_path: key_path.into(),
        }
    }

    fn acceptor(&self) -> Result<TlsAcceptor> {
        let certs = load_certs(&self.cert_path)?;
        let key = load_key(&self.key_path)?;

        let config = rustls::ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(certs, key)
            .context("certificate and private key do not match")?;

        Ok(TlsAcceptor::from(Arc::new(config)))
    }
}

/// A running chat server.
pub struct ServerHandle {
    pub addr: SocketAddr,
    /// Whether the server serves `wss://` (TLS) connections.
    pub tls: bool,
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
    out: mpsc::Sender<ServerMessage>,
    /// Fires when the server decides to drop this client (e.g. it fell too far
    /// behind), waking its connection task so it can clean up.
    close: broadcast::Sender<()>,
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

/// Bind a plaintext chat server to `0.0.0.0:port` and start accepting connections.
pub async fn listen(port: u16) -> Result<ServerHandle> {
    serve(port, None).await
}

/// Bind a TLS chat server (`wss://`) to `0.0.0.0:port` using the given certificate.
pub async fn listen_tls(port: u16, tls: TlsConfig) -> Result<ServerHandle> {
    serve(port, Some(Arc::new(tls.acceptor()?))).await
}

async fn serve(port: u16, tls_acceptor: Option<Arc<TlsAcceptor>>) -> Result<ServerHandle> {
    let listener = TcpListener::bind(("0.0.0.0", port))
        .await
        .with_context(|| format!("failed to bind to port {}", port))?;

    let addr = listener.local_addr()?;
    let tls = tls_acceptor.is_some();
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
                        let acceptor = tls_acceptor.clone();

                        tokio::spawn(async move {
                            match acceptor {
                                Some(acceptor) => match acceptor.accept(stream).await {
                                    Ok(tls_stream) => {
                                        handle_connection(tls_stream, peer, room, shutdown).await;
                                    }
                                    Err(error) => {
                                        eprintln!("TLS handshake with {} failed: {}", peer, error);
                                    }
                                },
                                None => handle_connection(stream, peer, room, shutdown).await,
                            }
                        });
                    }
                    Err(error) => eprintln!("failed to accept connection: {}", error),
                },
                _ = accept_shutdown.recv() => break,
            }
        }
    });

    Ok(ServerHandle {
        addr,
        tls,
        shutdown: shutdown_tx,
        task,
    })
}

async fn handle_connection<S>(
    stream: S,
    peer: SocketAddr,
    room: Arc<Mutex<Room>>,
    mut shutdown_rx: broadcast::Receiver<()>,
) where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    let socket = match accept_async(stream).await {
        Ok(socket) => socket,
        Err(error) => {
            eprintln!("websocket handshake with {} failed: {}", peer, error);
            return;
        }
    };

    let (mut write, mut read) = socket.split();
    let (out_tx, mut out_rx) = mpsc::channel::<ServerMessage>(CLIENT_QUEUE_CAPACITY);
    let (close_tx, mut close_rx) = broadcast::channel::<()>(1);

    let id = {
        let mut room = room.lock().await;
        let id = room.assign_id();
        room.clients.insert(
            id,
            Client {
                nickname: None,
                out: out_tx.clone(),
                close: close_tx.clone(),
            },
        );
        id
    };

    let forwarder = tokio::spawn(async move {
        let mut ping = interval_at(Instant::now() + PING_INTERVAL, PING_INTERVAL);
        ping.set_missed_tick_behavior(MissedTickBehavior::Delay);

        loop {
            tokio::select! {
                message = out_rx.recv() => match message {
                    Some(message) => {
                        let text = match message.to_json() {
                            Ok(text) => text,
                            Err(_) => continue,
                        };

                        if write.send(Message::text(text)).await.is_err() {
                            break;
                        }
                    }
                    None => break,
                },
                _ = ping.tick() => {
                    if write.send(Message::Ping(Vec::new().into())).await.is_err() {
                        break;
                    }
                }
            }
        }

        let _ = write.close().await;
    });

    let mut last_seen = Instant::now();
    let mut idle_check = interval_at(Instant::now() + PING_INTERVAL, PING_INTERVAL);
    idle_check.set_missed_tick_behavior(MissedTickBehavior::Delay);

    loop {
        let item = tokio::select! {
            item = read.next() => item,
            _ = idle_check.tick() => {
                if last_seen.elapsed() >= IDLE_TIMEOUT {
                    break;
                }
                continue;
            }
            _ = shutdown_rx.recv() => break,
            _ = close_rx.recv() => break,
        };

        let text = match item {
            Some(Ok(Message::Text(text))) => text,
            Some(Ok(Message::Close(_))) | None => break,
            Some(Ok(_)) => {
                last_seen = Instant::now();
                continue;
            }
            Some(Err(_)) => break,
        };

        last_seen = Instant::now();

        match ClientMessage::from_json(&text) {
            Ok(ClientMessage::Join { nickname }) => {
                let nickname = nickname.trim().to_string();

                if nickname.is_empty() {
                    let _ = out_tx.try_send(ServerMessage::Error {
                        message: "nickname cannot be empty".to_string(),
                    });
                    break;
                }

                if sanitize::sanitize(&nickname) != nickname {
                    let _ = out_tx.try_send(ServerMessage::Error {
                        message: "nickname contains invalid characters".to_string(),
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
                        let _ = out_tx.try_send(ServerMessage::Error {
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

                let _ = out_tx.try_send(ServerMessage::Welcome {
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
                        let _ = out_tx.try_send(ServerMessage::Error {
                            message: "you must join before sending messages".to_string(),
                        });
                    }
                }
            }
            Err(error) => {
                let _ = out_tx.try_send(ServerMessage::Error {
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
    let mut room = room.lock().await;

    let mut slow: Vec<u64> = Vec::new();
    for (id, client) in &room.clients {
        if client.out.try_send(message.clone()).is_err() {
            slow.push(*id);
        }
    }

    // A client whose queue is full (or whose connection is gone) is too far
    // behind to keep up, so drop it rather than let its backlog grow forever.
    for id in slow {
        if let Some(client) = room.clients.remove(&id) {
            let _ = client.close.send(());
        }
    }
}

fn load_certs(path: &Path) -> Result<Vec<CertificateDer<'static>>> {
    let data = std::fs::read(path).with_context(|| format!("failed to read {}", path.display()))?;
    let mut reader = data.as_slice();

    let certs = rustls_pemfile::certs(&mut reader)
        .collect::<std::result::Result<Vec<_>, _>>()
        .with_context(|| format!("failed to parse certificates from {}", path.display()))?;

    if certs.is_empty() {
        bail!("no certificates found in {}", path.display());
    }

    Ok(certs)
}

fn load_key(path: &Path) -> Result<PrivateKeyDer<'static>> {
    let data = std::fs::read(path).with_context(|| format!("failed to read {}", path.display()))?;
    let mut reader = data.as_slice();

    rustls_pemfile::private_key(&mut reader)
        .with_context(|| format!("failed to parse private key from {}", path.display()))?
        .ok_or_else(|| anyhow!("no private key found in {}", path.display()))
}
