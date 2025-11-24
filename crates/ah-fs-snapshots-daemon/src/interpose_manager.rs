// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

#[cfg(target_os = "macos")]
mod platform {
    use std::ffi::CString;
    use std::path::{Path, PathBuf};
    use std::sync::Arc;

    use serde_json::json;
    use tokio::process::{Child, Command};
    use tokio::sync::Mutex;
    use tokio::task::JoinHandle;
    use tokio::time::{Duration, Instant, sleep};
    use tokio_util::sync::CancellationToken;
    use tracing::{debug, info, warn};

    use crate::types::{
        AgentfsFuseState, AgentfsInterposeMountHints, AgentfsInterposeMountRequest,
        AgentfsInterposeStatusData,
    };

    const DEFAULT_RUNTIME_DIR: &str = "/tmp/agentfs-interpose";
    const DEFAULT_DAEMON_BIN: &str = "agentfs-daemon";
    const DEFAULT_TIMEOUT_MS: u32 = 5_000;
    const BACKOFF_MIN: Duration = Duration::from_secs(1);
    const BACKOFF_MAX: Duration = Duration::from_secs(30);

    #[derive(Debug, thiserror::Error)]
    pub enum InterposeError {
        #[error("failed to prepare runtime: {0}")]
        Runtime(String),
        #[error("failed to spawn agentfs-daemon: {0}")]
        Spawn(String),
        #[error("daemon did not become ready within {0:?}")]
        ReadyTimeout(Duration),
        #[error("operation cancelled")]
        Cancelled,
        #[error("I/O error: {0}")]
        Io(#[from] std::io::Error),
    }

    impl InterposeError {
        fn runtime<E: Into<String>>(msg: E) -> Self {
            Self::Runtime(msg.into())
        }
    }

    pub struct AgentfsInterposeManager {
        state: Mutex<InterposeState>,
    }

    struct InterposeState {
        session: Option<Arc<ManagedInterposeSession>>,
    }

    impl AgentfsInterposeManager {
        pub fn new() -> Self {
            debug!(
                operation = "interpose_manager_new",
                "Creating new AgentfsInterposeManager"
            );
            let manager = Self {
                state: Mutex::new(InterposeState { session: None }),
            };
            debug!(
                operation = "interpose_manager_created",
                "AgentfsInterposeManager created successfully"
            );
            manager
        }

        pub async fn mount(
            &self,
            mut request: AgentfsInterposeMountRequest,
            hints: Option<AgentfsInterposeMountHints>,
            log_level: &str,
            log_to_file: bool,
            log_dir: &std::path::Path,
            session_id: &str,
        ) -> Result<AgentfsInterposeStatusData, InterposeError> {
            debug!(operation = "interpose_mount_start", repo_root_len = %request.repo_root.len(), has_hints = %hints.is_some(), "Starting interpose mount operation");

            if request.mount_timeout_ms == 0 {
                debug!(operation = "interpose_mount_timeout_default", original_timeout = %request.mount_timeout_ms, default_timeout = %DEFAULT_TIMEOUT_MS, "Setting default mount timeout");
                request.mount_timeout_ms = DEFAULT_TIMEOUT_MS;
            }

            debug!(operation = "interpose_mount_normalize_repo", repo_root_bytes = %request.repo_root.len(), "Normalizing repository root path");
            let repo_path = path_from_bytes(&request.repo_root)?;
            let normalized_root = normalize_repo_root(&repo_path);
            if !normalized_root.exists() {
                return Err(InterposeError::runtime(format!(
                    "repository root does not exist: {}",
                    normalized_root.display()
                )));
            }
            if !normalized_root.is_dir() {
                return Err(InterposeError::runtime(format!(
                    "repository root is not a directory: {}",
                    normalized_root.display()
                )));
            }
            debug!(operation = "interpose_mount_repo_normalized", normalized_root = %normalized_root.display(), "Repository root path normalized");
            request.repo_root = normalized_root.to_string_lossy().into_owned().into_bytes();
            let hint_paths = MountHintPaths::from_option(hints.as_ref())?;
            let hint_signature = MountHintSignature::from_paths(&hint_paths);

            let guard = self.state.lock().await;
            let replacement_candidate = if let Some(existing_ref) = guard.session.as_ref() {
                if existing_ref.matches(&request, &hint_signature) {
                    // Ensure callers only observe a ready session; wait for the supervisor to finish
                    // booting agentfs-daemon instead of returning a stale snapshot while the socket
                    // is still missing.
                    let existing = existing_ref.clone();
                    drop(guard);
                    let timeout = Duration::from_millis(request.mount_timeout_ms as u64);
                    return existing.wait_until_ready(timeout).await;
                }
                info!(
                    old_repo = %existing_ref.repo_root_string(),
                    new_repo = %normalized_root.display(),
                    "AgentFS interpose mount request differs; preparing replacement session"
                );
                Some(existing_ref.clone())
            } else {
                None
            };
            drop(guard);

            let runtime = Arc::new(InterposeRuntimePaths::prepare(
                &normalized_root,
                request.uid,
                request.gid,
                hint_paths.runtime_dir.as_deref(),
                hint_paths.socket_path.as_deref(),
            )?);
            let session = Arc::new(
                ManagedInterposeSession::start(
                    request.clone(),
                    runtime.clone(),
                    hint_signature.clone(),
                    LogConfig::new(log_level, log_to_file, log_dir),
                    session_id,
                )
                .await
                .map_err(InterposeError::runtime)?,
            );
            {
                let mut guard = self.state.lock().await;
                guard.session = Some(session.clone());
            }

            let timeout = Duration::from_millis(request.mount_timeout_ms as u64);
            match session.wait_until_ready(timeout).await {
                Ok(status) => {
                    if let Some(existing) = replacement_candidate {
                        existing.shutdown().await.ok();
                    }
                    Ok(status)
                }
                Err(err) => {
                    session.shutdown().await.ok();
                    let mut guard = self.state.lock().await;
                    if let Some(existing) = replacement_candidate {
                        guard.session = Some(existing);
                    } else {
                        guard.session = None;
                    }
                    Err(err)
                }
            }
        }

