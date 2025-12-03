// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

#[cfg(unix)]
use std::ffi::CString;
use std::fs::{self, OpenOptions};
#[cfg(unix)]
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;

use agentfs_core::FsConfig;
use agentfs_core::config::BackstoreMode;
use tokio::process::{Child, Command};
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use tokio::time::{Duration, Instant, sleep};
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, warn};

#[cfg(test)]
use crate::types::AgentfsMaterializationMode;
use crate::types::{
    AgentfsFuseBackstore, AgentfsFuseMountRequest, AgentfsFuseState, AgentfsFuseStatusData,
};

const DEFAULT_RUNTIME_DIR: &str = "/run/agentfs-fuse";
const DEFAULT_HOST_BIN: &str = "agentfs-fuse-host";
const DEFAULT_MOUNT_TIMEOUT_MS: u32 = 15_000;
const BACKOFF_MIN: Duration = Duration::from_secs(1);
const BACKOFF_MAX: Duration = Duration::from_secs(30);

/// Errors emitted by the AgentFS FUSE manager.
#[derive(Debug, thiserror::Error)]
pub enum FuseError {
    #[error("AgentFS FUSE host already mounted elsewhere ({0})")]
    AlreadyMounted(String),
    #[error("failed to prepare runtime: {0}")]
    Runtime(String),
    #[error("failed to spawn agentfs-fuse-host: {0}")]
    Spawn(String),
    #[error("mount did not become ready within {0:?}")]
    ReadyTimeout(Duration),
    #[error("operation cancelled")]
    Cancelled,
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

impl FuseError {
    fn runtime<E: Into<String>>(msg: E) -> Self {
        Self::Runtime(msg.into())
    }
}

pub struct AgentfsFuseManager {
    state: Mutex<FuseState>,
}

struct FuseState {
    mount: Option<Arc<ManagedFuseMount>>,
}

impl AgentfsFuseManager {
    pub fn new() -> Self {
        debug!(
            operation = "fuse_manager_new",
            "Creating new AgentfsFuseManager"
        );
        let manager = Self {
            state: Mutex::new(FuseState { mount: None }),
        };
        debug!(
            operation = "fuse_manager_created",
            "AgentfsFuseManager created successfully"
        );
        manager
    }

