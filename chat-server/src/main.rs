use std::collections::{HashMap, HashSet};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;

/// Type alias for the list of users connected to the chat server
type UserList = Arc<Mutex<HashMap<Arc<String>, TcpStream>>>;

/// Handles a connected client.
/// 
/// This function processes messages sent by the client and broadcasts them to
/// other connected users. It also removes the user from the list when they leave.
fn handle_client(stream: TcpStream, username: Arc<String>, user_list: UserList) {
    let reader = BufReader::new(stream);
    for line in reader.lines() {
        let message = match line {
            Ok(msg) => msg,
            Err(e) => e.to_string(),
        };
        if message == "/leave" {
            break;
        }
        // Broadcast message to everyone in the user_list, except the sender
        let mut user_list = user_list.lock().unwrap();
        for (user, user_stream) in user_list.iter_mut() {
            if user != &username {
                writeln!(user_stream, "[{}]: {}", username, message)
                    .expect("Failed to send message");
            }
        }
    }

    // Cleanup after user leaves
    user_list.lock().unwrap().remove(&username);
    println!("User {} has left", username);
}

/// Main function that initializes the server and listens for incoming connections.
/// The server waits for a username from the client, verifies its uniqueness, and then
/// allows the user to join the chat room.
fn main() {
    let listener = TcpListener::bind("0.0.0.0:12345").expect("Failed to bind");
    let user_list = Arc::new(Mutex::new(HashMap::new()));
    let mut active_usernames = HashSet::new();
    let mut stream;

    for s in listener.incoming() {
        match s {
            Ok(s) => {
                println!("Received a connection from: {:?}", s.peer_addr().unwrap());
                stream = s
            }
            Err(e) => {
                println!("Failed to accept new connection: {}", e.to_string());
                continue;
            }
        };

        // Get a unique username from the client.
        let mut buffer = [0; 512];
        let mut username = String::new();
        loop {
            // writeln!(&mut stream, "Please enter a valid username").expect("Failed to write");
            let bytes_read = stream.read(&mut buffer).expect("Failed to read username");
            username.push_str(
                &String::from_utf8_lossy(&buffer[..bytes_read])
                    .trim()
                    .to_string(),
            );

            // Ensure the username is unique
            if active_usernames.contains(&username) || username.contains("/leave") {
                writeln!(&mut stream, "Username is already taken").expect("Failed to write");
                continue;
            }
            break;
        }

        // Arc avoids unecessary `String` allocations
        let usr = Arc::new(username);

        // Register user
        println!("User {} has joined", usr.as_str());
        active_usernames.insert(usr.clone());
        user_list
            .lock()
            .unwrap()
            .insert(usr.clone(), stream.try_clone().expect("Failed to clone"));

        // Spawn a new thread to handle this client's connection
        let user_list_clone = Arc::clone(&user_list);
        thread::spawn(move || {
            handle_client(stream, usr, user_list_clone);
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn test_username_uniqueness() {
        let mut users = HashSet::new();
        let username = "user1".to_string();
        assert!(users.insert(username.clone()));
        assert!(!users.insert(username));
    }

    #[test]
    fn test_handle_client_broadcasts() {
        let mut buffer = [0; 512];
        let message = "Hello".as_bytes();
        let mut stream1 = TcpStream::connect("127.0.0.1:12345").unwrap();
        let mut stream2 = TcpStream::connect("127.0.0.1:12345").unwrap();

        stream1.write(message).unwrap();
        let bytes_read = stream2.read(&mut buffer).unwrap();

        assert_eq!(&buffer[..bytes_read], message);
    }

    // Additional unit tests for other functionality can be added here.
}