        pub async fn unmount(&self) -> Result<(), InterposeError> {
            let session = {
                let mut guard = self.state.lock().await;
                guard.session.take()
            };

            if let Some(handle) = session {
                handle.shutdown().await?;
            }

            Ok(())
        }

        pub async fn status(&self) -> AgentfsInterposeStatusData {
            let guard = self.state.lock().await;
            if let Some(session) = guard.session.as_ref() {
                session.snapshot().await
            } else {
                AgentfsInterposeStatusData {
                    state: AgentfsFuseState::Unmounted.as_code(),
                    socket_path: Vec::new(),
                    pid: 0,
                    restart_count: 0,
                    log_path: runtime_dir_from_env()
                        .join("agentfs-daemon.log")
                        .to_string_lossy()
                        .into_owned()
                        .into_bytes(),
                    runtime_dir: runtime_dir_from_env().to_string_lossy().into_owned().into_bytes(),
                    last_error: Vec::new(),
                    repo_root: Vec::new(),
                }
            }
        }
    }

    impl Default for AgentfsInterposeManager {
        fn default() -> Self {
            Self::new()
        }
    }

    struct ManagedInterposeSession {
        spec: AgentfsInterposeMountRequest,
        hint_signature: MountHintSignature,
        runtime: Arc<InterposeRuntimePaths>,
        status: Arc<Mutex<InterposeSupervisorSnapshot>>,
        shutdown: CancellationToken,
        supervisor: Mutex<Option<JoinHandle<()>>>,
    }

    impl ManagedInterposeSession {
        async fn start(
            spec: AgentfsInterposeMountRequest,
            runtime: Arc<InterposeRuntimePaths>,
            hint_signature: MountHintSignature,
            log_config: LogConfig,
            session_id: &str,
        ) -> Result<Self, String> {
            let status = Arc::new(Mutex::new(InterposeSupervisorSnapshot::new(&spec)));
            let shutdown = CancellationToken::new();
            let supervisor = tokio::spawn(supervisor_loop(
                spec.clone(),
                runtime.clone(),
                status.clone(),
                shutdown.clone(),
                log_config.clone(),
                session_id.to_string(),
            ));

            Ok(Self {
                spec,
                hint_signature,
                runtime,
                status,
                shutdown,
                supervisor: Mutex::new(Some(supervisor)),
            })
        }

        fn matches(
            &self,
            other: &AgentfsInterposeMountRequest,
            other_hints: &MountHintSignature,
        ) -> bool {
            let left = path_from_bytes(&self.spec.repo_root).map(|p| normalize_repo_root(&p));
            let right = path_from_bytes(&other.repo_root).map(|p| normalize_repo_root(&p));
            matches!((left, right), (Ok(a), Ok(b)) if a == b) && self.hint_signature == *other_hints
        }