    pub async fn mount(
        &self,
        mut request: AgentfsFuseMountRequest,
    ) -> Result<AgentfsFuseStatusData, FuseError> {
        debug!(operation = "fuse_mount_start", mount_point = ?String::from_utf8_lossy(&request.mount_point), "Starting FUSE mount operation");

        if request.mount_timeout_ms == 0 {
            debug!(operation = "fuse_mount_timeout_default", original_timeout = %request.mount_timeout_ms, default_timeout = %DEFAULT_MOUNT_TIMEOUT_MS, "Setting default mount timeout");
            request.mount_timeout_ms = DEFAULT_MOUNT_TIMEOUT_MS;
        }

        debug!(
            operation = "fuse_mount_acquire_lock",
            "Acquiring state lock for mount operation"
        );
        let mut guard = self.state.lock().await;
        debug!(
            operation = "fuse_mount_lock_acquired",
            "State lock acquired"
        );

        if let Some(existing) = guard.mount.as_ref() {
            debug!(operation = "fuse_mount_check_existing", existing_mount_point = %existing.mount_point_string(), "Checking against existing mount");
            if existing.matches(&request) {
                debug!(operation = "fuse_mount_reuse_existing", mount_point = ?String::from_utf8_lossy(&request.mount_point), "Mount request matches existing AgentFS FUSE mount, reusing");
                return Ok(existing.snapshot().await);
            }

            let existing_mp = existing.mount_point_string();
            debug!(operation = "fuse_mount_conflict", existing_mount_point = %existing_mp, requested_mount_point = ?String::from_utf8_lossy(&request.mount_point), "Mount conflict detected");
            return Err(FuseError::AlreadyMounted(existing_mp));
        }

        debug!(operation = "fuse_mount_no_conflict", mount_point = ?String::from_utf8_lossy(&request.mount_point), "No existing mount conflict, proceeding with new mount");

        debug!(operation = "fuse_mount_prepare_runtime", mount_point = ?String::from_utf8_lossy(&request.mount_point), "Preparing FUSE runtime paths");
        let runtime = Arc::new(FuseRuntimePaths::prepare(&request)?);
        debug!(operation = "fuse_mount_runtime_prepared", mount_point = ?String::from_utf8_lossy(&request.mount_point), "FUSE runtime paths prepared");

        debug!(operation = "fuse_mount_build_config", mount_point = ?String::from_utf8_lossy(&request.mount_point), "Building filesystem configuration");
        let fs_config = Arc::new(build_fs_config(&request)?);
        debug!(operation = "fuse_mount_config_built", mount_point = ?String::from_utf8_lossy(&request.mount_point), "Filesystem configuration built");

        debug!(operation = "fuse_mount_persist_config", mount_point = ?String::from_utf8_lossy(&request.mount_point), "Persisting configuration to runtime directory");
        runtime.persist_config(&fs_config)?;
        debug!(operation = "fuse_mount_config_persisted", mount_point = ?String::from_utf8_lossy(&request.mount_point), "Configuration persisted successfully");

        debug!(operation = "fuse_mount_start_process", mount_point = ?String::from_utf8_lossy(&request.mount_point), "Starting managed FUSE mount process");
        let mount = Arc::new(
            ManagedFuseMount::start(request.clone(), runtime.clone(), fs_config.clone())
                .await
                .map_err(FuseError::runtime)?,
        );
        debug!(operation = "fuse_mount_process_started", mount_point = ?String::from_utf8_lossy(&request.mount_point), "Managed FUSE mount process started successfully");

        guard.mount = Some(mount.clone());
        drop(guard);
        debug!(operation = "fuse_mount_state_updated", mount_point = ?String::from_utf8_lossy(&request.mount_point), "Mount state updated in manager");

        let timeout = Duration::from_millis(request.mount_timeout_ms as u64);
        debug!(operation = "fuse_mount_wait_ready", mount_point = ?String::from_utf8_lossy(&request.mount_point), timeout_ms = %request.mount_timeout_ms, "Waiting for mount to become ready");

        match mount.wait_until_ready(timeout).await {
            Ok(status) => {
                debug!(operation = "fuse_mount_ready", mount_point = ?String::from_utf8_lossy(&request.mount_point), "FUSE mount is ready and operational");
                Ok(status)
            }
            Err(err) => {
                warn!(error = %err, operation = "fuse_mount_startup_failed", mount_point = ?String::from_utf8_lossy(&request.mount_point), "AgentFS FUSE mount failed during start-up; shutting down");
                debug!(operation = "fuse_mount_cleanup_failed", mount_point = ?String::from_utf8_lossy(&request.mount_point), "Cleaning up failed mount");
                mount.shutdown().await.ok();
                let mut guard = self.state.lock().await;
                guard.mount = None;
                debug!(operation = "fuse_mount_state_cleared", mount_point = ?String::from_utf8_lossy(&request.mount_point), "Failed mount cleared from manager state");
                Err(err)
            }
        }
    }

    pub async fn unmount(&self) -> Result<(), FuseError> {
        let mount = {
            let mut guard = self.state.lock().await;
            guard.mount.take()
        };

        if let Some(handle) = mount {
            handle.shutdown().await?;
        }

        Ok(())
    }

