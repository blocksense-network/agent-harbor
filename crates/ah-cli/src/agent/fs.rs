// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

#[cfg(feature = "agentfs")]
use crate::transport::{
    ControlTransport, build_interpose_get_request, build_interpose_set_request,
    send_control_request,
};
#[cfg(feature = "agentfs")]
use agentfs_proto::*;
#[cfg(feature = "agentfs")]
use ah_fs_snapshots::{AgentFsProvider, FsSnapshotProvider};
use ah_fs_snapshots::{ProviderCapabilities, provider_for};
use anyhow::{Result, anyhow};
use clap::{Args, Subcommand};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tracing::debug;
/// JSON output for filesystem status
#[derive(Serialize, Deserialize, Clone)]
struct FsCapabilitiesJson {
    score: u8,
    supports_cow_overlay: bool,
}

#[derive(Serialize, Deserialize, Clone)]
struct ProviderStatusJson {
    name: String,
    capabilities: FsCapabilitiesJson,
    detection_notes: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone)]
struct FsStatusJson {
    path: String,
    filesystem_type: String,
    mount_point: Option<String>,
    selected: ProviderStatusJson,
    #[cfg(feature = "agentfs")]
    agentfs: Option<ProviderStatusJson>,
}

fn build_provider_status(caps: &ProviderCapabilities) -> ProviderStatusJson {
    ProviderStatusJson {
        name: format!("{:?}", caps.kind),
        capabilities: FsCapabilitiesJson {
            score: caps.score,
            supports_cow_overlay: caps.supports_cow_overlay,
        },
        detection_notes: caps.notes.clone(),
    }
}

#[derive(Args, Clone)]
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

#[derive(Args, Clone)]
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

#[derive(Args, Clone)]
pub struct SnapshotsOptions {
    /// Session ID (branch name or repo/branch)
    #[arg(value_name = "SESSION_ID")]
    session_id: String,
}

#[derive(Args, Clone)]
pub struct SnapshotOptions {
    /// Recorder IPC socket path for notification (optional)
    #[arg(long)]
    recorder_socket: Option<String>,
}

#[derive(Subcommand, Clone)]
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

#[cfg(feature = "agentfs")]
#[derive(Subcommand, Clone)]
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

#[derive(Clone, Subcommand)]
pub enum AgentFsCommands {
    /// Run filesystem detection and report capabilities
    Status(StatusOptions),

    /// Create initial AgentFS snapshot for a session
    InitSession(InitSessionOptions),

    /// Create a snapshot at the current state
    Snapshot(SnapshotOptions),

    /// List snapshots for a session
    Snapshots(SnapshotsOptions),

    /// Branch operations
    Branch {
        #[command(subcommand)]
        subcommand: BranchCommands,
    },

    /// Interpose configuration operations
    #[cfg(feature = "agentfs")]
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
            AgentFsCommands::Snapshot(opts) => Self::snapshot(opts).await,
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
            #[cfg(feature = "agentfs")]
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

        let provider = provider_for(&path)?;
        let capabilities = provider.detect_capabilities(&path);
        let selected_status = build_provider_status(&capabilities);
        #[cfg(feature = "agentfs")]
        let selected_is_agentfs = selected_status.name == "AgentFs";

        #[cfg(feature = "agentfs")]
        let agentfs_status = {
            let caps = AgentFsProvider::new().detect_capabilities(&path);
            Some(build_provider_status(&caps))
        };

        if opts.detect_only {
            if opts.json {
                let json = FsStatusJson {
                    path: path.display().to_string(),
                    filesystem_type: Self::detect_filesystem_type(&path),
                    mount_point: Self::detect_mount_point(&path),
                    selected: selected_status.clone(),
                    #[cfg(feature = "agentfs")]
                    agentfs: agentfs_status.clone(),
                };
                println!("{}", serde_json::to_string_pretty(&json)?);
            } else {
                println!("Filesystem detection for: {}", path.display());
                println!("Filesystem type: {}", Self::detect_filesystem_type(&path));
                if let Some(mount) = Self::detect_mount_point(&path) {
                    println!("Mount point: {}", mount);
                }
                Self::print_provider_status("Selected provider", &selected_status, opts.verbose);
                #[cfg(feature = "agentfs")]
                if let Some(agentfs) = &agentfs_status {
                    println!();
                    Self::print_provider_status("AgentFS provider (experimental)", agentfs, true);
                }
            }
            return Ok(());
        }

