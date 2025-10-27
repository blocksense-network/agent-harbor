// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

use crate::transport::{
    ControlTransport, build_interpose_get_request, build_interpose_set_request,
    send_control_request,
};
use agentfs_proto::Response;
use ah_fs_snapshots::{
    FsSnapshotProvider, ProviderCapabilities, SnapshotProviderKind, provider_for,
};
use anyhow::{Result, anyhow};
use clap::{Args, Subcommand};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// JSON output for filesystem status
#[derive(Serialize, Deserialize)]
struct FsStatusJson {
    path: String,
    filesystem_type: String,
    mount_point: Option<String>,
    provider: String,
    capabilities: FsCapabilitiesJson,
    detection_notes: Vec<String>,
}

#[derive(Serialize, Deserialize)]
struct FsCapabilitiesJson {
    score: u8,
    supports_cow_overlay: bool,
}

#[derive(Args)]
pub struct StatusOptions {
    /// Path to analyze (default: current working directory)
    #[arg(short, long)]
    path: Option<PathBuf>,

    /// Emit machine-readable JSON output
    #[arg(long)]
    json: bool,

    /// Include detailed capability information
    #[arg(long)]
    verbose: bool,

    /// Only perform detection without provider selection
    #[arg(long)]
    detect_only: bool,
}

#[derive(Args)]
pub struct InitSessionOptions {
    /// Optional name for the initial snapshot
    #[arg(short, long)]
    name: Option<String>,

    /// Repository path (defaults to current directory)
    #[arg(short, long)]
    repo: Option<PathBuf>,

    /// Workspace name
    #[arg(short, long)]
    workspace: Option<String>,
}

#[derive(Args)]
pub struct SnapshotsOptions {
    /// Session ID (branch name or repo/branch)
    #[arg(value_name = "SESSION_ID")]
    session_id: String,
}

#[derive(Subcommand)]
pub enum BranchCommands {
    /// Create a new branch from a snapshot
    Create {
        /// Snapshot ID to create branch from
        #[arg(value_name = "SNAPSHOT_ID")]
        snapshot_id: String,

        /// Optional name for the branch
        #[arg(short, long)]
        name: Option<String>,
    },
    /// Bind current process to a branch
    Bind {
        /// Branch ID to bind to
        #[arg(value_name = "BRANCH_ID")]
        branch_id: String,
    },
    /// Execute command in branch context
    Exec {
        /// Branch ID to bind to
        #[arg(value_name = "BRANCH_ID")]
        branch_id: String,

        /// Command to execute
        #[arg(value_name = "COMMAND")]
        command: Vec<String>,
    },
}

#[derive(Subcommand)]
pub enum InterposeCommands {
    /// Get current interpose configuration
    Get {
        /// AgentFS mount point
        #[arg(long)]
        mount: Option<PathBuf>,
    },
    /// Set interpose configuration options
    Set {
        /// AgentFS mount point
        #[arg(long)]
        mount: Option<PathBuf>,

        /// Enable/disable interpose mode
        #[arg(long)]
        enabled: Option<bool>,

        /// Maximum file size for bounded copy (bytes)
        #[arg(long)]
        max_copy_bytes: Option<u64>,

        /// Require reflink support for forwarding
        #[arg(long)]
        require_reflink: Option<bool>,
    },
}

#[derive(Subcommand)]
pub enum AgentFsCommands {
    /// Run filesystem detection and report capabilities
    Status(StatusOptions),

    /// Create initial AgentFS snapshot for a session
    InitSession(InitSessionOptions),

    /// Create a snapshot at the current state
    Snapshot,

    /// List snapshots for a session
    Snapshots(SnapshotsOptions),

    /// Branch operations
    Branch {
        #[command(subcommand)]
        subcommand: BranchCommands,
    },

    /// Interpose configuration operations
    Interpose {
        #[command(subcommand)]
        subcommand: InterposeCommands,
    },
}

