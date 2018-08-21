extern crate ws;

mod server;

use std::process::Command;
use std::io;
use std::net::SocketAddr;

use server::server_manager::ServerManager;
use server::server_manager_impl::ServerManagerFactory;
use server::server_manager_impl::ServerManagerImpl;

fn handle_add_server(mut server_manager: ServerManagerImpl) {
    println!("Enter port to bind to: ");

    let mut port = String::new();

    io::stdin().read_line(&mut port)
        .expect("Failed to read line");

    let port_num: u32 = match port.trim().parse() {
        Ok(num) => num,
        Err(_) => {
            println!("Unable to parse port number");
            return
        },
    };

    server_manager.add_server(port_num);
}

fn handle_remove_server(mut server_manager: ServerManagerImpl) {
    println!("Enter port to remove: ");

    let mut port = String::new();

    io::stdin().read_line(&mut port)
        .expect("Failed to read line");

    let port_num: u32 = match port.trim().parse() {
        Ok(num) => num,
        Err(_) => {
            println!("Unable to parse port number");
            return
        }
    };

    let remove_result = server_manager.remove_server(port_num);

    if remove_result.is_ok() {
        println!("{}", remove_result.unwrap());
        return;
    }

    println!("{}", remove_result.unwrap_err());
}

fn handle_join_server(mut server_manager: ServerManagerImpl) {
    println!("Enter server address (i.e 127.0.0.1:8080): ");

    let mut addr = String::new();

    io::stdin().read_line(&mut addr)
        .expect("Failed to read line");

    let result = addr.trim().parse::<SocketAddr>();

    if result.is_ok() {
        let sock_addr = result.unwrap();
        server_manager.join_server(sock_addr);
        return;
    }

    println!("Error: {}", result.unwrap_err());
}

fn main() {

    let output = Command::new("clear").output().unwrap_or_else(|e| {
        panic!("failed to execute process: {}", e)
    });

    println!("{}", String::from_utf8_lossy(&output.stdout));

    let server_manager = ServerManagerImpl::new();

    let row = "*".repeat(38);
    let welcome_line = "*    Welcome to terminal-chat-rs!    *";
    let author_line = "*    @author Dimeji Ogunyoye         *";

    let create_server_option = "[1] Create server";
    let remove_server_option = "[2] Remove server";
    let join_server_option = "[3] Join server";

    println!("{}", row);
    println!("{}", welcome_line);
    println!("{}", author_line);
    println!("{}\n", row);
    println!("{}", create_server_option);
    println!("{}", remove_server_option);
    println!("{}\n", join_server_option);

    println!("Please select an option: ");

    let mut option = String::new();

    io::stdin().read_line(&mut option)
        .expect("Failed to read line");

    let option_num: u32 = match option.trim().parse() {
        Ok(num) => num,
        Err(_) => {
            println!("Unable to parse option number");
            return
        },
    };

    match option_num {
        1 => {
            println!("Creating server...");
            handle_add_server(server_manager);
        },
        2 => {
            println!("Removing server...");
            handle_remove_server(server_manager);
        },
        3 => {
            println!("Joining server...");
            handle_join_server(server_manager);
        },
        _ => println!("Unknown option")
    }
}
