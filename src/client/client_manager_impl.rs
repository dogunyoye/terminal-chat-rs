use ws::{connect};
use client::client_manager::ClientManager;

use std::net::SocketAddr;

pub struct ClientManagerImpl {
    // TO-DO - Implement me
}

pub trait ClientManagerFactory {
    fn new() -> ClientManagerImpl;
}

impl ClientManager for ClientManagerImpl {

    fn join_server(&mut self, username:String, sock_addr:SocketAddr) {
        println!("Joining server: {} as {}", sock_addr, username);
        let url = "ws://".to_owned() + &sock_addr.to_string();
        connect(url, |out| {

            move |msg| {
                Ok(())
            }
        }).unwrap();
    }
}

impl ClientManagerFactory for ClientManagerImpl {

    fn new() -> ClientManagerImpl {
        ClientManagerImpl {}
    }
}