    pub async fn status(&self) -> AgentfsFuseStatusData {
        let guard = self.state.lock().await;
        if let Some(mount) = guard.mount.as_ref() {
            mount.snapshot().await
        } else {
            let runtime_root = runtime_dir_from_env();
            AgentfsFuseStatusData {
                state: AgentfsFuseState::Unmounted.as_code(),
                mount_point: Vec::new(),
                pid: 0,
                restart_count: 0,
                log_path: runtime_root
                    .join("agentfs-fuse-host.log")
                    .to_string_lossy()
                    .into_owned()
                    .into_bytes(),
                runtime_dir: runtime_root.to_string_lossy().into_owned().into_bytes(),
                last_error: Vec::new(),
                backstore: AgentfsFuseBackstore::InMemory(Vec::new()),
            }
        }
    }
}

impl Default for AgentfsFuseManager {
    fn default() -> Self {
        Self::new()
    }
}

struct ManagedFuseMount {
    spec: AgentfsFuseMountRequest,
    runtime: Arc<FuseRuntimePaths>,
    mount_point: PathBuf,
    status: Arc<Mutex<FuseSupervisorSnapshot>>,
    shutdown: CancellationToken,
    supervisor: Mutex<Option<JoinHandle<()>>>,
}

impl ManagedFuseMount {
    async fn start(
        spec: AgentfsFuseMountRequest,
        runtime: Arc<FuseRuntimePaths>,
        fs_config: Arc<FsConfig>,
    ) -> Result<Self, String> {
        let mount_point = path_from_bytes(&spec.mount_point)?;
        let status = Arc::new(Mutex::new(FuseSupervisorSnapshot::new()));
        let shutdown = CancellationToken::new();
        let supervisor = tokio::spawn(supervisor_loop(
            spec.clone(),
            runtime.clone(),
            fs_config.clone(),
            status.clone(),
            shutdown.clone(),
        ));

        Ok(Self {
            spec,
            runtime,
            mount_point,
            status,
            shutdown,
            supervisor: Mutex::new(Some(supervisor)),
        })
    }

    fn matches(&self, other: &AgentfsFuseMountRequest) -> bool {
        &self.spec == other
    }

    fn mount_point_string(&self) -> String {
        self.mount_point.to_string_lossy().to_string()
    }

    async fn snapshot(&self) -> AgentfsFuseStatusData {
        let snapshot = self.status.lock().await.clone();
        snapshot.to_status(&self.spec, &self.runtime)
    }

    async fn wait_until_ready(
        self: &Arc<Self>,
        timeout: Duration,
    ) -> Result<AgentfsFuseStatusData, FuseError> {
        let start = Instant::now();
        loop {
            let snapshot = self.status.lock().await.clone();
            match snapshot.state {
                AgentfsFuseState::Running => {
                    return Ok(snapshot.to_status(&self.spec, &self.runtime));
                }
                AgentfsFuseState::Failed => {
                    return Err(FuseError::runtime(
                        String::from_utf8_lossy(&snapshot.last_error).to_string(),
                    ));
                }
                _ => {
                    if start.elapsed() >= timeout {
                        return Err(FuseError::ReadyTimeout(timeout));
                    }
                }
            }

            sleep(Duration::from_millis(200)).await;
        }
    }

    async fn shutdown(self: &Arc<Self>) -> Result<(), FuseError> {
        self.shutdown.cancel();
        if let Some(handle) = self.supervisor.lock().await.take() {
            if let Err(join_err) = handle.await {
                if !join_err.is_cancelled() {
                    error!(error = %join_err, "FUSE supervisor task panicked");
                }
            }
        }
        Ok(())
    }
}

#[derive(Clone)]
struct FuseSupervisorSnapshot {
    state: AgentfsFuseState,
    pid: u64,
    restart_count: u32,
    last_error: Vec<u8>,
}

impl FuseSupervisorSnapshot {
    fn new() -> Self {
        Self {
            state: AgentfsFuseState::Starting,
            pid: 0,
            restart_count: 0,
            last_error: Vec::new(),
        }
    }

