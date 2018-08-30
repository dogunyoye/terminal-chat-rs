use std::net::SocketAddr;

pub trait ClientManager {

    /**
     * Connect to a websocket server
     **/
    fn join_server(&mut self, username:String, sock_addr:SocketAddr);
}
