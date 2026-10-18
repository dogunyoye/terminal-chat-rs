pub mod client;
pub mod server;

pub use client::ClientHandle;
pub use server::ServerHandle;

#[cfg(test)]
mod tests {
    use std::net::SocketAddr;
    use std::time::Duration;

    use tokio::sync::mpsc;

    use super::*;
    use crate::event::AppEvent;
    use crate::protocol::{ClientMessage, ServerMessage};

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
        let addr = SocketAddr::from(([127, 0, 0, 1], server.addr.port()));
        let (tx, mut rx) = mpsc::unbounded_channel();

        let client = client::connect(addr, nickname.to_string(), tx)
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
        let addr = SocketAddr::from(([127, 0, 0, 1], server.addr.port()));
        let (tx, mut rx) = mpsc::unbounded_channel();
        let _alice = client::connect(addr, "alice".to_string(), tx)
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

        let addr = SocketAddr::from(([127, 0, 0, 1], server.addr.port()));
        let (tx, mut rx) = mpsc::unbounded_channel();
        let _duplicate = client::connect(addr, "alice".to_string(), tx)
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
}
