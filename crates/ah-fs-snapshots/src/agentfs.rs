// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! AgentFS snapshot provider implementation.
//!
//! The provider bootstraps a lightweight AgentFS daemon (macOS only for now),
//! issues control-plane requests over the SSZ protocol to create snapshots and
//! branches, and tracks per-workspace cleanup tokens so the orchestrator can
//! tear down resources even after crashes.

use ah_fs_snapshots_traits::{
    Error, FsSnapshotProvider, PreparedWorkspace, ProviderCapabilities, Result, SnapshotInfo,
    SnapshotProviderKind, SnapshotRef, WorkingCopyMode, generate_unique_id,
};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

#[cfg_attr(feature = "agentfs", allow(dead_code))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PlatformSupport {
    Supported,
    Unsupported(&'static str),
}

impl PlatformSupport {
    #[cfg(feature = "agentfs")]
    fn detect() -> Self {
        PlatformSupport::Supported
    }

    #[cfg(not(feature = "agentfs"))]
    fn detect() -> Self {
        PlatformSupport::Unsupported("AgentFS provider not compiled; enable the `agentfs` feature")
    }
}

const META_SESSION_TOKEN: &str = "agentfs.session.token";
const META_BRANCH_ID: &str = "agentfs.branch.id";
const META_SOCKET_PATH: &str = "agentfs.socket.path";
const META_SNAPSHOT_ID: &str = "agentfs.snapshot.id";

pub struct AgentFsProvider {
    platform: PlatformSupport,
    state: Arc<Mutex<AgentFsState>>,
}

impl AgentFsProvider {
    pub fn new() -> Self {
        Self {
            platform: PlatformSupport::detect(),
            state: Arc::new(Mutex::new(AgentFsState::default())),
        }
    }

    fn experimental_flag_enabled() -> bool {
        std::env::var("AH_ENABLE_AGENTFS_PROVIDER")
            .map(|value| value != "0")
            .unwrap_or(false)
    }

    #[cfg(feature = "agentfs")]
    fn unsupported_mode(mode: WorkingCopyMode) -> Result<PreparedWorkspace> {
        Err(Error::provider(format!(
            "AgentFS provider does not support {:?} working-copy mode yet",
            mode
        )))
    }
}

impl FsSnapshotProvider for AgentFsProvider {
    fn kind(&self) -> SnapshotProviderKind {
        SnapshotProviderKind::AgentFs
    }

    fn detect_capabilities(&self, _repo: &Path) -> ProviderCapabilities {
        match self.platform {
            PlatformSupport::Supported => {
                let mut notes = Vec::new();
                notes.push("AgentFS control-plane integration available on this platform".into());
                if !Self::experimental_flag_enabled() {
                    notes.push(
                        "Set AH_ENABLE_AGENTFS_PROVIDER=1 to opt into AgentFS provider experiments."
                            .into(),
                    );
                }

                ProviderCapabilities {
                    kind: self.kind(),
                    score: if Self::experimental_flag_enabled() {
                        70
                    } else {
                        0
                    },
                    supports_cow_overlay: true,
                    notes,
                }
            }
            PlatformSupport::Unsupported(reason) => ProviderCapabilities {
                kind: self.kind(),
                score: 0,
                supports_cow_overlay: false,
                notes: vec![reason.to_string()],
            },
        }
    }

    fn prepare_writable_workspace(
        &self,
        repo: &Path,
        mode: WorkingCopyMode,
    ) -> Result<PreparedWorkspace> {
        if !Self::experimental_flag_enabled() {
            return Err(Error::provider(
                "AgentFS provider is experimental; set AH_ENABLE_AGENTFS_PROVIDER=1 to enable it",
            ));
        }

        match self.platform {
            PlatformSupport::Supported => {}
            PlatformSupport::Unsupported(reason) => {
                return Err(Error::provider(reason));
            }
        }

        #[cfg(all(feature = "agentfs", target_os = "macos"))]
        {
            let requested_mode = match mode {
                WorkingCopyMode::Auto | WorkingCopyMode::CowOverlay => WorkingCopyMode::CowOverlay,
                other => return Self::unsupported_mode(other),
            };

            let harness =
                runtime::AgentFsHarness::start(repo).map_err(|e| Error::provider(e.to_string()))?;

            self.prepare_agentfs_workspace(harness, repo, requested_mode)
        }

        #[cfg(not(all(feature = "agentfs", target_os = "macos")))]
        {
            let _ = repo;
            let _ = mode;
            Err(Error::provider(
                "AgentFS provider is not compiled for this platform",
            ))
        }
    }

