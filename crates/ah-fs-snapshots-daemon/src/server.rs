// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

use crate::fuse_manager::AgentfsFuseManager;
use crate::interpose_manager::AgentfsInterposeManager;
use crate::operations::{
    handle_btrfs_clone, handle_btrfs_delete, handle_btrfs_snapshot, handle_mount_agentfs_fuse,
    handle_status_agentfs_fuse, handle_status_agentfs_interpose, handle_unmount_agentfs_fuse,
    handle_unmount_agentfs_interpose, handle_zfs_clone, handle_zfs_delete,
    handle_zfs_list_snapshots, handle_zfs_snapshot,
};
use crate::types::{
    AgentfsFuseMountRequest, AgentfsInterposeMountHints, AgentfsInterposeMountRequest, Request,
    Response,
};
use anyhow::{Result, anyhow};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio_stream::{StreamExt, wrappers::UnixListenerStream};
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
    state: Arc<DaemonState>,
}

impl DaemonServer {
    pub fn new(socket_path: PathBuf, state: Arc<DaemonState>) -> Result<Self> {
        debug!(operation = "server_new", socket_path = %socket_path.display(), "Initializing daemon server");

        // Ensure socket directory exists
        if let Some(parent) = socket_path.parent() {
            debug!(operation = "server_create_socket_dir", parent_path = %parent.display(), "Creating socket directory");
            std::fs::create_dir_all(parent)?;
            debug!(operation = "server_socket_dir_created", parent_path = %parent.display(), "Socket directory created successfully");
        }

        // Remove existing socket if it exists
        if socket_path.exists() {
            debug!(operation = "server_remove_stale_socket", socket_path = %socket_path.display(), "Removing existing socket file");
            std::fs::remove_file(&socket_path)?;
            debug!(operation = "server_stale_socket_removed", socket_path = %socket_path.display(), "Existing socket file removed");
        }

        debug!(operation = "server_bind_socket", socket_path = %socket_path.display(), "Binding Unix socket listener");
        let listener = UnixListener::bind(&socket_path)?;
        debug!(operation = "server_socket_bound", socket_path = %socket_path.display(), "Unix socket listener bound successfully");

        // Set permissions to allow anyone to connect (since tests run as regular user)
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            debug!(operation = "server_set_socket_permissions", socket_path = %socket_path.display(), "Setting socket permissions to 0o666");
            let mut perms = std::fs::metadata(&socket_path)?.permissions();
            perms.set_mode(0o666);
            std::fs::set_permissions(&socket_path, perms)?;
            debug!(operation = "server_socket_permissions_set", socket_path = %socket_path.display(), permissions = "0o666", "Socket permissions set successfully");
        }

        info!(operation = "start_server", socket_path = %socket_path.display(), "Daemon listening on socket");
        debug!(operation = "server_init_complete", socket_path = %socket_path.display(), "Daemon server initialization completed");

        Ok(Self {
            socket_path,
            listener: Some(listener),
            state,
        })
    }

    pub async fn run(&mut self) -> Result<()> {
        debug!(operation = "server_run_start", "Starting server run loop");
        let listener = self.listener.take().ok_or_else(|| anyhow!("Server not initialized"))?;
        let mut stream = UnixListenerStream::new(listener);
        debug!(
            operation = "server_listener_stream_created",
            "UnixListenerStream created successfully"
        );

        info!(
            operation = "server_running",
            "AH filesystem snapshots daemon started. Press Ctrl+C to stop."
        );

        let mut connection_count = 0;
        while let Some(stream) = stream.next().await {
            connection_count += 1;
            match stream {
                Ok(socket) => {
                    debug!(operation = "server_accept_connection", connection_count = %connection_count, "Accepted new client connection, spawning handler");
                    let state = self.state.clone();
                    tokio::spawn(async move {
                        if let Err(e) = handle_client(state, socket).await {
                            error!(operation = "handle_client", error = %e, connection_count = %connection_count, "Error handling client");
                        } else {
                            debug!(operation = "handle_client", connection_count = %connection_count, "Client connection handled successfully");
                        }
                    });
                }
                Err(e) => {
                    warn!(operation = "accept_connection", error = %e, connection_count = %connection_count, "Error accepting connection");
                }
            }
        }

        debug!(operation = "server_run_loop_exit", total_connections = %connection_count, "Server run loop exited");
        Ok(())
    }

    pub async fn shutdown(self) -> Result<()> {
        info!(operation = "shutdown", "Shutting down daemon");
        debug!(operation = "shutdown_cleanup", socket_path = %self.socket_path.display(), "Starting shutdown cleanup");

        // Remove the socket file
        if self.socket_path.exists() {
            debug!(operation = "shutdown_remove_socket", socket_path = %self.socket_path.display(), "Removing socket file");
            std::fs::remove_file(&self.socket_path)?;
            debug!(operation = "shutdown_socket_removed", socket_path = %self.socket_path.display(), "Socket file removed successfully");
        } else {
            debug!(operation = "shutdown_socket_not_found", socket_path = %self.socket_path.display(), "Socket file not found during shutdown");
        }

        debug!(
            operation = "shutdown_complete",
            "Daemon shutdown completed successfully"
        );
        Ok(())
    }
}