impl AgentFsCommands {
    pub async fn run(self) -> Result<()> {
        match self {
            AgentFsCommands::Status(opts) => Self::status(opts).await,
            AgentFsCommands::InitSession(opts) => Self::init_session(opts).await,
            AgentFsCommands::Snapshot => Self::snapshot().await,
            AgentFsCommands::Snapshots(opts) => Self::list_snapshots(opts).await,
            AgentFsCommands::Branch { subcommand } => match subcommand {
                BranchCommands::Create { snapshot_id, name } => {
                    Self::branch_create(snapshot_id, name).await
                }
                BranchCommands::Bind { branch_id } => Self::branch_bind(branch_id).await,
                BranchCommands::Exec { branch_id, command } => {
                    Self::branch_exec(branch_id, command).await
                }
            },
            AgentFsCommands::Interpose { subcommand } => match subcommand {
                InterposeCommands::Get { mount } => Self::interpose_get(mount).await,
                InterposeCommands::Set {
                    mount,
                    enabled,
                    max_copy_bytes,
                    require_reflink,
                } => Self::interpose_set(mount, enabled, max_copy_bytes, require_reflink).await,
            },
        }
    }

    async fn status(opts: StatusOptions) -> Result<()> {
        let path = opts.path.unwrap_or_else(|| std::env::current_dir().unwrap());

        // Detect filesystem capabilities
        let provider = provider_for(&path)?;
        let capabilities = provider.detect_capabilities(&path);

        if opts.detect_only {
            // Only show detection results
            if opts.json {
                let json = FsStatusJson {
                    path: path.display().to_string(),
                    filesystem_type: Self::detect_filesystem_type(&path),
                    mount_point: Self::detect_mount_point(&path),
                    provider: format!("{:?}", capabilities.kind),
                    capabilities: FsCapabilitiesJson {
                        score: capabilities.score,
                        supports_cow_overlay: capabilities.supports_cow_overlay,
                    },
                    detection_notes: capabilities.notes,
                };
                println!("{}", serde_json::to_string_pretty(&json)?);
            } else {
                println!("Filesystem detection for: {}", path.display());
                println!("Filesystem type: {}", Self::detect_filesystem_type(&path));
                if let Some(mount) = Self::detect_mount_point(&path) {
                    println!("Mount point: {}", mount);
                }
                println!("Provider: {:?}", capabilities.kind);
                println!("Capability score: {}", capabilities.score);
                if capabilities.supports_cow_overlay {
                    println!("Supports CoW overlay: yes");
                } else {
                    println!("Supports CoW overlay: no");
                }
                if !capabilities.notes.is_empty() {
                    println!("Detection notes:");
                    for note in &capabilities.notes {
                        println!("  - {}", note);
                    }
                }
            }
        } else {
            // Full provider selection
            if opts.json {
                let json = FsStatusJson {
                    path: path.display().to_string(),
                    filesystem_type: Self::detect_filesystem_type(&path),
                    mount_point: Self::detect_mount_point(&path),
                    provider: format!("{:?}", capabilities.kind),
                    capabilities: FsCapabilitiesJson {
                        score: capabilities.score,
                        supports_cow_overlay: capabilities.supports_cow_overlay,
                    },
                    detection_notes: capabilities.notes,
                };
                println!("{}", serde_json::to_string_pretty(&json)?);
            } else {
                println!("Filesystem status for: {}", path.display());
                println!("Filesystem type: {}", Self::detect_filesystem_type(&path));
                if let Some(mount) = Self::detect_mount_point(&path) {
                    println!("Mount point: {}", mount);
                }
                println!("Selected provider: {:?}", capabilities.kind);
                println!("Capability score: {}", capabilities.score);
                if capabilities.supports_cow_overlay {
                    println!("Supports CoW overlay: yes");
                } else {
                    println!("Supports CoW overlay: no");
                }
                if opts.verbose && !capabilities.notes.is_empty() {
                    println!("Detection notes:");
                    for note in &capabilities.notes {
                        println!("  - {}", note);
                    }
                }
            }
        }

        Ok(())
    }

    fn detect_filesystem_type(path: &PathBuf) -> String {
        // Simple filesystem type detection using /proc/mounts or similar
        // For now, return a placeholder
        "unknown".to_string()
    }

    fn detect_mount_point(path: &PathBuf) -> Option<String> {
        // Simple mount point detection
        // For now, return None
        None
    }

