use ws::{connect, Sender, Handler, Handshake, Message, Result};

use std::thread;
use client::client_manager::ClientManager;
use std::collections::HashMap;

use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

pub struct ClientManagerImpl {
    connections: Arc<Mutex<HashMap<String, Sender>>>
}

pub trait ClientManagerFactory {
    fn new() -> ClientManagerImpl;
}

impl ClientManagerFactory for ClientManagerImpl {
    fn new() -> ClientManagerImpl {
        ClientManagerImpl {
            connections: Arc::new(Mutex::new(HashMap::new()))
        }
    }
}

struct Client<'a> {
    out: Sender,
    url: String,
    connections_map: &'a Arc<Mutex<HashMap<String, Sender>>>
}

impl<'a> Handler for Client<'a> {

    fn on_open(&mut self, _: Handshake) -> Result<()> {
        self.connections_map.lock().unwrap().insert(self.url.clone(), self.out.clone());
        self.out.send("Hello WebSocket")
    }

    fn on_message(&mut self, msg: Message) -> Result<()> {
        println!("Got message: {}", msg);
        Ok(())
    }

    // TO-DO handle on close
}

impl ClientManagerImpl {

    pub fn get_connections_map(&mut self) -> Arc<Mutex<HashMap<String, Sender>>> {
        self.connections.clone()
    }
}

impl ClientManager for ClientManagerImpl {

    fn join_server(&mut self, username:String, sock_addr:SocketAddr) {
        println!("Joining server: {} as {}", sock_addr, username);
        let url = "ws://".to_owned() + &sock_addr.to_string();

        let connections_map = self.connections.clone();

        thread::spawn(move || connect(url, |out| {
            let c = &connections_map;
            Client { out: out, url: sock_addr.to_string(), connections_map: c }
        }).unwrap());
    }
}