        if opts.json {
            let json = FsStatusJson {
                path: path.display().to_string(),
                filesystem_type: Self::detect_filesystem_type(&path),
                mount_point: Self::detect_mount_point(&path),
                selected: selected_status.clone(),
                #[cfg(feature = "agentfs")]
                agentfs: agentfs_status.clone(),
            };
            println!("{}", serde_json::to_string_pretty(&json)?);
            return Ok(());
        }

        println!("Filesystem status for: {}", path.display());
        println!("Filesystem type: {}", Self::detect_filesystem_type(&path));
        if let Some(mount) = Self::detect_mount_point(&path) {
            println!("Mount point: {}", mount);
        }

        Self::print_provider_status("Selected provider", &selected_status, opts.verbose);

        #[cfg(feature = "agentfs")]
        if let Some(agentfs) = &agentfs_status {
            if !selected_is_agentfs {
                println!();
                Self::print_provider_status("AgentFS provider (experimental)", agentfs, true);
            }
        }

        Ok(())
    }

    fn print_provider_status(title: &str, provider: &ProviderStatusJson, verbose: bool) {
        println!("{title}: {}", provider.name);
        println!("Capability score: {}", provider.capabilities.score);
        println!(
            "Supports CoW overlay: {}",
            if provider.capabilities.supports_cow_overlay {
                "yes"
            } else {
                "no"
            }
        );

        if verbose || title.contains("AgentFS") {
            if !provider.detection_notes.is_empty() {
                println!("Notes:");
                for note in &provider.detection_notes {
                    println!("  - {}", note);
                }
            }
        }
    }

    fn detect_filesystem_type(_path: &PathBuf) -> String {
        // Simple filesystem type detection using /proc/mounts or similar
        // For now, return a placeholder
        "unknown".to_string()
    }

    fn detect_mount_point(_path: &PathBuf) -> Option<String> {
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

    async fn snapshot(opts: SnapshotOptions) -> Result<()> {
        let repo_path = std::env::current_dir().unwrap();

        // For ZFS testing, create ZFS provider directly (like the integration tests)
        // Fallback to the generic provider detection
        let provider = ah_fs_snapshots::provider_for(&repo_path)?;
        let snapshot_label = uuid::Uuid::new_v4().to_string();

        // Check if we should notify the recorder (via IPC socket parameter)
        if let Some(recorder_socket) = opts.recorder_socket {
            // Notify recorder before creating the snapshot (with placeholder ID = 0)
            match Self::notify_recorder(
                &recorder_socket,
                0, // placeholder snapshot ID
                &snapshot_label.as_str(),
            )
            .await
            {
                Ok(_) => tracing::debug!("DEBUG: Successfully notified recorder"),
                Err(e) => tracing::debug!("DEBUG: Failed to notify recorder: {}", e),
            }
        }

        // Create a minimal PreparedWorkspace for in-place mode
        use ah_fs_snapshots::{PreparedWorkspace, WorkingCopyMode};
        let workspace = PreparedWorkspace {
            exec_path: repo_path.clone(),
            working_copy: WorkingCopyMode::InPlace,
            provider: provider.kind(),
            cleanup_token: format!("test:inplace:{}", repo_path.display()),
        };

        // Create the actual filesystem snapshot
        let snapshot_ref = provider.snapshot_now(&workspace, Some(&snapshot_label))?;

        // Output the snapshot information in a format that the mock agent can parse
        debug!("Snapshot created: {}", snapshot_ref.id);
        debug!("Provider: {:?}", snapshot_ref.provider);
        if let Some(label) = &snapshot_ref.label {
            debug!("Label: {}", label);
        }

        Ok(())
    }

    /// Notify the recorder about a new snapshot via IPC (legacy single-phase)
    async fn notify_recorder(socket_path: &str, snapshot_id: u64, label: &str) -> Result<()> {
        use ah_recorder::IpcClient;
        use std::path::PathBuf;

        let client = IpcClient::new(PathBuf::from(socket_path));

        match client.notify_snapshot(snapshot_id, label.to_string()).await {
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

    #[cfg(feature = "agentfs")]
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

    #[cfg(feature = "agentfs")]
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
