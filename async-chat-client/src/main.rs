use clap::Parser;
use mio::net::TcpStream;
use mio::unix::SourceFd; // For handling `Stdin` on Unix-like systems
use mio::{Events, Interest, Poll, Token};
use std::env;
use std::io::{self, Read, Write};
use std::net::SocketAddr;
use std::os::unix::io::AsRawFd;

/// Command-line argument struct for configuring the chat application.
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

// Constants for the server and stdin events.
const SERVER: Token = Token(0);
const STDIN: Token = Token(1);

/// Entry point of the chat application. Manages connection and polling of events.
fn main() -> io::Result<()> {
    // Parse the command-line arguments
    let args = Args::parse();

    let host = env::var("HOST").unwrap_or(args.host);
    let port = env::var("PORT").unwrap_or(args.port);
    let username = env::var("USERNAME").unwrap_or(args.username);

    // Create a stream socket and initiate a connection
    let address = format!("{host}:{port}");
    let username = format!("{username}\n");
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

    // Register `Stdin` as a source for polling
    poll.registry()
        .register(&mut SourceFd(&stdin_fd), STDIN, Interest::READABLE)?;

    const BUF_SIZE: usize = 512;
    let mut input_buffer = Vec::new();
    let mut server_buffer = [0; BUF_SIZE];
    let mut bytes_to_send;
    let mut bytes_written = 0;
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
                                eprintln!("Error reading from server: {e}");
                                return Err(e);
                            }
                        }
                    }

                    if event.is_writable() {
                        if !username_sent {
                            input_buffer.extend_from_slice(username.as_bytes());
                            // In this simple chat app, we assume the username is short and will be sent in a single write.
                            // Note: This assumption may not hold in all cases, as `stream.write` does NOT guarantee that
                            // the entire buffer will be written at once. According to the documentation, we should loop
                            // until either a `WouldBlock` error occurs or the entire data buffer is sent.
                            let _ = stream.write(&input_buffer.as_slice());
                            username_sent = true;
                        }
                    }
                }

                STDIN => {
                    // Handle input from `Stdin`
                    let mut input = String::new();
                    stdin.read_line(&mut input).expect("Failed to read input");
                    input = input.trim().to_string();

                    if let Some(stripped) = input.strip_prefix("send ") {
                        let message = format!("{stripped}\n");
                        let msg_len = message.len();
                        input_buffer.clear();
                        input_buffer.extend_from_slice(message.as_bytes());
                        bytes_to_send = msg_len;
                        // If we receive a write readiness event but skip writing due to `!input_buffer.is_empty()`
                        // or an incomplete `input_buffer.extend_from_slice(message.as_bytes())` call, the code may
                        // not write to the stream as expected since we may miss the SERVER token.

                        // To handle this, we write to the stream as soon as user input is received from stdin.
                        // Note: there are more robust solutions for handling this, but for a basic chat app,
                        // this approach should be sufficient while maintaining asynchronous behavior.
                        match stream.write(&input_buffer[bytes_written..bytes_to_send]) {
                            // Continue writing until we hit a `WouldBlock`
                            Ok(n) if n < bytes_to_send => {
                                bytes_written += n;
                                continue;
                            }
                            // Our data buffer has been exhausted i.e. we have sent everything we need to
                            Ok(_v) => {
                                input_buffer.clear();
                                break;
                            }
                            // Encountered a `WouldBlock`, stop and poll again for readiness
                            Err(ref err) if would_block(err) => {
                                println!("{}", io::ErrorKind::WouldBlock);
                                break;
                            }
                            Err(e) => {
                                eprintln!("Error writing to server: {e}");
                                return Err(e);
                            }
                        }
                    } else if input == "leave" {
                        println!("Disconnecting...");
                        return Ok(());
                    } else {
                        println!("Invalid command. Use 'send <MSG>' or 'leave'");
                    }
                }

                _token => {
                    println!("Got a spurious event!")
                }
            }
        }
    }
}

fn would_block(err: &io::Error) -> bool {
    err.kind() == io::ErrorKind::WouldBlock
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn test_args_parsing() {
        // Arrange: create a sample set of arguments
        let args = Args::parse_from(&[
            "test",
            "--host",
            "192.168.0.1",
            "--port",
            "9000",
            "--username",
            "testuser",
        ]);

        // Assert: verify the parsed values match expected inputs
        assert_eq!(args.host, "192.168.0.1");
        assert_eq!(args.port, "9000");
        assert_eq!(args.username, "testuser");
    }

    #[test]
    fn test_username_initialization() {
        // Arrange: simulate username setup
        let username = "testuser\n";
        let mut input_buffer = Vec::new();

        // Act: extend input_buffer with the username bytes
        input_buffer.extend_from_slice(username.as_bytes());

        // Assert: check that the input buffer has the username content
        assert_eq!(
            String::from_utf8(input_buffer.clone()).unwrap(),
            "testuser\n"
        );
    }
}
