use ws::{listen, CloseCode, Sender, Handler, Message, Result};

use std::thread;
use std::thread::JoinHandle;
use std::collections::HashMap;
use std::collections::HashSet;
use server::server_manager::ServerManager;

struct Server {
    out: Sender
    // TO-DO add room management data structure
    // for server struct.
}

impl Handler for Server {

    fn on_message(&mut self, msg: Message) -> Result<()> {
        println!("Server got message '{}'. ", msg);
        self.out.send(msg)
    }

    fn on_close(&mut self, code: CloseCode, reason: &str) {
        println!("WebSocket closing for ({:?}) {}", code, reason);
        println!("Shutting down server after first connection closes.");
        self.out.shutdown().unwrap();
    }
}

pub struct ServerManagerImpl {
    servers: HashMap<String, JoinHandle<()>>
}

pub trait ServerManagerFactory {
    fn new() -> ServerManagerImpl;
}

impl ServerManagerFactory for ServerManagerImpl {
    fn new() -> ServerManagerImpl {
        let servers = HashMap::new();
        ServerManagerImpl {
            servers
        }
    }
}

impl ServerManager for ServerManagerImpl {

    fn add_server(&mut self, port:u32) {
        println!("Adding server on port {}", port);
        let sock_addr = "127.0.0.1:".to_owned() + &port.to_string();
        let key = sock_addr.clone();

        let join_handle: JoinHandle<_> = thread::spawn(move || listen(sock_addr, |out|
            Server { out }
        ).unwrap());

        if !self.servers.contains_key(&key) {
            println!("inserted!");
            self.servers.insert(key, join_handle);
        }

    }

    fn remove_server(&mut self, port:u32) {
        println!("Removing server on port {}", port);
    }
}
