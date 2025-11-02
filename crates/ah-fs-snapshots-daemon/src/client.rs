// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Client library for communicating with the AH filesystem snapshots daemon.

use crate::types::{Request, Response};
use ssz::{Decode, Encode};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::Path;

/// Default path to the daemon socket.
pub const DEFAULT_SOCKET_PATH: &str = "/tmp/agent-harbor/ah-fs-snapshots-daemon";

/// Client for communicating with the filesystem snapshots daemon.
#[derive(Clone, Debug)]
pub struct DaemonClient {
    socket_path: String,
}

impl DaemonClient {
    /// Create a new daemon client with the default socket path.
    pub fn new() -> Self {
        Self {
            socket_path: DEFAULT_SOCKET_PATH.to_string(),
        }
    }

    /// Create a new daemon client with a custom socket path.
    pub fn with_socket_path<P: AsRef<Path>>(socket_path: P) -> Self {
        Self {
            socket_path: socket_path.as_ref().to_string_lossy().to_string(),
        }
    }

    /// Check if the daemon socket exists.
    pub fn socket_exists(&self) -> bool {
        Path::new(&self.socket_path).exists()
    }

    /// Send a request to the daemon and wait for a response.
    pub fn send_request(&self, request: Request) -> Result<Response, DaemonError> {
        let mut stream = UnixStream::connect(&self.socket_path).map_err(|e| {
            DaemonError::ConnectionError(format!("Failed to connect to daemon socket: {}", e))
        })?;

        // Encode request as SSZ bytes and hex
        let request_bytes = request.as_ssz_bytes();
        let request_hex = hex::encode(&request_bytes);

        // Send request
        stream.write_all(format!("{}\n", request_hex).as_bytes()).map_err(|e| {
            DaemonError::CommunicationError(format!("Failed to send request: {}", e))
        })?;

        // Read response
        let mut reader = BufReader::new(stream);
        let mut response_line = String::new();

        reader.read_line(&mut response_line).map_err(|e| {
            DaemonError::CommunicationError(format!("Failed to read response: {}", e))
        })?;

        let response_hex = response_line.trim();

        // Decode hex to bytes, then SSZ to Response
        let response_bytes = hex::decode(response_hex).map_err(|e| {
            DaemonError::ProtocolError(format!("Failed to decode hex response: {}", e))
        })?;

        let response = Response::from_ssz_bytes(&response_bytes).map_err(|e| {
            DaemonError::ProtocolError(format!("Failed to decode SSZ response: {:?}", e))
        })?;

        Ok(response)
    }

    /// Ping the daemon to check if it's alive and responsive.
    pub fn ping(&self) -> Result<(), DaemonError> {
        let response = self.send_request(Request::ping())?;

        match response {
            Response::Success(_) => Ok(()),
            Response::Error(msg) => Err(DaemonError::DaemonError(
                String::from_utf8_lossy(&msg).to_string(),
            )),
            _ => Err(DaemonError::ProtocolError(
                "Unexpected response to ping".to_string(),
            )),
        }
    }

    /// List all snapshots for a ZFS dataset.
    pub fn list_zfs_snapshots(&self, dataset: &str) -> Result<Vec<String>, DaemonError> {
        let response = self.send_request(Request::list_zfs_snapshots(dataset.to_string()))?;

        match response {
            Response::SuccessWithList(data) => {
                let json_str = String::from_utf8_lossy(&data);
                serde_json::from_str(&json_str).map_err(|e| {
                    DaemonError::CommunicationError(format!(
                        "Failed to parse snapshot list JSON: {}",
                        e
                    ))
                })
            }
            Response::Error(msg) => Err(DaemonError::DaemonError(
                String::from_utf8_lossy(&msg).to_string(),
            )),
            _ => Err(DaemonError::ProtocolError(
                "Unexpected response to list_zfs_snapshots".to_string(),
            )),
        }
    }

    /// Create a ZFS snapshot.
    pub fn snapshot_zfs(&self, source: &str, snapshot: &str) -> Result<(), DaemonError> {
        let response = self.send_request(Request::snapshot_zfs(
            source.to_string(),
            snapshot.to_string(),
        ))?;

        match response {
            Response::Success(_) => Ok(()),
            Response::Error(msg) => Err(DaemonError::DaemonError(
                String::from_utf8_lossy(&msg).to_string(),
            )),
            _ => Err(DaemonError::ProtocolError(
                "Unexpected response to ZFS snapshot".to_string(),
            )),
        }
    }

