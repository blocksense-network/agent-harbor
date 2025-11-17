// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Command Trace Server — Tokio-based server for command trace protocol
//!
//! This crate provides a simple, high-level API for running a command trace server
//! that communicates with command trace shims over Unix domain sockets using the
//! SSZ-based protocol defined in `ah-command-trace-proto`.
//!
//! # Example
//!
//! ```no_run
//! use ah_command_trace_server::CommandTraceServer;
//! use tempfile::TempDir;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let temp_dir = TempDir::new()?;
//!     let socket_path = temp_dir.path().join("trace.sock");
//!
//!     let mut server = CommandTraceServer::new(socket_path);
//!
//!     // Start the server and handle messages
//!     server.run(|request| async move {
//!         // Handle incoming requests here
//!         match request {
//!             ah_command_trace_proto::Request::Handshake(msg) => {
//!                 Some(ah_command_trace_proto::Response::Handshake(
//!                     ah_command_trace_proto::HandshakeResponse {
//!                         success: true,
//!                         error_message: None,
//!                     }
//!                 ))
//!             }
//!         }
//!     }).await?;
//!
//!     Ok(())
//! }
//! ```

use ah_command_trace_proto::{Request, Response, decode_ssz, encode_ssz};
use futures::future::Future;
use std::path::Path;
use std::pin::Pin;
use thiserror::Error;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::oneshot;

/// Errors that can occur when running the command trace server
#[derive(Error, Debug)]
pub enum ServerError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("SSZ decode error")]
    SszDecode,

    #[error("Server shutdown requested")]
    Shutdown,
}

/// Handler function type for processing incoming requests
///
/// The handler receives a `Request` and returns an optional `Response`.
/// Returning `None` means no response should be sent (useful for one-way messages).
pub type RequestHandler = Box<
    dyn Fn(Request) -> Pin<Box<dyn Future<Output = Option<Response>> + Send>>
        + Send
        + Sync
        + 'static,
>;

/// Command Trace Server for handling shim connections over Unix sockets
pub struct CommandTraceServer {
    socket_path: std::path::PathBuf,
}

impl CommandTraceServer {
    /// Create a new server that will listen on the given socket path
    pub fn new<P: AsRef<Path>>(socket_path: P) -> Self {
        Self {
            socket_path: socket_path.as_ref().to_path_buf(),
        }
    }

    /// Run the server with the given request handler
    ///
    /// This method will bind to the socket, accept connections, and handle
    /// incoming requests using the provided handler function. The server
    /// will continue running until an error occurs or shutdown is requested
    /// via the shutdown receiver.
    pub async fn run<F, Fut>(
        &mut self,
        handler: F,
        mut shutdown: tokio::sync::oneshot::Receiver<()>,
    ) -> Result<(), ServerError>
    where
        F: Fn(Request) -> Fut + Send + Sync + Clone + 'static,
        Fut: Future<Output = Option<Response>> + Send + 'static,
    {
        // Remove existing socket if it exists
        if self.socket_path.exists() {
            std::fs::remove_file(&self.socket_path)?;
        }

        let listener = UnixListener::bind(&self.socket_path)?;
        eprintln!("Command trace server listening on {:?}", self.socket_path);

        loop {
            // Use timeout to make the server cancellable
            match tokio::time::timeout(std::time::Duration::from_millis(100), listener.accept())
                .await
            {
                Ok(Ok((stream, addr))) => {
                    eprintln!("Accepted connection from {:?}", addr);
                    let handler = handler.clone();
                    tokio::spawn(async move {
                        if let Err(e) = Self::handle_connection(stream, handler).await {
                            eprintln!("Error handling connection: {:?}", e);
                        }
                    });
                }
                Ok(Err(e)) => {
                    eprintln!("Accept error: {:?}", e);
                    return Err(ServerError::Io(e));
                }
                Err(_) => {
                    // Check if shutdown was requested
                    match shutdown.try_recv() {
                        Ok(_) => {
                            return Ok(());
                        }
                        Err(tokio::sync::oneshot::error::TryRecvError::Closed) => {
                            return Ok(());
                        }
                        Err(tokio::sync::oneshot::error::TryRecvError::Empty) => {
                            // No shutdown requested yet, continue
                        }
                    }
                    // Timeout - continue loop
                }
            }
        }
    }

