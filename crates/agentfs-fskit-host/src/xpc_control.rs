//! XPC control plane implementation for FSKit adapter

use super::FsKitAdapter;
use agentfs_proto::*;
use ssz::{Decode, Encode};
use std::sync::Arc;

#[cfg(target_os = "macos")]
use tokio::sync::mpsc;

/// XPC service for handling control operations
#[cfg(target_os = "macos")]
pub struct XpcControlService {
    adapter: Arc<FsKitAdapter>,
    service_name: String,
}

#[cfg(target_os = "macos")]
impl XpcControlService {
    pub fn new(adapter: Arc<FsKitAdapter>, service_name: String) -> Self {
        Self {
            adapter,
            service_name,
        }
    }

    /// Start the XPC service
    pub async fn start(&self) -> Result<(), Box<dyn std::error::Error>> {
        println!("Starting XPC control service: {}", self.service_name);

        // In a real implementation, this would set up XPC listeners
        // For now, this is a simplified async service
        let (_tx, mut _rx) = mpsc::channel::<String>(32);

        // Simulate XPC message handling
        // tokio::spawn(async move {
        //     while let Some(request) = rx.recv().await {
        //         let response = self.handle_request(&request).await;
        //         // Send response back via XPC
        //         println!("XPC Response: {:?}", response);
        //     }
        // });

        Ok(())
    }

    /// Handle incoming XPC request
    async fn handle_request(&self, request_data: &[u8]) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        // Decode SSZ request
        let request: Request = Request::from_ssz_bytes(request_data)
            .map_err(|e| format!("SSZ decode error: {:?}", e))?;

        // Validate request structure
        if let Err(e) = validate_request(&request) {
            let error = Response::error(format!("{}", e), Some(22)); // EINVAL
            return Ok(error.as_ssz_bytes());
        }

        // Route based on operation
        match request {
            Request::SnapshotCreate((_, req)) => self.handle_snapshot_create(req).await,
            Request::SnapshotList(_) => self.handle_snapshot_list(SnapshotListRequest {}).await,
            Request::BranchCreate((_, req)) => self.handle_branch_create(req).await,
            Request::BranchBind((_, req)) => self.handle_branch_bind(req).await,
        }
    }

    async fn handle_snapshot_create(&self, request: SnapshotCreateRequest) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        let name_str = request.name.as_ref().map(|n| String::from_utf8_lossy(n).to_string());
        match self.adapter.core().snapshot_create(name_str.as_deref()) {
            Ok(snapshot_id) => {
                // Get snapshot name from the list
                let snapshots = self.adapter.core().snapshot_list();
                let name = snapshots.iter()
                    .find(|(id, _)| *id == snapshot_id)
                    .and_then(|(_, name)| name.clone());

                let response = Response::snapshot_create(SnapshotInfo {
                    id: snapshot_id.to_string().into_bytes(),
                    name: name.map(|s| s.into_bytes()),
                });
                Ok(response.as_ssz_bytes())
            }
            Err(e) => {
                let response = Response::error(format!("{:?}", e), Some(self.map_error_code(&e)));
                Ok(response.as_ssz_bytes())
            }
        }
    }

    async fn handle_snapshot_list(&self, _request: SnapshotListRequest) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        let snapshots = self.adapter.core().snapshot_list();
        let snapshot_infos: Vec<SnapshotInfo> = snapshots
            .into_iter()
            .map(|(id, name)| SnapshotInfo {
                id: id.to_string().into_bytes(),
                name: name.map(|s| s.into_bytes()),
            })
            .collect();

        let response = Response::snapshot_list(snapshot_infos);
        Ok(response.as_ssz_bytes())
    }

    async fn handle_branch_create(&self, request: BranchCreateRequest) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        let from_str = String::from_utf8_lossy(&request.from).to_string();
        let name_str = request.name.as_ref().map(|n| String::from_utf8_lossy(n).to_string());
        match self.adapter.core().branch_create_from_snapshot(
            from_str.parse().map_err(|_| "Invalid snapshot ID")?,
            name_str.as_deref()
        ) {
            Ok(branch_id) => {
                // Get branch info from the list
                let branches = self.adapter.core().branch_list();
                let info = branches.iter()
                    .find(|b| b.id == branch_id)
                    .ok_or("Branch not found")?;

                let response = Response::branch_create(BranchInfo {
                    id: info.id.to_string().into_bytes(),
                    name: info.name.clone().map(|s| s.into_bytes()),
                    parent: info.parent.map(|p| p.to_string()).unwrap_or_default().into_bytes(),
                });
                Ok(response.as_ssz_bytes())
            }
            Err(e) => {
                let response = Response::error(format!("{:?}", e), Some(self.map_error_code(&e)));
                Ok(response.as_ssz_bytes())
            }
        }
    }

    async fn handle_branch_bind(&self, request: BranchBindRequest) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        let pid = request.pid.unwrap_or(std::process::id());
        let branch_str = String::from_utf8_lossy(&request.branch).to_string();
        let branch_id = branch_str.parse().map_err(|_| "Invalid branch ID")?;

        match self.adapter.core().bind_process_to_branch_with_pid(branch_id, pid) {
            Ok(()) => {
                let response = Response::branch_bind(request.branch.clone(), pid);
                Ok(response.as_ssz_bytes())
            }
            Err(e) => {
                let response = Response::error(format!("{:?}", e), Some(self.map_error_code(&e)));
                Ok(response.as_ssz_bytes())
            }
        }
    }

    fn map_error_code(&self, error: &agentfs_core::FsError) -> u32 {
        match error {
            agentfs_core::FsError::NotFound => 2,      // ENOENT
            agentfs_core::FsError::AlreadyExists => 17, // EEXIST
            agentfs_core::FsError::AccessDenied => 13,  // EACCES
            agentfs_core::FsError::InvalidArgument => 22, // EINVAL
            agentfs_core::FsError::Busy => 16,          // EBUSY
            agentfs_core::FsError::NoSpace => 28,       // ENOSPC
            agentfs_core::FsError::Unsupported => 95,   // ENOTSUP
            _ => 5, // EIO
        }
    }
}

/// Stub XPC implementation for non-macOS
#[cfg(not(target_os = "macos"))]
pub struct XpcControlService;

#[cfg(not(target_os = "macos"))]
impl XpcControlService {
    pub fn new(_adapter: Arc<FsKitAdapter>, _service_name: String) -> Self {
        Self
    }

    pub async fn start(&self) -> Result<(), Box<dyn std::error::Error>> {
        Err("XPC is only available on macOS".into())
    }
}