    fn snapshot_now(&self, ws: &PreparedWorkspace, label: Option<&str>) -> Result<SnapshotRef> {
        #[cfg(all(feature = "agentfs", target_os = "macos"))]
        {
            let (harness, cleanup_token, branch_id) = {
                let state_guard = self.state.lock().expect("AgentFS provider state poisoned");
                let session = state_guard.sessions.get(&ws.cleanup_token).ok_or_else(|| {
                    Error::provider(format!(
                        "Unknown AgentFS workspace cleanup token: {}",
                        ws.cleanup_token
                    ))
                })?;
                (
                    session.harness.clone(),
                    session.cleanup_token.clone(),
                    session.branch_id.clone(),
                )
            };

            let mut client = harness.connect().map_err(|e| Error::provider(e.to_string()))?;
            let snapshot = client
                .snapshot_create(label.map(|s| s.to_string()))
                .map_err(|e| Error::provider(e.to_string()))?;

            // Update session's latest snapshot.
            if let Some(state_session) = self
                .state
                .lock()
                .expect("AgentFS provider state poisoned")
                .sessions
                .get(&ws.cleanup_token)
            {
                *state_session
                    .current_snapshot
                    .lock()
                    .expect("AgentFS session snapshot state poisoned") = Some(snapshot.id.clone());
            }

            let mut meta = HashMap::new();
            meta.insert(META_SESSION_TOKEN.into(), cleanup_token.clone());
            meta.insert(META_BRANCH_ID.into(), branch_id);
            meta.insert(
                META_SOCKET_PATH.into(),
                harness.socket_path().display().to_string(),
            );
            meta.insert(META_SNAPSHOT_ID.into(), snapshot.id.clone());

            Ok(SnapshotRef {
                id: snapshot.id,
                label: snapshot.name,
                provider: SnapshotProviderKind::AgentFs,
                meta,
            })
        }

        #[cfg(not(all(feature = "agentfs", target_os = "macos")))]
        {
            let _ = ws;
            let _ = label;
            Err(Error::provider(
                "AgentFS provider is not compiled for this platform",
            ))
        }
    }

    fn mount_readonly(&self, snap: &SnapshotRef) -> Result<PathBuf> {
        #[cfg(all(feature = "agentfs", target_os = "macos"))]
        {
            let session_token = snap
                .meta
                .get(META_SESSION_TOKEN)
                .cloned()
                .ok_or_else(|| Error::provider("AgentFS snapshot missing session metadata"))?;
            let snapshot_id =
                snap.meta.get(META_SNAPSHOT_ID).cloned().unwrap_or_else(|| snap.id.clone());

            let (harness, workspace_path) = {
                let state_guard = self.state.lock().expect("AgentFS provider state poisoned");
                let session = state_guard.sessions.get(&session_token).ok_or_else(|| {
                    Error::provider(format!(
                        "AgentFS session referenced by snapshot not found: {}",
                        session_token
                    ))
                })?;
                (session.harness.clone(), session.workspace_path.clone())
            };

            let export = {
                let mut client = harness.connect().map_err(|e| Error::provider(e.to_string()))?;
                client
                    .snapshot_export(&snapshot_id)
                    .map_err(|e| Error::provider(e.to_string()))?
            };

            let readonly_path = export.path.clone();
            let cleanup_token = export.cleanup_token.clone();
            let mut missing_session = false;

            {
                let mut state_guard = self.state.lock().expect("AgentFS provider state poisoned");
                if let Some(session) = state_guard.sessions.get_mut(&session_token) {
                    session.readonly_exports.push(ReadonlyExport {
                        cleanup_token: cleanup_token.clone(),
                    });
                } else {
                    missing_session = true;
                }
            }

            if missing_session {
                let mut client = harness.connect().map_err(|e| Error::provider(e.to_string()))?;
                client
                    .snapshot_export_release(&cleanup_token)
                    .map_err(|e| Error::provider(e.to_string()))?;
                return Err(Error::provider(format!(
                    "AgentFS session referenced by snapshot not found: {}",
                    session_token
                )));
            }

            // Snapshot exports are currently control-plane only; populate the mount by mirroring
            // the prepared workspace contents so read-only consumers observe the same tree.
            Ok(readonly_path)
        }

        #[cfg(not(all(feature = "agentfs", target_os = "macos")))]
        {
            let _ = snap;
            Err(Error::provider(
                "AgentFS provider is not compiled for this platform",
            ))
        }
    }

