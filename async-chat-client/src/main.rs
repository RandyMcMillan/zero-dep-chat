use clap::Parser;
use mio::net::TcpStream;
use mio::unix::SourceFd; // For handling STDIN on Unix-like systems
use mio::{Events, Interest, Poll, Token};
use std::env;
use std::io::{self, Read, Write};
use std::net::SocketAddr;
use std::os::unix::io::AsRawFd;
// use std::time::Duration; // Unix-specific raw file descriptor support

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

    // Create a stream socket and initiate a connection
    let address = format!("{}:{}", host, port);
    let username = format!("{}\n", username);
    let server_address: SocketAddr = address.parse().unwrap();
    let mut stream = TcpStream::connect(server_address)?;
    println!("Connecting to server at {} as {}", &address, &username);

    // We'll need the raw file descriptor for the standard input stream
    let stdin = io::stdin();
    let stdin_fd = stdin.as_raw_fd();

    // Set up polling to handle both stdin and the TCP stream
    let mut poll = Poll::new()?;
    let mut events = Events::with_capacity(128);

    // Register the connection with the Poll instance
    poll.registry()
        .register(&mut stream, SERVER, Interest::READABLE | Interest::WRITABLE)?;

    // Register STDIN as a source for polling
    poll.registry()
        .register(&mut SourceFd(&stdin_fd), STDIN, Interest::READABLE)?;

    const BUF_SIZE: usize = 512;
    let mut input_buffer = Vec::new();
    let mut server_buffer = [0; BUF_SIZE];
    let mut bytes_to_send = 0;
    let mut username_sent = false;

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
                                println!("{}", msg.trim());
                            }
                            Err(ref err) if would_block(err) => {}
                            Err(e) => {
                                eprintln!("Error reading from server: {}", e);
                                return Err(e);
                            }
                        }
                    }

                    let mut bytes_written = 0;
                    if event.is_writable() {
                        println!("event is writable: {}", event.is_writable());
                        if !username_sent {
                            let username_len = username.len();
                            bytes_to_send = username_len;
                            input_buffer.extend_from_slice(username.as_bytes());
                            println!("username copied to input buffer");
                            username_sent = true;
                        }
                        if !input_buffer.is_empty() {
                            loop {
                                match stream.write(&input_buffer[bytes_written..bytes_to_send]) {
                                    Ok(n) if n < bytes_to_send => {
                                        println!("bytes written: {}", n);
                                        bytes_written += n;
                                        continue;
                                    }
                                    Ok(v) => {
                                        input_buffer.clear();
                                        println!("Write Ok: {}", v);
                                        break;
                                    }
                                    Err(ref err) if would_block(err) => {
                                        println!("{}", io::ErrorKind::WouldBlock);
                                        break;
                                    }
                                    Err(e) => {
                                        eprintln!("Error writing to server: {}", e);
                                        return Err(e);
                                    }
                                }
                            }
                        }
                    }
                }

                STDIN => {
                    // Handle input from STDIN
                    let mut input = String::new();
                    stdin.read_line(&mut input).expect("Failed to read input");
                    input = input.trim().to_string();

                    if input.starts_with("send ") {
                        let message = format!("{}\n", &input[5..]);
                        println!("message: {}", message);
                        let msg_len = message.len();
                        input_buffer.clear();
                        input_buffer.extend_from_slice(message.as_bytes());
                        bytes_to_send = msg_len;
                        poll.registry().reregister(
                            &mut stream,
                            SERVER,
                            Interest::READABLE | Interest::WRITABLE,
                        )?;
                        // ready_to_send = true;
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

fn would_block(err: &io::Error) -> bool {
    err.kind() == io::ErrorKind::WouldBlock
}
