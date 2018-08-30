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
        // TO-DO - Implement me
    }
}

impl ClientManagerFactory for ClientManagerImpl {

    fn new() -> ClientManagerImpl {
        ClientManagerImpl {}
    }
}