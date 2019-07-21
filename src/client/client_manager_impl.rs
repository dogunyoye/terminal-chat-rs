use ws::{connect, Sender, Handler, Handshake, Message, Result};

use std::io;
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
    username: String,
    url: String,
    connections_map: &'a Arc<Mutex<HashMap<String, Sender>>>
}

impl<'a> Handler for Client<'a> {

    fn on_open(&mut self, _: Handshake) -> Result<()> {
        self.connections_map.lock().unwrap().insert(self.url.clone(), self.out.clone());
        let message_prefix = self.username.to_string() + "@" + &self.url.to_string() + ": ";

        let client = self.out.clone();

        thread::spawn(move || {
            loop {
                let mut message = String::new();
                io::stdin().read_line(&mut message).expect("Failed to message");
                let formatted_message = message_prefix.clone() + message.trim();
                if !client.send(formatted_message).is_ok() {
                    println!("Failed to send message: '{}'", message.trim());
                }
            }
        });

        Ok(())
    }

    fn on_message(&mut self, msg: Message) -> Result<()> {
        println!("{}", msg);
        Ok(())
    }

    // TO-DO handle on close
}

impl ClientManager for ClientManagerImpl {

    fn join_server(&mut self, username:String, sock_addr:SocketAddr) {
        println!("Joining server: {} as {}", sock_addr, username);
        let url = "ws://".to_owned() + &sock_addr.to_string();

        let connections_map = self.connections.clone();

        thread::spawn(move || connect(url, |out| {
            let c = &connections_map;
            Client { out: out, username: username.clone(), url: sock_addr.to_string(), connections_map: c }
        }).unwrap()).join().unwrap();
    }
}