    fn to_status(
        &self,
        spec: &AgentfsFuseMountRequest,
        runtime: &FuseRuntimePaths,
    ) -> AgentfsFuseStatusData {
        AgentfsFuseStatusData {
            state: self.state.as_code(),
            mount_point: spec.mount_point.clone(),
            pid: self.pid,
            restart_count: self.restart_count,
            log_path: runtime.log_path.to_string_lossy().into_owned().into_bytes(),
            runtime_dir: runtime.root.to_string_lossy().into_owned().into_bytes(),
            last_error: self.last_error.clone(),
            backstore: spec.backstore.clone(),
        }
    }
}

#[derive(Clone)]
struct FuseRuntimePaths {
    root: PathBuf,
    config_path: PathBuf,
    pid_path: PathBuf,
    status_path: PathBuf,
    mountpoint_path: PathBuf,
    log_path: PathBuf,
}

impl FuseRuntimePaths {
    fn prepare(request: &AgentfsFuseMountRequest) -> Result<Self, FuseError> {
        let root = runtime_dir_from_env();
        fs::create_dir_all(&root)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&root)?.permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&root, perms)?;

            // Chown the runtime directory to the requesting user so the FUSE host
            // process (which runs as that user) can write logs and status files.
            if request.uid != 0 {
                let path_cstr = CString::new(root.as_os_str().as_bytes())
                    .map_err(|_| FuseError::runtime("runtime path contains null bytes"))?;
                unsafe {
                    if libc::chown(path_cstr.as_ptr(), request.uid, request.gid) != 0 {
                        let err = std::io::Error::last_os_error();
                        warn!(error = %err, path = %root.display(), "failed to chown runtime directory");
                    }
                }
            }
        }

        let runtime = Self {
            root: root.clone(),
            config_path: root.join("fs-config.json"),
            pid_path: root.join("agentfs-fuse-host.pid"),
            status_path: root.join("status.json"),
            mountpoint_path: root.join("mountpoint"),
            log_path: root.join("agentfs-fuse-host.log"),
        };

        let mount_path = path_from_bytes(&request.mount_point)
            .map_err(|e| FuseError::runtime(format!("invalid mount path: {e}")))?;
        runtime.persist_mountpoint_path(&mount_path)?;
        Ok(runtime)
    }

    fn persist_config(&self, config: &FsConfig) -> Result<(), FuseError> {
        let buf = serde_json::to_vec_pretty(config)
            .map_err(|e| FuseError::runtime(format!("failed to serialize config: {e}")))?;
        fs::write(&self.config_path, buf)?;
        Ok(())
    }

    fn persist_status(&self, status: &AgentfsFuseStatusData) {
        #[allow(unused_mut)]
        let mut last_error = None;
        if !status.last_error.is_empty() {
            last_error = Some(String::from_utf8_lossy(&status.last_error).to_string());
        }

        let doc = serde_json::json!({
            "state": fuse_state_label(AgentfsFuseState::from_code(status.state)),
            "mount_point": String::from_utf8_lossy(&status.mount_point),
            "pid": status.pid,
            "restart_count": status.restart_count,
            "log_path": String::from_utf8_lossy(&status.log_path),
            "runtime_dir": String::from_utf8_lossy(&status.runtime_dir),
            "last_error": last_error,
            "backstore": describe_backstore(&status.backstore),
        });

        if let Err(err) = fs::write(
            &self.status_path,
            serde_json::to_string_pretty(&doc).unwrap(),
        ) {
            warn!(error = %err, path = %self.status_path.display(), "failed to persist status summary");
        }
    }

    fn persist_pid(&self, pid: u32) {
        if let Err(err) = fs::write(&self.pid_path, pid.to_string()) {
            warn!(error = %err, path = %self.pid_path.display(), "failed to persist PID file");
        }
    }

    fn clear_pid(&self) {
        if let Err(err) = fs::remove_file(&self.pid_path) {
            if err.kind() != std::io::ErrorKind::NotFound {
                warn!(error = %err, path = %self.pid_path.display(), "failed to remove PID file");
            }
        }
    }

    fn persist_mountpoint_path(&self, mount_point: &Path) -> Result<(), FuseError> {
        fs::write(
            &self.mountpoint_path,
            mount_point.to_string_lossy().into_owned().into_bytes(),
        )?;
        Ok(())
    }

    fn clear_mountpoint(&self) {
        if let Err(err) = fs::remove_file(&self.mountpoint_path) {
            if err.kind() != std::io::ErrorKind::NotFound {
                warn!(error = %err, path = %self.mountpoint_path.display(), "failed to remove mountpoint metadata");
            }
        }
    }
}