    fn branch_from_snapshot(
        &self,
        snap: &SnapshotRef,
        mode: WorkingCopyMode,
    ) -> Result<PreparedWorkspace> {
        #[cfg(all(feature = "agentfs", target_os = "macos"))]
        {
            let session_token = snap
                .meta
                .get(META_SESSION_TOKEN)
                .ok_or_else(|| Error::provider("AgentFS snapshot missing session metadata"))?;
            let snapshot_id =
                snap.meta.get(META_SNAPSHOT_ID).cloned().unwrap_or_else(|| snap.id.clone());

            let requested_mode = match mode {
                WorkingCopyMode::Auto | WorkingCopyMode::CowOverlay => WorkingCopyMode::CowOverlay,
                other => return Self::unsupported_mode(other),
            };

            let (harness, workspace_path, source_repo, session_mode) = {
                let state_guard = self.state.lock().expect("AgentFS provider state poisoned");
                let session = state_guard.sessions.get(session_token).ok_or_else(|| {
                    Error::provider(format!(
                        "AgentFS session referenced by snapshot not found: {}",
                        session_token
                    ))
                })?;
                let workspace_path =
                    session.harness.workspace_path().map_err(|e| Error::provider(e.to_string()))?;
                (
                    session.harness.clone(),
                    workspace_path,
                    session.source_repo.clone(),
                    session.mode,
                )
            };

            if requested_mode != session_mode {
                return Err(Error::provider(format!(
                    "AgentFS workspace was prepared with {:?} but {:?} was requested",
                    session_mode, requested_mode
                )));
            }

            let mut client = harness.connect().map_err(|e| Error::provider(e.to_string()))?;
            let branch_name = format!("branch-{}", generate_unique_id());
            let branch = client
                .branch_create(&snapshot_id, Some(branch_name))
                .map_err(|e| Error::provider(e.to_string()))?;
            client
                .branch_bind(&branch.id, Some(std::process::id()))
                .map_err(|e| Error::provider(e.to_string()))?;

            let cleanup_token = generate_unique_id();
            let session = AgentFsSession {
                harness,
                cleanup_token: cleanup_token.clone(),
                workspace_path: workspace_path.clone(),
                source_repo,
                branch_id: branch.id.clone(),
                current_snapshot: Mutex::new(Some(snapshot_id.clone())),
                mode: requested_mode,
                readonly_exports: Vec::new(),
            };

            self.state
                .lock()
                .expect("AgentFS provider state poisoned")
                .sessions
                .insert(cleanup_token.clone(), session);

            let workspace = PreparedWorkspace {
                exec_path: workspace_path,
                working_copy: requested_mode,
                provider: SnapshotProviderKind::AgentFs,
                cleanup_token,
            };

            Ok(workspace)
        }

        #[cfg(not(all(feature = "agentfs", target_os = "macos")))]
        {
            let _ = (snap, mode);
            Err(Error::provider(
                "AgentFS provider is not compiled for this platform",
            ))
        }
    }