    async fn init_session(opts: InitSessionOptions) -> Result<()> {
        // TODO: Once AgentFS and database persistence are implemented, this will:
        // 1. Resolve repository path (default to current dir)
        // 2. Detect appropriate snapshot provider for the path
        // 3. Prepare writable workspace if needed
        // 4. Create initial snapshot using provider.snapshot_now()
        // 5. Record snapshot metadata in database

        let repo_path = opts.repo.unwrap_or_else(|| std::env::current_dir().unwrap());

        println!(
            "Initializing session snapshots for repository: {}",
            repo_path.display()
        );
        if let Some(name) = &opts.name {
            println!("Snapshot name: {}", name);
        }
        if let Some(workspace) = &opts.workspace {
            println!("Workspace: {}", workspace);
        }
        println!("Note: AgentFS and database persistence not yet implemented in this milestone");
        println!("When implemented, this will create initial filesystem snapshots for time travel");

        Ok(())
    }

    async fn snapshot() -> Result<()> {
        let repo_path = std::env::current_dir().unwrap();

        // For ZFS testing, create ZFS provider directly (like the integration tests)
        #[cfg(feature = "zfs")]
        {
            use ah_fs_snapshots::{FsSnapshotProvider, WorkingCopyMode};
            use ah_fs_snapshots_zfs::ZfsProvider;

            let zfs_provider = ZfsProvider::new();

            // Check if this path is on ZFS
            let capabilities = zfs_provider.detect_capabilities(&repo_path);
            if capabilities.score > 0 {
                // Create a minimal PreparedWorkspace for in-place mode
                let workspace = ah_fs_snapshots::PreparedWorkspace {
                    exec_path: repo_path.clone(),
                    working_copy: WorkingCopyMode::InPlace,
                    provider: zfs_provider.kind(),
                    cleanup_token: format!("test:inplace:{}", repo_path.display()),
                };

                // Create the snapshot
                let snapshot_ref = zfs_provider.snapshot_now(&workspace, Some("checkpoint"))?;

                // Check if we're running under `ah agent record` and should notify the recorder
                if let Some(ipc_socket) = std::env::var("AH_RECORDER_IPC_SOCKET").ok() {
                    println!("DEBUG: Notifying recorder at socket: {}", ipc_socket);
                    match Self::notify_recorder(
                        &ipc_socket,
                        snapshot_ref.id.parse::<u64>().unwrap_or(0),
                        snapshot_ref.label.clone().unwrap_or_default(),
                    )
                    .await
                    {
                        Ok(_) => println!("DEBUG: Successfully notified recorder"),
                        Err(e) => println!("DEBUG: Failed to notify recorder: {}", e),
                    }
                } else {
                    println!("DEBUG: AH_RECORDER_IPC_SOCKET not set, not notifying recorder");
                }

                // Output the snapshot information in a format that the mock agent can parse
                println!("Snapshot created: {}", snapshot_ref.id);
                println!("Provider: {:?}", snapshot_ref.provider);
                if let Some(label) = &snapshot_ref.label {
                    println!("Label: {}", label);
                }
            } else {
                println!("Not on ZFS filesystem, cannot create snapshots");
            }
        }

        // Fallback to the generic provider detection
        let provider = ah_fs_snapshots::provider_for(&repo_path)?;

        // Create a minimal PreparedWorkspace for in-place mode
        use ah_fs_snapshots::{PreparedWorkspace, SnapshotProviderKind, WorkingCopyMode};
        let workspace = PreparedWorkspace {
            exec_path: repo_path.clone(),
            working_copy: WorkingCopyMode::InPlace,
            provider: provider.kind(),
            cleanup_token: format!("test:inplace:{}", repo_path.display()),
        };

        // Create the snapshot
        let snapshot_ref = provider.snapshot_now(&workspace, Some("checkpoint"))?;

        // Check if we're running under `ah agent record` and should notify the recorder
        if let Some(ipc_socket) = std::env::var("AH_RECORDER_IPC_SOCKET").ok() {
            Self::notify_recorder(
                &ipc_socket,
                snapshot_ref.id.parse::<u64>().unwrap_or(0),
                snapshot_ref.label.clone().unwrap_or_default(),
            )
            .await?;
        }

        // Output the snapshot information in a format that the mock agent can parse
        println!("Snapshot created: {}", snapshot_ref.id);
        println!("Provider: {:?}", snapshot_ref.provider);
        if let Some(label) = &snapshot_ref.label {
            println!("Label: {}", label);
        }

        Ok(())
    }

