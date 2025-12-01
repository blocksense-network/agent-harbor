// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

#[cfg(feature = "agentfs")]
use crate::transport::{
    ControlTransport, build_branch_bind_request, build_branch_create_request,
    build_interpose_get_request, build_interpose_set_request, build_snapshot_list_request,
    send_control_request,
};
#[cfg(feature = "agentfs")]
use agentfs_proto::*;
#[cfg(feature = "agentfs")]
use ah_fs_snapshots::{AgentFsProvider, FsSnapshotProvider};
use ah_fs_snapshots::{ProviderCapabilities, provider_for};
use anyhow::Result;
#[cfg(feature = "agentfs")]
use anyhow::anyhow; // Needed for anyhow! macro used in interpose operations
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
    /// Session ID (branch name or repo/branch) - currently ignored, lists all snapshots
    #[arg(value_name = "SESSION_ID")]
    session_id: Option<String>,

    /// AgentFS mount point (defaults to $XDG_RUNTIME_DIR/agentfs)
    #[arg(long)]
    mount: Option<PathBuf>,

    /// Emit machine-readable JSON output
    #[arg(long)]
    json: bool,
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

        /// AgentFS mount point (defaults to $XDG_RUNTIME_DIR/agentfs)
        #[arg(long)]
        mount: Option<PathBuf>,
    },
    /// Bind current process to a branch
    Bind {
        /// Branch ID to bind to
        #[arg(value_name = "BRANCH_ID")]
        branch_id: String,

        /// Process ID to bind (defaults to current process)
        #[arg(long)]
        pid: Option<u32>,

        /// AgentFS mount point (defaults to $XDG_RUNTIME_DIR/agentfs)
        #[arg(long)]
        mount: Option<PathBuf>,
    },
    /// Execute command in branch context
    Exec {
        /// Branch ID to bind to
        #[arg(value_name = "BRANCH_ID")]
        branch_id: String,

        /// AgentFS mount point (defaults to $XDG_RUNTIME_DIR/agentfs)
        #[arg(long)]
        mount: Option<PathBuf>,

        /// Command to execute
        #[arg(value_name = "COMMAND", trailing_var_arg = true, required = true)]
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
                BranchCommands::Create {
                    snapshot_id,
                    name,
                    mount,
                } => Self::branch_create(snapshot_id, name, mount).await,
                BranchCommands::Bind {
                    branch_id,
                    pid,
                    mount,
                } => Self::branch_bind(branch_id, pid, mount).await,
                BranchCommands::Exec {
                    branch_id,
                    mount,
                    command,
                } => Self::branch_exec(branch_id, mount, command).await,
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

    /// Get the default AgentFS mount point.
    ///
    /// Uses `$XDG_RUNTIME_DIR/agentfs` if available (typically `/run/user/<uid>/agentfs`),
    /// otherwise falls back to `/tmp/agentfs`.
    ///
    /// We prefer XDG_RUNTIME_DIR because:
    /// 1. It's the XDG standard for runtime files
    /// 2. It's per-user and secure (mode 0700)
    /// 3. It's outside `/tmp`, so sandbox `/tmp` isolation won't hide the FUSE mount
    fn default_mount_point() -> PathBuf {
        if let Ok(runtime_dir) = std::env::var("XDG_RUNTIME_DIR") {
            PathBuf::from(runtime_dir).join("agentfs")
        } else {
            // Fallback for systems without XDG_RUNTIME_DIR
            PathBuf::from("/tmp/agentfs")
        }
    }

    /// Get the mount point from an optional override or use the default
    fn get_mount_point(mount: Option<PathBuf>) -> PathBuf {
        mount.unwrap_or_else(Self::default_mount_point)
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
                tracing::info!(target: "ah-cli", "{}", serde_json::to_string_pretty(&json)?);
            } else {
                tracing::info!(target: "ah-cli", "Filesystem detection for: {}", path.display());
                tracing::info!(target: "ah-cli", "Filesystem type: {}", Self::detect_filesystem_type(&path));
                if let Some(mount) = Self::detect_mount_point(&path) {
                    tracing::info!(target: "ah-cli", "Mount point: {}", mount);
                }
                Self::print_provider_status("Selected provider", &selected_status, opts.verbose);
                #[cfg(feature = "agentfs")]
                if let Some(agentfs) = &agentfs_status {
                    tracing::info!(target: "ah-cli", "");
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
            tracing::info!(target: "ah-cli", "{}", serde_json::to_string_pretty(&json)?);
            return Ok(());
        }

        tracing::info!(target: "ah-cli", "Filesystem status for: {}", path.display());
        tracing::info!(target: "ah-cli", "Filesystem type: {}", Self::detect_filesystem_type(&path));
        if let Some(mount) = Self::detect_mount_point(&path) {
            tracing::info!(target: "ah-cli", "Mount point: {}", mount);
        }

        Self::print_provider_status("Selected provider", &selected_status, opts.verbose);

        #[cfg(feature = "agentfs")]
        if let Some(agentfs) = &agentfs_status {
            if !selected_is_agentfs {
                tracing::info!(target: "ah-cli", "");
                Self::print_provider_status("AgentFS provider (experimental)", agentfs, true);
            }
        }

        Ok(())
    }

    fn print_provider_status(title: &str, provider: &ProviderStatusJson, verbose: bool) {
        tracing::info!(target: "ah-cli", "{title}: {}", provider.name);
        tracing::info!(target: "ah-cli", "Capability score: {}", provider.capabilities.score);
        tracing::info!(target: "ah-cli", "Supports CoW overlay: {}",
            if provider.capabilities.supports_cow_overlay { "yes" } else { "no" }
        );

        if (verbose || title.contains("AgentFS")) && !provider.detection_notes.is_empty() {
            tracing::info!(target: "ah-cli", "Notes:");
            for note in &provider.detection_notes {
                tracing::info!(target: "ah-cli", "  - {}", note);
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

        tracing::info!(target: "ah-cli", "Initializing session snapshots for repository: {}", repo_path.display());
        if let Some(name) = &opts.name {
            tracing::info!(target: "ah-cli", "Snapshot name: {}", name);
        }
        if let Some(workspace) = &opts.workspace {
            tracing::info!(target: "ah-cli", "Workspace: {}", workspace);
        }
        tracing::info!(target: "ah-cli", "Note: AgentFS and database persistence not yet implemented in this milestone");
        tracing::info!(target: "ah-cli", "When implemented, this will create initial filesystem snapshots for time travel");

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
                snapshot_label.as_str(),
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
        #[cfg(feature = "agentfs")]
        {
            let mount_point = Self::get_mount_point(opts.mount);
            let control_path = mount_point.join(".agentfs").join("control");

            if !control_path.exists() {
                return Err(anyhow!(
                    "AgentFS control file not found at {:?}. Is the filesystem mounted?",
                    control_path
                ));
            }

            let transport = ControlTransport::new(mount_point.clone())?;
            let request = build_snapshot_list_request();

            match send_control_request(transport, request).await {
                Ok(Response::SnapshotList(list)) => {
                    if opts.json {
                        // JSON output
                        let snapshots: Vec<serde_json::Value> = list
                            .snapshots
                            .iter()
                            .map(|s| {
                                let id = String::from_utf8_lossy(&s.id).into_owned();
                                let name = s
                                    .name
                                    .as_ref()
                                    .map(|n| String::from_utf8_lossy(n).into_owned());
                                serde_json::json!({
                                    "id": id,
                                    "name": name
                                })
                            })
                            .collect();
                        tracing::info!(target: "ah-cli", "{}", serde_json::to_string_pretty(&snapshots)?);
                    } else {
                        // Human-readable output
                        if list.snapshots.is_empty() {
                            tracing::info!(target: "ah-cli", "No snapshots found");
                        } else {
                            tracing::info!(target: "ah-cli", "Snapshots:");
                            for snapshot in &list.snapshots {
                                let id = String::from_utf8_lossy(&snapshot.id);
                                let name = snapshot
                                    .name
                                    .as_ref()
                                    .map(|n| String::from_utf8_lossy(n).into_owned())
                                    .unwrap_or_else(|| "-".to_string());
                                tracing::info!(target: "ah-cli", "  SNAPSHOT\t{}\t{}", id, name);
                            }
                        }
                    }
                    Ok(())
                }
                Ok(Response::Error(err)) => Err(anyhow!(
                    "snapshot_list failed: {} (errno={})",
                    String::from_utf8_lossy(&err.error),
                    err.code.unwrap_or_default()
                )),
                Ok(other) => Err(anyhow!("unexpected response: {:?}", other)),
                Err(e) => Err(anyhow!("control request failed: {}", e)),
            }
        }

        #[cfg(not(feature = "agentfs"))]
        {
            let _ = opts;
            Err(anyhow::anyhow!(
                "AgentFS support not compiled in; enable the 'agentfs' feature"
            ))
        }
    }

    async fn branch_create(
        snapshot_id: String,
        name: Option<String>,
        mount: Option<PathBuf>,
    ) -> Result<()> {
        #[cfg(feature = "agentfs")]
        {
            let mount_point = Self::get_mount_point(mount);
            let control_path = mount_point.join(".agentfs").join("control");

            if !control_path.exists() {
                return Err(anyhow!(
                    "AgentFS control file not found at {:?}. Is the filesystem mounted?",
                    control_path
                ));
            }

            let transport = ControlTransport::new(mount_point.clone())?;
            let request = build_branch_create_request(snapshot_id.clone(), name.clone());

            match send_control_request(transport, request).await {
                Ok(Response::BranchCreate(resp)) => {
                    let branch_id = String::from_utf8_lossy(&resp.branch.id);
                    let branch_name = resp
                        .branch
                        .name
                        .as_ref()
                        .map(|n| String::from_utf8_lossy(n).into_owned())
                        .unwrap_or_else(|| "-".to_string());

                    if branch_name != "-" {
                        tracing::info!(target: "ah-cli", "BRANCH_ID={}\tNAME={}", branch_id, branch_name);
                    } else {
                        tracing::info!(target: "ah-cli", "BRANCH_ID={}", branch_id);
                    }
                    Ok(())
                }
                Ok(Response::Error(err)) => Err(anyhow!(
                    "branch_create failed: {} (errno={})",
                    String::from_utf8_lossy(&err.error),
                    err.code.unwrap_or_default()
                )),
                Ok(other) => Err(anyhow!("unexpected response: {:?}", other)),
                Err(e) => Err(anyhow!("control request failed: {}", e)),
            }
        }

        #[cfg(not(feature = "agentfs"))]
        {
            let _ = (snapshot_id, name, mount);
            Err(anyhow::anyhow!(
                "AgentFS support not compiled in; enable the 'agentfs' feature"
            ))
        }
    }

    async fn branch_bind(
        branch_id: String,
        pid: Option<u32>,
        mount: Option<PathBuf>,
    ) -> Result<()> {
        #[cfg(feature = "agentfs")]
        {
            let mount_point = Self::get_mount_point(mount);
            let control_path = mount_point.join(".agentfs").join("control");

            if !control_path.exists() {
                return Err(anyhow!(
                    "AgentFS control file not found at {:?}. Is the filesystem mounted?",
                    control_path
                ));
            }

            // Use provided PID or default to current process
            let target_pid = pid.unwrap_or_else(std::process::id);

            let transport = ControlTransport::new(mount_point.clone())?;
            let request = build_branch_bind_request(branch_id.clone(), Some(target_pid));

            match send_control_request(transport, request).await {
                Ok(Response::BranchBind(resp)) => {
                    tracing::info!(target: "ah-cli", "BRANCH_BIND_OK\tBRANCH={}\tPID={}",
                        String::from_utf8_lossy(&resp.branch),
                        resp.pid
                    );
                    Ok(())
                }
                Ok(Response::Error(err)) => Err(anyhow!(
                    "branch_bind failed: {} (errno={})",
                    String::from_utf8_lossy(&err.error),
                    err.code.unwrap_or_default()
                )),
                Ok(other) => Err(anyhow!("unexpected response: {:?}", other)),
                Err(e) => Err(anyhow!("control request failed: {}", e)),
            }
        }

        #[cfg(not(feature = "agentfs"))]
        {
            let _ = (branch_id, pid, mount);
            Err(anyhow::anyhow!(
                "AgentFS support not compiled in; enable the 'agentfs' feature"
            ))
        }
    }

    async fn branch_exec(
        branch_id: String,
        mount: Option<PathBuf>,
        command: Vec<String>,
    ) -> Result<()> {
        #[cfg(feature = "agentfs")]
        {
            use std::process::Command;

            if command.is_empty() {
                return Err(anyhow!("No command specified to execute"));
            }

            let mount_point = Self::get_mount_point(mount);
            let control_path = mount_point.join(".agentfs").join("control");

            if !control_path.exists() {
                return Err(anyhow!(
                    "AgentFS control file not found at {:?}. Is the filesystem mounted?",
                    control_path
                ));
            }

            // First, bind the current process to the branch
            let transport = ControlTransport::new(mount_point.clone())?;
            let current_pid = std::process::id();
            let request = build_branch_bind_request(branch_id.clone(), Some(current_pid));

            match send_control_request(transport, request).await {
                Ok(Response::BranchBind(_)) => {
                    debug!(
                        "Successfully bound PID {} to branch {}",
                        current_pid, branch_id
                    );
                }
                Ok(Response::Error(err)) => {
                    return Err(anyhow!(
                        "branch_bind failed: {} (errno={})",
                        String::from_utf8_lossy(&err.error),
                        err.code.unwrap_or_default()
                    ));
                }
                Ok(other) => {
                    return Err(anyhow!("unexpected response: {:?}", other));
                }
                Err(e) => {
                    return Err(anyhow!("control request failed: {}", e));
                }
            }

            // Execute the command
            // The child process should inherit the branch binding
            let program = &command[0];
            let args = if command.len() > 1 {
                &command[1..]
            } else {
                &[]
            };

            debug!("Executing command: {} {:?}", program, args);

            let status = Command::new(program)
                .args(args)
                .status()
                .map_err(|e| anyhow!("failed to execute command '{}': {}", program, e))?;

            if status.success() {
                Ok(())
            } else {
                let code = status.code().unwrap_or(1);
                Err(anyhow!("command exited with status {}", code))
            }
        }

        #[cfg(not(feature = "agentfs"))]
        {
            let _ = (branch_id, mount, command);
            Err(anyhow::anyhow!(
                "AgentFS support not compiled in; enable the 'agentfs' feature"
            ))
        }
    }

    #[cfg(feature = "agentfs")]
    async fn interpose_get(mount: Option<PathBuf>) -> Result<()> {
        let mount_point =
            mount.ok_or_else(|| anyhow!("Mount point is required for interpose operations"))?;

        // Create control transport
        let transport = ControlTransport::new(mount_point)?;

        // Build and send get request for each configuration key
        let keys = ["enabled", "max_copy_bytes", "require_reflink"];

        tracing::info!(target: "ah-cli", "Current interpose configuration:");

        for key in &keys {
            let request = build_interpose_get_request(key.to_string());
            match send_control_request(transport.clone(), request).await {
                Ok(response) => match response {
                    Response::InterposeSetGet(response) => {
                        let value = String::from_utf8(response.value)
                            .map_err(|e| anyhow!("Invalid UTF-8 in response: {}", e))?;
                        tracing::info!(target: "ah-cli", "- {}: {}", key, value);
                    }
                    Response::Error(error_response) => {
                        tracing::info!(target: "ah-cli", "- {}: Error {}", key, String::from_utf8_lossy(&error_response.error));
                    }
                    _ => {
                        tracing::info!(target: "ah-cli", "- {}: Unexpected response type", key);
                    }
                },
                Err(e) => {
                    tracing::info!(target: "ah-cli", "- {}: Failed to query ({})", key, e);
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

        tracing::info!(target: "ah-cli", "Setting interpose configuration:");

        // Send set requests for each provided option
        if let Some(enabled) = enabled {
            let value = enabled.to_string();
            let request = build_interpose_set_request("enabled".to_string(), value.clone());
            match send_control_request(transport.clone(), request).await {
                Ok(response) => match response {
                    Response::InterposeSetGet(response) => {
                        let updated_value = String::from_utf8(response.value)
                            .map_err(|e| anyhow!("Invalid UTF-8 in response: {}", e))?;
                        tracing::info!(target: "ah-cli", "- enabled: {} (confirmed: {})", value, updated_value);
                    }
                    Response::Error(error_response) => {
                        tracing::info!(target: "ah-cli", "- enabled: Failed to set - {}", String::from_utf8_lossy(&error_response.error));
                    }
                    _ => {
                        tracing::info!(target: "ah-cli", "- enabled: Unexpected response type");
                    }
                },
                Err(e) => {
                    tracing::info!(target: "ah-cli", "- enabled: Failed to set ({})", e);
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
                        tracing::info!(target: "ah-cli", "- max_copy_bytes: {} (confirmed: {})", value, updated_value);
                    }
                    Response::Error(error_response) => {
                        tracing::info!(target: "ah-cli", "- max_copy_bytes: Failed to set - {}", String::from_utf8_lossy(&error_response.error));
                    }
                    _ => {
                        tracing::info!(target: "ah-cli", "- max_copy_bytes: Unexpected response type");
                    }
                },
                Err(e) => {
                    tracing::info!(target: "ah-cli", "- max_copy_bytes: Failed to set ({})", e);
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
                        tracing::info!(target: "ah-cli", "- require_reflink: {} (confirmed: {})", value, updated_value);
                    }
                    Response::Error(error_response) => {
                        tracing::info!(target: "ah-cli", "- require_reflink: Failed to set - {}", String::from_utf8_lossy(&error_response.error));
                    }
                    _ => {
                        tracing::info!(target: "ah-cli", "- require_reflink: Unexpected response type");
                    }
                },
                Err(e) => {
                    tracing::info!(target: "ah-cli", "- require_reflink: Failed to set ({})", e);
                }
            }
        }

        if enabled.is_none() && max_copy_bytes.is_none() && require_reflink.is_none() {
            tracing::info!(target: "ah-cli", "No configuration options specified to set");
        }

        Ok(())
    }
}