async fn handle_client(state: Arc<DaemonState>, mut socket: UnixStream) -> Result<()> {
    let session_id = ah_logging::correlation_id();

    info!(
        "SESSION_ID_LOGGING: ah-fs-snapshots-daemon generated session_id={}",
        session_id
    );

    debug!(
        operation = "handle_client",
        session_id = %session_id,
        "Handling new client connection"
    );

    let (reader, mut writer) = socket.split();
    let mut reader = BufReader::new(reader);
    let mut line = String::new();

    debug!(operation = "handle_client_read_request", session_id = %session_id, "Reading request from client");
    // Read one line (the request)
    let n = reader.read_line(&mut line).await?;
    if n == 0 {
        // Client disconnected
        debug!(operation = "handle_client_client_disconnected", session_id = %session_id, "Client disconnected without sending data");
        return Ok(());
    }

    debug!(operation = "handle_client_request_received", session_id = %session_id, bytes_read = %n, line_length = %line.len(), "Received request line from client");

    // Parse SSZ-encoded request from hex string
    let line_trimmed = line.trim();
    debug!(operation = "handle_client_hex_decode", session_id = %session_id, hex_length = %line_trimmed.len(), "Decoding hex string to SSZ bytes");
    let request_bytes = hex::decode(line_trimmed)?;
    debug!(operation = "handle_client_ssz_decode", session_id = %session_id, bytes_len = %request_bytes.len(), "Decoding SSZ bytes to request");
    let request: Request = decode_ssz(&request_bytes)?;
    debug!(operation = "handle_client_request_parsed", session_id = %session_id, "Request parsed successfully");

    // Process the request
    debug!(operation = "handle_client_process_request", session_id = %session_id, "Processing request through daemon state");
    let response = state.process_request(request, session_id.clone()).await;
    debug!(operation = "handle_client_request_processed", session_id = %session_id, "Request processed successfully");

    // Encode response as SSZ and send as hex
    debug!(operation = "handle_client_encode_response", session_id = %session_id, "Encoding response as SSZ bytes");
    let response_bytes = encode_ssz(&response);
    let response_hex = hex::encode(&response_bytes);
    debug!(operation = "handle_client_response_encoded", session_id = %session_id, response_hex_length = %response_hex.len(), "Response encoded as hex string");

    // Write response followed by newline
    debug!(operation = "handle_client_write_response", session_id = %session_id, "Writing response to client");
    writer.write_all(response_hex.as_bytes()).await?;
    writer.write_all(b"\n").await?;
    writer.flush().await?;
    debug!(operation = "handle_client_response_sent", session_id = %session_id, bytes_written = %(response_hex.len() + 1), "Response sent to client successfully");

    debug!(
        operation = "handle_client",
        session_id = %session_id,
        "Handled client request successfully"
    );

    Ok(())
}

pub struct DaemonState {
    fuse_manager: Arc<AgentfsFuseManager>,
    interpose_manager: Arc<AgentfsInterposeManager>,
    log_level: String,
    log_to_file: bool,
    log_dir: std::path::PathBuf,
}

impl DaemonState {
    pub fn new(log_level: String, log_to_file: bool, log_dir: std::path::PathBuf) -> Self {
        Self {
            fuse_manager: Arc::new(AgentfsFuseManager::new()),
            interpose_manager: Arc::new(AgentfsInterposeManager::new()),
            log_level,
            log_to_file,
            log_dir,
        }
    }

    pub fn fuse_manager(&self) -> &AgentfsFuseManager {
        &self.fuse_manager
    }

    pub fn interpose_manager(&self) -> &AgentfsInterposeManager {
        &self.interpose_manager
    }

    pub fn log_level(&self) -> &str {
        &self.log_level
    }

    pub fn log_to_file(&self) -> bool {
        self.log_to_file
    }

    pub fn log_dir(&self) -> &std::path::Path {
        &self.log_dir
    }

