use crate::operations::process_request;
use crate::types::Request;
use anyhow::{anyhow, Result};
use std::path::PathBuf;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio_stream::{wrappers::UnixListenerStream, StreamExt};
use tracing::{debug, error, info, warn};

// SSZ encoding/decoding functions for daemon communication
fn encode_ssz(data: &impl ssz::Encode) -> Vec<u8> {
    data.as_ssz_bytes()
}

fn decode_ssz<T: ssz::Decode>(data: &[u8]) -> Result<T> {
    T::from_ssz_bytes(data).map_err(|e| anyhow!("SSZ decode error: {:?}", e))
}

pub struct DaemonServer {
    socket_path: PathBuf,
    listener: Option<UnixListener>,
}

impl DaemonServer {
    pub fn new(socket_path: PathBuf) -> Result<Self> {
        // Ensure socket directory exists
        if let Some(parent) = socket_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        // Remove existing socket if it exists
        if socket_path.exists() {
            std::fs::remove_file(&socket_path)?;
        }

        let listener = UnixListener::bind(&socket_path)?;
        // Set permissions to allow anyone to connect (since tests run as regular user)
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&socket_path)?.permissions();
            perms.set_mode(0o666);
            std::fs::set_permissions(&socket_path, perms)?;
        }

        info!("Daemon listening on socket: {}", socket_path.display());

        Ok(Self {
            socket_path,
            listener: Some(listener),
        })
    }

    pub async fn run(&mut self) -> Result<()> {
        let listener = self.listener.take().ok_or_else(|| anyhow!("Server not initialized"))?;
        let mut stream = UnixListenerStream::new(listener);

        info!("AH filesystem snapshots daemon started. Press Ctrl+C to stop.");

        while let Some(stream) = stream.next().await {
            match stream {
                Ok(socket) => {
                    tokio::spawn(async move {
                        if let Err(e) = handle_client(socket).await {
                            error!("Error handling client: {}", e);
                        }
                    });
                }
                Err(e) => {
                    warn!("Error accepting connection: {}", e);
                }
            }
        }

        Ok(())
    }

    pub async fn shutdown(self) -> Result<()> {
        info!("Shutting down daemon...");

        // Remove the socket file
        if self.socket_path.exists() {
            std::fs::remove_file(&self.socket_path)?;
        }

        Ok(())
    }
}

async fn handle_client(mut socket: UnixStream) -> Result<()> {
    debug!("Handling new client connection");

    let (reader, mut writer) = socket.split();
    let mut reader = BufReader::new(reader);
    let mut line = String::new();

    // Read one line (the request)
    let n = reader.read_line(&mut line).await?;
    if n == 0 {
        // Client disconnected
        return Ok(());
    }

    // Parse SSZ-encoded request from hex string
    let request_bytes = hex::decode(line.trim())?;
    let request: Request = decode_ssz(&request_bytes)?;

    // Process the request
    let response = process_request(request).await;

    // Encode response as SSZ and send as hex
    let response_bytes = encode_ssz(&response);
    let response_hex = hex::encode(&response_bytes);

    // Write response followed by newline
    writer.write_all(response_hex.as_bytes()).await?;
    writer.write_all(b"\n").await?;
    writer.flush().await?;

    debug!("Handled client request successfully");

    Ok(())
}