fn runtime_dir_from_env() -> PathBuf {
    std::env::var("AGENTFS_FUSE_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(DEFAULT_RUNTIME_DIR))
}

fn host_bin_from_env() -> PathBuf {
    std::env::var("AGENTFS_FUSE_HOST_BIN")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(DEFAULT_HOST_BIN))
}

fn path_from_bytes(bytes: &[u8]) -> Result<PathBuf, String> {
    let s = String::from_utf8(bytes.to_vec()).map_err(|e| format!("invalid UTF-8 path: {e}"))?;
    Ok(PathBuf::from(s))
}

fn fuse_state_label(state: AgentfsFuseState) -> &'static str {
    match state {
        AgentfsFuseState::Starting => "starting",
        AgentfsFuseState::Running => "running",
        AgentfsFuseState::BackingOff => "backing_off",
        AgentfsFuseState::Unmounted => "unmounted",
        AgentfsFuseState::Failed => "failed",
        AgentfsFuseState::Unknown => "unknown",
    }
}

fn describe_backstore(backstore: &AgentfsFuseBackstore) -> String {
    match backstore {
        AgentfsFuseBackstore::InMemory(_) => "InMemory".to_string(),
        AgentfsFuseBackstore::HostFs(opts) => format!(
            "HostFs(root={}, prefer_native_snapshots={})",
            String::from_utf8_lossy(&opts.root),
            opts.prefer_native_snapshots
        ),
        AgentfsFuseBackstore::RamDisk(opts) => format!("RamDisk({}MiB)", opts.size_mb),
    }
}

fn build_fs_config(request: &AgentfsFuseMountRequest) -> Result<FsConfig, FuseError> {
    let mut config = FsConfig::default();
    config.cache.writeback_cache = request.writeback_cache;
    config.backstore = backstore_from_request(&request.backstore)?;
    Ok(config)
}

fn backstore_from_request(backstore: &AgentfsFuseBackstore) -> Result<BackstoreMode, FuseError> {
    match backstore {
        AgentfsFuseBackstore::InMemory(_) => Ok(BackstoreMode::InMemory),
        AgentfsFuseBackstore::HostFs(opts) => {
            let root = path_from_bytes(&opts.root)
                .map_err(|e| FuseError::runtime(format!("invalid HostFs root: {e}")))?;
            fs::create_dir_all(&root)?;
            Ok(BackstoreMode::HostFs {
                root,
                prefer_native_snapshots: opts.prefer_native_snapshots,
            })
        }
        AgentfsFuseBackstore::RamDisk(opts) => Ok(BackstoreMode::RamDisk {
            size_mb: opts.size_mb,
        }),
    }
}