    /// Clone a ZFS snapshot.
    pub fn clone_zfs(&self, snapshot: &str, clone: &str) -> Result<Option<String>, DaemonError> {
        let response =
            self.send_request(Request::clone_zfs(snapshot.to_string(), clone.to_string()))?;

        match response {
            Response::Success(_) => Ok(None),
            Response::SuccessWithMountpoint(mountpoint) => {
                Ok(Some(String::from_utf8_lossy(&mountpoint).to_string()))
            }
            Response::Error(msg) => Err(DaemonError::DaemonError(
                String::from_utf8_lossy(&msg).to_string(),
            )),
            _ => Err(DaemonError::ProtocolError(
                "Unexpected response to ZFS clone".to_string(),
            )),
        }
    }

    /// Delete a ZFS dataset.
    pub fn delete_zfs(&self, target: &str) -> Result<(), DaemonError> {
        let response = self.send_request(Request::delete_zfs(target.to_string()))?;

        match response {
            Response::Success(_) => Ok(()),
            Response::Error(msg) => Err(DaemonError::DaemonError(
                String::from_utf8_lossy(&msg).to_string(),
            )),
            _ => Err(DaemonError::ProtocolError(
                "Unexpected response to ZFS delete".to_string(),
            )),
        }
    }

    /// Create a Btrfs snapshot.
    pub fn snapshot_btrfs(&self, source: &str, destination: &str) -> Result<(), DaemonError> {
        let response = self.send_request(Request::snapshot_btrfs(
            source.to_string(),
            destination.to_string(),
        ))?;

        match response {
            Response::Success(_) => Ok(()),
            Response::Error(msg) => Err(DaemonError::DaemonError(
                String::from_utf8_lossy(&msg).to_string(),
            )),
            _ => Err(DaemonError::ProtocolError(
                "Unexpected response to Btrfs snapshot".to_string(),
            )),
        }
    }

    /// Clone a Btrfs subvolume.
    pub fn clone_btrfs(
        &self,
        source: &str,
        destination: &str,
    ) -> Result<Option<String>, DaemonError> {
        let response = self.send_request(Request::clone_btrfs(
            source.to_string(),
            destination.to_string(),
        ))?;

        match response {
            Response::Success(_) => Ok(None),
            Response::SuccessWithPath(path) => Ok(Some(String::from_utf8_lossy(&path).to_string())),
            Response::Error(msg) => Err(DaemonError::DaemonError(
                String::from_utf8_lossy(&msg).to_string(),
            )),
            _ => Err(DaemonError::ProtocolError(
                "Unexpected response to Btrfs clone".to_string(),
            )),
        }
    }

    /// Delete a Btrfs subvolume.
    pub fn delete_btrfs(&self, target: &str) -> Result<(), DaemonError> {
        let response = self.send_request(Request::delete_btrfs(target.to_string()))?;

        match response {
            Response::Success(_) => Ok(()),
            Response::Error(msg) => Err(DaemonError::DaemonError(
                String::from_utf8_lossy(&msg).to_string(),
            )),
            _ => Err(DaemonError::ProtocolError(
                "Unexpected response to Btrfs delete".to_string(),
            )),
        }
    }
}

impl Default for DaemonClient {
    fn default() -> Self {
        Self::new()
    }
}

/// Errors that can occur when communicating with the daemon.
#[derive(Debug, thiserror::Error)]
pub enum DaemonError {
    #[error("Failed to connect to daemon: {0}")]
    ConnectionError(String),

    #[error("Communication error: {0}")]
    CommunicationError(String),

    #[error("Protocol error: {0}")]
    ProtocolError(String),

    #[error("Daemon error: {0}")]
    DaemonError(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[ah_test_utils::logged_test]
    fn test_client_creation() {
        let client = DaemonClient::new();
        assert_eq!(client.socket_path, DEFAULT_SOCKET_PATH);

        let custom_path = "/tmp/custom/socket";
        let custom_client = DaemonClient::with_socket_path(custom_path);
        assert_eq!(custom_client.socket_path, custom_path);
    }

    #[ah_test_utils::logged_test]
    fn test_request_construction() {
        let ping_req = Request::ping();
        assert!(matches!(ping_req, Request::Ping(_)));

        let snap_req = Request::snapshot_zfs("source".to_string(), "snapshot".to_string());
        assert!(matches!(snap_req, Request::SnapshotZfs(_)));

        let clone_req = Request::clone_zfs("snapshot".to_string(), "clone".to_string());
        assert!(matches!(clone_req, Request::CloneZfs(_)));

        let delete_req = Request::delete_zfs("target".to_string());
        assert!(matches!(delete_req, Request::DeleteZfs(_)));
    }
}