    fn list_snapshots(&self, directory: &Path) -> Result<Vec<SnapshotInfo>> {
        #[cfg(all(feature = "agentfs", target_os = "macos"))]
        {
            let state_guard = self.state.lock().expect("AgentFS provider state poisoned");
            let session = state_guard
                .sessions
                .values()
                .find(|session| session.workspace_path == directory);
            let session = match session {
                Some(s) => s,
                None => {
                    return Err(Error::provider(format!(
                        "No active AgentFS session for repository {}",
                        directory.display()
                    )));
                }
            };

            let mut client =
                session.harness.connect().map_err(|e| Error::provider(e.to_string()))?;
            let snapshots = client.snapshot_list().map_err(|e| Error::provider(e.to_string()))?;

            let results = snapshots
                .into_iter()
                .map(|record| SnapshotInfo {
                    snapshot: SnapshotRef {
                        id: record.id.clone(),
                        label: record.name.clone(),
                        provider: SnapshotProviderKind::AgentFs,
                        meta: {
                            let mut meta = HashMap::new();
                            meta.insert(META_SESSION_TOKEN.into(), session.cleanup_token.clone());
                            meta.insert(META_BRANCH_ID.into(), session.branch_id.clone());
                            meta.insert(META_SNAPSHOT_ID.into(), record.id.clone());
                            meta
                        },
                    },
                    created_at: 0,
                    session_id: None,
                })
                .collect();

            Ok(results)
        }

        #[cfg(not(all(feature = "agentfs", target_os = "macos")))]
        {
            let _ = directory;
            Err(Error::provider(
                "AgentFS provider is not compiled for this platform",
            ))
        }
    }

    fn cleanup(&self, token: &str) -> Result<()> {
        #[cfg(all(feature = "agentfs", target_os = "macos"))]
        {
            let session = {
                let mut guard = self.state.lock().expect("AgentFS provider state poisoned");
                guard.sessions.remove(token)
            };

            if let Some(session) = session {
                session.release_exports()?;
            }

            Ok(())
        }

        #[cfg(not(all(feature = "agentfs", target_os = "macos")))]
        {
            let _ = token;
            Ok(())
        }
    }
}

impl AgentFsProvider {
    /// Common AgentFS workspace preparation logic
    #[cfg(all(feature = "agentfs", target_os = "macos"))]
    fn prepare_agentfs_workspace(
        &self,
        harness: runtime::AgentFsHarness,
        repo: &Path,
        requested_mode: WorkingCopyMode,
    ) -> Result<PreparedWorkspace> {
        let workspace_path =
            harness.workspace_path().map_err(|e| Error::provider(e.to_string()))?;
        let mut client = harness.connect().map_err(|e| Error::provider(e.to_string()))?;

        // Create an initial snapshot (base) and a dedicated branch for this workspace.
        let base_snapshot =
            client.snapshot_create(None).map_err(|e| Error::provider(e.to_string()))?;
        let branch_name = generate_unique_id();
        let branch = client
            .branch_create(&base_snapshot.id, Some(branch_name.clone()))
            .map_err(|e| Error::provider(e.to_string()))?;

        // Note: Process binding should happen when the actual process is spawned,
        // not during workspace preparation. The interposition mechanism will
        // handle binding based on environment variables and daemon state.

        let cleanup_token = generate_unique_id();
        let session = AgentFsSession {
            harness: harness.clone(),
            cleanup_token: cleanup_token.clone(),
            workspace_path: workspace_path.clone(),
            source_repo: repo.to_path_buf(),
            branch_id: branch.id.clone(),
            current_snapshot: Mutex::new(Some(base_snapshot.id.clone())),
            mode: requested_mode,
            readonly_exports: Vec::new(),
        };

        // Set the branch ID environment variable so interposition binds processes to this branch
        harness.set_branch_id_env(&branch.id);

        self.state
            .lock()
            .expect("AgentFS provider state poisoned")
            .sessions
            .insert(cleanup_token.clone(), session);

        tracing::info!(
            provider = "AgentFS",
            mode = ?requested_mode,
            branch_id = %branch.id,
            "Successfully prepared AgentFS workspace"
        );

        Ok(PreparedWorkspace {
            exec_path: workspace_path,
            working_copy: requested_mode,
            provider: SnapshotProviderKind::AgentFs,
            cleanup_token,
        })
    }