    /// Notify the recorder about a new snapshot via IPC
    async fn notify_recorder(socket_path: &str, snapshot_id: u64, label: String) -> Result<()> {
        use ah_recorder::IpcClient;
        use std::path::PathBuf;

        let client = IpcClient::new(PathBuf::from(socket_path));

        match client.notify_snapshot(snapshot_id, label.clone()).await {
            Ok(response) => {
                if response.is_success() {
                    tracing::debug!(
                        "Notified recorder of snapshot {} at byte offset {}",
                        snapshot_id,
                        response.anchor_byte().unwrap_or(0)
                    );
                } else if let Some(error_msg) = response.error_message() {
                    tracing::warn!(
                        "Recorder returned error for snapshot {}: {}",
                        snapshot_id,
                        error_msg
                    );
                }
                Ok(())
            }
            Err(e) => {
                tracing::warn!("Failed to notify recorder: {}", e);
                // Don't fail the snapshot operation if IPC notification fails
                Ok(())
            }
        }
    }

    async fn list_snapshots(opts: SnapshotsOptions) -> Result<()> {
        // TODO: Once database persistence is implemented, this will:
        // 1. Parse session_id (branch name or repo/branch)
        // 2. Query fs_snapshots table to find snapshots for the session
        // 3. Display formatted list of snapshots with metadata

        println!("Snapshots for session '{}':", opts.session_id);
        println!("Note: Database persistence not yet implemented in this milestone");
        println!("When implemented, this will show:");
        println!("- Snapshot ID");
        println!("- Timestamp");
        println!("- Provider type");
        println!("- Reference/path");
        println!("- Optional labels and metadata");

        // For now, show that the command structure is ready
        println!(
            "\nCommand parsing successful for session: {}",
            opts.session_id
        );

        Ok(())
    }

    async fn branch_create(snapshot_id: String, name: Option<String>) -> Result<()> {
        // TODO: Once AgentFS integration is implemented, this will:
        // 1. Validate snapshot_id exists
        // 2. Get the provider for the snapshot
        // 3. Call provider.branch_from_snapshot() to create writable branch
        // 4. Record branch metadata in database

        println!("Creating branch from snapshot '{}'", snapshot_id);
        if let Some(name) = &name {
            println!("Branch name: {}", name);
        }
        println!("Note: AgentFS integration not yet implemented in this milestone");
        println!("When implemented, this will create a writable branch for time travel");

        Ok(())
    }

    async fn branch_bind(branch_id: String) -> Result<()> {
        // TODO: Once AgentFS integration is implemented, this will:
        // 1. Validate branch_id exists
        // 2. Bind the current process to the branch view
        // 3. Set up the filesystem overlay for the process

        println!("Binding to branch '{}'", branch_id);
        println!("Note: AgentFS process binding not yet implemented in this milestone");
        println!("When implemented, this will make the branch view available to child processes");

        Ok(())
    }

    async fn branch_exec(branch_id: String, command: Vec<String>) -> Result<()> {
        // TODO: Once AgentFS integration is implemented, this will:
        // 1. Bind to the specified branch
        // 2. Execute the command in that branch context
        // 3. Return the command's exit status

        println!("Executing command in branch '{}' context", branch_id);
        println!("Command: {:?}", command);
        println!("Note: AgentFS branch execution not yet implemented in this milestone");
        println!("When implemented, this will run the command with the branch filesystem view");

        Ok(())
    }

