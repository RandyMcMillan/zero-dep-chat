use std::env;
use std::io::{self, BufRead, BufReader, Write};
use std::net::TcpStream;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

// static SERVER_DISCONNECTED: OnceLock<bool> = OnceLock::new();

/// Function to display the client prompt and handle user input
fn client_prompt(stream: Arc<Mutex<TcpStream>>) {
    let stdin = io::stdin();
    let input = stdin.lock();
    let mut lines = input.lines();

    loop {
        match lines.next() {
            // Reading from `Stdin` is a blocking operation.
            Some(Ok(line)) => {
                if let Some(stripped) = line.strip_prefix("send ") {
                    let msg = stripped.to_string();
                    // let formatted_msg = format!("{}", msg);
                    let mut stream = stream.lock().unwrap();
                    writeln!(stream, "{}", msg).expect("Failed to send message");
                } else if line.trim() == "leave" {
                    println!("Disconnecting from the server...");
                    break;
                } else {
                    println!("Invalid command. Use 'send <MSG>' or 'leave'");
                }
            }
            Some(Err(e)) => {
                println!("Read error: {}", e);
            }
            None => {
                // if *SERVER_DISCONNECTED.get().unwrap_or(&false) {
                //     println!("Disconnecting from the server...");
                //     break;
                // }
            }
        }
    }
}

/// Function to listen for messages from the server
fn listen_for_messages(stream: Arc<Mutex<TcpStream>>, first_connect: bool) {
    thread::spawn(move || {
        let mut reader = BufReader::new(
            stream
                .lock()
                .unwrap()
                .try_clone()
                .expect("Failed to clone stream"),
        );

        loop {
            let mut buffer = String::new();
            match reader.read_line(&mut buffer) {
                Ok(0) => {
                    println!("Server disconnected.");
                    // SERVER_DISCONNECTED.get_or_init(|| true);
                    break;
                }
                Ok(_) => {
                    println!("{}", buffer.trim());
                }
                Err(e) => {
                    if first_connect {
                    } else {
                        eprintln!("Error reading from server: {}", e);
                        break;
                    }
                }
            }
            thread::sleep(Duration::from_millis(100));
        }
    });
}

fn main() {
    // Get the host, port, and username from environment variables or command-line arguments
    let env_host = env::var("CHAT_HOST").unwrap_or("127.0.0.1".to_string());
    let env_chat_port = env::var("CHAT_PORT").unwrap_or("12345".to_string());

    let args: Vec<String> = env::args().collect();
    let (host, port, username) = match args.len() {
        4 => {
            if args[3].is_empty() {
                panic!("No username provided!")
            }
            (&args[1], &args[2], &args[3])
        }
        3 => (
            &args[1],
            &args[2],
            &env::var("CHAT_USERNAME").expect("No username provided!"),
        ),
        2 => (&args[1], &env_chat_port, &env_host),
        1 => (
            &env_host,
            &env_chat_port,
            &env::var("CHAT_USERNAME").expect("No username provided!"),
        ), // Only program name, fallback to env vars
        _ => panic!("Too many arguments!"), // Handle cases where there are too many arguments
    };

    // Attempt to connect to the chat server
    let address = format!("{}:{}", host, port);
    let mut stream = TcpStream::connect(&address).expect("Failed to connect to server");

    writeln!(&mut stream, "{}", username).expect("Failed to write");

    // Set the stream to non-blocking mode
    stream
        .set_nonblocking(true)
        .expect("Failed to set non-blocking");

    let stream = Arc::new(Mutex::new(stream));
    println!("Connected to the server at {} as {}", address, username);

    // Spawn a thread to listen for messages from the server
    listen_for_messages(stream.clone(), true);

    // Display client prompt and handle user input
    client_prompt(stream);

    // Gracefully exit
    println!("Client disconnected.");
}