async fn supervisor_loop(
    request: AgentfsFuseMountRequest,
    runtime: Arc<FuseRuntimePaths>,
    fs_config: Arc<FsConfig>,
    status: Arc<Mutex<FuseSupervisorSnapshot>>,
    shutdown: CancellationToken,
) {
    let host_bin = host_bin_from_env();
    let mount_point = match path_from_bytes(&request.mount_point) {
        Ok(path) => path,
        Err(err) => {
            error!(error = %err, "invalid mount point provided; supervisor exiting");
            return;
        }
    };

    let control_path = mount_point.join(".agentfs").join("control");
    let mut backoff = BACKOFF_MIN;
    let mut restart_count = 0u32;

    loop {
        if shutdown.is_cancelled() {
            break;
        }

        update_status(
            &status,
            &request,
            &runtime,
            AgentfsFuseState::Starting,
            0,
            restart_count,
            None,
        )
        .await;

        if let Err(err) = ensure_mountpoint(&mount_point, request.uid, request.gid) {
            update_status(
                &status,
                &request,
                &runtime,
                AgentfsFuseState::Failed,
                0,
                restart_count,
                Some(err.to_string()),
            )
            .await;
            break;
        }

        runtime
            .persist_config(&fs_config)
            .unwrap_or_else(|e| warn!(error = %e, "failed to rewrite FsConfig before mount"));

        match spawn_agentfs_host(&host_bin, &request, &mount_point, &runtime).await {
            Ok(mut child) => {
                let pid = child.id().unwrap_or_default();
                runtime.persist_pid(pid);
                let ready_deadline = Duration::from_millis(if request.mount_timeout_ms == 0 {
                    DEFAULT_MOUNT_TIMEOUT_MS
                } else {
                    request.mount_timeout_ms
                } as u64);

                match wait_for_control(&control_path, ready_deadline, &shutdown).await {
                    Ok(()) => {
                        backoff = BACKOFF_MIN;
                        update_status(
                            &status,
                            &request,
                            &runtime,
                            AgentfsFuseState::Running,
                            pid as u64,
                            restart_count,
                            None,
                        )
                        .await;
                    }
                    Err(err) => {
                        warn!(error = %err, "AgentFS FUSE control plane did not become ready in time");
                        let _ = terminate_child(&mut child).await;
                        runtime.clear_pid();
                        restart_count += 1;
                        update_status(
                            &status,
                            &request,
                            &runtime,
                            AgentfsFuseState::BackingOff,
                            0,
                            restart_count,
                            Some(err.to_string()),
                        )
                        .await;
                        if wait_backoff(backoff, &shutdown).await {
                            break;
                        }
                        backoff = (backoff * 2).min(BACKOFF_MAX);
                        continue;
                    }
                }

                let wait_fut = child.wait();
                tokio::select! {
                    status_res = wait_fut => {
                        runtime.clear_pid();
                        if shutdown.is_cancelled() {
                            break;
                        }

                        let exit_msg = match status_res {
                            Ok(exit) => format!("agentfs-fuse-host exited: {exit}"),
                            Err(err) => format!("failed waiting on agentfs-fuse-host: {err}"),
                        };

                        warn!(message = %exit_msg, "agentfs-fuse-host crashed; restarting with backoff");
                        restart_count += 1;
                        update_status(
                            &status,
                            &request,
                            &runtime,
                            AgentfsFuseState::BackingOff,
                            0,
                            restart_count,
                            Some(exit_msg.clone()),
                        )
                        .await;
                        let _ = try_unmount(&mount_point).await;
                        if wait_backoff(backoff, &shutdown).await {
                            break;
                        }
                        backoff = (backoff * 2).min(BACKOFF_MAX);
                        continue;
                    }
                    _ = shutdown.cancelled() => {
                        let _ = terminate_child(&mut child).await;
                        let _ = child.wait().await;
                        runtime.clear_pid();
                        break;
                    }
                }
            }
            Err(err) => {
                restart_count += 1;
                update_status(
                    &status,
                    &request,
                    &runtime,
                    AgentfsFuseState::BackingOff,
                    0,
                    restart_count,
                    Some(err.to_string()),
                )
                .await;

                if wait_backoff(backoff, &shutdown).await {
                    break;
                }
                backoff = (backoff * 2).min(BACKOFF_MAX);
            }
        }
    }

    let _ = try_unmount(&mount_point).await;
    runtime.clear_pid();
    runtime.clear_mountpoint();
    update_status(
        &status,
        &request,
        &runtime,
        AgentfsFuseState::Unmounted,
        0,
        restart_count,
        None,
    )
    .await;
}

async fn update_status(
    shared: &Arc<Mutex<FuseSupervisorSnapshot>>,
    req: &AgentfsFuseMountRequest,
    runtime: &Arc<FuseRuntimePaths>,
    state: AgentfsFuseState,
    pid: u64,
    restart_count: u32,
    last_error: Option<String>,
) {
    let mut guard = shared.lock().await;
    guard.state = state;
    guard.pid = pid;
    guard.restart_count = restart_count;
    guard.last_error = last_error.map(|s| s.into_bytes()).unwrap_or_default();
    let snapshot = guard.clone();
    drop(guard);
    runtime.persist_status(&snapshot.to_status(req, runtime));
}