    /// Prepare a writable workspace using an existing AgentFS daemon socket
    pub fn prepare_writable_workspace_with_socket(
        &self,
        repo: &Path,
        mode: WorkingCopyMode,
        socket_path: &Path,
    ) -> Result<PreparedWorkspace> {
        if !Self::experimental_flag_enabled() {
            return Err(Error::provider(
                "AgentFS provider is experimental; set AH_ENABLE_AGENTFS_PROVIDER=1 to enable it",
            ));
        }

        match self.platform {
            PlatformSupport::Supported => {}
            PlatformSupport::Unsupported(reason) => {
                return Err(Error::provider(reason));
            }
        }

        #[cfg(all(feature = "agentfs", target_os = "macos"))]
        {
            let requested_mode = match mode {
                WorkingCopyMode::Auto | WorkingCopyMode::CowOverlay => WorkingCopyMode::CowOverlay,
                other => return Self::unsupported_mode(other),
            };

            let harness = runtime::AgentFsHarness::connect_to_socket(socket_path, repo)
                .map_err(|e| Error::provider(e.to_string()))?;

            let workspace = self.prepare_agentfs_workspace(harness, repo, requested_mode)?;

            tracing::info!(
                agentfs_socket = %socket_path.display(),
                "Successfully prepared AgentFS workspace with existing daemon"
            );

            Ok(workspace)
        }

        #[cfg(not(all(feature = "agentfs", target_os = "macos")))]
        {
            Err(Error::provider(
                "AgentFS provider not available on this platform",
            ))
        }
    }
}

struct AgentFsState {
    #[cfg(all(feature = "agentfs", target_os = "macos"))]
    sessions: HashMap<String, AgentFsSession>,
}

impl Default for AgentFsState {
    #[cfg(all(feature = "agentfs", target_os = "macos"))]
    fn default() -> Self {
        Self {
            sessions: HashMap::new(),
        }
    }

    #[cfg(not(all(feature = "agentfs", target_os = "macos")))]
    fn default() -> Self {
        Self {}
    }
}

#[cfg(all(feature = "agentfs", target_os = "macos"))]
struct AgentFsSession {
    harness: runtime::AgentFsHarness,
    cleanup_token: String,
    workspace_path: PathBuf,
    source_repo: PathBuf,
    branch_id: String,
    current_snapshot: Mutex<Option<String>>,
    mode: WorkingCopyMode,
    readonly_exports: Vec<ReadonlyExport>,
}

#[cfg(all(feature = "agentfs", target_os = "macos"))]
struct ReadonlyExport {
    cleanup_token: String,
}

#[cfg(all(feature = "agentfs", target_os = "macos"))]
impl AgentFsSession {
    fn release_exports(&self) -> Result<()> {
        if self.readonly_exports.is_empty() {
            return Ok(());
        }

        let mut client = self.harness.connect().map_err(|e| Error::provider(e.to_string()))?;

        for export in &self.readonly_exports {
            client
                .snapshot_export_release(&export.cleanup_token)
                .map_err(|e| Error::provider(e.to_string()))?;
        }

        Ok(())
    }
}

#[cfg(all(feature = "agentfs", target_os = "macos"))]
mod runtime {
    use super::*;
    use agentfs_client::{AgentFsClient, ClientConfig};
    use agentfs_interpose_e2e_tests::find_daemon_path;
    use anyhow::{Context, Result, anyhow};
    use std::ffi::{CString, OsString};
    use std::path::{Path, PathBuf};
    use std::process::{Child, Command, Stdio};
    use std::time::{Duration, Instant};
    use tempfile::TempDir;

