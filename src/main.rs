mod server;

use std::process::Command;
use std::io;

use server::server_manager::ServerManager;
use server::server_manager_impl::ServerManagerImpl;

use std::collections::HashMap;

fn main() {

    let output = Command::new("clear").output().unwrap_or_else(|e| {
        panic!("failed to execute process: {}", e)
    });

    println!("{}", String::from_utf8_lossy(&output.stdout));

    let server_manager = ServerManagerImpl {
        servers: HashMap::new()
    };

    let row = "*".repeat(38);
    let welcome_line = "*    Welcome to terminal-chat-rs!    *";
    let author_line = "*    @author Dimeji Ogunyoye         *";

    let create_server_option = "[1] Create server";
    let remove_server_option = "[2] Remove server";
    let join_room_option = "[3] Join room";

    println!("{}", row);
    println!("{}", welcome_line);
    println!("{}", author_line);
    println!("{}\n", row);
    println!("{}", create_server_option);
    println!("{}", remove_server_option);
    println!("{}\n", join_room_option);

    println!("Please select an option: ");

    let mut option = String::new();

    io::stdin().read_line(&mut option)
        .expect("Failed to read line");

    let option_num: u32 = match option.trim().parse() {
        Ok(num) => num,
        Err(_) => {
            println!("Unable to parse number");
            return
        },
    };

    match option_num {
        1 => {
            println!("Creating server...");
            server_manager.add_server(8080);
        },
        2 => println!("Removing server..."),
        3 => println!("Joining room..."),
        _ => println!("Unknown option")
    }
}
