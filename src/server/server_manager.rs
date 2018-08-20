use std::result::Result;
use std::net::SocketAddr;

pub trait ServerManager {
    /**
     * Add a websocket server
     **/
    fn add_server(&mut self, port:u32);

    /**
     * Remove a websocket server
     **/
    fn remove_server(&mut self, port:u32) -> Result<String, String>;

    /**
     * Connect to a websocket server
     **/
    fn join_server(&mut self, sock_addr:SocketAddr);
}