    async fn interpose_get(mount: Option<PathBuf>) -> Result<()> {
        let mount_point =
            mount.ok_or_else(|| anyhow!("Mount point is required for interpose operations"))?;

        // Create control transport
        let transport = ControlTransport::new(mount_point)?;

        // Build and send get request for each configuration key
        let keys = ["enabled", "max_copy_bytes", "require_reflink"];

        println!("Current interpose configuration:");

        for key in &keys {
            let request = build_interpose_get_request(key.to_string());
            match send_control_request(transport.clone(), request).await {
                Ok(response) => match response {
                    Response::InterposeSetGet(response) => {
                        let value = String::from_utf8(response.value)
                            .map_err(|e| anyhow!("Invalid UTF-8 in response: {}", e))?;
                        println!("- {}: {}", key, value);
                    }
                    Response::Error(error_response) => {
                        println!(
                            "- {}: Error {}",
                            key,
                            String::from_utf8_lossy(&error_response.error)
                        );
                    }
                    _ => {
                        println!("- {}: Unexpected response type", key);
                    }
                },
                Err(e) => {
                    println!("- {}: Failed to query ({})", key, e);
                }
            }
        }

        Ok(())
    }

    async fn interpose_set(
        mount: Option<PathBuf>,
        enabled: Option<bool>,
        max_copy_bytes: Option<u64>,
        require_reflink: Option<bool>,
    ) -> Result<()> {
        let mount_point =
            mount.ok_or_else(|| anyhow!("Mount point is required for interpose operations"))?;

        // Create control transport
        let transport = ControlTransport::new(mount_point)?;

        println!("Setting interpose configuration:");

        // Send set requests for each provided option
        if let Some(enabled) = enabled {
            let value = enabled.to_string();
            let request = build_interpose_set_request("enabled".to_string(), value.clone());
            match send_control_request(transport.clone(), request).await {
                Ok(response) => match response {
                    Response::InterposeSetGet(response) => {
                        let updated_value = String::from_utf8(response.value)
                            .map_err(|e| anyhow!("Invalid UTF-8 in response: {}", e))?;
                        println!("- enabled: {} (confirmed: {})", value, updated_value);
                    }
                    Response::Error(error_response) => {
                        println!(
                            "- enabled: Failed to set - {}",
                            String::from_utf8_lossy(&error_response.error)
                        );
                    }
                    _ => {
                        println!("- enabled: Unexpected response type");
                    }
                },
                Err(e) => {
                    println!("- enabled: Failed to set ({})", e);
                }
            }
        }

        if let Some(max_copy_bytes) = max_copy_bytes {
            let value = max_copy_bytes.to_string();
            let request = build_interpose_set_request("max_copy_bytes".to_string(), value.clone());
            match send_control_request(transport.clone(), request).await {
                Ok(response) => match response {
                    Response::InterposeSetGet(response) => {
                        let updated_value = String::from_utf8(response.value)
                            .map_err(|e| anyhow!("Invalid UTF-8 in response: {}", e))?;
                        println!("- max_copy_bytes: {} (confirmed: {})", value, updated_value);
                    }
                    Response::Error(error_response) => {
                        println!(
                            "- max_copy_bytes: Failed to set - {}",
                            String::from_utf8_lossy(&error_response.error)
                        );
                    }
                    _ => {
                        println!("- max_copy_bytes: Unexpected response type");
                    }
                },
                Err(e) => {
                    println!("- max_copy_bytes: Failed to set ({})", e);
                }
            }
        }

        if let Some(require_reflink) = require_reflink {
            let value = require_reflink.to_string();
            let request = build_interpose_set_request("require_reflink".to_string(), value.clone());
            match send_control_request(transport.clone(), request).await {
                Ok(response) => match response {
                    Response::InterposeSetGet(response) => {
                        let updated_value = String::from_utf8(response.value)
                            .map_err(|e| anyhow!("Invalid UTF-8 in response: {}", e))?;
                        println!(
                            "- require_reflink: {} (confirmed: {})",
                            value, updated_value
                        );
                    }
                    Response::Error(error_response) => {
                        println!(
                            "- require_reflink: Failed to set - {}",
                            String::from_utf8_lossy(&error_response.error)
                        );
                    }
                    _ => {
                        println!("- require_reflink: Unexpected response type");
                    }
                },
                Err(e) => {
                    println!("- require_reflink: Failed to set ({})", e);
                }
            }
        }

        if enabled.is_none() && max_copy_bytes.is_none() && require_reflink.is_none() {
            println!("No configuration options specified to set");
        }

        Ok(())
    }
}
