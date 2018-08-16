pub trait ServerManager {
    /**
     * Add a websocket server
     **/
    fn add_server(&self, port:u32);

    /**
     * Remove a websocket server
     **/
    fn remove_server(&self, port:u32);
}
