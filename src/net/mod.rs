pub mod client;
pub mod server;

use std::time::Duration;

pub use client::ClientHandle;
pub use server::ServerHandle;

/// How often each side sends a WebSocket ping to the other.
pub(crate) const PING_INTERVAL: Duration = Duration::from_secs(20);
/// A connection that has been silent for this long is considered dead.
pub(crate) const IDLE_TIMEOUT: Duration = Duration::from_secs(60);

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::time::Duration;

    use tokio::sync::mpsc;

    use super::*;
    use crate::event::AppEvent;
    use crate::protocol::{ClientMessage, ServerMessage};

    /// A `host:port` target for the given server (exercises URL normalization).
    fn target(server: &ServerHandle) -> String {
        format!("127.0.0.1:{}", server.addr.port())
    }

    /// Write a throwaway self-signed certificate (SAN `127.0.0.1`) to disk.
    fn self_signed_cert() -> (PathBuf, PathBuf) {
        let key_pair = rcgen::KeyPair::generate().unwrap();
        let params = rcgen::CertificateParams::new(vec!["127.0.0.1".to_string()]).unwrap();
        let cert = params.self_signed(&key_pair).unwrap();

        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "terminal-chat-tls-{}-{}",
            std::process::id(),
            unique
        ));
        std::fs::create_dir_all(&dir).unwrap();

        let cert_path = dir.join("cert.pem");
        let key_path = dir.join("key.pem");
        std::fs::write(&cert_path, cert.pem()).unwrap();
        std::fs::write(&key_path, key_pair.serialize_pem()).unwrap();

        (cert_path, key_path)
    }

    async fn wait_for<F>(rx: &mut mpsc::UnboundedReceiver<AppEvent>, mut matches: F) -> AppEvent
    where
        F: FnMut(&AppEvent) -> bool,
    {
        loop {
            match tokio::time::timeout(Duration::from_secs(5), rx.recv()).await {
                Ok(Some(event)) => {
                    if matches(&event) {
                        return event;
                    }
                }
                Ok(None) => panic!("event channel closed unexpectedly"),
                Err(_) => panic!("timed out waiting for an event"),
            }
        }
    }

    async fn connect_to(
        server: &ServerHandle,
        nickname: &str,
    ) -> (ClientHandle, mpsc::UnboundedReceiver<AppEvent>) {
        let (tx, mut rx) = mpsc::unbounded_channel();

        let client = client::connect(&target(server), nickname.to_string(), tx)
            .await
            .expect("client should connect");

        wait_for(&mut rx, |event| {
            matches!(event, AppEvent::Server(ServerMessage::Welcome { .. }))
        })
        .await;

        (client, rx)
    }

    #[tokio::test]
    async fn welcome_lists_the_joining_user() {
        let server = server::listen(0).await.unwrap();
        let (tx, mut rx) = mpsc::unbounded_channel();
        let _alice = client::connect(&target(&server), "alice".to_string(), tx)
            .await
            .unwrap();

        let event = wait_for(&mut rx, |event| {
            matches!(event, AppEvent::Server(ServerMessage::Welcome { .. }))
        })
        .await;

        match event {
            AppEvent::Server(ServerMessage::Welcome { nickname, users }) => {
                assert_eq!(nickname, "alice");
                assert_eq!(users, vec!["alice".to_string()]);
            }
            _ => unreachable!(),
        }
    }

    #[tokio::test]
    async fn chat_is_broadcast_to_every_participant() {
        let server = server::listen(0).await.unwrap();
        let (alice, mut alice_rx) = connect_to(&server, "alice").await;
        let (_bob, mut bob_rx) = connect_to(&server, "bob").await;

        alice
            .send(ClientMessage::Chat {
                content: "hello world".to_string(),
            })
            .unwrap();

        for rx in [&mut alice_rx, &mut bob_rx] {
            let event = wait_for(rx, |event| {
                matches!(
                    event,
                    AppEvent::Server(ServerMessage::Chat { content, .. }) if content == "hello world"
                )
            })
            .await;

            match event {
                AppEvent::Server(ServerMessage::Chat { nickname, .. }) => {
                    assert_eq!(nickname, "alice");
                }
                _ => unreachable!(),
            }
        }
    }

    #[tokio::test]
    async fn duplicate_nicknames_are_rejected() {
        let server = server::listen(0).await.unwrap();
        let (_alice, _alice_rx) = connect_to(&server, "alice").await;

        let (tx, mut rx) = mpsc::unbounded_channel();
        let _duplicate = client::connect(&target(&server), "alice".to_string(), tx)
            .await
            .unwrap();

        let event = wait_for(&mut rx, |event| {
            matches!(event, AppEvent::Server(ServerMessage::Error { .. }))
        })
        .await;

        match event {
            AppEvent::Server(ServerMessage::Error { message }) => {
                assert!(message.contains("already taken"), "unexpected: {message}");
            }
            _ => unreachable!(),
        }
    }

    #[tokio::test]
    async fn leaving_is_announced() {
        let server = server::listen(0).await.unwrap();
        let (alice, mut alice_rx) = connect_to(&server, "alice").await;
        let (bob, _bob_rx) = connect_to(&server, "bob").await;

        wait_for(&mut alice_rx, |event| {
            matches!(
                event,
                AppEvent::Server(ServerMessage::Joined { nickname }) if nickname == "bob"
            )
        })
        .await;

        drop(bob);

        wait_for(&mut alice_rx, |event| {
            matches!(
                event,
                AppEvent::Server(ServerMessage::Left { nickname }) if nickname == "bob"
            )
        })
        .await;

        drop(alice);
    }

    #[tokio::test]
    async fn shutdown_disconnects_clients() {
        let server = server::listen(0).await.unwrap();
        let (_alice, mut alice_rx) = connect_to(&server, "alice").await;

        server.shutdown();

        wait_for(&mut alice_rx, |event| {
            matches!(event, AppEvent::Disconnected { .. })
        })
        .await;
    }

    #[tokio::test]
    async fn nicknames_with_control_characters_are_rejected() {
        let server = server::listen(0).await.unwrap();
        let (tx, mut rx) = mpsc::unbounded_channel();
        let _client = client::connect(&target(&server), "bad\u{1b}]0;x".to_string(), tx)
            .await
            .unwrap();

        let event = wait_for(&mut rx, |event| {
            matches!(event, AppEvent::Server(ServerMessage::Error { .. }))
        })
        .await;

        match event {
            AppEvent::Server(ServerMessage::Error { message }) => {
                assert!(
                    message.contains("invalid characters"),
                    "unexpected: {message}"
                );
            }
            _ => unreachable!(),
        }
    }

    #[tokio::test]
    async fn tls_server_accepts_loopback_client() {
        let (cert, key) = self_signed_cert();
        let server = server::listen_tls(0, server::TlsConfig::new(cert, key))
            .await
            .unwrap();
        assert!(server.tls);

        let target = format!("wss://127.0.0.1:{}", server.addr.port());
        let (tx, mut rx) = mpsc::unbounded_channel();
        let alice = client::connect_untrusted(&target, "alice".to_string(), tx)
            .await
            .expect("client should connect over TLS");

        wait_for(&mut rx, |event| {
            matches!(event, AppEvent::Server(ServerMessage::Welcome { .. }))
        })
        .await;

        alice
            .send(ClientMessage::Chat {
                content: "over tls".to_string(),
            })
            .unwrap();

        wait_for(&mut rx, |event| {
            matches!(
                event,
                AppEvent::Server(ServerMessage::Chat { content, .. }) if content == "over tls"
            )
        })
        .await;
    }
}