        fn repo_root_string(&self) -> String {
            String::from_utf8_lossy(&self.spec.repo_root).to_string()
        }

        async fn snapshot(&self) -> AgentfsInterposeStatusData {
            let snapshot = self.status.lock().await.clone();
            snapshot.to_status(&self.runtime)
        }

        async fn wait_until_ready(
            self: &Arc<Self>,
            timeout: Duration,
        ) -> Result<AgentfsInterposeStatusData, InterposeError> {
            let start = Instant::now();
            loop {
                let snapshot = self.status.lock().await.clone();
                match snapshot.state {
                    AgentfsFuseState::Running => {
                        return Ok(snapshot.to_status(&self.runtime));
                    }
                    AgentfsFuseState::Failed => {
                        return Err(InterposeError::runtime(
                            String::from_utf8_lossy(&snapshot.last_error).to_string(),
                        ));
                    }
                    _ => {
                        if start.elapsed() >= timeout {
                            return Err(InterposeError::ReadyTimeout(timeout));
                        }
                    }
                }
                sleep(Duration::from_millis(200)).await;
            }
        }

        async fn shutdown(self: &Arc<Self>) -> Result<(), InterposeError> {
            self.shutdown.cancel();
            if let Some(handle) = self.supervisor.lock().await.take() {
                if let Err(join_err) = handle.await {
                    if !join_err.is_cancelled() {
                        warn!(error = %join_err, "interpose supervisor task panicked");
                    }
                }
            }
            Ok(())
        }
    }

    #[derive(Clone)]
    struct InterposeSupervisorSnapshot {
        state: AgentfsFuseState,
        pid: u64,
        restart_count: u32,
        last_error: Vec<u8>,
        repo_root: Vec<u8>,
    }

    impl InterposeSupervisorSnapshot {
        fn new(spec: &AgentfsInterposeMountRequest) -> Self {
            Self {
                state: AgentfsFuseState::Starting,
                pid: 0,
                restart_count: 0,
                last_error: Vec::new(),
                repo_root: spec.repo_root.clone(),
            }
        }

        fn to_status(&self, runtime: &InterposeRuntimePaths) -> AgentfsInterposeStatusData {
            AgentfsInterposeStatusData {
                state: self.state.as_code(),
                socket_path: runtime.socket_path.to_string_lossy().into_owned().into_bytes(),
                pid: self.pid,
                restart_count: self.restart_count,
                log_path: runtime.log_path.to_string_lossy().into_owned().into_bytes(),
                runtime_dir: runtime.root.to_string_lossy().into_owned().into_bytes(),
                last_error: self.last_error.clone(),
                repo_root: self.repo_root.clone(),
            }
        }
    }

    struct InterposeRuntimePaths {
        root: PathBuf,
        socket_path: PathBuf,
        pid_path: PathBuf,
        status_path: PathBuf,
        log_path: PathBuf,
        owner_uid: u32,
        owner_gid: u32,
    }

    impl InterposeRuntimePaths {
        fn prepare(
            repo_root: &Path,
            uid: u32,
            gid: u32,
            runtime_hint: Option<&Path>,
            socket_hint: Option<&Path>,
        ) -> Result<Self, InterposeError> {
            let mut root = runtime_hint.map(PathBuf::from).unwrap_or_else(runtime_dir_from_env);
            if root.as_os_str().is_empty() {
                root = runtime_dir_from_env();
            }

            if runtime_hint.is_none() {
                if let Some(parent) = socket_hint.and_then(|path| path.parent()) {
                    if !parent.as_os_str().is_empty() {
                        root = parent.to_path_buf();
                    }
                }
            }

            std::fs::create_dir_all(&root)?;

            let socket_path = if let Some(hint) = socket_hint {
                if let Some(parent) = hint.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                hint.to_path_buf()
            } else {
                root.join("agentfs.sock")
            };

            let runtime = Self {
                root: root.clone(),
                socket_path,
                pid_path: root.join("agentfs-daemon.pid"),
                status_path: root.join("status.json"),
                log_path: root.join("agentfs-daemon.log"),
                owner_uid: uid,
                owner_gid: gid,
            };

            runtime.clear_socket();
            runtime.persist_repo_root(repo_root);
            runtime.ensure_runtime_perms();
            Ok(runtime)
        }

