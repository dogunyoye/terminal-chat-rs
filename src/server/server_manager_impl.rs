extern crate ws;

use std::collections::HashMap;
use std::collections::HashSet;
use server::server_manager::ServerManager;

pub struct ServerManagerImpl {
    pub servers: HashMap<u32, HashSet<String>>
}

impl ServerManager for ServerManagerImpl {

    fn add_server(&self, port:u32) {
        println!("Adding server on port {}", port);
    }

    fn remove_server(&self, port:u32){
        println!("Removing server on port {}", port);
    }
}
