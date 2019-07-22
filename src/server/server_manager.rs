use std::result::Result;

pub trait ServerManager {
    /**
     * Add a websocket server
     **/
    fn add_server(&mut self, port:u32) -> Result<String, String>;

    /**
     * Remove a websocket server
     **/
    fn remove_server(&mut self, port:u32) -> Result<String, String>;
}
