use clap::Parser;
use mio::net::TcpStream;
use mio::unix::SourceFd; // For handling STDIN on Unix-like systems
use mio::{Events, Interest, Poll, Token};
use std::env;
use std::io::{self, Read, Write};
use std::net::SocketAddr;
use std::os::unix::io::AsRawFd; // Unix-specific raw file descriptor support

/// Struct for command-line argument parsing using `clap`.
#[derive(Parser)]
struct Args {
    /// The host of the server (default: 127.0.0.1)
    #[arg(long, default_value = "127.0.0.1")]
    host: String,

    /// The port of the server (default: 8080)
    #[arg(short, long, default_value = "12345")]
    port: String,

    /// The username used for identification
    #[arg(short, long)]
    username: String,
}

const SERVER: Token = Token(0);
const STDIN: Token = Token(1);

fn main() -> io::Result<()> {
    // Parse the command-line arguments
    let args = Args::parse();

    let host = env::var("HOST").unwrap_or(args.host);
    let port = env::var("PORT").unwrap_or(args.port);
    let username = env::var("USERNAME").unwrap_or(args.username);

    // Create a client socket and initiate a connection
    let address = format!("{}:{}", host, port);
    let server_address: SocketAddr = address.parse().unwrap();
    let mut stream = TcpStream::connect(server_address)?;
    writeln!(&mut stream, "{}", username).expect("Failed to write");
    println!("Connected to the server at {} as {}", &address, &username);

    // Get a handle to the client's standard input stream 
    let stdin = io::stdin();
    let stdin_fd = stdin.as_raw_fd(); // Get raw file descriptor

    // Set up polling to handle both stdin and the TCP stream
    let mut poll = Poll::new()?;
    let mut events = Events::with_capacity(128);

    // Register the server connection with the Poll instance
    poll.registry()
        .register(&mut stream, SERVER, Interest::READABLE | Interest::WRITABLE)?;

    // Register STDIN as a source for polling
    poll.registry()
        .register(&mut SourceFd(&stdin_fd), STDIN, Interest::READABLE)?;

    let mut input_buffer = Vec::new();
    let mut server_buffer = [0; 512];

    // Main event loop
    loop {
        poll.poll(&mut events, None)?;

        for event in events.iter() {
            match event.token() {
                SERVER => {
                    if event.is_readable() {
                        match stream.read(&mut server_buffer) {
                            Ok(0) => {
                                println!("Connection closed by server.");
                                return Ok(());
                            }
                            Ok(n) => {
                                let msg = String::from_utf8_lossy(&server_buffer[..n]);
                                println!("Server: {}", msg);
                            }
                            Err(e) => {
                                eprintln!("Error reading from server: {}", e);
                                return Err(e);
                            }
                        }
                    }

                    if event.is_writable() && !input_buffer.is_empty() {
                        match stream.write(&input_buffer) {
                            Ok(n) => {
                                input_buffer.drain(..n);
                            }
                            Err(e) => {
                                eprintln!("Error writing to server: {}", e);
                                return Err(e);
                            }
                        }
                    }
                }

                STDIN => {
                    // Handle input from STDIN
                    // println!("in STDIN input");
                    let mut input = String::new();
                    stdin
                        .read_line(&mut input)
                        .expect("Failed to read input");
                    input = input.trim().to_string();
                    // println!("in STDIN again");
                    if input.starts_with("send ") {
                        let message = format!("[{}]: {}", username, &input[5..]);
                        input_buffer.extend_from_slice(message.as_bytes());
                        // writeln!(&mut stream, "{}", message).expect("Failed to write");
                        // println!("msg: {} input buffer: {:?}", &message, &input_buffer);
                    } else if input == "leave" {
                        println!("Disconnecting...");
                        return Ok(());
                    } else {
                        println!("Invalid command. Use 'send <MSG>' or 'leave'");
                    }
                }

                _ => unreachable!(),
            }
        }
    }
}
