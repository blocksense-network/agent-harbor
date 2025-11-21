// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

use crate::fuse_manager::AgentfsFuseManager;
use crate::operations::{
    handle_btrfs_clone, handle_btrfs_delete, handle_btrfs_snapshot, handle_mount_agentfs_fuse,
    handle_status_agentfs_fuse, handle_unmount_agentfs_fuse, handle_zfs_clone, handle_zfs_delete,
    handle_zfs_list_snapshots, handle_zfs_snapshot,
};
use crate::types::{AgentfsFuseMountRequest, Request, Response};
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

        info!(operation = "start_server", socket_path = %socket_path.display(), "Daemon listening on socket");

        Ok(Self {
            socket_path,
            listener: Some(listener),
            state,
        })
    }

    pub async fn run(&mut self) -> Result<()> {
        let listener = self.listener.take().ok_or_else(|| anyhow!("Server not initialized"))?;
        let mut stream = UnixListenerStream::new(listener);

        info!(
            operation = "server_running",
            "AH filesystem snapshots daemon started. Press Ctrl+C to stop."
        );

        while let Some(stream) = stream.next().await {
            match stream {
                Ok(socket) => {
                    let state = self.state.clone();
                    tokio::spawn(async move {
                        if let Err(e) = handle_client(state, socket).await {
                            error!(operation = "handle_client", error = %e, "Error handling client");
                        }
                    });
                }
                Err(e) => {
                    warn!(operation = "accept_connection", error = %e, "Error accepting connection");
                }
            }
        }

        Ok(())
    }

    pub async fn shutdown(self) -> Result<()> {
        info!(operation = "shutdown", "Shutting down daemon");

        // Remove the socket file
        if self.socket_path.exists() {
            std::fs::remove_file(&self.socket_path)?;
        }

        Ok(())
    }
}

async fn handle_client(state: Arc<DaemonState>, mut socket: UnixStream) -> Result<()> {
    debug!(
        operation = "handle_client",
        "Handling new client connection"
    );

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
    let response = state.process_request(request).await;

    // Encode response as SSZ and send as hex
    let response_bytes = encode_ssz(&response);
    let response_hex = hex::encode(&response_bytes);

    // Write response followed by newline
    writer.write_all(response_hex.as_bytes()).await?;
    writer.write_all(b"\n").await?;
    writer.flush().await?;

    debug!(
        operation = "handle_client",
        "Handled client request successfully"
    );

    Ok(())
}

pub struct DaemonState {
    fuse_manager: Arc<AgentfsFuseManager>,
}

impl DaemonState {
    pub fn new() -> Self {
        Self {
            fuse_manager: Arc::new(AgentfsFuseManager::new()),
        }
    }

    pub fn fuse_manager(&self) -> &AgentfsFuseManager {
        &self.fuse_manager
    }

    pub async fn process_request(&self, request: Request) -> Response {
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
            Request::MountAgentfsFuse(req) => self.handle_mount_agentfs_fuse(req).await,
            Request::UnmountAgentfsFuse(_) => self.handle_unmount_agentfs_fuse().await,
            Request::StatusAgentfsFuse(_) => self.handle_status_agentfs_fuse().await,
        }
    }

    async fn handle_mount_agentfs_fuse(&self, request: AgentfsFuseMountRequest) -> Response {
        handle_mount_agentfs_fuse(self.fuse_manager(), request).await
    }

    async fn handle_unmount_agentfs_fuse(&self) -> Response {
        handle_unmount_agentfs_fuse(self.fuse_manager()).await
    }

    async fn handle_status_agentfs_fuse(&self) -> Response {
        handle_status_agentfs_fuse(self.fuse_manager()).await
    }
}

impl Default for DaemonState {
    fn default() -> Self {
        Self::new()
    }
}