    #[derive(Clone)]
    pub struct AgentFsHarness {
        inner: Arc<AgentFsHarnessInner>,
    }

    struct AgentFsHarnessInner {
        _temp_dir: Mutex<Option<TempDir>>,
        socket_path: PathBuf,
        daemon: Mutex<Option<Child>>,
        workspace_path: PathBuf,
        source_repo: PathBuf,
        client_config: ClientConfig,
        previous_socket: Option<OsString>,
        previous_exe: Option<OsString>,
        previous_branch_id: Option<OsString>,
        exe_path: PathBuf,
    }

    impl Drop for AgentFsHarnessInner {
        fn drop(&mut self) {
            if let Ok(mut child_guard) = self.daemon.lock() {
                if let Some(mut child) = child_guard.take() {
                    let _ = child.kill();
                    let _ = child.wait();
                }
            }

            if let Some(socket) = self.socket_path.to_str() {
                let _ = std::fs::remove_file(socket);
            }

            let current_socket = std::env::var_os("AGENTFS_INTERPOSE_SOCKET");
            let should_restore = current_socket
                .as_ref()
                .map(|val| val.as_os_str() == self.socket_path.as_os_str())
                .unwrap_or(false);

            if should_restore {
                match &self.previous_socket {
                    Some(prev) => std::env::set_var("AGENTFS_INTERPOSE_SOCKET", prev),
                    None => std::env::remove_var("AGENTFS_INTERPOSE_SOCKET"),
                }

                let current_exe_env = std::env::var_os("AGENTFS_INTERPOSE_EXE");
                let should_restore_exe = current_exe_env
                    .as_ref()
                    .map(|val| val.as_os_str() == self.exe_path.as_os_str())
                    .unwrap_or(false);
                if should_restore_exe {
                    match &self.previous_exe {
                        Some(prev) => std::env::set_var("AGENTFS_INTERPOSE_EXE", prev),
                        None => std::env::remove_var("AGENTFS_INTERPOSE_EXE"),
                    }
                }

                let current_branch_env = std::env::var_os("AGENTFS_BRANCH_ID");
                let should_restore_branch = current_branch_env.as_ref().is_some(); // Restore if we set any branch ID
                if should_restore_branch {
                    match &self.previous_branch_id {
                        Some(prev) => std::env::set_var("AGENTFS_BRANCH_ID", prev),
                        None => std::env::remove_var("AGENTFS_BRANCH_ID"),
                    }
                }
            }
        }
    }

    impl AgentFsHarness {
        pub fn start(repo: &Path) -> Result<Self> {
            let previous_socket = std::env::var_os("AGENTFS_INTERPOSE_SOCKET");
            let previous_branch_id = std::env::var_os("AGENTFS_BRANCH_ID");

            let temp_dir = TempDir::new().context("failed to create AgentFS harness temp dir")?;
            let socket_path = temp_dir.path().join("agentfs.sock");
            if let Some(parent) = socket_path.parent() {
                std::fs::create_dir_all(parent)
                    .context("failed to create socket directory for AgentFS harness")?;
            }
            if socket_path.exists() {
                let _ = std::fs::remove_file(&socket_path);
            }

            let daemon_path = find_daemon_path();
            let mut command = Command::new(daemon_path);
            command
                .arg(socket_path.to_string_lossy().to_string())
                .arg("--lower-dir")
                .arg(repo.to_string_lossy().to_string());

            if std::env::var("AGENTFS_HARNESS_DEBUG").is_ok() {
                command.stdout(Stdio::inherit()).stderr(Stdio::inherit());
            } else {
                command.stdout(Stdio::null()).stderr(Stdio::null());
            }

            let child = command.spawn().context("failed to spawn agentfs-daemon process")?;

            let client_config =
                ClientConfig::builder("ah-fs-snapshots-provider", env!("CARGO_PKG_VERSION"))
                    .feature("control-plane-only")
                    .build()
                    .context("failed to construct AgentFS client configuration")?;

            wait_for_socket(&socket_path, Duration::from_secs(3)).with_context(|| {
                format!(
                    "agentfs-daemon did not create socket {}",
                    socket_path.display()
                )
            })?;

            std::env::set_var("AGENTFS_INTERPOSE_SOCKET", &socket_path);

            let previous_exe = std::env::var_os("AGENTFS_INTERPOSE_EXE");
            let current_exe =
                std::env::current_exe().context("failed to resolve current executable")?;
            std::env::set_var("AGENTFS_INTERPOSE_EXE", &current_exe);
            trigger_interpose_reconnect();

            Ok(Self {
                inner: Arc::new(AgentFsHarnessInner {
                    _temp_dir: Mutex::new(Some(temp_dir)),
                    socket_path,
                    daemon: Mutex::new(Some(child)),
                    workspace_path: repo.to_path_buf(),
                    source_repo: repo.to_path_buf(),
                    client_config,
                    previous_socket,
                    previous_exe,
                    previous_branch_id,
                    exe_path: current_exe,
                }),
            })
        }

