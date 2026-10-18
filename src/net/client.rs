use std::sync::Arc;

use anyhow::{bail, Context, Result};
use futures_util::{SinkExt, StreamExt};
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{DigitallySignedStruct, SignatureScheme};
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{
    connect_async, connect_async_tls_with_config, Connector, MaybeTlsStream, WebSocketStream,
};

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
/// `target` may be a `ws://` or `wss://` URL, or a bare `host:port` (in which
/// case `ws://` is assumed). Hostnames are resolved using DNS and `wss://`
/// connects over TLS.
///
/// Received messages are delivered over `events`.
pub async fn connect(
    target: &str,
    nickname: String,
    events: mpsc::UnboundedSender<AppEvent>,
) -> Result<ClientHandle> {
    let url = normalize_url(target)?;

    let (socket, _) = connect_async(&url)
        .await
        .with_context(|| format!("failed to connect to {}", url))?;

    Ok(spawn(socket, nickname, events))
}

/// Connect to a `wss://` server that presents a certificate the system does not
/// trust.
///
/// This is only used for the host's own loopback connection to the server it is
/// running (`127.0.0.1`), where the address is guaranteed to be local. Remote
/// connections always verify certificates against the system trust store.
pub async fn connect_untrusted(
    target: &str,
    nickname: String,
    events: mpsc::UnboundedSender<AppEvent>,
) -> Result<ClientHandle> {
    let url = normalize_url(target)?;
    let connector = Connector::Rustls(Arc::new(accept_any_certificate_config()?));

    let (socket, _) = connect_async_tls_with_config(&url, None, false, Some(connector))
        .await
        .with_context(|| format!("failed to connect to {}", url))?;

    Ok(spawn(socket, nickname, events))
}

fn accept_any_certificate_config() -> Result<rustls::ClientConfig> {
    let provider = Arc::new(rustls::crypto::ring::default_provider());

    let config = rustls::ClientConfig::builder_with_provider(provider.clone())
        .with_safe_default_protocol_versions()
        .context("failed to configure TLS protocol versions")?
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(AcceptAnyCertificate(provider)))
        .with_no_client_auth();

    Ok(config)
}

#[derive(Debug)]
struct AcceptAnyCertificate(Arc<rustls::crypto::CryptoProvider>);

impl ServerCertVerifier for AcceptAnyCertificate {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> std::result::Result<ServerCertVerified, rustls::Error> {
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> std::result::Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(
            message,
            cert,
            dss,
            &self.0.signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> std::result::Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(
            message,
            cert,
            dss,
            &self.0.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.0.signature_verification_algorithms.supported_schemes()
    }
}

/// Turn user input into a WebSocket URL.
///
/// Accepts `ws://…`/`wss://…` URLs as-is and treats anything else as a
/// `host[:port]` for a plaintext `ws://` connection.
pub fn normalize_url(target: &str) -> Result<String> {
    let target = target.trim();

    if target.is_empty() {
        bail!("enter a server address");
    }

    if target.starts_with("ws://") || target.starts_with("wss://") {
        return Ok(target.to_string());
    }

    if let Some((scheme, _)) = target.split_once("://") {
        bail!("unsupported scheme '{}' (use ws:// or wss://)", scheme);
    }

    Ok(format!("ws://{}", target))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adds_ws_scheme_to_bare_addresses() {
        assert_eq!(
            normalize_url("127.0.0.1:8080").unwrap(),
            "ws://127.0.0.1:8080"
        );
        assert_eq!(normalize_url("example.com").unwrap(), "ws://example.com");
    }

    #[test]
    fn trims_whitespace() {
        assert_eq!(
            normalize_url("  example.com:9000  ").unwrap(),
            "ws://example.com:9000"
        );
    }

    #[test]
    fn keeps_explicit_schemes() {
        assert_eq!(
            normalize_url("wss://chat.example.com/room").unwrap(),
            "wss://chat.example.com/room"
        );
        assert_eq!(
            normalize_url("ws://10.0.0.1:8080").unwrap(),
            "ws://10.0.0.1:8080"
        );
    }

    #[test]
    fn rejects_unsupported_schemes_and_empty_input() {
        assert!(normalize_url("https://example.com").is_err());
        assert!(normalize_url("   ").is_err());
    }

    #[tokio::test]
    #[ignore = "requires internet access"]
    async fn connects_over_tls_to_a_public_endpoint() {
        let _ = rustls::crypto::ring::default_provider().install_default();

        let (tx, _rx) = mpsc::unbounded_channel();

        match connect("wss://echo.websocket.org", "tester".to_string(), tx).await {
            Ok(_client) => {}
            Err(error) => panic!("failed to connect over TLS: {error:#}"),
        }
    }
}