    pub async fn process_request(&self, request: Request, session_id: String) -> Response {
        info!(operation = "process_request", request = ?request, "Processing request");

        match request {
            Request::Ping(_) => Response::success(),
            Request::ListZfsSnapshots(dataset) => {
                let dataset_str = String::from_utf8_lossy(&dataset).to_string();
                handle_zfs_list_snapshots(dataset_str).await
            }
            Request::CloneZfs((snapshot, clone)) => {
                let snapshot_str = String::from_utf8_lossy(&snapshot).to_string();
                let clone_str = String::from_utf8_lossy(&clone).to_string();
                handle_zfs_clone(snapshot_str, clone_str).await
            }
            Request::SnapshotZfs((source, snapshot)) => {
                let source_str = String::from_utf8_lossy(&source).to_string();
                let snapshot_str = String::from_utf8_lossy(&snapshot).to_string();
                handle_zfs_snapshot(source_str, snapshot_str).await
            }
            Request::DeleteZfs(target) => {
                let target_str = String::from_utf8_lossy(&target).to_string();
                handle_zfs_delete(target_str).await
            }
            Request::CloneBtrfs((source, destination)) => {
                let source_str = String::from_utf8_lossy(&source).to_string();
                let destination_str = String::from_utf8_lossy(&destination).to_string();
                handle_btrfs_clone(source_str, destination_str).await
            }
            Request::SnapshotBtrfs((source, destination)) => {
                let source_str = String::from_utf8_lossy(&source).to_string();
                let destination_str = String::from_utf8_lossy(&destination).to_string();
                handle_btrfs_snapshot(source_str, destination_str).await
            }
            Request::DeleteBtrfs(target) => {
                let target_str = String::from_utf8_lossy(&target).to_string();
                handle_btrfs_delete(target_str).await
            }
            Request::MountAgentfsFuse(req) => {
                self.handle_mount_agentfs_fuse(req, &session_id).await
            }
            Request::UnmountAgentfsFuse(_) => self.handle_unmount_agentfs_fuse().await,
            Request::StatusAgentfsFuse(_) => self.handle_status_agentfs_fuse().await,
            Request::MountAgentfsInterpose(req) => {
                self.do_mount_agentfs_interpose(
                    req,
                    None,
                    self.log_level().to_string(),
                    self.log_to_file(),
                    self.log_dir(),
                    session_id.clone(),
                )
                .await
            }
            Request::MountAgentfsInterposeWithHints((req, hints)) => {
                self.do_mount_agentfs_interpose(
                    req,
                    Some(hints),
                    self.log_level().to_string(),
                    self.log_to_file(),
                    self.log_dir(),
                    session_id.clone(),
                )
                .await
            }
            Request::UnmountAgentfsInterpose(_) => self.handle_unmount_agentfs_interpose().await,
            Request::StatusAgentfsInterpose(_) => self.handle_status_agentfs_interpose().await,
        }
    }

    async fn handle_mount_agentfs_fuse(
        &self,
        request: AgentfsFuseMountRequest,
        session_id: &str,
    ) -> Response {
        handle_mount_agentfs_fuse(self.fuse_manager(), request, session_id).await
    }

    async fn handle_unmount_agentfs_fuse(&self) -> Response {
        handle_unmount_agentfs_fuse(self.fuse_manager()).await
    }

    async fn handle_status_agentfs_fuse(&self) -> Response {
        handle_status_agentfs_fuse(self.fuse_manager()).await
    }

    async fn do_mount_agentfs_interpose(
        &self,
        request: AgentfsInterposeMountRequest,
        hints: Option<AgentfsInterposeMountHints>,
        log_level: String,
        log_to_file: bool,
        log_dir: &std::path::Path,
        session_id: String,
    ) -> Response {
        let manager = self.interpose_manager();
        match manager
            .mount(
                request,
                hints,
                &log_level,
                log_to_file,
                log_dir,
                &session_id,
            )
            .await
        {
            Ok(status) => {
                debug!(operation = "handle_mount_agentfs_interpose_success", session_id = %session_id, socket_path = ?status.socket_path, runtime_dir = ?status.runtime_dir, state = ?status.state, "Interpose mount request completed successfully");
                Response::agentfs_interpose_status(status)
            }
            Err(err) => {
                debug!(operation = "handle_mount_agentfs_interpose_error", session_id = %session_id, error = %err, "Interpose mount request failed");
                Response::error(err.to_string())
            }
        }
    }

    async fn handle_unmount_agentfs_interpose(&self) -> Response {
        handle_unmount_agentfs_interpose(self.interpose_manager()).await
    }

    async fn handle_status_agentfs_interpose(&self) -> Response {
        handle_status_agentfs_interpose(self.interpose_manager()).await
    }
}

impl Default for DaemonState {
    fn default() -> Self {
        Self::new("info".to_string(), false, get_log_directory())
    }
}

/// Get the standard log directory for the current user (handles sudo case)
pub fn get_log_directory() -> std::path::PathBuf {
    // Create component-specific log path in user's home directory (even when running as root)
    let user_home = if let Ok(sudo_user) = std::env::var("SUDO_USER") {
        // When run with sudo, try to get the original user's home directory
        std::env::var(format!("HOME_{}", sudo_user)).unwrap_or_else(|_| {
            // Fallback: construct path assuming standard macOS home directory location
            format!("/Users/{}", sudo_user)
        })
    } else {
        // Not run with sudo, use current user's home
        std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string())
    };

    let mut log_dir = std::path::PathBuf::from(user_home);
    log_dir.push("Library");
    log_dir.push("Logs");
    log_dir.push("agent-harbor");

    // Ensure the directory exists
    std::fs::create_dir_all(&log_dir).ok();

    log_dir
}