    /// Handle a single client connection
    async fn handle_connection<F, Fut>(
        mut stream: UnixStream,
        handler: F,
    ) -> Result<(), ServerError>
    where
        F: Fn(Request) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Option<Response>> + Send + 'static,
    {
        loop {
            // Read message length (4 bytes, little endian)
            let mut len_buf = [0u8; 4];
            match stream.read_exact(&mut len_buf).await {
                Ok(_) => {}
                Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                    // Connection closed cleanly
                    break;
                }
                Err(e) => return Err(ServerError::Io(e)),
            }

            let msg_len = u32::from_le_bytes(len_buf) as usize;

            // Read SSZ message
            let mut msg_buf = vec![0u8; msg_len];
            stream.read_exact(&mut msg_buf).await?;

            // Decode the request
            let request: Request = decode_ssz(&msg_buf).map_err(|_| ServerError::SszDecode)?;

            // Call the handler
            if let Some(response) = handler(request).await {
                // Encode and send the response
                let response_bytes = encode_ssz(&response);
                let response_len = (response_bytes.len() as u32).to_le_bytes();

                stream.write_all(&response_len).await?;
                stream.write_all(&response_bytes).await?;
                stream.flush().await?;
            }
        }

        Ok(())
    }
}

impl Drop for CommandTraceServer {
    fn drop(&mut self) {
        // Clean up the socket file when the server is dropped
        if self.socket_path.exists() {
            let _ = std::fs::remove_file(&self.socket_path);
        }
    }
}

/// Test utilities for the command trace server
pub mod test_utils {
    use super::*;
    use std::sync::Arc;
    use tokio::sync::Mutex;

    /// A test server that collects received requests for inspection
    #[derive(Clone)]
    pub struct TestServer {
        socket_path: std::path::PathBuf,
        received_requests: Arc<Mutex<Vec<Request>>>,
    }

    impl TestServer {
        /// Create a new test server
        pub fn new<P: AsRef<Path>>(socket_path: P) -> Self {
            Self {
                socket_path: socket_path.as_ref().to_path_buf(),
                received_requests: Arc::new(Mutex::new(Vec::new())),
            }
        }

        /// Get the collected requests (for test assertions)
        pub async fn get_requests(&self) -> Vec<Request> {
            self.received_requests.lock().await.clone()
        }

        /// Run the server, collecting all requests and responding with success to handshakes
        pub async fn run(&self) -> Result<(), ServerError> {
            let mut server = CommandTraceServer::new(&self.socket_path);
            let requests = Arc::clone(&self.received_requests);

            // Create shutdown channel
            let (shutdown_tx, shutdown_rx) = oneshot::channel();

            let handler = move |request: Request| {
                let requests = Arc::clone(&requests);
                async move {
                    requests.lock().await.push(request.clone());

                    // Auto-respond to handshake requests
                    match request {
                        Request::Handshake(_) => Some(Response::Handshake(
                            ah_command_trace_proto::HandshakeResponse {
                                success: true,
                                error_message: None,
                            },
                        )),
                    }
                }
            };

            // Run the server with a timeout for testing
            let server_future = server.run(handler, shutdown_rx);
            let timeout_future =
                tokio::time::timeout(std::time::Duration::from_secs(5), server_future);

            match timeout_future.await {
                Ok(result) => result,
                Err(_) => {
                    // Timeout reached, drop the shutdown sender to signal shutdown
                    drop(shutdown_tx);
                    // Give a moment for the server to detect shutdown
                    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                    Ok(())
                }
            }
        }
    }
}