        fn persist_repo_root(&self, repo_root: &Path) {
            let doc = json!({
                "repo_root": repo_root.to_string_lossy(),
            });
            let path = self.root.join("repo.json");
            let _ = std::fs::write(&path, doc.to_string());
            self.fix_path_perms(&path);
        }

        fn persist_status(&self, status: &AgentfsInterposeStatusData) {
            let doc = json!({
                "state": state_label(AgentfsFuseState::from_code(status.state)),
                "socket_path": String::from_utf8_lossy(&status.socket_path),
                "pid": status.pid,
                "restart_count": status.restart_count,
                "log_path": String::from_utf8_lossy(&status.log_path),
                "runtime_dir": String::from_utf8_lossy(&status.runtime_dir),
                "repo_root": String::from_utf8_lossy(&status.repo_root),
                "last_error": if status.last_error.is_empty() {
                    serde_json::Value::Null
                } else {
                    serde_json::Value::String(String::from_utf8_lossy(&status.last_error).into())
                },
            });

            if let Err(err) = std::fs::write(&self.status_path, doc.to_string()) {
                warn!(error = %err, path = %self.status_path.display(), "failed to persist interpose status");
            }
            self.fix_path_perms(&self.status_path);
        }

        fn persist_pid(&self, pid: u32) {
            if let Err(err) = std::fs::write(&self.pid_path, pid.to_string()) {
                warn!(error = %err, path = %self.pid_path.display(), "failed to persist interpose pid");
            }
            self.fix_path_perms(&self.pid_path);
        }

        fn clear_pid(&self) {
            if let Err(err) = std::fs::remove_file(&self.pid_path) {
                if err.kind() != std::io::ErrorKind::NotFound {
                    warn!(error = %err, path = %self.pid_path.display(), "failed to remove interpose pid");
                }
            }
        }

        fn clear_socket(&self) {
            if let Err(err) = std::fs::remove_file(&self.socket_path) {
                if err.kind() != std::io::ErrorKind::NotFound {
                    warn!(error = %err, path = %self.socket_path.display(), "failed to remove socket");
                }
            }
        }

        fn ensure_runtime_perms(&self) {
            self.fix_path_perms(&self.root);
        }

        fn fix_path_perms(&self, path: &Path) {
            if let Err(err) = set_owner_mode(path, self.owner_uid, self.owner_gid, path.is_dir()) {
                warn!(error = %err, path = %path.display(), "failed to adjust permissions");
            }
        }
    }

    #[derive(Clone)]
    struct LogConfig {
        level: String,
        to_file: bool,
        dir: PathBuf,
    }

    impl LogConfig {
        fn new(level: &str, to_file: bool, dir: &Path) -> Self {
            Self {
                level: level.to_string(),
                to_file,
                dir: dir.to_path_buf(),
            }
        }

        fn level(&self) -> &str {
            &self.level
        }

        fn to_file(&self) -> bool {
            self.to_file
        }

        fn dir(&self) -> &Path {
            &self.dir
        }
    }

    #[derive(Default, Clone, PartialEq, Eq)]
    struct MountHintSignature {
        socket: Option<String>,
        runtime: Option<String>,
    }

    impl MountHintSignature {
        fn from_paths(paths: &MountHintPaths) -> Self {
            Self {
                socket: paths.socket_path.as_ref().map(|p| p.to_string_lossy().into_owned()),
                runtime: paths.runtime_dir.as_ref().map(|p| p.to_string_lossy().into_owned()),
            }
        }
    }

    struct MountHintPaths {
        socket_path: Option<PathBuf>,
        runtime_dir: Option<PathBuf>,
    }

    impl MountHintPaths {
        fn from_option(hints: Option<&AgentfsInterposeMountHints>) -> Result<Self, InterposeError> {
            let mut socket_path = None;
            let mut runtime_dir = None;

            if let Some(hints) = hints {
                if !hints.socket_path.is_empty() {
                    socket_path = Some(path_from_bytes(&hints.socket_path)?);
                }
                if !hints.runtime_dir.is_empty() {
                    runtime_dir = Some(path_from_bytes(&hints.runtime_dir)?);
                }
            }

            Ok(Self {
                socket_path,
                runtime_dir,
            })
        }
    }

