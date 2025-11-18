// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

// IPC server for receiving snapshot notifications from external commands
//
// See: specs/Public/ah-agent-record.md section 7 for protocol specification
//
// The recorder provides a Unix domain socket (or TCP on Windows) for receiving
// snapshot notifications from `ah agent fs snapshot` and other external commands.
// When a filesystem snapshot is taken during recording, the external command
// notifies the recorder, which writes a REC_SNAPSHOT record to the .ahr file
// and updates the .snapshots.jsonl sidecar.
//
// Protocol: Length-prefixed SSZ bytes over Unix domain socket
// Format: [4-byte little-endian length][raw SSZ bytes]

use ssz::{Decode, Encode};
use ssz_derive::{Decode as SszDecode, Encode as SszEncode};
use std::io;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use tokio::io::{AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::mpsc;
use tracing::{debug, error, info};

/// IPC request messages (SSZ union enum)
#[derive(Clone, Debug, PartialEq, Eq, SszEncode, SszDecode)]
#[ssz(enum_behaviour = "union")]
pub enum Request {
    /// Snapshot notification: (snapshot_id, label as UTF-8 bytes)
    /// snapshot_id may be 0 for pending snapshots (will be updated later)
    Snapshot((u64, Vec<u8>)),
}

impl Request {
    /// Create a snapshot notification request
    /// snapshot_id may be 0 if the actual ID will be determined later
    pub fn snapshot(snapshot_id: u64, label: String) -> Self {
        Self::Snapshot((snapshot_id, label.into_bytes()))
    }
}

/// IPC response messages (SSZ union enum)
#[derive(Clone, Debug, PartialEq, Eq, SszEncode, SszDecode)]
#[ssz(enum_behaviour = "union")]
pub enum Response {
    /// Success response: (snapshot_id, anchor_byte, ts_ns)
    Success((u64, u64, u64)),
    /// Error response: error message as UTF-8 bytes
    Error(Vec<u8>),
}

impl Response {
    /// Create a success response
    pub fn success(snapshot_id: u64, anchor_byte: u64, ts_ns: u64) -> Self {
        Self::Success((snapshot_id, anchor_byte, ts_ns))
    }

    /// Create an error response
    pub fn error(message: String) -> Self {
        Self::Error(message.into_bytes())
    }

    /// Get snapshot_id from success response
    pub fn snapshot_id(&self) -> Option<u64> {
        match self {
            Self::Success((id, _, _)) => Some(*id),
            _ => None,
        }
    }

    /// Get anchor_byte from success response
    pub fn anchor_byte(&self) -> Option<u64> {
        match self {
            Self::Success((_, anchor, _)) => Some(*anchor),
            _ => None,
        }
    }

    /// Get ts_ns from success response
    pub fn ts_ns(&self) -> Option<u64> {
        match self {
            Self::Success((_, _, ts)) => Some(*ts),
            _ => None,
        }
    }

    /// Get error message from error response
    pub fn error_message(&self) -> Option<String> {
        match self {
            Self::Error(bytes) => Some(String::from_utf8_lossy(bytes).to_string()),
            _ => None,
        }
    }

    /// Check if response is success
    pub fn is_success(&self) -> bool {
        matches!(self, Self::Success(_))
    }
}

/// Internal command sent from IPC handler to recorder
#[derive(Debug)]
pub enum IpcCommand {
    /// Record a snapshot notification
    Snapshot {
        snapshot_id: u64,
        label: String,
        response_tx: tokio::sync::oneshot::Sender<Response>,
    },
    /// Shutdown the IPC server
    Shutdown,
}

/// IPC server configuration
#[derive(Debug, Clone)]
pub struct IpcServerConfig {
    /// Path to Unix domain socket (required on Unix platforms)
    pub socket_path: PathBuf,
}

/// IPC server handle
pub struct IpcServer {
    /// Channel for sending commands to the recorder
    command_tx: mpsc::UnboundedSender<IpcCommand>,
    /// Server shutdown flag
    shutdown: Arc<AtomicBool>,
    /// Current PTY byte offset (updated by recorder)
    current_byte_offset: Arc<AtomicU64>,
}

impl IpcServer {
    /// Start the IPC server
    ///
    /// Returns a handle to the server and a receiver for IPC commands.
    /// The caller (recorder) should process commands from the receiver
    /// and update the current_byte_offset as PTY bytes are processed.
    pub async fn start(
        config: IpcServerConfig,
        current_byte_offset: Arc<AtomicU64>,
    ) -> io::Result<(Self, mpsc::UnboundedReceiver<IpcCommand>)> {
        let (command_tx, command_rx) = mpsc::unbounded_channel();
        let shutdown = Arc::new(AtomicBool::new(false));

        // Remove existing socket if present
        if config.socket_path.exists() {
            std::fs::remove_file(&config.socket_path)?;
        }

        // Create parent directory if needed
        if let Some(parent) = config.socket_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let listener = UnixListener::bind(&config.socket_path)?;
        info!("IPC server listening on {:?}", config.socket_path);

        let server = Self {
            command_tx: command_tx.clone(),
            shutdown: shutdown.clone(),
            current_byte_offset,
        };

        // Spawn accept loop
        tokio::spawn(async move {
            debug!("IPC server accept loop started");
            loop {
                if shutdown.load(Ordering::Relaxed) {
                    break;
                }

                match listener.accept().await {
                    Ok((stream, _addr)) => {
                        debug!("IPC server accepted connection");
                        let tx = command_tx.clone();
                        tokio::spawn(async move {
                            if let Err(e) = handle_connection(stream, tx).await {
                                error!("Error handling IPC connection: {}", e);
                            }
                        });
                    }
                    Err(e) => {
                        error!("Error accepting IPC connection: {}", e);
                        break;
                    }
                }
            }

            // Cleanup socket
            let _ = std::fs::remove_file(&config.socket_path);
            debug!("IPC server shut down");
        });

        Ok((server, command_rx))
    }

    /// Get the current PTY byte offset
    pub fn current_byte_offset(&self) -> u64 {
        self.current_byte_offset.load(Ordering::Relaxed)
    }

    /// Shutdown the IPC server
    pub async fn shutdown(&self) {
        self.shutdown.store(true, Ordering::Relaxed);
        let _ = self.command_tx.send(IpcCommand::Shutdown);
    }
}

/// Handle a single IPC connection
async fn handle_connection(
    stream: UnixStream,
    command_tx: mpsc::UnboundedSender<IpcCommand>,
) -> io::Result<()> {
    let mut reader = BufReader::new(stream);

    // Read length prefix (4 bytes, little-endian)
    let mut len_buf = [0u8; 4];
    reader.read_exact(&mut len_buf).await?;
    let request_len = u32::from_le_bytes(len_buf) as usize;

    // Read raw SSZ request bytes
    let mut request_bytes = vec![0u8; request_len];
    reader.read_exact(&mut request_bytes).await?;

    // Decode SSZ to Request
    let request = Request::from_ssz_bytes(&request_bytes).map_err(|e| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("Failed to decode SSZ request: {:?}", e),
        )
    })?;

    match request {
        Request::Snapshot((snapshot_id, label_bytes)) => {
            let label = String::from_utf8_lossy(&label_bytes).to_string();
            debug!(
                "Received snapshot notification: id={}, label={:?}",
                snapshot_id, label
            );

            // Send command to recorder and wait for response
            let (response_tx, response_rx) = tokio::sync::oneshot::channel();

            command_tx
                .send(IpcCommand::Snapshot {
                    snapshot_id,
                    label,
                    response_tx,
                })
                .map_err(|_| io::Error::other("Failed to send command to recorder"))?;

            // Wait for recorder to process snapshot and respond
            let response =
                response_rx.await.map_err(|_| io::Error::other("Recorder did not respond"))?;

            // Send length-prefixed response
            let response_bytes = response.as_ssz_bytes();
            let response_len = response_bytes.len() as u32;

            let mut stream = reader.into_inner();
            stream.write_all(&response_len.to_le_bytes()).await?;
            stream.write_all(&response_bytes).await?;
            stream.flush().await?;

            debug!("Sent snapshot response: success={}", response.is_success());
        }
    }

    Ok(())
}