async fn spawn_agentfs_host(
    binary: &Path,
    request: &AgentfsFuseMountRequest,
    mount_point: &Path,
    runtime: &FuseRuntimePaths,
) -> Result<Child, FuseError> {
    let mut cmd = Command::new(binary);
    cmd.arg("--config").arg(&runtime.config_path);
    if request.allow_other {
        cmd.arg("--allow-other");
    }
    if request.allow_root {
        cmd.arg("--allow-root");
    }
    if request.auto_unmount {
        cmd.arg("--auto-unmount");
    }
    if request.writeback_cache {
        cmd.arg("--writeback-cache");
    }
    // Add overlay materialization mode
    cmd.arg("--overlay-materialization");
    cmd.arg(request.materialization_mode.to_cli_arg());
    cmd.arg(mount_point);

    let stdout_file = OpenOptions::new().create(true).append(true).open(&runtime.log_path)?;
    let stderr_file = stdout_file.try_clone()?;
    cmd.stdout(Stdio::from(stdout_file));
    cmd.stderr(Stdio::from(stderr_file));

    // Drop privileges to the requesting user before spawning when the daemon
    // itself is running as root. In user-mode (non-root) we must NOT attempt to
    // manipulate supplemental groups because that requires CAP_SETGID and
    // results in EPERM, preventing the host process from spawning.
    #[cfg(unix)]
    {
        let current_euid = unsafe { libc::geteuid() };
        if current_euid == 0 && request.uid != 0 {
            // The unsafe block is required by CommandExt but the operations are
            // limited to setting the uid/gid of the child process.
            unsafe {
                let uid = request.uid;
                let gid = request.gid;
                cmd.pre_exec(move || {
                    // Set supplementary groups first (must be done before setuid)
                    if libc::setgroups(0, std::ptr::null()) != 0 {
                        return Err(std::io::Error::last_os_error());
                    }
                    // Set GID before UID (setuid drops privilege to change gid)
                    if libc::setgid(gid) != 0 {
                        return Err(std::io::Error::last_os_error());
                    }
                    if libc::setuid(uid) != 0 {
                        return Err(std::io::Error::last_os_error());
                    }
                    Ok(())
                });
            }
            debug!(
                uid = request.uid,
                gid = request.gid,
                "dropping privileges for FUSE host process"
            );
        } else {
            debug!(
                uid = request.uid,
                gid = request.gid,
                current_euid,
                "running as non-root; skipping privilege drop for FUSE host process"
            );
        }
    }

    info!(
        mount_point = %mount_point.display(),
        binary = %binary.display(),
        uid = request.uid,
        gid = request.gid,
        allow_other = request.allow_other,
        auto_unmount = request.auto_unmount,
        "spawning agentfs-fuse-host"
    );

    cmd.spawn().map_err(|err| FuseError::Spawn(err.to_string()))
}

fn ensure_mountpoint(path: &Path, uid: u32, gid: u32) -> Result<(), std::io::Error> {
    fs::create_dir_all(path)?;
    #[cfg(unix)]
    {
        let bytes = path.as_os_str().as_bytes();
        if let Ok(c_path) = CString::new(bytes) {
            unsafe {
                let res = libc::chown(c_path.as_ptr(), uid, gid);
                if res != 0 {
                    let err = std::io::Error::last_os_error();
                    warn!(error = %err, path = %path.display(), "failed to chown mountpoint");
                }
            }
        } else {
            warn!(path = %path.display(), "mountpoint path contains interior null bytes; skipping chown");
        }
    }
    Ok(())
}

async fn wait_for_control(
    control_path: &Path,
    timeout: Duration,
    shutdown: &CancellationToken,
) -> Result<(), FuseError> {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if control_path.exists() {
            return Ok(());
        }

        if shutdown.is_cancelled() {
            return Err(FuseError::Cancelled);
        }

        sleep(Duration::from_millis(200)).await;
    }

    Err(FuseError::ReadyTimeout(timeout))
}