        /// Connect to an existing AgentFS daemon at the specified socket path
        pub fn connect_to_socket(socket_path: &Path, repo: &Path) -> Result<Self> {
            let previous_socket = std::env::var_os("AGENTFS_INTERPOSE_SOCKET");
            let previous_exe = std::env::var_os("AGENTFS_INTERPOSE_EXE");
            let previous_branch_id = std::env::var_os("AGENTFS_BRANCH_ID");

            let client_config =
                ClientConfig::builder("ah-fs-snapshots-provider", env!("CARGO_PKG_VERSION"))
                    .feature("control-plane-only")
                    .build()
                    .context("failed to construct AgentFS client configuration")?;

            // Test the connection
            let _client = AgentFsClient::connect(socket_path, &client_config)
                .context("failed to connect to existing AgentFS daemon")?;

            let current_exe =
                std::env::current_exe().context("failed to resolve current executable")?;

            Ok(Self {
                inner: Arc::new(AgentFsHarnessInner {
                    _temp_dir: Mutex::new(None), // No temp dir for existing daemon
                    socket_path: socket_path.to_path_buf(),
                    daemon: Mutex::new(None), // No daemon process to manage
                    workspace_path: repo.to_path_buf(),
                    source_repo: repo.to_path_buf(),
                    client_config,
                    previous_socket,
                    previous_exe,
                    previous_branch_id,
                    exe_path: current_exe,
                }),
            })
        }

        pub fn socket_path(&self) -> &Path {
            &self.inner.socket_path
        }

        pub fn connect(&self) -> Result<AgentFsClient> {
            AgentFsClient::connect(self.socket_path(), &self.inner.client_config)
        }

        /// Set the branch ID environment variable for interposition
        pub fn set_branch_id_env(&self, branch_id: &str) {
            std::env::set_var("AGENTFS_BRANCH_ID", branch_id);
        }

        pub fn workspace_path(&self) -> Result<PathBuf> {
            Ok(self.inner.workspace_path.clone())
        }

        #[allow(dead_code)]
        pub fn source_repo(&self) -> &Path {
            &self.inner.source_repo
        }
    }

    fn trigger_interpose_reconnect() {
        unsafe {
            eprintln!("AgentFsHarness: attempting to trigger interpose reconnect");
            let symbol = CString::new("agentfs_interpose_force_reconnect")
                .expect("CString conversion for reconnect symbol");
            let func_ptr = libc::dlsym(libc::RTLD_DEFAULT, symbol.as_ptr());
            if func_ptr.is_null() {
                eprintln!("AgentFsHarness: reconnect symbol not found in interpose shim");
                return;
            }
            let func: unsafe extern "C" fn() = std::mem::transmute(func_ptr);
            func();
        }
    }

    fn wait_for_socket(path: &Path, timeout: Duration) -> Result<()> {
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            if path.exists() {
                return Ok(());
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        Err(anyhow!(
            "socket {} not created within {:?}",
            path.display(),
            timeout
        ))
    }
}
