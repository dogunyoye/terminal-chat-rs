use ws::{listen, CloseCode, Sender, Handler, Handshake, Message};

use ws::Result as WSResult;

use std::thread;
use std::thread::JoinHandle;
use std::collections::HashMap;
use server::server_manager::ServerManager;

use std::result::Result;

struct Server {
    out: Sender,
    rooms: HashMap<String, Vec<Sender>>
}

impl Handler for Server {

    fn on_open(&mut self, _: Handshake) -> WSResult<()> {
        println!("token in open {:?}", self.out.token());
        Ok(())
    }

    fn on_message(&mut self, msg: Message) -> WSResult<()> {
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

        let mut rooms : HashMap<String, Vec<Sender>> = HashMap::new();
        let default_list : Vec<Sender> = Vec::new();

        rooms.insert("default".to_string(), default_list);

        let join_handle: JoinHandle<_> =
            thread::spawn(move || listen(sock_addr, |out|
                Server { out, rooms: rooms.clone()}
        ).unwrap());

        if !self.servers.contains_key(&key) {
            self.servers.insert(key, join_handle);
        }

    }

    fn remove_server(&mut self, port:u32) -> Result<String, String> {
        println!("Removing server on port {}", port);

        let sock_addr = "127.0.0.1:".to_owned() + &port.to_string();
        let value: Option<JoinHandle<()>> = self.servers.remove(&sock_addr);

        if value.is_none() {
            let error_messsage = format!("Server on port {} does not exist", port);
            return Err(error_messsage);
        }

        let thread_handle = value.unwrap();
        let join_result = thread_handle.join();

        if join_result.is_ok() {
            let ok_message = format!("Server on port {} removed", port);
            return Ok(ok_message);
        }

        return Err("Couldn't join on the associated thread".to_string());
    }
}