/// IPC client for sending snapshot notifications
pub struct IpcClient {
    socket_path: PathBuf,
}

impl IpcClient {
    /// Create a new IPC client
    pub fn new(socket_path: PathBuf) -> Self {
        Self { socket_path }
    }

    /// Send a snapshot notification (legacy)
    pub async fn notify_snapshot(&self, snapshot_id: u64, label: String) -> io::Result<Response> {
        let stream = UnixStream::connect(&self.socket_path).await?;
        let mut reader = BufReader::new(stream);

        // Create and encode request
        let request = Request::snapshot(snapshot_id, label);
        let request_bytes = request.as_ssz_bytes();
        let request_len = request_bytes.len() as u32;

        // Send length-prefixed request
        let stream = reader.get_mut();
        stream.write_all(&request_len.to_le_bytes()).await?;
        stream.write_all(&request_bytes).await?;
        stream.flush().await?;

        // Read length-prefixed response
        let mut len_buf = [0u8; 4];
        reader.read_exact(&mut len_buf).await?;
        let response_len = u32::from_le_bytes(len_buf) as usize;

        let mut response_bytes = vec![0u8; response_len];
        reader.read_exact(&mut response_bytes).await?;

        // Decode response
        let response = Response::from_ssz_bytes(&response_bytes).map_err(|e| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Failed to decode SSZ response: {:?}", e),
            )
        })?;

        Ok(response)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ssz::{Decode, Encode};

    #[test]
    fn test_request_roundtrip() {
        let req = Request::snapshot(42, "test-checkpoint".to_string());
        let bytes = req.as_ssz_bytes();
        let decoded = Request::from_ssz_bytes(&bytes).unwrap();

        match decoded {
            Request::Snapshot((id, label_bytes)) => {
                assert_eq!(id, 42);
                assert_eq!(String::from_utf8_lossy(&label_bytes), "test-checkpoint");
            }
        }
    }

    #[test]
    fn test_request_empty_label() {
        let req = Request::snapshot(99, String::new());
        let bytes = req.as_ssz_bytes();
        let decoded = Request::from_ssz_bytes(&bytes).unwrap();

        match decoded {
            Request::Snapshot((id, label_bytes)) => {
                assert_eq!(id, 99);
                assert_eq!(label_bytes.len(), 0);
            }
        }
    }

    #[test]
    fn test_response_success() {
        let resp = Response::success(42, 1000, 123456789);
        let bytes = resp.as_ssz_bytes();
        let decoded = Response::from_ssz_bytes(&bytes).unwrap();

        assert!(decoded.is_success());
        assert_eq!(decoded.snapshot_id(), Some(42));
        assert_eq!(decoded.anchor_byte(), Some(1000));
        assert_eq!(decoded.ts_ns(), Some(123456789));
    }

    #[test]
    fn test_response_error() {
        let resp = Response::error("test error".to_string());
        let bytes = resp.as_ssz_bytes();
        let decoded = Response::from_ssz_bytes(&bytes).unwrap();

        assert!(!decoded.is_success());
        assert_eq!(decoded.error_message(), Some("test error".to_string()));
    }
}
