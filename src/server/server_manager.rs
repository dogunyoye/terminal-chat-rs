pub trait ServerManager {
    /**
     * Add a websocket server
     **/
    fn add_server(&mut self, port:u32);

    /**
     * Remove a websocket server
     **/
    fn remove_server(&mut self, port:u32);
}