    fn state_label(state: AgentfsFuseState) -> &'static str {
        match state {
            AgentfsFuseState::Starting => "starting",
            AgentfsFuseState::Running => "running",
            AgentfsFuseState::BackingOff => "backing_off",
            AgentfsFuseState::Unmounted => "unmounted",
            AgentfsFuseState::Failed => "failed",
            AgentfsFuseState::Unknown => "unknown",
        }
    }

    async fn supervisor_loop(
        spec: AgentfsInterposeMountRequest,
        runtime: Arc<InterposeRuntimePaths>,
        status: Arc<Mutex<InterposeSupervisorSnapshot>>,
        shutdown: CancellationToken,
        log_config: LogConfig,
        _session_id: String,
    ) {
        let daemon_bin = daemon_bin_from_env();
        let repo_root = match path_from_bytes(&spec.repo_root) {
            Ok(path) => normalize_repo_root(&path),
            Err(err) => {
                warn!(error = %err, "invalid repo root passed to AgentFS interpose manager");
                update_status(
                    &status,
                    &runtime,
                    AgentfsFuseState::Failed,
                    0,
                    0,
                    Some(err.to_string()),
                )
                .await;
                return;
            }
        };

        let mut backoff = BACKOFF_MIN;
        let mut restart_count = 0;

        loop {
            if shutdown.is_cancelled() {
                break;
            }

            match spawn_agentfs_daemon(
                &daemon_bin,
                &runtime,
                &repo_root,
                spec.uid,
                spec.gid,
                &log_config,
            )
            .await
            {
                Ok(mut child) => {
                    let pid = child.id().unwrap_or_default();
                    runtime.persist_pid(pid);
                    let ready_deadline = Duration::from_millis(spec.mount_timeout_ms as u64);
                    match wait_for_socket(
                        &runtime.socket_path,
                        ready_deadline,
                        &shutdown,
                        runtime.owner_uid,
                        runtime.owner_gid,
                    )
                    .await
                    {
                        Ok(()) => {
                            backoff = BACKOFF_MIN;
                            update_status(
                                &status,
                                &runtime,
                                AgentfsFuseState::Running,
                                pid as u64,
                                restart_count,
                                None,
                            )
                            .await;
                        }
                        Err(err) => {
                            warn!(error = %err, "agentfs-daemon socket not ready");
                            let _ = terminate_child(&mut child).await;
                            runtime.clear_pid();
                            runtime.clear_socket();
                            restart_count += 1;
                            update_status(
                                &status,
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
                            runtime.clear_socket();
                            if shutdown.is_cancelled() {
                                break;
                            }
                            let exit_msg = match status_res {
                                Ok(exit) => format!("agentfs-daemon exited: {exit}"),
                                Err(err) => format!("failed waiting on agentfs-daemon: {err}"),
                            };
                            warn!(message = %exit_msg, "agentfs-daemon crashed; restarting");
                            restart_count += 1;
                            update_status(
                                &status,
                                &runtime,
                                AgentfsFuseState::BackingOff,
                                0,
                                restart_count,
                                Some(exit_msg.clone()),
                            ).await;
                            if wait_backoff(backoff, &shutdown).await {
                                break;
                            }
                            backoff = (backoff * 2).min(BACKOFF_MAX);
                        }
                        _ = shutdown.cancelled() => {
                            let _ = terminate_child(&mut child).await;
                            let _ = child.wait().await;
                            runtime.clear_pid();
                            runtime.clear_socket();
                            break;
                        }
                    }
                }
                Err(err) => {
                    warn!(error = %err, "failed to spawn agentfs-daemon");
                    restart_count += 1;
                    update_status(
                        &status,
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

        runtime.clear_pid();
        runtime.clear_socket();
        update_status(&status, &runtime, AgentfsFuseState::Unmounted, 0, 0, None).await;
    }

    async fn spawn_agentfs_daemon(
        binary: &Path,
        runtime: &InterposeRuntimePaths,
        repo_root: &Path,
        uid: u32,
        gid: u32,
        log_config: &LogConfig,
    ) -> Result<Child, InterposeError> {
        let mut cmd = Command::new(binary);
        cmd.arg(&runtime.socket_path)
            .arg("--lower-dir")
            .arg(repo_root)
            .arg("--owner-uid")
            .arg(uid.to_string())
            .arg("--owner-gid")
            .arg(gid.to_string())
            .arg("--log-level")
            .arg(log_config.level());

        if log_config.to_file() {
            // Pass the log directory
            cmd.arg("--log-dir").arg(log_config.dir());
            // Pass the filename for agentfs-daemon
            cmd.arg("--log-file").arg("agentfs-daemon.log");
        }

        // For debugging, inherit stdout/stderr so we can see logs in test output
        cmd.stdout(std::process::Stdio::inherit());
        cmd.stderr(std::process::Stdio::inherit());

        info!(
            repo = %repo_root.display(),
            socket = %runtime.socket_path.display(),
            binary = %binary.display(),
            "spawning agentfs-daemon via supervisor",
        );

        cmd.spawn().map_err(|err| InterposeError::Spawn(err.to_string()))
    }

    async fn update_status(
        shared: &Arc<Mutex<InterposeSupervisorSnapshot>>,
        runtime: &Arc<InterposeRuntimePaths>,
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
        runtime.persist_status(&snapshot.to_status(runtime));
    }

    fn runtime_dir_from_env() -> PathBuf {
        std::env::var("AGENTFS_INTERPOSE_RUNTIME_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from(DEFAULT_RUNTIME_DIR))
    }

    fn daemon_bin_from_env() -> PathBuf {
        std::env::var("AGENTFS_INTERPOSE_DAEMON_BIN")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from(DEFAULT_DAEMON_BIN))
    }

    fn path_from_bytes(bytes: &[u8]) -> Result<PathBuf, InterposeError> {
        let s = String::from_utf8(bytes.to_vec())
            .map_err(|e| InterposeError::runtime(format!("invalid UTF-8 path: {e}")))?;
        Ok(PathBuf::from(s))
    }

    fn normalize_repo_root(path: &Path) -> PathBuf {
        path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
    }

    async fn wait_for_socket(
        path: &Path,
        timeout: Duration,
        shutdown: &CancellationToken,
        uid: u32,
        gid: u32,
    ) -> Result<(), InterposeError> {
        let start = Instant::now();
        while start.elapsed() < timeout {
            if path.exists() {
                if let Err(err) = set_owner_mode(path, uid, gid, false) {
                    warn!(error = %err, path = %path.display(), "failed to adjust socket perms");
                }
                return Ok(());
            }

            if shutdown.is_cancelled() {
                return Err(InterposeError::Cancelled);
            }

            sleep(Duration::from_millis(200)).await;
        }

        Err(InterposeError::ReadyTimeout(timeout))
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

    async fn wait_backoff(delay: Duration, shutdown: &CancellationToken) -> bool {
        tokio::select! {
            _ = shutdown.cancelled() => true,
            _ = sleep(delay) => false,
        }
    }

    fn set_owner_mode(path: &Path, uid: u32, gid: u32, is_dir: bool) -> std::io::Result<()> {
        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStrExt;
            use std::os::unix::fs::PermissionsExt;
            let c_path = CString::new(path.as_os_str().as_bytes())?;
            unsafe {
                libc::chown(c_path.as_ptr(), uid, gid);
            }
            let mut perms = std::fs::metadata(path)?.permissions();
            perms.set_mode(if is_dir { 0o770 } else { 0o660 });
            std::fs::set_permissions(path, perms)?;
        }
        Ok(())
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use once_cell::sync::Lazy;
        use std::path::PathBuf;
        use tempfile::tempdir;

        static GUARD: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

        fn make_stub_daemon(dir: &Path) -> PathBuf {
            let script = dir.join("stub-agentfs-daemon.sh");
            let contents = r#"#!/usr/bin/env bash
set -euo pipefail
socket="$1"
lower_dir=""
shift
while [[ $# -gt 0 ]]; do
  case "$1" in
    --lower-dir)
      lower_dir="$2"
      shift 2
      ;;
    *)
      shift
      ;;
  esac
done
mkdir -p "$(dirname "$socket")"
touch "$socket"
trap 'rm -f "$socket"; exit 0' SIGTERM SIGINT
while true; do sleep 1; done
"#;
            std::fs::write(&script, contents).unwrap();
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let mut perms = std::fs::metadata(&script).unwrap().permissions();
                perms.set_mode(0o755);
                std::fs::set_permissions(&script, perms).unwrap();
            }
            script
        }

        fn sample_request(repo: &Path) -> AgentfsInterposeMountRequest {
            AgentfsInterposeMountRequest {
                repo_root: repo.to_string_lossy().into_owned().into_bytes(),
                uid: 0,
                gid: 0,
                mount_timeout_ms: 2_000,
            }
        }

        #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
        async fn mount_and_unmount_stub_daemon() {
            let _lock = GUARD.lock().await;
            let runtime_dir = tempdir().unwrap();
            let stub_dir = tempdir().unwrap();
            let repo_dir = tempdir().unwrap();

            let stub = make_stub_daemon(stub_dir.path());
            std::env::set_var("AGENTFS_INTERPOSE_RUNTIME_DIR", runtime_dir.path());
            std::env::set_var("AGENTFS_INTERPOSE_DAEMON_BIN", &stub);

            let manager = AgentfsInterposeManager::new();
            let req = sample_request(repo_dir.path());
            let status = manager
                .mount(
                    req,
                    None,
                    "info",
                    false,
                    std::path::Path::new("/tmp"),
                    "test-session",
                )
                .await
                .expect("mount works");
            assert_eq!(
                AgentfsFuseState::from_code(status.state),
                AgentfsFuseState::Running
            );
            assert!(status.pid > 0);
            assert!(!status.socket_path.is_empty());

            manager.unmount().await.expect("unmounted");
            let status = manager.status().await;
            assert_eq!(
                AgentfsFuseState::from_code(status.state),
                AgentfsFuseState::Unmounted
            );
        }

        #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
        async fn mount_respects_socket_hint() {
            let _lock = GUARD.lock().await;
            let runtime_dir = tempdir().unwrap();
            let socket_dir = tempdir().unwrap();
            let stub_dir = tempdir().unwrap();
            let repo_dir = tempdir().unwrap();

            let stub = make_stub_daemon(stub_dir.path());
            std::env::set_var("AGENTFS_INTERPOSE_DAEMON_BIN", &stub);

            let socket_path = socket_dir.path().join("agentfs.sock");
            let req = sample_request(repo_dir.path());
            let hints = AgentfsInterposeMountHints {
                socket_path: socket_path.to_string_lossy().into_owned().into_bytes(),
                runtime_dir: runtime_dir.path().to_string_lossy().into_owned().into_bytes(),
            };

            let manager = AgentfsInterposeManager::new();
            let status = manager
                .mount(
                    req,
                    Some(hints),
                    "info",
                    false,
                    std::path::Path::new("/tmp"),
                    "test-session-hints",
                )
                .await
                .expect("mount works");
            let reported_socket = PathBuf::from(String::from_utf8(status.socket_path).unwrap());
            assert_eq!(reported_socket, socket_path);
            assert!(
                reported_socket.exists(),
                "daemon should create hinted socket"
            );

            manager.unmount().await.expect("unmounted");
        }
    }
}

#[cfg(not(target_os = "macos"))]
mod platform {
    use std::path::Path;

    use crate::types::{
        AgentfsFuseState, AgentfsInterposeMountHints, AgentfsInterposeMountRequest,
        AgentfsInterposeStatusData,
    };

    #[derive(Debug, thiserror::Error)]
    pub enum InterposeError {
        #[error("AgentFS interpose mounts are only supported on macOS")]
        Unsupported,
    }

    pub struct AgentfsInterposeManager;

    impl AgentfsInterposeManager {
        pub fn new() -> Self {
            Self
        }

        pub async fn mount(
            &self,
            _request: AgentfsInterposeMountRequest,
            _hints: Option<AgentfsInterposeMountHints>,
            _log_level: &str,
            _log_to_file: bool,
            _log_dir: &Path,
            _session_id: &str,
        ) -> Result<AgentfsInterposeStatusData, InterposeError> {
            Err(InterposeError::Unsupported)
        }

        pub async fn unmount(&self) -> Result<(), InterposeError> {
            Err(InterposeError::Unsupported)
        }

        pub async fn status(&self) -> AgentfsInterposeStatusData {
            AgentfsInterposeStatusData {
                state: AgentfsFuseState::Failed.as_code(),
                socket_path: Vec::new(),
                pid: 0,
                restart_count: 0,
                log_path: Vec::new(),
                runtime_dir: Vec::new(),
                last_error: b"AgentFS interpose mounts are only supported on macOS".to_vec(),
                repo_root: Vec::new(),
            }
        }
    }

    impl Default for AgentfsInterposeManager {
        fn default() -> Self {
            Self::new()
        }
    }
}

#[allow(unused_imports)]
pub use platform::{AgentfsInterposeManager, InterposeError};