async fn terminate_child(child: &mut Child) -> std::io::Result<()> {
    if let Some(pid) = child.id() {
        unsafe {
            libc::kill(pid as i32, libc::SIGTERM);
        }

        let mut waited = Duration::from_millis(0);
        while waited < Duration::from_secs(5) {
            if let Ok(Some(_)) = child.try_wait() {
                return Ok(());
            }
            sleep(Duration::from_millis(250)).await;
            waited += Duration::from_millis(250);
        }

        unsafe {
            libc::kill(pid as i32, libc::SIGKILL);
        }
    }

    Ok(())
}

async fn try_unmount(path: &Path) -> Result<(), FuseError> {
    if run_umount("fusermount", &["-u"], path).await.is_ok() {
        return Ok(());
    }

    let _ = run_umount("umount", &[], path).await;
    Ok(())
}

async fn run_umount(binary: &str, base_args: &[&str], mount_point: &Path) -> Result<(), FuseError> {
    let mut cmd = Command::new(binary);
    cmd.args(base_args).arg(mount_point);
    match cmd.output().await {
        Ok(output) if output.status.success() => Ok(()),
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if stderr.contains("not mounted") {
                return Ok(());
            }
            Err(FuseError::runtime(format!(
                "{binary} -u {} failed: {stderr}",
                mount_point.display()
            )))
        }
        Err(err) => Err(FuseError::runtime(format!("failed to run {binary}: {err}"))),
    }
}

async fn wait_backoff(delay: Duration, shutdown: &CancellationToken) -> bool {
    tokio::select! {
        _ = shutdown.cancelled() => true,
        _ = sleep(delay) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    use once_cell::sync::Lazy;

    static TEST_GUARD: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

    fn make_stub_host_script(dir: &Path) -> PathBuf {
        let script_path = dir.join("stub-agentfs-fuse-host.sh");
        let content = r#"#!/usr/bin/env bash
set -euo pipefail
mountpoint="${@: -1}"
mkdir -p "$mountpoint/.agentfs"
touch "$mountpoint/.agentfs/control"
trap 'rm -f "$mountpoint/.agentfs/control"; exit 0' SIGTERM SIGINT
while true; do sleep 1; done
"#;
        fs::write(&script_path, content).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&script_path).unwrap().permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&script_path, perms).unwrap();
        }
        script_path
    }

    fn sample_request(mount_point: &Path) -> AgentfsFuseMountRequest {
        AgentfsFuseMountRequest {
            mount_point: mount_point.to_string_lossy().into_owned().into_bytes(),
            uid: 0,
            gid: 0,
            allow_other: true,
            allow_root: true,
            auto_unmount: false,
            writeback_cache: false,
            mount_timeout_ms: 2_000,
            backstore: AgentfsFuseBackstore::InMemory(Vec::new()),
            materialization_mode: AgentfsMaterializationMode::default(),
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn mount_and_unmount_stub_host() {
        let _guard = TEST_GUARD.lock().await;
        let runtime_dir = tempdir().unwrap();
        let mount_dir = tempdir().unwrap();
        let host_stub_dir = tempdir().unwrap();
        let stub_bin = make_stub_host_script(host_stub_dir.path());
        std::env::set_var("AGENTFS_FUSE_RUNTIME_DIR", runtime_dir.path());
        std::env::set_var("AGENTFS_FUSE_HOST_BIN", &stub_bin);

        let manager = AgentfsFuseManager::new();
        let req = sample_request(mount_dir.path());
        let status = manager.mount(req.clone()).await.expect("mount works");
        assert_eq!(
            AgentfsFuseState::from_code(status.state),
            AgentfsFuseState::Running
        );
        assert!(status.pid > 0);

        manager.unmount().await.expect("unmounted");
        let status = manager.status().await;
        assert_eq!(
            AgentfsFuseState::from_code(status.state),
            AgentfsFuseState::Unmounted
        );
    }
}
