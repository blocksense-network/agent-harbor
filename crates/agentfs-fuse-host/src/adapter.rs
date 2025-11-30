// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! AgentFS FUSE adapter implementation
//!
//! Maps FUSE operations to AgentFS Core calls.

#[cfg(not(all(feature = "fuse", target_os = "linux")))]
compile_error!("This module requires the 'fuse' feature on Linux");

use agentfs_core::{
    Attributes, FallocateMode, FsConfig, FsCore, FsError, HandleId, OpenOptions, ShareMode,
    SpecialNodeKind, error::FsResult, vfs::PID,
};
use agentfs_proto::messages::{StatData, TimespecData};
use crossbeam_queue::SegQueue;
use fuser::{
    BackingId, FUSE_ROOT_ID, FileAttr, FileType, Notifier, ReplyAttr, ReplyCreate, ReplyData,
    ReplyDirectory, ReplyEmpty, ReplyEntry, ReplyOpen, ReplyStatfs, ReplyWrite, ReplyXattr,
    Request, TimeOrNow,
    consts::{FOPEN_DIRECT_IO, FUSE_PASSTHROUGH},
    fuse_forget_one,
};
use libc::{
    EACCES, EBADF, EEXIST, EINVAL, EIO, EISDIR, ELOOP, ENAMETOOLONG, ENOENT, ENOSYS, ENOTDIR,
    ENOTEMPTY, ENOTSUP, EPERM, ESTALE, O_ACCMODE, c_int,
};
use ssz::{Decode, Encode};
use std::collections::{HashMap, HashSet};
use std::ffi::{OsStr, OsString};
use std::fs::{File, OpenOptions as StdOpenOptions};
use std::io::Read;
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::os::unix::fs::{FileExt, MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::{
    Arc, Condvar, Mutex, Weak,
    atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering},
};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tracing::{debug, error, info, warn};

/// Special inode for the .agentfs directory
const AGENTFS_DIR_INO: u64 = FUSE_ROOT_ID + 1;

/// Special inode for the .agentfs/control file
const CONTROL_FILE_INO: u64 = FUSE_ROOT_ID + 2;

/// Base value for dynamically allocated filesystem inodes mapped from AgentFS node IDs.
const FIRST_DYNAMIC_INO: u64 = CONTROL_FILE_INO + 1;

/// IOCTL command for AgentFS control operations (matches _IOWR('A','F', 4096))
const AGENTFS_IOCTL_CMD: u32 = 0xD000_4146;

/// Maximum single path component length to guard against overly long names
const NAME_MAX: usize = 255;

/// File-handle space reserved for lower pass-through handles
const LOWER_HANDLE_BASE: u64 = 1u64 << 60;

/// Base inode number reserved for lower-only entries that have not been materialized
const LOWER_INODE_BASE: u64 = 1u64 << 58;

/// Synthetic PID assignments for requests that arrive without a valid client PID.
const PSEUDO_PID_BASE: u32 = 0x4000_0000;

/// FUSE capability bit for writeback caching (ABI 7.23+).
const FUSE_WRITEBACK_CACHE_FLAG: u64 = 1 << 16;

/// Handle that lets the launcher thread install the kernel notifier after the FUSE
/// session has been spawned.
#[derive(Clone)]
pub struct NotifierRegistration {
    slot: Weak<Mutex<Option<Notifier>>>,
}

impl NotifierRegistration {
    fn new(slot: &Arc<Mutex<Option<Notifier>>>) -> Self {
        Self {
            slot: Arc::downgrade(slot),
        }
    }

    /// Installs the notifier if the filesystem is still alive.
    pub fn install(&self, notifier: Notifier) {
        if let Some(slot) = self.slot.upgrade() {
            if let Ok(mut guard) = slot.lock() {
                *guard = Some(notifier);
            } else {
                warn!(
                    target: "agentfs::fuse",
                    "failed to acquire notifier slot lock; cache invalidations disabled"
                );
            }
        } else {
            warn!(
                target: "agentfs::fuse",
                "filesystem dropped before notifier could be installed"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentfs_proto::{Request as ControlRequest, Response as ControlResponse};

    #[test]
    fn cache_ttls_follow_config() {
        let mut config = FsConfig::default();
        config.cache.attr_ttl_ms = 1500;
        config.cache.entry_ttl_ms = 2500;
        config.cache.negative_ttl_ms = 3500;

        let fuse = AgentFsFuse::new(config).expect("fuse init");
        assert_eq!(fuse.attr_ttl, Duration::from_millis(1500));
        assert_eq!(fuse.entry_ttl, Duration::from_millis(2500));
        assert_eq!(fuse.negative_ttl, Duration::from_millis(3500));
    }

    #[test]
    fn control_ioctl_snapshot_roundtrip() {
        let fuse = AgentFsFuse::new(FsConfig::default()).expect("fuse init");

        let create_req = ControlRequest::snapshot_create(Some("snap-one".to_string()));
        let create_resp = fuse
            .handle_control_ioctl(&create_req.as_ssz_bytes())
            .expect("snapshot create resp");
        match ControlResponse::from_ssz_bytes(&create_resp).expect("decode response") {
            ControlResponse::SnapshotCreate(info) => {
                assert!(info.snapshot.name.is_some());
            }
            other => panic!("unexpected response: {:?}", other),
        }

        let list_req = ControlRequest::snapshot_list();
        let list_resp =
            fuse.handle_control_ioctl(&list_req.as_ssz_bytes()).expect("snapshot list resp");
        match ControlResponse::from_ssz_bytes(&list_resp).expect("decode list") {
            ControlResponse::SnapshotList(entries) => {
                assert_eq!(entries.snapshots.len(), 1);
            }
            other => panic!("unexpected response: {:?}", other),
        }
    }
}

struct LowerHandle {
    file: File,
}

struct PassthroughHandle {
    backing: BackingId,
    writable: bool,
}

#[derive(Default)]
struct PassthroughMetrics {
    attempts: AtomicU64,
    successes: AtomicU64,
    not_ready: AtomicU64,
    missing_path: AtomicU64,
    open_failed: AtomicU64,
    ioctl_failed: AtomicU64,
}

impl PassthroughMetrics {
    fn record_attempt(&self) {
        self.attempts.fetch_add(1, Ordering::Relaxed);
    }

    fn record_success(&self) {
        self.successes.fetch_add(1, Ordering::Relaxed);
    }

    fn record_not_ready(&self) {
        self.not_ready.fetch_add(1, Ordering::Relaxed);
    }

    fn record_missing_path(&self) {
        self.missing_path.fetch_add(1, Ordering::Relaxed);
    }

    fn record_open_failed(&self) {
        self.open_failed.fetch_add(1, Ordering::Relaxed);
    }

    fn record_ioctl_failed(&self) {
        self.ioctl_failed.fetch_add(1, Ordering::Relaxed);
    }

    fn snapshot(&self) -> PassthroughMetricsSnapshot {
        PassthroughMetricsSnapshot {
            attempts: self.attempts.load(Ordering::Relaxed),
            successes: self.successes.load(Ordering::Relaxed),
            not_ready: self.not_ready.load(Ordering::Relaxed),
            missing_path: self.missing_path.load(Ordering::Relaxed),
            open_failed: self.open_failed.load(Ordering::Relaxed),
            ioctl_failed: self.ioctl_failed.load(Ordering::Relaxed),
        }
    }
}

struct PassthroughMetricsSnapshot {
    attempts: u64,
    successes: u64,
    not_ready: u64,
    missing_path: u64,
    open_failed: u64,
    ioctl_failed: u64,
}

struct WriteTrace {
    req_id: u64,
    ino: u64,
    fh: u64,
    offset: i64,
    size: usize,
    start: Instant,
    inflight: Arc<AtomicU64>,
    max_inflight: Arc<AtomicU64>,
}

impl WriteTrace {
    fn new(
        req_id: u64,
        ino: u64,
        fh: u64,
        offset: i64,
        size: usize,
        inflight: Arc<AtomicU64>,
        max_inflight: Arc<AtomicU64>,
    ) -> Self {
        Self {
            req_id,
            ino,
            fh,
            offset,
            size,
            start: Instant::now(),
            inflight,
            max_inflight,
        }
    }

    fn finish(self) {
        let remaining = self.inflight.fetch_sub(1, Ordering::SeqCst).saturating_sub(1);
        debug!(
            target: "agentfs::write",
            event = "finish",
            req_id = self.req_id,
            ino = self.ino,
            fh = self.fh,
            offset = self.offset,
            size = self.size,
            duration_ms = self.start.elapsed().as_secs_f64() * 1000.0,
            inflight = remaining,
            max_inflight = self.max_inflight.load(Ordering::SeqCst)
        );
    }
}

struct WriteJob {
    pid: PID,
    handle_id: HandleId,
    offset: u64,
    data: Vec<u8>,
    completion: Arc<WriteHandleState>,
    trace: Option<WriteTrace>,
}

struct WriteHandleState {
    inflight: AtomicUsize,
    waiter: (Mutex<()>, Condvar),
    error: Mutex<Option<i32>>,
}

impl WriteHandleState {
    fn new() -> Self {
        Self {
            inflight: AtomicUsize::new(0),
            waiter: (Mutex::new(()), Condvar::new()),
            error: Mutex::new(None),
        }
    }

    fn begin_write(&self) {
        self.inflight.fetch_add(1, Ordering::SeqCst);
    }

    fn finish_write(&self) {
        if self.inflight.fetch_sub(1, Ordering::SeqCst) == 1 {
            let (lock, cvar) = &self.waiter;
            if let Ok(guard) = lock.lock() {
                cvar.notify_all();
                drop(guard);
            }
        }
    }

    fn wait_for_all(&self) {
        let (lock, cvar) = &self.waiter;
        let mut guard = lock.lock().unwrap();
        while self.inflight.load(Ordering::Acquire) > 0 {
            guard = cvar.wait(guard).unwrap();
        }
    }

    fn record_error(&self, errno: i32) {
        let mut guard = self.error.lock().unwrap();
        *guard = Some(errno);
    }

    fn current_error(&self) -> Option<i32> {
        *self.error.lock().unwrap()
    }
}

struct WriteDispatcher {
    queue: Arc<SegQueue<WriteJob>>,
    signal: Arc<(Mutex<bool>, Condvar)>,
    shutdown: Arc<AtomicBool>,
    handles: Vec<JoinHandle<()>>,
}

impl WriteDispatcher {
    fn new(core: Arc<FsCore>, thread_count: usize) -> Self {
        let queue = Arc::new(SegQueue::<WriteJob>::new());
        let signal = Arc::new((Mutex::new(false), Condvar::new()));
        let shutdown = Arc::new(AtomicBool::new(false));
        let mut handles = Vec::with_capacity(thread_count);

        for _ in 0..thread_count {
            let queue_clone = Arc::clone(&queue);
            let signal_clone = Arc::clone(&signal);
            let shutdown_clone = Arc::clone(&shutdown);
            let core_clone = Arc::clone(&core);
            handles.push(thread::spawn(move || {
                loop {
                    if shutdown_clone.load(Ordering::Acquire) {
                        break;
                    }

                    match queue_clone.pop() {
                        Some(mut job) => {
                            let result =
                                core_clone.write(&job.pid, job.handle_id, job.offset, &job.data);
                            if let Err(err) = result {
                                let errno = errno_from_fs_error(&err);
                                job.completion.record_error(errno);
                            }
                            job.completion.finish_write();
                            if let Some(trace) = job.trace.take() {
                                trace.finish();
                            }
                        }
                        None => {
                            let (lock, cvar) = &*signal_clone;
                            let guard = lock.lock().unwrap();
                            let _ = cvar.wait_timeout(guard, Duration::from_millis(5)).unwrap();
                        }
                    }
                }
            }));
        }

        Self {
            queue,
            signal,
            shutdown,
            handles,
        }
    }

    fn submit(&self, job: WriteJob) {
        self.queue.push(job);
        let (lock, cvar) = &*self.signal;
        if let Ok(mut pending) = lock.lock() {
            *pending = true;
            cvar.notify_one();
        }
    }
}

impl Drop for WriteDispatcher {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::Release);
        let (lock, cvar) = &*self.signal;
        if let Ok(mut pending) = lock.lock() {
            *pending = true;
            cvar.notify_all();
        }
        for handle in self.handles.drain(..) {
            let _ = handle.join();
        }
    }
}

struct RemovalOutcome {
    node_id: u64,
    has_other_bindings: bool,
}

fn errno_from_fs_error(err: &FsError) -> i32 {
    match err {
        FsError::AccessDenied => EACCES,
        FsError::AlreadyExists => EEXIST,
        FsError::OperationNotPermitted => EPERM,
        FsError::InvalidArgument => EINVAL,
        FsError::IsADirectory => EISDIR,
        FsError::NotADirectory => ENOTDIR,
        FsError::NotFound => ENOENT,
        FsError::Busy => libc::EBUSY,
        FsError::TooManyOpenFiles => libc::EMFILE,
        FsError::NoSpace => libc::ENOSPC,
        FsError::InvalidName => ENAMETOOLONG,
        FsError::Unsupported => ENOTSUP,
        _ => EIO,
    }
}

/// AgentFS FUSE filesystem adapter
pub struct AgentFsFuse {
    /// Core filesystem instance
    core: Arc<FsCore>,
    /// Internal PID used for privileged filesystem operations
    internal_pid: PID,
    /// Configuration
    config: FsConfig,
    /// UID of the FUSE host process (used for control file ownership)
    process_uid: u32,
    /// GID of the FUSE host process (used for control file ownership)
    process_gid: u32,
    /// TTL for attribute cache responses
    attr_ttl: Duration,
    /// TTL for directory entry cache responses
    entry_ttl: Duration,
    /// TTL for negative lookups (future use)
    #[allow(dead_code)]
    negative_ttl: Duration,
    /// Cache of inode to path mappings for control operations
    inodes: HashMap<u64, Vec<u8>>, // inode -> canonical path
    /// Reverse mapping from path to inode
    paths: HashMap<Vec<u8>, u64>, // path -> inode
    /// Open handles tracked per inode for fh-less operations
    inode_handles: HashMap<u64, HashSet<u64>>,
    /// Map handle IDs to (inode, owning pid)
    handle_index: HashMap<u64, (u64, u32)>,
    /// Per-handle write back state
    handle_write_states: HashMap<u64, Arc<WriteHandleState>>,
    /// Cache of special node kinds keyed by canonical path
    path_special_kinds: HashMap<Vec<u8>, SpecialNodeKind>,
    /// If true, force DIRECT_IO on opened files to bypass the kernel page cache
    force_direct_io: bool,
    /// Whether to attempt Linux passthrough fast-path
    enable_passthrough: bool,
    /// True when kernel accepted the passthrough capability
    passthrough_ready: bool,
    /// Active passthrough handles, keyed by FUSE file handle
    passthrough_handles: HashMap<u64, PassthroughHandle>,
    passthrough_metrics: PassthroughMetrics,
    /// Lower pass-through handles
    lower_handles: HashMap<u64, LowerHandle>,
    /// Next pass-through handle id
    next_lower_fh: u64,
    /// Tracks inodes representing lower-only entries (not yet materialized in AgentFS)
    lower_only_inodes: HashSet<u64>,
    /// Next synthetic inode id for lower-only entries
    next_lower_inode: u64,
    /// Explicit whiteouts for lower-only entries
    whiteouts: HashSet<Vec<u8>>,
    /// Whether to emit per-write latency tracing
    trace_writes: bool,
    /// Number of in-flight write requests (only tracked when tracing enabled)
    inflight_writes: Arc<AtomicU64>,
    /// Max observed concurrent write depth
    max_write_depth: Arc<AtomicU64>,
    /// Worker dispatcher for offloading writes
    write_dispatcher: WriteDispatcher,
    /// Optional kernel notifier for cache invalidation
    notifier_slot: Arc<Mutex<Option<Notifier>>>,
    /// Outstanding lookup refcounts per inode
    lookup_refcounts: HashMap<u64, u64>,
    /// Inodes that have been unlinked but not yet forgotten by the kernel
    tombstoned_inodes: HashSet<u64>,
    /// Global mutex to serialize metadata mutations (chmod/chown/utimens)
    metadata_lock: Arc<Mutex<()>>,
    /// Counter for assigning unique synthetic PIDs when the kernel reports pid=0
    pseudo_pid_counter: AtomicU32,
    /// Cached synthetic PID per (uid, gid, groups_hash) tuple for pid=0 requests
    pseudo_pid_map: Mutex<HashMap<(u32, u32, u64), u32>>,
}

impl AgentFsFuse {
    /// Create a new FUSE adapter with the given configuration
    pub fn new(config: FsConfig) -> FsResult<Self> {
        info!(
            target = "agentfs::fuse",
            default_uid = config.security.default_uid,
            default_gid = config.security.default_gid,
            "mount default owner"
        );
        info!(
            target = "agentfs::fuse",
            enforce_posix_permissions = config.security.enforce_posix_permissions,
            root_bypass_permissions = config.security.root_bypass_permissions,
            "security policy"
        );
        let force_direct_io = std::env::var("AGENTFS_FUSE_DIRECT_IO")
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false);
        let core = Arc::new(FsCore::new(config.clone())?);
        let host_pid = std::process::id();
        let internal_pid = core.register_process_with_groups(host_pid, host_pid, 0, 0, &[0]);
        if let Err(err) = core.set_owner(
            &internal_pid,
            Path::new("/"),
            config.security.default_uid,
            config.security.default_gid,
        ) {
            warn!(
                target: "agentfs::fuse",
                ?err,
                "failed to set root directory owner"
            );
        } else if let Ok(attrs) = core.getattr(&internal_pid, Path::new("/")) {
            info!(
                target = "agentfs::fuse",
                root_uid = attrs.uid,
                root_gid = attrs.gid,
                "root owner after init"
            );
        }
        let mut inodes = HashMap::new();
        let mut paths = HashMap::new();

        // Pre-populate special inodes
        inodes.insert(FUSE_ROOT_ID, b"/".to_vec());
        paths.insert(b"/".to_vec(), FUSE_ROOT_ID);
        inodes.insert(AGENTFS_DIR_INO, b"/.agentfs".to_vec());
        paths.insert(b"/.agentfs".to_vec(), AGENTFS_DIR_INO);
        inodes.insert(CONTROL_FILE_INO, b"/.agentfs/control".to_vec());
        paths.insert(b"/.agentfs/control".to_vec(), CONTROL_FILE_INO);

        let attr_ttl = Duration::from_millis(config.cache.attr_ttl_ms as u64);
        let entry_ttl = Duration::from_millis(config.cache.entry_ttl_ms as u64);
        let negative_ttl = Duration::from_millis(config.cache.negative_ttl_ms as u64);
        let trace_writes = std::env::var("AGENTFS_FUSE_TRACE_WRITES")
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false);
        let enable_passthrough = std::env::var("AGENTFS_FUSE_PASSTHROUGH")
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false);

        let inflight_writes = Arc::new(AtomicU64::new(0));
        let max_write_depth = Arc::new(AtomicU64::new(0));
        let write_dispatcher = WriteDispatcher::new(Arc::clone(&core), write_worker_count());
        let notifier_slot = Arc::new(Mutex::new(None));

        // Get the process UID/GID to use for control file ownership.
        // When the FUSE host runs as the user (not root), this ensures the control file
        // is accessible to the user without requiring --allow-other.
        let process_uid = unsafe { libc::getuid() };
        let process_gid = unsafe { libc::getgid() };
        debug!(
            target: "agentfs::fuse",
            process_uid,
            process_gid,
            "FUSE host process identity"
        );

        Ok(Self {
            core,
            internal_pid,
            config,
            process_uid,
            process_gid,
            attr_ttl,
            entry_ttl,
            negative_ttl,
            inodes,
            paths,
            inode_handles: HashMap::new(),
            handle_index: HashMap::new(),
            handle_write_states: HashMap::new(),
            path_special_kinds: HashMap::new(),
            force_direct_io,
            enable_passthrough,
            passthrough_ready: false,
            passthrough_handles: HashMap::new(),
            passthrough_metrics: PassthroughMetrics::default(),
            lower_handles: HashMap::new(),
            next_lower_fh: LOWER_HANDLE_BASE,
            lower_only_inodes: HashSet::new(),
            next_lower_inode: LOWER_INODE_BASE,
            whiteouts: HashSet::new(),
            trace_writes,
            inflight_writes,
            max_write_depth,
            write_dispatcher,
            notifier_slot,
            lookup_refcounts: HashMap::new(),
            tombstoned_inodes: HashSet::new(),
            metadata_lock: Arc::new(Mutex::new(())),
            pseudo_pid_counter: AtomicU32::new(0),
            pseudo_pid_map: Mutex::new(HashMap::new()),
        })
    }

    /// Returns a handle that allows the launcher thread to install the kernel notifier.
    pub fn notifier_registration(&self) -> NotifierRegistration {
        NotifierRegistration::new(&self.notifier_slot)
    }

    fn notifier(&self) -> Option<Notifier> {
        self.notifier_slot.lock().ok().and_then(|slot| slot.clone())
    }

    fn invalidate_inode_metadata(&self, ino: u64) {
        if matches!(ino, FUSE_ROOT_ID | AGENTFS_DIR_INO | CONTROL_FILE_INO) {
            return;
        }
        let path_hint =
            self.inode_to_path(ino).map(|bytes| self.path_from_bytes(bytes).to_path_buf());
        self.spawn_notifier_task(move |notifier| {
            if let Err(err) = notifier.inval_inode(ino, 0, 0) {
                warn!(
                    target: "agentfs::fuse",
                    ino,
                    path = path_hint
                        .as_ref()
                        .map(|p| p.display().to_string())
                        .unwrap_or_else(|| "<unknown>".into()),
                    %err,
                    "failed to invalidate inode after metadata change"
                );
            }
        });
    }

    fn spawn_notifier_task<F>(&self, task: F)
    where
        F: FnOnce(Notifier) + Send + 'static,
    {
        if let Some(notifier) = self.notifier() {
            if let Err(err) = thread::Builder::new()
                .name("agentfs-fuse-inval".into())
                .spawn(move || task(notifier))
            {
                warn!(
                    target: "agentfs::fuse",
                    ?err,
                    "failed to spawn notifier task"
                );
            }
        }
    }

    /// Get the path for a given inode
    fn inode_to_path(&self, ino: u64) -> Option<&[u8]> {
        self.inodes.get(&ino).map(|p| p.as_slice())
    }

    fn inode_is_tombstoned(&self, ino: u64) -> bool {
        self.tombstoned_inodes.contains(&ino)
    }

    fn clear_tombstone(&mut self, ino: u64) {
        self.tombstoned_inodes.remove(&ino);
    }

    fn bump_lookup(&mut self, ino: u64, delta: u64) {
        if delta == 0 || !Self::track_lookups_for(ino) {
            return;
        }
        let counter = self.lookup_refcounts.entry(ino).or_insert(0);
        *counter = counter.saturating_add(delta);
    }

    /// Returns true if the lookup refcount reaches zero and the inode can be freed.
    fn drop_lookup(&mut self, ino: u64, released: u64) -> bool {
        if released == 0 {
            return false;
        }
        if let Some(counter) = self.lookup_refcounts.get_mut(&ino) {
            if *counter > released {
                *counter -= released;
                return false;
            }
        }
        self.lookup_refcounts.remove(&ino);
        self.clear_tombstone(ino);
        true
    }

    fn track_lookups_for(ino: u64) -> bool {
        ino != FUSE_ROOT_ID && ino != AGENTFS_DIR_INO && ino != CONTROL_FILE_INO
    }

    fn resolve_parent_and_name(
        &self,
        canonical_path: &[u8],
        parent_hint: Option<u64>,
        name_hint: Option<&[u8]>,
    ) -> Option<(u64, OsString)> {
        let name_bytes = if let Some(name) = name_hint {
            name.to_vec()
        } else {
            let path = self.path_from_bytes(canonical_path);
            path.file_name()?.as_bytes().to_vec()
        };

        let parent_ino = if let Some(parent) = parent_hint {
            Some(parent)
        } else {
            let path = self.path_from_bytes(canonical_path);
            let parent_path = path.parent()?;
            let normalized = if parent_path.as_os_str().is_empty() {
                Path::new("/")
            } else {
                parent_path
            };
            let parent_bytes = normalized.as_os_str().as_bytes().to_vec();
            self.paths.get(&parent_bytes).copied()
        }?;

        Some((parent_ino, OsString::from_vec(name_bytes)))
    }

    fn invalidate_entry(&self, parent_ino: u64, name_bytes: &[u8]) {
        let name_os = OsString::from_vec(name_bytes.to_vec());
        self.spawn_notifier_task(move |notifier| {
            if let Err(err) = notifier.inval_entry(parent_ino, &name_os) {
                warn!(
                    target: "agentfs::fuse",
                    parent = parent_ino,
                    name = %name_os.to_string_lossy(),
                    %err,
                    "failed to invalidate entry"
                );
            }
        });
    }

    /// Gets the AgentFS PID for the client process making the request.
    /// This registers the process with its correct UID/GID if not seen before.
    fn get_client_pid(&self, req: &Request) -> PID {
        let client_pid = req.pid();
        let client_uid = req.uid();
        let client_gid = req.gid();
        let (effective_pid, groups, used_pseudo, groups_hash) = if client_pid == 0 {
            let groups = vec![client_gid];
            let hash = Self::groups_hash(&groups);
            let pseudo = self.synthetic_pid_for_identity(client_uid, client_gid, hash);
            (pseudo, groups, true, hash)
        } else {
            let groups = Self::load_process_groups(client_pid, client_gid);
            let hash = Self::groups_hash(&groups);
            (client_pid, groups, false, hash)
        };
        debug!(
            target: "agentfs::metadata",
            fuse_pid = client_pid,
            client_uid,
            client_gid,
            effective_pid,
            synthetic = used_pseudo,
            groups = ?groups,
            groups_hash = format_args!("{:016x}", groups_hash),
            "registering client process"
        );

        self.core.register_process_with_groups(
            effective_pid,
            effective_pid,
            client_uid,
            client_gid,
            groups.as_slice(),
        )
    }

    fn synthetic_pid_for_identity(&self, uid: u32, gid: u32, groups_hash: u64) -> u32 {
        let mut map = self.pseudo_pid_map.lock().unwrap();
        if let Some(pid) = map.get(&(uid, gid, groups_hash)) {
            return *pid;
        }
        if let Some(((existing_uid, existing_gid, seen_hash), seen_pid)) = map
            .iter()
            .find(|((seen_uid, seen_gid, _), _)| *seen_uid == uid && *seen_gid == gid)
            .map(|(key, pid)| (*key, *pid))
        {
            debug!(
                target: "agentfs::permissions",
                event = "synthetic_pid_groups_diverge",
                uid = existing_uid,
                gid = existing_gid,
                previous_groups_hash = format_args!("{:016x}", seen_hash),
                new_groups_hash = format_args!("{:016x}", groups_hash),
                previous_pid = seen_pid
            );
        }
        let next = self.pseudo_pid_counter.fetch_add(1, Ordering::Relaxed) + 1;
        let pid = PSEUDO_PID_BASE.saturating_add(next);
        map.insert((uid, gid, groups_hash), pid);
        pid
    }

    fn groups_hash(groups: &[u32]) -> u64 {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        groups.hash(&mut hasher);
        hasher.finish()
    }

    fn load_process_groups(pid: u32, primary_gid: u32) -> Vec<u32> {
        let status_path = format!("/proc/{pid}/status");
        let mut groups = match std::fs::read_to_string(&status_path) {
            Ok(contents) => contents
                .lines()
                .find_map(|line| {
                    line.strip_prefix("Groups:").map(|rest| {
                        rest.split_whitespace()
                            .filter_map(|g| g.parse::<u32>().ok())
                            .collect::<Vec<_>>()
                    })
                })
                .unwrap_or_default(),
            Err(err) => {
                warn!(
                    target: "agentfs::fuse",
                    pid,
                    %status_path,
                    %err,
                    "failed to read supplementary groups; falling back to primary gid"
                );
                Vec::new()
            }
        };
        debug!(
            target: "agentfs::metadata",
            pid,
            primary_gid = primary_gid,
            groups = ?groups,
            "resolved process groups"
        );

        if groups.is_empty() || !groups.contains(&primary_gid) {
            groups.push(primary_gid);
        }

        groups
    }

    fn fuse_ino_from_node(&self, node_id: u64) -> u64 {
        node_id + FIRST_DYNAMIC_INO
    }

    fn node_id_from_inode(&self, ino: u64) -> Option<u64> {
        if self.lower_only_inodes.contains(&ino) {
            return None;
        }
        if ino >= FIRST_DYNAMIC_INO {
            Some(ino - FIRST_DYNAMIC_INO)
        } else {
            None
        }
    }

    fn ensure_inode_for_path(
        &mut self,
        pid: &PID,
        canonical_path: &[u8],
        logical_path: &Path,
    ) -> FsResult<u64> {
        if let Some(&inode) = self.paths.get(canonical_path) {
            return Ok(inode);
        }
        match self.core.resolve_path_public(pid, logical_path) {
            Ok((node_id, _)) => Ok(self.record_path_for_node(canonical_path.to_vec(), node_id)),
            Err(FsError::NotFound) => {
                if self.overlay_enabled() && self.lower_entry_exists(logical_path) {
                    Ok(self.record_lower_inode(canonical_path.to_vec()))
                } else {
                    Err(FsError::NotFound)
                }
            }
            Err(err) => Err(err),
        }
    }

    /// Associate a path with a node, preserving canonical mapping.
    fn record_path_for_node(&mut self, path: Vec<u8>, node_id: u64) -> u64 {
        let inode = self.fuse_ino_from_node(node_id);
        self.whiteouts.remove(&path);
        if let Some(old) = self.paths.insert(path.clone(), inode) {
            if self.lower_only_inodes.remove(&old) {
                self.inodes.remove(&old);
            }
        }
        self.inodes.insert(inode, path);
        self.lower_only_inodes.remove(&inode);
        self.clear_tombstone(inode);
        inode
    }

    fn record_lower_inode(&mut self, path: Vec<u8>) -> u64 {
        let inode = self.next_lower_inode;
        self.next_lower_inode = self.next_lower_inode.wrapping_add(1);
        self.whiteouts.remove(&path);
        self.paths.insert(path.clone(), inode);
        self.inodes.insert(inode, path);
        self.lower_only_inodes.insert(inode);
        inode
    }

    fn cache_special_kind_for_inode(&mut self, inode: u64, special: Option<&SpecialNodeKind>) {
        if let (Some(kind), Some(path_bytes)) = (special, self.inodes.get(&inode)) {
            let path = path_bytes.clone();
            debug!(
                target: "agentfs::fuse",
                event = "cache_special_kind",
                ino = inode,
                path = %self.path_from_bytes(&path).display(),
                special = ?kind
            );
            self.path_special_kinds.insert(path, kind.clone());
        }
    }

    fn clear_cached_special_kind(&mut self, path: &[u8]) {
        if self.path_special_kinds.remove(path).is_some() {
            debug!(
                target: "agentfs::fuse",
                event = "drop_special_kind",
                path = %self.path_from_bytes(path).display()
            );
        }
    }

    fn clear_whiteout(&mut self, path: &[u8]) {
        self.whiteouts.remove(path);
    }

    fn is_whiteouted(&self, path: &[u8]) -> bool {
        self.whiteouts.contains(path)
    }

    fn record_whiteout(&mut self, path: Vec<u8>) {
        self.whiteouts.insert(path);
    }

    fn overlay_enabled(&self) -> bool {
        self.config.overlay.enabled && self.config.overlay.lower_root.is_some()
    }

    fn lower_root(&self) -> Option<&Path> {
        self.config.overlay.lower_root.as_deref()
    }

    fn lower_full_path(&self, path: &Path) -> Option<PathBuf> {
        self.lower_root().map(|root| {
            let rel = path.strip_prefix("/").unwrap_or(path);
            root.join(rel)
        })
    }

    fn lower_entry_exists(&self, path: &Path) -> bool {
        self.lower_full_path(path).map(|p| p.exists()).unwrap_or(false)
    }

    fn start_write_trace(
        &self,
        req: &Request,
        ino: u64,
        fh: u64,
        offset: i64,
        size: usize,
    ) -> Option<WriteTrace> {
        if !self.trace_writes {
            return None;
        }

        let inflight = self.inflight_writes.fetch_add(1, Ordering::SeqCst) + 1;
        let mut current = self.max_write_depth.load(Ordering::SeqCst);
        while inflight > current {
            match self.max_write_depth.compare_exchange(
                current,
                inflight,
                Ordering::SeqCst,
                Ordering::SeqCst,
            ) {
                Ok(_) => break,
                Err(old) => current = old,
            }
        }

        debug!(
            target: "agentfs::write",
            event = "start",
            req_id = req.unique(),
            ino,
            fh,
            offset,
            size,
            inflight,
            max_inflight = self.max_write_depth.load(Ordering::SeqCst)
        );

        Some(WriteTrace::new(
            req.unique(),
            ino,
            fh,
            offset,
            size,
            Arc::clone(&self.inflight_writes),
            Arc::clone(&self.max_write_depth),
        ))
    }

    fn alloc_lower_handle(&mut self, file: File) -> u64 {
        let fh = self.next_lower_fh;
        self.next_lower_fh = self.next_lower_fh.saturating_add(1);
        self.lower_handles.insert(fh, LowerHandle { file });
        fh
    }

    fn ensure_upper_parent_dirs(&mut self, pid: &PID, path: &Path) -> FsResult<()> {
        if let Some(parent) = path.parent() {
            use std::path::Component;
            let mut current = PathBuf::from("/");
            for component in parent.components() {
                if let Component::Normal(name) = component {
                    current.push(name);
                    let upper_exists = self.core.has_upper_entry(pid, &current)?;
                    if upper_exists {
                        continue;
                    }
                    match self.core.mkdir(pid, &current, 0o755) {
                        Ok(()) => {}
                        Err(FsError::AlreadyExists) => {}
                        Err(err) => return Err(err),
                    }
                }
            }
        }
        Ok(())
    }

    fn copy_lower_to_upper(&mut self, pid: &PID, path: &Path, canonical: &[u8]) -> FsResult<()> {
        let lower_path = self.lower_full_path(path).ok_or(FsError::NotFound)?;
        if !lower_path.exists() {
            return Err(FsError::NotFound);
        }

        self.ensure_upper_parent_dirs(pid, path)?;

        let opts = OpenOptions {
            read: true,
            write: true,
            create: true,
            ..OpenOptions::default()
        };
        let handle = self.core.create(pid, path, &opts)?;

        let mut lower_file = File::open(&lower_path)?;
        let mut buffer = vec![0u8; 8192];
        let mut offset = 0u64;
        loop {
            let read_bytes = lower_file.read(&mut buffer)?;
            if read_bytes == 0 {
                break;
            }
            self.core.write(pid, handle, offset, &buffer[..read_bytes])?;
            offset += read_bytes as u64;
        }
        self.core.close(pid, handle)?;

        let metadata = std::fs::metadata(&lower_path)?;
        #[cfg(unix)]
        {
            let mode = metadata.permissions().mode();
            let _ = self.core.set_mode(pid, path, mode);
            let _ = self.core.set_owner(pid, path, metadata.uid() as u32, metadata.gid() as u32);
        }

        self.clear_whiteout(canonical);
        if let Ok((node_id, _)) = self.core.resolve_path_public(pid, path) {
            self.record_path_for_node(canonical.to_vec(), node_id);
        }
        Ok(())
    }

    /// Remove a single path mapping and update canonical bookkeeping.
    fn remove_path_mapping(&mut self, path: &[u8]) -> Option<RemovalOutcome> {
        let inode = self.paths.remove(path)?;
        if self.lower_only_inodes.remove(&inode) {
            self.inodes.remove(&inode);
            return None;
        }
        let node_id = match self.node_id_from_inode(inode) {
            Some(id) => id,
            None => {
                // Special entries don't participate in removal bookkeeping.
                return None;
            }
        };
        let removed_was_canonical =
            self.inodes.get(&inode).map(|p| p.as_slice() == path).unwrap_or(false);
        let mut has_other_bindings = false;

        if removed_was_canonical {
            if let Some((replacement, _)) = self.paths.iter().find(|(_, &ino)| ino == inode) {
                has_other_bindings = true;
                self.inodes.insert(inode, replacement.clone());
            } else {
                self.inodes.remove(&inode);
            }
        } else if self.inodes.contains_key(&inode) {
            has_other_bindings = true;
        }

        Some(RemovalOutcome {
            node_id,
            has_other_bindings,
        })
    }

    fn purge_by_canonical_path(
        &mut self,
        canonical_path: &[u8],
        parent_hint: Option<u64>,
        name_hint: Option<&[u8]>,
    ) -> Option<RemovalOutcome> {
        let removal = self.remove_path_mapping(canonical_path)?;
        self.clear_cached_special_kind(canonical_path);

        if !removal.has_other_bindings {
            let inode = self.fuse_ino_from_node(removal.node_id);
            self.tombstoned_inodes.insert(inode);
            debug!(
                target: "agentfs::fuse",
                event = "inode_tombstone",
                ino = inode,
                path = %self.path_from_bytes(canonical_path).display()
            );
        }

        if let Some((parent_ino, name_os)) =
            self.resolve_parent_and_name(canonical_path, parent_hint, name_hint)
        {
            let inode = self.fuse_ino_from_node(removal.node_id);
            let drop_inode = !removal.has_other_bindings;
            self.spawn_notifier_task(move |notifier| {
                if let Err(err) = notifier.inval_entry(parent_ino, &name_os) {
                    warn!(
                        target: "agentfs::fuse",
                        ino = inode,
                        parent = parent_ino,
                        name = %name_os.to_string_lossy(),
                        %err,
                        "failed to invalidate directory entry"
                    );
                }
                if drop_inode {
                    if let Err(err) = notifier.inval_inode(inode, 0, 0) {
                        warn!(
                            target: "agentfs::fuse",
                            ino = inode,
                            %err,
                            "failed to invalidate inode attributes"
                        );
                    }
                }
            });
        }

        Some(removal)
    }

    fn track_handle(&mut self, ino: u64, fh: u64, pid: &PID) {
        self.inode_handles.entry(ino).or_default().insert(fh);
        self.handle_index.insert(fh, (ino, pid.as_u32()));
        self.handle_write_states
            .entry(fh)
            .or_insert_with(|| Arc::new(WriteHandleState::new()));
    }

    fn untrack_handle(&mut self, ino: u64, fh: u64) {
        if let Some(handles) = self.inode_handles.get_mut(&ino) {
            handles.remove(&fh);
            if handles.is_empty() {
                self.inode_handles.remove(&ino);
            }
        }
        self.handle_index.remove(&fh);
        self.handle_write_states.remove(&fh);
    }

    fn handle_for_inode(&self, ino: u64) -> Option<(u64, PID)> {
        self.inode_handles
            .get(&ino)
            .and_then(|handles| handles.iter().next())
            .and_then(|fh| self.handle_index.get(fh).map(|(_, pid)| (*fh, PID::new(*pid))))
    }

    fn handle_write_state(&self, fh: u64) -> Option<Arc<WriteHandleState>> {
        self.handle_write_states.get(&fh).cloned()
    }

    fn wait_for_handle_writes(&self, fh: u64) -> Result<(), i32> {
        if let Some(state) = self.handle_write_state(fh) {
            state.wait_for_all();
            if let Some(errno) = state.current_error() {
                return Err(errno);
            }
        }
        Ok(())
    }

    fn forget_inode(&mut self, inode: u64) {
        if inode == FUSE_ROOT_ID || inode == AGENTFS_DIR_INO || inode == CONTROL_FILE_INO {
            return;
        }

        self.lookup_refcounts.remove(&inode);
        self.tombstoned_inodes.remove(&inode);

        if self.lower_only_inodes.remove(&inode) {
            self.inodes.remove(&inode);
            self.paths.retain(|_, &mut ino| ino != inode);
            return;
        }

        if let Some(handles) = self.inode_handles.remove(&inode) {
            for fh in handles {
                self.handle_index.remove(&fh);
            }
        }

        self.inodes.remove(&inode);
        self.paths.retain(|_, &mut ino| ino != inode);
    }

    /// Convert a path slice to a Path
    fn path_from_bytes<'a>(&self, path: &'a [u8]) -> &'a Path {
        Path::new(OsStr::from_bytes(path))
    }

    /// Convert FsCore Attributes to FUSE FileAttr
    fn attr_to_fuse(&mut self, attr: &Attributes, ino: u64) -> FileAttr {
        self.cache_special_kind_for_inode(ino, attr.special_kind.as_ref());
        let (kind, rdev) = if attr.is_dir {
            (FileType::Directory, 0)
        } else if attr.is_symlink {
            (FileType::Symlink, 0)
        } else if let Some(special) = &attr.special_kind {
            match special {
                agentfs_core::SpecialNodeKind::Fifo => (FileType::NamedPipe, 0),
                agentfs_core::SpecialNodeKind::CharDevice { dev } => {
                    (FileType::CharDevice, (*dev).try_into().unwrap_or(u32::MAX))
                }
                agentfs_core::SpecialNodeKind::BlockDevice { dev } => {
                    (FileType::BlockDevice, (*dev).try_into().unwrap_or(u32::MAX))
                }
                agentfs_core::SpecialNodeKind::Socket => (FileType::Socket, 0),
            }
        } else {
            (FileType::RegularFile, 0)
        };

        let mut file_attr = FileAttr {
            ino,
            size: attr.len,
            blocks: attr.len.div_ceil(512), // 512-byte blocks
            atime: SystemTime::UNIX_EPOCH
                + Duration::new(attr.times.atime.max(0) as u64, attr.times.atime_nsec),
            mtime: SystemTime::UNIX_EPOCH
                + Duration::new(attr.times.mtime.max(0) as u64, attr.times.mtime_nsec),
            ctime: SystemTime::UNIX_EPOCH
                + Duration::new(attr.times.ctime.max(0) as u64, attr.times.ctime_nsec),
            crtime: SystemTime::UNIX_EPOCH
                + Duration::new(
                    attr.times.birthtime.max(0) as u64,
                    attr.times.birthtime_nsec,
                ),
            kind,
            perm: attr.mode() as u16,
            nlink: attr.nlink.max(1),
            uid: attr.uid,
            gid: attr.gid,
            rdev,
            blksize: 512,
            flags: 0, // macOS specific
        };

        if ino == FUSE_ROOT_ID {
            file_attr.uid = self.config.security.default_uid;
            file_attr.gid = self.config.security.default_gid;
        }

        file_attr
    }

    fn stat_to_file_attr(&self, stat: &StatData, ino: u64) -> FileAttr {
        let kind = match stat.st_mode & libc::S_IFMT {
            m if m == libc::S_IFDIR => FileType::Directory,
            m if m == libc::S_IFLNK => FileType::Symlink,
            m if m == libc::S_IFCHR => FileType::CharDevice,
            m if m == libc::S_IFBLK => FileType::BlockDevice,
            m if m == libc::S_IFIFO => FileType::NamedPipe,
            m if m == libc::S_IFSOCK => FileType::Socket,
            _ => FileType::RegularFile,
        };

        let to_system_time = |secs: u64, nanos: u32| {
            SystemTime::UNIX_EPOCH + Duration::from_secs(secs) + Duration::from_nanos(nanos as u64)
        };

        FileAttr {
            ino,
            size: stat.st_size,
            blocks: stat.st_blocks,
            atime: to_system_time(stat.st_atime, stat.st_atime_nsec),
            mtime: to_system_time(stat.st_mtime, stat.st_mtime_nsec),
            ctime: to_system_time(stat.st_ctime, stat.st_ctime_nsec),
            crtime: to_system_time(stat.st_ctime, stat.st_ctime_nsec),
            kind,
            perm: (stat.st_mode & 0o7777) as u16,
            nlink: stat.st_nlink,
            uid: stat.st_uid,
            gid: stat.st_gid,
            rdev: stat.st_rdev as u32,
            blksize: stat.st_blksize,
            flags: 0,
        }
    }

    fn open_flags(&self) -> u32 {
        if self.force_direct_io {
            FOPEN_DIRECT_IO
        } else {
            0
        }
    }

    fn passthrough_active(&self) -> bool {
        self.enable_passthrough && self.passthrough_ready
    }

    fn passthrough_writable(&self, fh: u64) -> bool {
        self.passthrough_handles.get(&fh).map(|state| state.writable).unwrap_or(false)
    }

    fn log_passthrough_stats(&self) {
        if !self.enable_passthrough {
            return;
        }
        let snapshot = self.passthrough_metrics.snapshot();
        info!(
            target: "agentfs::fuse",
            attempts = snapshot.attempts,
            successes = snapshot.successes,
            not_ready = snapshot.not_ready,
            missing_path = snapshot.missing_path,
            open_failed = snapshot.open_failed,
            ioctl_failed = snapshot.ioctl_failed,
            "passthrough metrics snapshot"
        );
    }

    fn maybe_refresh_passthrough_metadata(&self, fh: u64) {
        if !self.passthrough_writable(fh) {
            return;
        }
        if let Err(err) = self.core.refresh_backing_len(HandleId(fh)) {
            warn!(
                target: "agentfs::fuse",
                fh,
                ?err,
                "failed to refresh metadata for passthrough handle"
            );
        }
    }

    fn try_passthrough_open(
        &mut self,
        ino: u64,
        user_path: &Path,
        handle_id: HandleId,
        options: &OpenOptions,
        reply: ReplyOpen,
    ) -> Result<(), ReplyOpen> {
        self.passthrough_metrics.record_attempt();
        if !self.passthrough_active() {
            debug!(
                target: "agentfs::fuse",
                ino,
                fh = handle_id.0,
                path = %user_path.display(),
                reason = "not_ready",
                "passthrough disabled or unsupported"
            );
            self.passthrough_metrics.record_not_ready();
            return Err(reply);
        }

        let requires_data_path = options.read || options.write || options.append;
        if !requires_data_path {
            debug!(
                target: "agentfs::fuse",
                ino,
                fh = handle_id.0,
                path = %user_path.display(),
                reason = "metadata_only",
                "passthrough skipped (metadata access)"
            );
            return Err(reply);
        }

        let Some(path) = self.core.handle_backing_path(handle_id) else {
            debug!(
                target: "agentfs::fuse",
                ino,
                fh = handle_id.0,
                path = %user_path.display(),
                reason = "missing_content",
                "passthrough skipped (no backing file path)"
            );
            self.passthrough_metrics.record_missing_path();
            return Err(reply);
        };

        let mut opts = StdOpenOptions::new();
        opts.read(true);
        if options.write || options.append || options.truncate {
            opts.write(true);
        }
        if options.append {
            opts.append(true);
        }

        let file = match opts.open(&path) {
            Ok(f) => f,
            Err(err) => {
                debug!(
                    target: "agentfs::fuse",
                    ino,
                    fh = handle_id.0,
                    path = %user_path.display(),
                    backing = %path.display(),
                    %err,
                    reason = "open_failed",
                    "passthrough backing open failed"
                );
                self.passthrough_metrics.record_open_failed();
                warn!(
                    target: "agentfs::fuse",
                    fh = handle_id.0,
                    path = %path.display(),
                    %err,
                    "failed to open backing file for passthrough"
                );
                return Err(reply);
            }
        };

        let backing_id = match reply.open_backing(&file) {
            Ok(id) => id,
            Err(err) => {
                debug!(
                    target: "agentfs::fuse",
                    ino,
                    fh = handle_id.0,
                    path = %user_path.display(),
                    backing = %path.display(),
                    %err,
                    reason = "ioctl_failed",
                    "open_backing ioctl failed"
                );
                self.passthrough_metrics.record_ioctl_failed();
                warn!(
                    target: "agentfs::fuse",
                    fh = handle_id.0,
                    %err,
                    "open_backing ioctl failed"
                );
                if matches!(err.raw_os_error(), Some(libc::EPERM) | Some(libc::EACCES)) {
                    self.passthrough_ready = false;
                    warn!(
                        target: "agentfs::fuse",
                        "Passthrough disabled after permission error (requires CAP_SYS_ADMIN on /dev/fuse connection)"
                    );
                    self.log_passthrough_stats();
                }
                return Err(reply);
            }
        };

        let fh = handle_id.0;
        let writable = options.write || options.append || options.truncate;
        self.passthrough_handles.insert(
            fh,
            PassthroughHandle {
                backing: backing_id,
                writable,
            },
        );

        if let Some(state) = self.passthrough_handles.get(&fh) {
            debug!(
                target: "agentfs::fuse",
                ino,
                fh,
                path = %user_path.display(),
                backing = %path.display(),
                writable,
                "passthrough enabled"
            );
            self.passthrough_metrics.record_success();
            reply.opened_passthrough(fh, self.open_flags(), &state.backing);
            Ok(())
        } else {
            Err(reply)
        }
    }

    /// Convert FUSE flags to OpenOptions
    fn fuse_flags_to_options(&self, flags: i32) -> OpenOptions {
        use libc::{O_APPEND, O_CREAT, O_RDWR, O_TRUNC, O_WRONLY};

        let mut options = OpenOptions::default();

        // POSIX semantics allow multiple opens unless explicit locking is requested.
        // Default to sharing read/write/delete so multiple descriptors can coexist.
        options.share.push(ShareMode::Read);
        options.share.push(ShareMode::Write);
        options.share.push(ShareMode::Delete);

        // Access mode
        if flags & O_RDWR != 0 {
            options.read = true;
            options.write = true;
        } else if flags & O_WRONLY != 0 {
            options.write = true;
        } else {
            options.read = true;
        }

        // Creation flags
        if flags & O_CREAT != 0 {
            options.create = true;
        }
        // Note: O_EXCL (create_new) is not directly supported in agentfs_core::OpenOptions
        // This would need to be handled at a higher level
        if flags & O_TRUNC != 0 {
            options.truncate = true;
        }
        if flags & O_APPEND != 0 {
            options.append = true;
        }

        options
    }

    /// Handle control plane operations via ioctl
    fn handle_control_ioctl(&self, data: &[u8]) -> Result<Vec<u8>, c_int> {
        use agentfs_proto::*;

        let request_bytes = Self::extract_framed_payload(data).inspect_err(|&code| {
            error!("Malformed control request payload (errno={})", code);
        })?;

        let request: Request = <Request as Decode>::from_ssz_bytes(request_bytes).map_err(|e| {
            error!("Failed to decode SSZ control request: {:?}", e);
            EINVAL
        })?;

        // Validate request structure
        if let Err(e) = validate_request(&request) {
            error!("Request validation failed: {}", e);
            let response = Response::error(format!("{}", e), Some(EINVAL as u32));
            return Ok(Self::frame_response_bytes(
                <Response as Encode>::as_ssz_bytes(&response),
            ));
        }

        match request {
            Request::SnapshotCreate((_, req)) => {
                let name_str = req.name.as_ref().map(|n| String::from_utf8_lossy(n).to_string());
                match self.core.snapshot_create(name_str.as_deref()) {
                    Ok(snapshot_id) => {
                        // Get snapshot name from the list (inefficient but works for now)
                        let snapshots = self.core.snapshot_list();
                        let name = snapshots
                            .iter()
                            .find(|(id, _)| *id == snapshot_id)
                            .and_then(|(_, name)| name.clone());

                        let response = Response::snapshot_create(SnapshotInfo {
                            id: snapshot_id.to_string().into_bytes(),
                            name: name.map(|s| s.into_bytes()),
                        });
                        Ok(Self::frame_response_bytes(
                            <Response as Encode>::as_ssz_bytes(&response),
                        ))
                    }
                    Err(e) => {
                        let errno = match e {
                            FsError::NotFound => ENOENT,
                            FsError::AlreadyExists => EEXIST,
                            FsError::AccessDenied => EACCES,

                            FsError::OperationNotPermitted => EPERM,
                            FsError::InvalidArgument => EINVAL,
                            _ => EIO,
                        };
                        let response = Response::error(format!("{:?}", e), Some(errno as u32));
                        Ok(Self::frame_response_bytes(
                            <Response as Encode>::as_ssz_bytes(&response),
                        ))
                    }
                }
            }
            Request::SnapshotList(_) => {
                let snapshots = self.core.snapshot_list();
                let snapshot_infos: Vec<SnapshotInfo> = snapshots
                    .into_iter()
                    .map(|(id, name)| SnapshotInfo {
                        id: id.to_string().into_bytes(),
                        name: name.map(|s| s.into_bytes()),
                    })
                    .collect();

                let response = Response::snapshot_list(snapshot_infos);
                Ok(Self::frame_response_bytes(response.as_ssz_bytes()))
            }
            Request::BranchCreate((_, req)) => {
                let from_str = String::from_utf8_lossy(&req.from).to_string();
                let name_str = req.name.as_ref().map(|n| String::from_utf8_lossy(n).to_string());
                match self.core.branch_create_from_snapshot(
                    from_str.parse().map_err(|_| EINVAL)?,
                    name_str.as_deref(),
                ) {
                    Ok(branch_id) => {
                        // Get branch info from the list
                        let branches = self.core.branch_list();
                        let info = branches.iter().find(|b| b.id == branch_id).ok_or(EIO)?;

                        let response = Response::branch_create(BranchInfo {
                            id: info.id.to_string().into_bytes(),
                            name: info.name.clone().map(|s| s.into_bytes()),
                            parent: info
                                .parent
                                .map(|p| p.to_string())
                                .unwrap_or_default()
                                .into_bytes(),
                        });
                        Ok(Self::frame_response_bytes(
                            <Response as Encode>::as_ssz_bytes(&response),
                        ))
                    }
                    Err(e) => {
                        let errno = match e {
                            FsError::NotFound => ENOENT,
                            FsError::AlreadyExists => EEXIST,
                            FsError::AccessDenied => EACCES,

                            FsError::OperationNotPermitted => EPERM,
                            FsError::InvalidArgument => EINVAL,
                            _ => EIO,
                        };
                        let response = Response::error(format!("{:?}", e), Some(errno as u32));
                        Ok(Self::frame_response_bytes(
                            <Response as Encode>::as_ssz_bytes(&response),
                        ))
                    }
                }
            }
            Request::BranchBind((_, req)) => {
                let pid = req.pid.unwrap_or_else(std::process::id);
                let branch_str = String::from_utf8_lossy(&req.branch).to_string();
                let branch_id = branch_str.parse().map_err(|_| EINVAL)?;

                match self.core.bind_process_to_branch_with_pid(branch_id, pid) {
                    Ok(()) => {
                        let response = Response::branch_bind(req.branch.clone(), pid);
                        Ok(Self::frame_response_bytes(
                            <Response as Encode>::as_ssz_bytes(&response),
                        ))
                    }
                    Err(e) => {
                        let errno = match e {
                            FsError::NotFound => ENOENT,
                            FsError::AlreadyExists => EEXIST,
                            FsError::AccessDenied => EACCES,

                            FsError::OperationNotPermitted => EPERM,
                            FsError::InvalidArgument => EINVAL,
                            _ => EIO,
                        };
                        let response = Response::error(format!("{:?}", e), Some(errno as u32));
                        Ok(Self::frame_response_bytes(
                            <Response as Encode>::as_ssz_bytes(&response),
                        ))
                    }
                }
            }
            _ => {
                // For now, only handle the basic control operations mentioned in the milestone
                let response =
                    Response::error("Unsupported operation".to_string(), Some(ENOTSUP as u32));
                Ok(Self::frame_response_bytes(response.as_ssz_bytes()))
            }
        }
    }

    fn extract_framed_payload(data: &[u8]) -> Result<&[u8], c_int> {
        if data.len() < 4 {
            return Err(EINVAL);
        }
        let mut len_bytes = [0u8; 4];
        len_bytes.copy_from_slice(&data[..4]);
        let payload_len = u32::from_le_bytes(len_bytes) as usize;
        let end = 4 + payload_len;
        if payload_len == 0 || end > data.len() {
            return Err(EINVAL);
        }
        Ok(&data[4..end])
    }

    fn frame_response_bytes(mut payload: Vec<u8>) -> Vec<u8> {
        let mut framed = Vec::with_capacity(payload.len() + 4);
        framed.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        framed.append(&mut payload);
        framed
    }
}

fn write_worker_count() -> usize {
    if let Ok(value) = std::env::var("AGENTFS_FUSE_WRITE_THREADS") {
        value
            .parse::<usize>()
            .ok()
            .and_then(|n| if n == 0 { None } else { Some(n) })
            .unwrap_or_else(|| thread::available_parallelism().map(|p| p.get()).unwrap_or(1))
    } else {
        thread::available_parallelism().map(|p| p.get()).unwrap_or(1).max(2)
    }
}

const DEFAULT_MAX_WRITE_BYTES: u32 = 4 * 1024 * 1024;
const DEFAULT_MAX_BACKGROUND: u16 = 64;
const MAX_SUPPORTED_WRITE_BYTES: u32 = 16 * 1024 * 1024;

fn desired_max_write_bytes() -> u32 {
    std::env::var("AGENTFS_FUSE_MAX_WRITE")
        .ok()
        .and_then(|v| v.parse::<u32>().ok())
        .filter(|value| *value > 0)
        .map(|value| value.min(MAX_SUPPORTED_WRITE_BYTES))
        .unwrap_or(DEFAULT_MAX_WRITE_BYTES)
}

fn desired_max_background() -> u16 {
    std::env::var("AGENTFS_FUSE_MAX_BACKGROUND")
        .ok()
        .and_then(|v| v.parse::<u16>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(DEFAULT_MAX_BACKGROUND)
}

fn configure_max_write(config: &mut fuser::KernelConfig) -> (u32, bool) {
    let desired = desired_max_write_bytes();
    match config.set_max_write(desired) {
        Ok(_) => (desired, false),
        Err(limit) => {
            let _ = config.set_max_write(limit);
            (limit, true)
        }
    }
}

fn configure_max_background(config: &mut fuser::KernelConfig) -> (u16, bool) {
    let desired = desired_max_background();
    match config.set_max_background(desired) {
        Ok(_) => (desired, false),
        Err(limit) => {
            let _ = config.set_max_background(limit);
            (limit, true)
        }
    }
}

fn desired_congestion_threshold(max_background: u16) -> u16 {
    let numerator = 3u32 * max_background as u32;
    let denominator = 4u32;
    let mut value = (numerator / denominator).max(1) as u16;
    if value == 0 {
        value = 1;
    }
    value
}

fn configure_congestion_threshold(
    config: &mut fuser::KernelConfig,
    max_background: u16,
) -> (u16, bool) {
    let desired = desired_congestion_threshold(max_background);
    match config.set_congestion_threshold(desired) {
        Ok(_) => (desired, false),
        Err(limit) => {
            let _ = config.set_congestion_threshold(limit);
            (limit, true)
        }
    }
}

impl Drop for AgentFsFuse {
    fn drop(&mut self) {
        self.log_passthrough_stats();
    }
}

impl fuser::Filesystem for AgentFsFuse {
    fn init(&mut self, _req: &Request, config: &mut fuser::KernelConfig) -> Result<(), c_int> {
        if self.enable_passthrough {
            match config.add_capabilities(FUSE_PASSTHROUGH) {
                Ok(()) => {
                    self.passthrough_ready = true;
                    info!("Passthrough capability negotiated with kernel");
                }
                Err(unsupported) => {
                    self.passthrough_ready = false;
                    warn!(
                        "Kernel rejected passthrough capability bits: {:#x}",
                        unsupported
                    );
                }
            }
        } else {
            self.passthrough_ready = false;
        }
        if self.enable_passthrough && !self.passthrough_ready {
            self.log_passthrough_stats();
        }

        if self.force_direct_io {
            info!("Forcing DIRECT_IO on all regular file handles (kernel page cache bypass)");
        }
        if self.config.cache.writeback_cache {
            match config.add_capabilities(FUSE_WRITEBACK_CACHE_FLAG) {
                Ok(()) => info!("Requested kernel writeback cache capability"),
                Err(missing) => warn!(
                    "Kernel rejected writeback cache capability (missing bits: {:#x})",
                    missing
                ),
            }
        }

        let (max_write, clamped_write) = configure_max_write(config);
        if clamped_write {
            warn!(
                "Kernel limited max_write to {} bytes (desired {}).",
                max_write,
                desired_max_write_bytes()
            );
        } else {
            info!("Configured FUSE max_write={} bytes", max_write);
        }

        let (max_background, clamped_background) = configure_max_background(config);
        if clamped_background {
            warn!(
                "Kernel limited max_background to {} (desired {}).",
                max_background,
                desired_max_background()
            );
        } else {
            info!("Configured FUSE max_background={}", max_background);
        }

        let (congestion, clamped_congestion) =
            configure_congestion_threshold(config, max_background);
        if clamped_congestion {
            warn!(
                "Kernel limited congestion_threshold to {} (derived from max_background={}).",
                congestion, max_background
            );
        } else {
            info!("Configured FUSE congestion_threshold={}", congestion);
        }

        info!(
            "AgentFS FUSE adapter initialized (write_threads={}, trace_writes={})",
            self.write_dispatcher.handles.len(),
            self.trace_writes
        );
        Ok(())
    }

    fn destroy(&mut self) {
        info!("AgentFS FUSE adapter destroyed");
    }

    fn forget(&mut self, _req: &Request, ino: u64, nlookup: u64) {
        if !Self::track_lookups_for(ino) {
            return;
        }
        if self.drop_lookup(ino, nlookup) {
            self.forget_inode(ino);
        }
    }

    fn batch_forget(&mut self, _req: &Request, nodes: &[fuse_forget_one]) {
        for node in nodes {
            let ino = node.nodeid;
            if !Self::track_lookups_for(ino) {
                continue;
            }
            if self.drop_lookup(ino, node.nlookup) {
                self.forget_inode(ino);
            }
        }
    }

    fn lookup(&mut self, req: &Request, parent: u64, name: &OsStr, reply: ReplyEntry) {
        let name_bytes = name.as_bytes();

        if name_bytes.len() > NAME_MAX {
            reply.error(ENAMETOOLONG);
            return;
        }

        // Handle special .agentfs directory and control file
        if parent == FUSE_ROOT_ID && name == ".agentfs" {
            let attr = FileAttr {
                ino: AGENTFS_DIR_INO,
                size: 0,
                blocks: 0,
                atime: SystemTime::UNIX_EPOCH,
                mtime: SystemTime::UNIX_EPOCH,
                ctime: SystemTime::UNIX_EPOCH,
                crtime: SystemTime::UNIX_EPOCH,
                kind: FileType::Directory,
                perm: 0o755,
                nlink: 2,
                uid: self.process_uid,
                gid: self.process_gid,
                rdev: 0,
                blksize: 512,
                flags: 0,
            };
            reply.entry(&self.entry_ttl, &attr, 0);
            return;
        }

        if parent == AGENTFS_DIR_INO && name == "control" {
            let attr = FileAttr {
                ino: CONTROL_FILE_INO,
                size: 0,
                blocks: 0,
                atime: SystemTime::UNIX_EPOCH,
                mtime: SystemTime::UNIX_EPOCH,
                ctime: SystemTime::UNIX_EPOCH,
                crtime: SystemTime::UNIX_EPOCH,
                kind: FileType::RegularFile,
                perm: 0o600,
                nlink: 1,
                uid: self.process_uid,
                gid: self.process_gid,
                rdev: 0,
                blksize: 512,
                flags: 0,
            };
            reply.entry(&self.entry_ttl, &attr, 0);
            return;
        }

        // For other paths, construct the full path
        let parent_path = match self.inode_to_path(parent) {
            Some(p) => p,
            None => {
                reply.error(ENOENT);
                return;
            }
        };

        let client_pid = self.get_client_pid(req);
        let parent_path_ref = self.path_from_bytes(parent_path);
        if let Err(err) =
            self.core.check_path_access(&client_pid, parent_path_ref, false, false, true)
        {
            reply.error(errno_from_fs_error(&err));
            return;
        }

        let mut full_path = parent_path.to_vec();
        if !full_path.ends_with(b"/") {
            full_path.push(b'/');
        }
        full_path.extend_from_slice(name_bytes);

        if self.is_whiteouted(&full_path) {
            reply.error(ENOENT);
            return;
        }

        let path = self.path_from_bytes(&full_path);
        match self.core.getattr(&client_pid, path) {
            Ok(attr) => match self.ensure_inode_for_path(&client_pid, &full_path, path) {
                Ok(ino) => {
                    let tombstoned = self.inode_is_tombstoned(ino);
                    if tombstoned {
                        debug!(
                            target: "agentfs::fuse",
                            event = "lookup_tombstone_hit",
                            ino,
                            parent,
                            name = %name.to_string_lossy()
                        );
                        debug_assert!(!tombstoned, "lookup resurrected tombstoned inode {}", ino);
                    }
                    if tombstoned {
                        reply.error(ENOENT);
                        return;
                    }
                    let fuse_attr = self.attr_to_fuse(&attr, ino);
                    reply.entry(&self.entry_ttl, &fuse_attr, 0);
                    self.bump_lookup(ino, 1);
                }
                Err(err) => reply.error(errno_from_fs_error(&err)),
            },
            Err(FsError::NotFound) => reply.error(ENOENT),
            Err(err) => reply.error(errno_from_fs_error(&err)),
        }
    }

    fn access(&mut self, req: &Request, ino: u64, mask: i32, reply: ReplyEmpty) {
        let path_bytes = match self.inode_to_path(ino) {
            Some(p) => p,
            None => {
                reply.error(ENOENT);
                return;
            }
        };

        if self.is_whiteouted(path_bytes) {
            reply.error(ENOENT);
            return;
        }

        let want_read = (mask & libc::R_OK) != 0;
        let want_write = (mask & libc::W_OK) != 0;
        let want_exec = (mask & libc::X_OK) != 0;

        let path = self.path_from_bytes(path_bytes);
        let client_pid = self.get_client_pid(req);
        match self.core.check_path_access(&client_pid, path, want_read, want_write, want_exec) {
            Ok(()) => reply.ok(),
            Err(FsError::NotFound) => reply.error(ENOENT),
            Err(err) => reply.error(errno_from_fs_error(&err)),
        }
    }

    fn getattr(&mut self, req: &Request, ino: u64, fh: Option<u64>, reply: ReplyAttr) {
        if let Some(path_bytes) = self.inode_to_path(ino) {
            if self.is_whiteouted(path_bytes) {
                reply.error(ENOENT);
                return;
            }
        }

        let client_pid = self.get_client_pid(req);

        let mut fh_candidate = fh.map(|fh_raw| (fh_raw, client_pid));
        if fh_candidate.is_none() {
            fh_candidate = self.handle_for_inode(ino);
        }

        if let Some((fh_raw, owner_pid)) = fh_candidate {
            debug!("getattr using fh {} for ino {}", fh_raw, ino);
            if let Err(errno) = self.wait_for_handle_writes(fh_raw) {
                reply.error(errno);
                return;
            }
            let handle_id = HandleId(fh_raw);
            match self.core.fstat(&owner_pid, handle_id) {
                Ok(stat) => {
                    let fuse_attr = self.stat_to_file_attr(&stat, ino);
                    reply.attr(&self.attr_ttl, &fuse_attr);
                }
                Err(FsError::BadFileDescriptor) => reply.error(EBADF),
                Err(FsError::NotFound) => reply.error(ENOENT),
                Err(_) => reply.error(EIO),
            }
            return;
        }

        // Handle special inodes
        if ino == FUSE_ROOT_ID {
            let attr = FileAttr {
                ino: FUSE_ROOT_ID,
                size: 0,
                blocks: 0,
                atime: SystemTime::UNIX_EPOCH,
                mtime: SystemTime::UNIX_EPOCH,
                ctime: SystemTime::UNIX_EPOCH,
                crtime: SystemTime::UNIX_EPOCH,
                kind: FileType::Directory,
                perm: 0o755,
                nlink: 2,
                uid: self.process_uid,
                gid: self.process_gid,
                rdev: 0,
                blksize: 512,
                flags: 0,
            };
            reply.attr(&self.attr_ttl, &attr);
            return;
        }

        if ino == AGENTFS_DIR_INO {
            let attr = FileAttr {
                ino: AGENTFS_DIR_INO,
                size: 0,
                blocks: 0,
                atime: SystemTime::UNIX_EPOCH,
                mtime: SystemTime::UNIX_EPOCH,
                ctime: SystemTime::UNIX_EPOCH,
                crtime: SystemTime::UNIX_EPOCH,
                kind: FileType::Directory,
                perm: 0o755,
                nlink: 2,
                uid: self.process_uid,
                gid: self.process_gid,
                rdev: 0,
                blksize: 512,
                flags: 0,
            };
            reply.attr(&self.attr_ttl, &attr);
            return;
        }

        if ino == CONTROL_FILE_INO {
            let attr = FileAttr {
                ino: CONTROL_FILE_INO,
                size: 0,
                blocks: 0,
                atime: SystemTime::UNIX_EPOCH,
                mtime: SystemTime::UNIX_EPOCH,
                ctime: SystemTime::UNIX_EPOCH,
                crtime: SystemTime::UNIX_EPOCH,
                kind: FileType::RegularFile,
                perm: 0o600,
                nlink: 1,
                uid: self.process_uid,
                gid: self.process_gid,
                rdev: 0,
                blksize: 512,
                flags: 0,
            };
            reply.attr(&self.attr_ttl, &attr);
            return;
        }

        // Regular files/directories
        let path_bytes = match self.inode_to_path(ino) {
            Some(p) => p,
            None => {
                reply.error(ENOENT);
                return;
            }
        };

        let path = self.path_from_bytes(path_bytes);
        match self.core.getattr(&client_pid, path) {
            Ok(attr) => {
                let fuse_attr = self.attr_to_fuse(&attr, ino);
                reply.attr(&self.attr_ttl, &fuse_attr);
            }
            Err(FsError::NotFound) => reply.error(ENOENT),
            Err(_) => reply.error(EIO),
        }
    }

    fn statfs(&mut self, req: &Request, ino: u64, reply: ReplyStatfs) {
        let path = if ino == FUSE_ROOT_ID {
            self.path_from_bytes(b"/")
        } else {
            match self.inode_to_path(ino) {
                Some(bytes) => self.path_from_bytes(bytes),
                None => {
                    reply.error(ENOENT);
                    return;
                }
            }
        };

        let client_pid = self.get_client_pid(req);
        match self.core.statfs(&client_pid, path) {
            Ok(stats) => {
                reply.statfs(
                    stats.f_blocks,
                    stats.f_bfree,
                    stats.f_bavail,
                    stats.f_files,
                    stats.f_ffree,
                    stats.f_bsize,
                    stats.f_namemax,
                    stats.f_frsize,
                );
            }
            Err(FsError::NotFound) => reply.error(ENOENT),
            Err(_) => reply.error(EIO),
        }
    }

    fn open(&mut self, req: &Request, ino: u64, flags: i32, reply: ReplyOpen) {
        // Special handling for control file
        if ino == CONTROL_FILE_INO {
            reply.opened(0, self.open_flags()); // fh=0 for control file
            return;
        }

        if self.inode_is_tombstoned(ino) {
            reply.error(ESTALE);
            return;
        }

        let path_bytes = match self.inode_to_path(ino) {
            Some(p) => p,
            None => {
                reply.error(ESTALE);
                return;
            }
        };

        let canonical_path = path_bytes.to_vec();

        if self.is_whiteouted(&canonical_path) {
            reply.error(ENOENT);
            return;
        }

        let path = self.path_from_bytes(&canonical_path);
        let user_path = path;
        let options = self.fuse_flags_to_options(flags);
        let access_mode = flags & O_ACCMODE;
        let requested_read = access_mode == libc::O_RDONLY || access_mode == libc::O_RDWR;
        let requested_write = access_mode == libc::O_WRONLY || access_mode == libc::O_RDWR;
        let client_pid = self.get_client_pid(req);
        let req_uid = req.uid();
        let req_gid = req.gid();

        let cached_special = self.path_special_kinds.get(&canonical_path).cloned();
        let mut is_fifo = matches!(cached_special, Some(SpecialNodeKind::Fifo));
        if !is_fifo {
            if let Some(node_id_u64) = self.node_id_from_inode(ino) {
                if matches!(
                    self.core.node_special_kind(node_id_u64),
                    Some(SpecialNodeKind::Fifo)
                ) {
                    is_fifo = true;
                }
            }
        }
        match self.core.getattr(&client_pid, path) {
            Ok(attrs) => {
                if let Some(kind) = attrs.special_kind.as_ref() {
                    self.cache_special_kind_for_inode(ino, Some(kind));
                    is_fifo = matches!(kind, SpecialNodeKind::Fifo);
                }
                debug!(
                    target: "agentfs::fuse",
                    event = "open_metadata",
                    ino,
                    path = %user_path.display(),
                    uid = req_uid,
                    gid = req_gid,
                    special = ?attrs.special_kind,
                    cached_special = ?cached_special,
                    fifo = is_fifo
                );
            }
            Err(err) => {
                debug!(
                    target: "agentfs::fuse",
                    event = "open_meta_failed",
                    ino,
                    path = %user_path.display(),
                    uid = req_uid,
                    gid = req_gid,
                    cached_special = ?cached_special,
                    fifo = is_fifo,
                    ?err
                );
            }
        }

        debug!(
            target: "agentfs::fuse",
            event = "open_request",
            ino,
            path = %user_path.display(),
            raw_flags = format_args!("{:#x}", flags),
            requested_read,
            requested_write,
            options_read = options.read,
            options_write = options.write,
            fifo = is_fifo
        );

        if is_fifo {
            if let Err(err) = self.core.check_path_access(
                &client_pid,
                path,
                requested_read,
                requested_write,
                false,
            ) {
                reply.error(errno_from_fs_error(&err));
                return;
            }
        }

        if let Some(parent_path) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            if let Err(err) =
                self.core.check_path_access(&client_pid, parent_path, false, false, true)
            {
                reply.error(errno_from_fs_error(&err));
                return;
            }
        }

        // Enforce O_NOFOLLOW semantics: fail if target is a symlink
        #[allow(non_upper_case_globals)]
        {
            use libc::O_NOFOLLOW;
            if flags & O_NOFOLLOW != 0 {
                if let Ok(stat) = self.core.lstat(&client_pid, path) {
                    let mode = stat.st_mode;
                    if (mode & libc::S_IFMT) == libc::S_IFLNK {
                        reply.error(ELOOP);
                        return;
                    }
                }
            }
        }

        match self.core.open(&client_pid, path, &options) {
            Ok(handle_id) => {
                debug!(
                    target: "agentfs::fuse",
                    event = "open_allow",
                    ino,
                    path = %user_path.display(),
                    uid = req_uid,
                    gid = req_gid,
                    read = options.read,
                    write = options.write,
                    fifo = is_fifo
                );
                self.track_handle(ino, handle_id.0, &client_pid);
                let reply = if self.passthrough_active() {
                    match self.try_passthrough_open(
                        ino,
                        self.path_from_bytes(&canonical_path),
                        handle_id,
                        &options,
                        reply,
                    ) {
                        Ok(()) => return,
                        Err(reply) => reply,
                    }
                } else {
                    reply
                };
                reply.opened(handle_id.0, self.open_flags());
            }
            Err(FsError::AccessDenied) => {
                debug!(
                    target: "agentfs::fuse",
                    event = "open_deny",
                    ino,
                    path = %user_path.display(),
                    uid = req_uid,
                    gid = req_gid,
                    read = options.read,
                    write = options.write,
                    fifo = is_fifo
                );
                reply.error(EACCES);
            }
            Err(FsError::NotFound) | Err(FsError::Unsupported) => {
                if !self.overlay_enabled() || !self.lower_entry_exists(path) {
                    reply.error(ENOENT);
                    return;
                }

                if options.write || options.create || options.truncate || options.append {
                    let internal_pid = self.internal_pid;
                    if let Err(err) = self.copy_lower_to_upper(&internal_pid, path, &canonical_path)
                    {
                        error!("failed to materialize lower entry: {:?}", err);
                        reply.error(EIO);
                        return;
                    }
                    match self.core.open(&client_pid, path, &options) {
                        Ok(handle_id) => {
                            self.track_handle(ino, handle_id.0, &client_pid);
                            reply.opened(handle_id.0, self.open_flags());
                        }
                        Err(FsError::AccessDenied) => {
                            debug!(
                                target: "agentfs::fuse",
                                event = "open_deny",
                                ino,
                                path = %user_path.display(),
                                uid = req_uid,
                                gid = req_gid,
                                read = options.read,
                                write = options.write,
                                fifo = is_fifo
                            );
                            reply.error(EACCES)
                        }
                        Err(FsError::NotFound) => reply.error(ENOENT),
                        Err(_) => reply.error(EIO),
                    }
                } else if options.read {
                    if let Some(lower_path) = self.lower_full_path(path) {
                        match File::open(&lower_path) {
                            Ok(file) => {
                                let fh = self.alloc_lower_handle(file);
                                reply.opened(fh, 0);
                            }
                            Err(err) => {
                                error!("failed to open lower file {:?}: {:?}", lower_path, err);
                                reply.error(EIO);
                            }
                        }
                    } else {
                        reply.error(ENOENT);
                    }
                } else {
                    reply.error(EACCES);
                }
            }
            Err(err) => {
                error!(
                    target: "agentfs::fuse",
                    path = ?path,
                    ?err,
                    "open failed"
                );
                reply.error(EIO);
            }
        }
    }

    fn opendir(&mut self, req: &Request, ino: u64, _flags: i32, reply: ReplyOpen) {
        if ino == AGENTFS_DIR_INO {
            reply.opened(0, 0);
            return;
        }

        let path_bytes = match self.inode_to_path(ino) {
            Some(p) => p,
            None => {
                reply.error(ENOENT);
                return;
            }
        };

        if self.is_whiteouted(path_bytes) {
            reply.error(ENOENT);
            return;
        }

        let path = self.path_from_bytes(path_bytes);
        let client_pid = self.get_client_pid(req);
        if let Err(err) = self.core.check_path_access(&client_pid, path, true, false, false) {
            reply.error(errno_from_fs_error(&err));
            return;
        }

        reply.opened(0, 0);
    }

    fn readlink(&mut self, req: &Request, ino: u64, reply: ReplyData) {
        // Only valid for symlinks
        let path_bytes = match self.inode_to_path(ino) {
            Some(p) => p,
            None => {
                reply.error(ENOENT);
                return;
            }
        };

        let path = self.path_from_bytes(path_bytes);
        let client_pid = self.get_client_pid(req);
        match self.core.readlink(&client_pid, path) {
            Ok(target) => reply.data(target.as_bytes()),
            Err(FsError::NotFound) => reply.error(ENOENT),
            Err(FsError::InvalidArgument) => reply.error(EINVAL),
            Err(_) => reply.error(EIO),
        }
    }

    fn mknod(
        &mut self,
        req: &Request,
        parent: u64,
        name: &OsStr,
        mode: u32,
        umask: u32,
        rdev: u32,
        reply: ReplyEntry,
    ) {
        if name.as_bytes().len() > NAME_MAX {
            reply.error(ENAMETOOLONG);
            return;
        }

        let parent_path = match self.inode_to_path(parent) {
            Some(p) => p,
            None => {
                reply.error(ENOENT);
                return;
            }
        };

        let mut full_path = parent_path.to_vec();
        if !full_path.ends_with(b"/") {
            full_path.push(b'/');
        }
        full_path.extend_from_slice(name.as_bytes());

        let path = self.path_from_bytes(&full_path);
        let perm_bits = mode & 0o7777;
        let masked_perm = perm_bits & (!umask & 0o7777);
        let file_type = mode & libc::S_IFMT;
        let final_mode = file_type | masked_perm;
        debug!(
            target: "agentfs::fuse",
            event = "mknod_request",
            path = %path.display(),
            mode = format_args!("{:#o}", mode),
            file_type = format_args!("{:#o}", file_type),
            rdev
        );

        let client_pid = self.get_client_pid(req);
        match self.core.mknod(&client_pid, path, final_mode, rdev as u64) {
            Ok(()) => match self.core.getattr(&client_pid, path) {
                Ok(attr) => match self.ensure_inode_for_path(&client_pid, &full_path, path) {
                    Ok(ino) => {
                        let fuse_attr = self.attr_to_fuse(&attr, ino);
                        reply.entry(&self.entry_ttl, &fuse_attr, 0);
                        debug!(
                            target: "agentfs::fuse",
                            event = "mknod_success",
                            ino,
                            path = %self.path_from_bytes(&full_path).display(),
                            special = ?attr.special_kind
                        );
                        self.bump_lookup(ino, 1);
                    }
                    Err(err) => reply.error(errno_from_fs_error(&err)),
                },
                Err(_) => reply.error(EIO),
            },
            Err(FsError::AlreadyExists) => reply.error(EEXIST),
            Err(FsError::NotADirectory) => reply.error(ENOTDIR),
            Err(FsError::AccessDenied) => reply.error(EACCES),
            Err(FsError::InvalidArgument) => reply.error(EINVAL),
            Err(FsError::Unsupported) => reply.error(ENOSYS),
            Err(FsError::NotFound) => reply.error(ENOENT),
            Err(_) => reply.error(EIO),
        }
    }

    fn read(
        &mut self,
        req: &Request,
        ino: u64,
        fh: u64,
        offset: i64,
        size: u32,
        _flags: i32,
        _lock_owner: Option<u64>,
        reply: ReplyData,
    ) {
        // Special handling for control file - no data to read
        if ino == CONTROL_FILE_INO {
            reply.data(&[]);
            return;
        }

        if let Some(lower) = self.lower_handles.get(&fh) {
            let mut buf = vec![0u8; size as usize];
            match lower.file.read_at(&mut buf, offset as u64) {
                Ok(bytes_read) => {
                    buf.truncate(bytes_read);
                    reply.data(&buf);
                }
                Err(_) => reply.error(EIO),
            }
            return;
        }

        let handle_id = HandleId(fh);
        let mut buf = vec![0u8; size as usize];
        let client_pid = self.get_client_pid(req);

        match self.core.read(&client_pid, handle_id, offset as u64, &mut buf) {
            Ok(bytes_read) => {
                buf.truncate(bytes_read);
                reply.data(&buf);
            }
            Err(FsError::NotFound) => reply.error(ENOENT),
            Err(_) => reply.error(EIO),
        }
    }

    fn write(
        &mut self,
        req: &Request,
        ino: u64,
        fh: u64,
        offset: i64,
        data: &[u8],
        _write_flags: u32,
        _flags: i32,
        _lock_owner: Option<u64>,
        reply: ReplyWrite,
    ) {
        // Control file is not writable
        if ino == CONTROL_FILE_INO {
            reply.error(EACCES);
            return;
        }

        if self.lower_handles.contains_key(&fh) {
            reply.error(EACCES);
            return;
        }

        if offset < 0 {
            reply.error(EINVAL);
            return;
        }

        let handle_id = HandleId(fh);
        let client_pid = self.get_client_pid(req);
        let mut buffer = Vec::with_capacity(data.len());
        buffer.extend_from_slice(data);
        let trace = self.start_write_trace(req, ino, fh, offset, buffer.len());

        let state = match self.handle_write_state(fh) {
            Some(state) => state,
            None => {
                if let Some(trace) = trace {
                    trace.finish();
                }
                reply.error(EIO);
                return;
            }
        };

        if let Some(errno) = state.current_error() {
            if let Some(trace) = trace {
                trace.finish();
            }
            reply.error(errno);
            return;
        }

        state.begin_write();

        let job = WriteJob {
            pid: client_pid,
            handle_id,
            offset: offset as u64,
            data: buffer,
            completion: state.clone(),
            trace,
        };

        self.write_dispatcher.submit(job);
        reply.written(data.len() as u32);
    }

    fn release(
        &mut self,
        req: &Request,
        ino: u64,
        fh: u64,
        _flags: i32,
        _lock_owner: Option<u64>,
        _flush: bool,
        reply: ReplyEmpty,
    ) {
        // Control file has no handle to release
        if ino == CONTROL_FILE_INO {
            reply.ok();
            return;
        }

        if let Err(errno) = self.wait_for_handle_writes(fh) {
            reply.error(errno);
            return;
        }

        if self.passthrough_handles.contains_key(&fh) {
            self.maybe_refresh_passthrough_metadata(fh);
        }

        if self.lower_handles.remove(&fh).is_some() {
            reply.ok();
            return;
        }

        let handle_id = HandleId(fh);
        let client_pid = self.get_client_pid(req);

        match self.core.close(&client_pid, handle_id) {
            Ok(()) => reply.ok(),
            Err(_) => reply.error(EIO),
        }
        self.passthrough_handles.remove(&fh);
        self.untrack_handle(ino, fh);
    }

    fn readdir(
        &mut self,
        req: &Request,
        ino: u64,
        _fh: u64,
        offset: i64,
        mut reply: ReplyDirectory,
    ) {
        if ino == AGENTFS_DIR_INO {
            // List the .agentfs directory contents
            if offset == 0 && reply.add(CONTROL_FILE_INO, 1, FileType::RegularFile, "control") {
                return;
            }
            reply.ok();
            return;
        }

        let path_bytes = match self.inode_to_path(ino) {
            Some(p) => p,
            None => {
                reply.error(ENOENT);
                return;
            }
        };

        let path = self.path_from_bytes(path_bytes);
        let path_bytes_vec = path_bytes.to_vec(); // Clone to avoid borrowing issues

        let client_pid = self.get_client_pid(req);
        match self.core.readdir_plus(&client_pid, path) {
            Ok(entries) => {
                for (i, (entry, _attr)) in entries.iter().enumerate().skip(offset as usize) {
                    let mut entry_path = path_bytes_vec.clone();
                    if !entry_path.ends_with(b"/") {
                        entry_path.push(b'/');
                    }
                    entry_path.extend_from_slice(entry.name.as_bytes());

                    if self.is_whiteouted(&entry_path) {
                        continue;
                    }

                    let entry_path_buf = self.path_from_bytes(&entry_path);
                    let entry_ino = match self.ensure_inode_for_path(
                        &client_pid,
                        &entry_path,
                        entry_path_buf,
                    ) {
                        Ok(ino) => ino,
                        Err(err) => {
                            debug!(
                                target: "agentfs::fuse",
                                event = "readdir_lookup_failed",
                                path = %entry_path_buf.display(),
                                errno = errno_from_fs_error(&err),
                                ?err
                            );
                            continue;
                        }
                    };

                    let file_type = if entry.is_dir {
                        FileType::Directory
                    } else if entry.is_symlink {
                        FileType::Symlink
                    } else {
                        FileType::RegularFile
                    };

                    if !reply.add(entry_ino, (i + 1) as i64, file_type, &entry.name) {
                        break;
                    }
                }
                reply.ok();
            }
            Err(FsError::NotFound) => reply.error(ENOENT),
            Err(FsError::NotADirectory) => reply.error(ENOTDIR),
            Err(_) => reply.error(EIO),
        }
    }

    fn create(
        &mut self,
        req: &Request,
        parent: u64,
        name: &OsStr,
        mode: u32,
        _umask: u32,
        flags: i32,
        reply: ReplyCreate,
    ) {
        // Guard component length
        if name.as_bytes().len() > NAME_MAX {
            reply.error(ENAMETOOLONG);
            return;
        }
        let parent_path = match self.inode_to_path(parent) {
            Some(p) => p,
            None => {
                reply.error(ENOENT);
                return;
            }
        };

        let mut full_path = parent_path.to_vec();
        if !full_path.ends_with(b"/") {
            full_path.push(b'/');
        }
        full_path.extend_from_slice(name.as_bytes());

        let path = self.path_from_bytes(&full_path);
        let options = self.fuse_flags_to_options(flags);
        let client_pid = self.get_client_pid(req);

        if let Err(err) = self.ensure_upper_parent_dirs(&client_pid, path) {
            reply.error(match err {
                FsError::AccessDenied => EACCES,
                FsError::OperationNotPermitted => EPERM,
                FsError::NotFound => ENOENT,
                FsError::InvalidArgument => EINVAL,
                _ => EIO,
            });
            return;
        }

        let final_mode = mode & 0o7777;

        match self.core.create(&client_pid, path, &options) {
            Ok(handle_id) => {
                if let Err(e) = self.core.set_mode(&client_pid, path, final_mode) {
                    reply.error(match e {
                        FsError::AccessDenied => EACCES,
                        FsError::OperationNotPermitted => EPERM,
                        FsError::InvalidArgument => EINVAL,
                        _ => EIO,
                    });
                    let _ = self.core.close(&client_pid, handle_id);
                    return;
                }

                match self.core.getattr(&client_pid, path) {
                    Ok(attr) => match self.ensure_inode_for_path(&client_pid, &full_path, path) {
                        Ok(ino) => {
                            let fuse_attr = self.attr_to_fuse(&attr, ino);
                            self.track_handle(ino, handle_id.0, &client_pid);
                            self.bump_lookup(ino, 1);
                            reply.created(
                                &self.entry_ttl,
                                &fuse_attr,
                                0,
                                handle_id.0,
                                self.open_flags(),
                            );
                        }
                        Err(err) => reply.error(errno_from_fs_error(&err)),
                    },
                    Err(_) => reply.error(EIO),
                }
            }
            Err(FsError::NotFound) => reply.error(ENOENT),
            Err(FsError::AlreadyExists) => reply.error(EEXIST),
            Err(FsError::AccessDenied) => reply.error(EACCES),
            Err(_) => reply.error(EIO),
        }
    }

    fn mkdir(
        &mut self,
        req: &Request,
        parent: u64,
        name: &OsStr,
        mode: u32,
        _umask: u32,
        reply: ReplyEntry,
    ) {
        // Guard component length
        if name.as_bytes().len() > NAME_MAX {
            reply.error(ENAMETOOLONG);
            return;
        }
        let parent_path = match self.inode_to_path(parent) {
            Some(p) => p,
            None => {
                reply.error(ENOENT);
                return;
            }
        };

        let mut full_path = parent_path.to_vec();
        if !full_path.ends_with(b"/") {
            full_path.push(b'/');
        }
        full_path.extend_from_slice(name.as_bytes());

        let path = self.path_from_bytes(&full_path);
        let client_pid = self.get_client_pid(req);

        if let Err(err) = self.ensure_upper_parent_dirs(&client_pid, path) {
            reply.error(match err {
                FsError::AccessDenied => EACCES,

                FsError::OperationNotPermitted => EPERM,
                FsError::NotFound => ENOENT,
                _ => EIO,
            });
            return;
        }

        match self.core.mkdir(&client_pid, path, mode) {
            Ok(()) => match self.core.getattr(&client_pid, path) {
                Ok(attr) => match self.ensure_inode_for_path(&client_pid, &full_path, path) {
                    Ok(ino) => {
                        let fuse_attr = self.attr_to_fuse(&attr, ino);
                        reply.entry(&self.entry_ttl, &fuse_attr, 0);
                        self.bump_lookup(ino, 1);
                    }
                    Err(err) => reply.error(errno_from_fs_error(&err)),
                },
                Err(_) => reply.error(EIO),
            },
            Err(FsError::NotFound) => reply.error(ENOENT),
            Err(FsError::AlreadyExists) => reply.error(EEXIST),
            Err(FsError::AccessDenied) => reply.error(EACCES),
            Err(_) => reply.error(EIO),
        }
    }

    fn unlink(&mut self, req: &Request, parent: u64, name: &OsStr, reply: ReplyEmpty) {
        // Guard component length
        if name.as_bytes().len() > NAME_MAX {
            reply.error(ENAMETOOLONG);
            return;
        }
        let parent_path = match self.inode_to_path(parent) {
            Some(p) => p,
            None => {
                reply.error(ENOENT);
                return;
            }
        };

        let mut full_path = parent_path.to_vec();
        if !full_path.ends_with(b"/") {
            full_path.push(b'/');
        }
        full_path.extend_from_slice(name.as_bytes());

        let path = self.path_from_bytes(&full_path);
        let client_pid = self.get_client_pid(req);

        match self.core.unlink(&client_pid, path) {
            Ok(()) => {
                self.purge_by_canonical_path(&full_path, Some(parent), Some(name.as_bytes()));
                reply.ok();
            }
            Err(FsError::NotFound) => {
                if self.overlay_enabled() && self.lower_entry_exists(path) {
                    self.record_whiteout(full_path.clone());
                    self.purge_by_canonical_path(&full_path, Some(parent), Some(name.as_bytes()));
                    reply.ok();
                } else {
                    reply.error(ENOENT);
                }
            }
            Err(FsError::AccessDenied) => reply.error(EACCES),
            Err(FsError::IsADirectory) => reply.error(EISDIR),
            Err(_) => reply.error(EIO),
        }
    }

    fn rmdir(&mut self, req: &Request, parent: u64, name: &OsStr, reply: ReplyEmpty) {
        // Guard component length
        if name.as_bytes().len() > NAME_MAX {
            reply.error(ENAMETOOLONG);
            return;
        }
        let parent_path = match self.inode_to_path(parent) {
            Some(p) => p,
            None => {
                reply.error(ENOENT);
                return;
            }
        };

        let mut full_path = parent_path.to_vec();
        if !full_path.ends_with(b"/") {
            full_path.push(b'/');
        }
        full_path.extend_from_slice(name.as_bytes());

        let path = self.path_from_bytes(&full_path);
        let client_pid = self.get_client_pid(req);

        match self.core.rmdir(&client_pid, path) {
            Ok(()) => {
                self.purge_by_canonical_path(&full_path, Some(parent), Some(name.as_bytes()));
                reply.ok();
            }
            Err(FsError::NotFound) => reply.error(ENOENT),
            Err(FsError::Busy) => reply.error(ENOTEMPTY),
            Err(FsError::AccessDenied) => reply.error(EACCES),
            Err(FsError::NotADirectory) => reply.error(ENOTDIR),
            Err(FsError::InvalidArgument) => reply.error(EINVAL),
            Err(_) => reply.error(EIO),
        }
    }

    fn rename(
        &mut self,
        req: &Request,
        parent: u64,
        name: &OsStr,
        newparent: u64,
        newname: &OsStr,
        _flags: u32,
        reply: ReplyEmpty,
    ) {
        // Guard component length
        if name.as_bytes().len() > NAME_MAX || newname.as_bytes().len() > NAME_MAX {
            reply.error(ENAMETOOLONG);
            return;
        }
        let parent_path = match self.inode_to_path(parent) {
            Some(p) => p,
            None => {
                reply.error(ENOENT);
                return;
            }
        };

        let newparent_path = match self.inode_to_path(newparent) {
            Some(p) => p,
            None => {
                reply.error(ENOENT);
                return;
            }
        };

        let mut old_path = parent_path.to_vec();
        if !old_path.ends_with(b"/") {
            old_path.push(b'/');
        }
        old_path.extend_from_slice(name.as_bytes());

        let mut new_path = newparent_path.to_vec();
        if !new_path.ends_with(b"/") {
            new_path.push(b'/');
        }
        new_path.extend_from_slice(newname.as_bytes());

        if old_path == new_path {
            reply.ok();
            return;
        }

        let old_path_obj = self.path_from_bytes(&old_path);
        let new_path_obj = self.path_from_bytes(&new_path);
        let client_pid = self.get_client_pid(req);

        let old_name_bytes = name.as_bytes();
        let new_name_bytes = newname.as_bytes();

        match self.core.rename(&client_pid, old_path_obj, new_path_obj) {
            Ok(()) => {
                self.purge_by_canonical_path(&new_path, Some(newparent), Some(new_name_bytes));

                let moved_special = self.path_special_kinds.remove(&old_path);
                if let Some(removal) = self.remove_path_mapping(&old_path) {
                    let _inode = self.record_path_for_node(new_path.clone(), removal.node_id);
                    if let Some(kind) = moved_special {
                        self.path_special_kinds.insert(new_path.clone(), kind);
                    }
                } else if let Some(kind) = moved_special {
                    // Restore metadata if the mapping removal failed.
                    self.path_special_kinds.insert(old_path.clone(), kind);
                }

                self.invalidate_entry(parent, old_name_bytes);
                self.invalidate_entry(newparent, new_name_bytes);
                reply.ok();
            }
            Err(FsError::NotFound) => reply.error(ENOENT),
            Err(FsError::AlreadyExists) => reply.error(EEXIST),
            Err(FsError::AccessDenied) => reply.error(EACCES),
            Err(FsError::NotADirectory) => reply.error(ENOTDIR),
            Err(FsError::IsADirectory) => reply.error(EISDIR),
            Err(FsError::Busy) => reply.error(ENOTEMPTY),
            Err(FsError::InvalidArgument) => reply.error(EINVAL),
            Err(_) => reply.error(EIO),
        }
    }

    fn symlink(
        &mut self,
        req: &Request,
        parent: u64,
        name: &OsStr,
        link: &std::path::Path,
        reply: ReplyEntry,
    ) {
        // Guard component length
        if name.as_bytes().len() > NAME_MAX {
            reply.error(ENAMETOOLONG);
            return;
        }

        let parent_path = match self.inode_to_path(parent) {
            Some(p) => p,
            None => {
                reply.error(ENOENT);
                return;
            }
        };

        // Construct link path (where the symlink will live)
        let mut linkpath = parent_path.to_vec();
        if !linkpath.ends_with(b"/") {
            linkpath.push(b'/');
        }
        linkpath.extend_from_slice(name.as_bytes());
        let linkpath_obj = self.path_from_bytes(&linkpath);

        let client_pid = self.get_client_pid(req);
        match self.core.symlink(&client_pid, &link.to_string_lossy(), linkpath_obj) {
            Ok(()) => match self.core.getattr(&client_pid, linkpath_obj) {
                Ok(attr) => {
                    match self.ensure_inode_for_path(&client_pid, &linkpath, linkpath_obj) {
                        Ok(ino) => {
                            let fuse_attr = self.attr_to_fuse(&attr, ino);
                            reply.entry(&self.entry_ttl, &fuse_attr, 0);
                            self.bump_lookup(ino, 1);
                        }
                        Err(err) => reply.error(errno_from_fs_error(&err)),
                    }
                }
                Err(_) => reply.error(EIO),
            },
            Err(FsError::AlreadyExists) => reply.error(EEXIST),
            Err(FsError::NotFound) => reply.error(ENOENT),
            Err(FsError::NotADirectory) => reply.error(ENOTDIR),
            Err(FsError::AccessDenied) => reply.error(EACCES),
            Err(FsError::InvalidArgument) => reply.error(EINVAL),
            Err(_) => reply.error(EIO),
        }
    }

    fn link(
        &mut self,
        req: &Request,
        ino: u64,
        newparent: u64,
        newname: &OsStr,
        reply: ReplyEntry,
    ) {
        // Guard component length
        if newname.as_bytes().len() > NAME_MAX {
            reply.error(ENAMETOOLONG);
            return;
        }

        let old_path_bytes = match self.inode_to_path(ino) {
            Some(p) => p,
            None => {
                reply.error(ENOENT);
                return;
            }
        };
        let newparent_path = match self.inode_to_path(newparent) {
            Some(p) => p,
            None => {
                reply.error(ENOENT);
                return;
            }
        };

        let mut new_path = newparent_path.to_vec();
        if !new_path.ends_with(b"/") {
            new_path.push(b'/');
        }
        new_path.extend_from_slice(newname.as_bytes());

        let old_path = self.path_from_bytes(old_path_bytes);
        let new_path_obj = self.path_from_bytes(&new_path);
        let client_pid = self.get_client_pid(req);

        match self.core.link(&client_pid, old_path, new_path_obj) {
            Ok(()) => match self.core.getattr(&client_pid, new_path_obj) {
                Ok(attr) => {
                    if let Some(node_id) = self.node_id_from_inode(ino) {
                        self.record_path_for_node(new_path.clone(), node_id);
                        let fuse_attr = self.attr_to_fuse(&attr, ino);
                        reply.entry(&self.entry_ttl, &fuse_attr, 0);
                        self.bump_lookup(ino, 1);
                    } else {
                        reply.error(EIO);
                    }
                }
                Err(_) => reply.error(EIO),
            },
            Err(FsError::AlreadyExists) => reply.error(EEXIST),
            Err(FsError::NotFound) => reply.error(ENOENT),
            Err(FsError::IsADirectory) => reply.error(EISDIR),
            Err(FsError::AccessDenied) => reply.error(EACCES),
            Err(_) => reply.error(EIO),
        }
    }

    fn ioctl(
        &mut self,
        _req: &Request,
        ino: u64,
        _fh: u64,
        _flags: u32,
        cmd: u32,
        data: &[u8],
        out_size: u32,
        reply: fuser::ReplyIoctl,
    ) {
        // Only handle ioctl on the control file
        if ino != CONTROL_FILE_INO {
            reply.error(libc::ENOTTY);
            return;
        }

        if cmd != AGENTFS_IOCTL_CMD {
            reply.error(libc::ENOTTY);
            return;
        }

        match self.handle_control_ioctl(data) {
            Ok(response_data) => {
                if response_data.len() > out_size as usize {
                    reply.error(libc::EINVAL);
                } else {
                    reply.ioctl(0, &response_data);
                }
            }
            Err(errno) => reply.error(errno),
        }
    }

    fn fsync(&mut self, req: &Request, ino: u64, fh: u64, datasync: bool, reply: ReplyEmpty) {
        if ino == CONTROL_FILE_INO {
            reply.ok(); // No-op for control file
            return;
        }

        if let Some(lower) = self.lower_handles.get(&fh) {
            let result = if datasync {
                lower.file.sync_data()
            } else {
                lower.file.sync_all()
            };

            match result {
                Ok(()) => reply.ok(),
                Err(err) => {
                    if let Some(errno) = err.raw_os_error() {
                        reply.error(errno);
                    } else {
                        reply.error(EIO);
                    }
                }
            }
            return;
        }

        let handle_id = HandleId(fh);
        let client_pid = self.get_client_pid(req);

        if let Err(errno) = self.wait_for_handle_writes(fh) {
            reply.error(errno);
            return;
        }

        if self.passthrough_handles.contains_key(&fh) {
            self.maybe_refresh_passthrough_metadata(fh);
        }

        match self.core.fsync(&client_pid, handle_id, datasync) {
            Ok(()) => reply.ok(),
            Err(FsError::AccessDenied) => reply.error(EACCES),
            Err(FsError::InvalidArgument) => reply.error(EINVAL),
            Err(FsError::NotFound) => reply.error(ENOENT),
            Err(_) => reply.error(EIO),
        }
    }

    fn flush(&mut self, _req: &Request, ino: u64, fh: u64, _lock_owner: u64, reply: ReplyEmpty) {
        if ino == CONTROL_FILE_INO {
            reply.ok(); // No-op for control file
            return;
        }

        if self.passthrough_handles.contains_key(&fh) {
            self.maybe_refresh_passthrough_metadata(fh);
        }

        if let Err(errno) = self.wait_for_handle_writes(fh) {
            reply.error(errno);
        } else {
            reply.ok();
        }
    }

    fn getxattr(&mut self, req: &Request, ino: u64, name: &OsStr, size: u32, reply: ReplyXattr) {
        if ino == CONTROL_FILE_INO {
            reply.error(libc::ENODATA);
            return;
        }

        let path_bytes = match self.inode_to_path(ino) {
            Some(p) => p,
            None => {
                reply.error(ENOENT);
                return;
            }
        };

        let path = self.path_from_bytes(path_bytes);
        let name_str = name.to_str().unwrap_or("");
        let client_pid = self.get_client_pid(req);

        match self.core.xattr_get(&client_pid, path, name_str) {
            Ok(value) => {
                if size == 0 {
                    reply.size(value.len() as u32);
                } else if value.len() <= size as usize {
                    reply.data(&value);
                } else {
                    reply.error(libc::ERANGE);
                }
            }
            Err(FsError::NotFound) => reply.error(libc::ENODATA),
            Err(_) => reply.error(EIO),
        }
    }

    fn setxattr(
        &mut self,
        req: &Request,
        ino: u64,
        name: &OsStr,
        value: &[u8],
        flags: i32,
        _position: u32,
        reply: ReplyEmpty,
    ) {
        if ino == CONTROL_FILE_INO {
            reply.error(libc::EPERM);
            return;
        }

        let path_bytes = match self.inode_to_path(ino) {
            Some(p) => p,
            None => {
                reply.error(ENOENT);
                return;
            }
        };

        let path = self.path_from_bytes(path_bytes);
        let name_str = name.to_str().unwrap_or("");
        let client_pid = self.get_client_pid(req);

        // Handle flags (XATTR_CREATE, XATTR_REPLACE) with proper check-then-act logic
        let create = flags == libc::XATTR_CREATE;
        let replace = flags == libc::XATTR_REPLACE;

        // Check-then-act logic
        if create || replace {
            let exists = self.core.xattr_get(&client_pid, path, name_str).is_ok();

            if create && exists {
                reply.error(libc::EEXIST); // Fail: XATTR_CREATE and it already exists
                return;
            }
            if replace && !exists {
                reply.error(libc::ENODATA); // Fail: XATTR_REPLACE and it doesn't exist
                return;
            }
        }

        // Proceed with the set operation
        match self.core.xattr_set(&client_pid, path, name_str, value) {
            Ok(()) => reply.ok(),
            Err(FsError::NotFound) => reply.error(ENOENT), // File not found
            Err(_) => reply.error(EIO),
        }
    }

    fn listxattr(&mut self, req: &Request, ino: u64, size: u32, reply: ReplyXattr) {
        if ino == CONTROL_FILE_INO {
            reply.size(0);
            return;
        }

        let path_bytes = match self.inode_to_path(ino) {
            Some(p) => p,
            None => {
                reply.error(ENOENT);
                return;
            }
        };

        let path = self.path_from_bytes(path_bytes);
        let client_pid = self.get_client_pid(req);

        match self.core.xattr_list(&client_pid, path) {
            Ok(names) => {
                let mut buffer = Vec::new();
                for name in &names {
                    buffer.extend_from_slice(name.as_bytes());
                    buffer.push(0); // NUL terminator
                }

                if size == 0 {
                    reply.size(buffer.len() as u32);
                } else if buffer.len() <= size as usize {
                    reply.data(&buffer);
                } else {
                    reply.error(libc::ERANGE);
                }
            }
            Err(FsError::NotFound) => reply.error(ENOENT),
            Err(_) => reply.error(EIO),
        }
    }

    fn removexattr(&mut self, req: &Request, ino: u64, name: &OsStr, reply: ReplyEmpty) {
        if ino == CONTROL_FILE_INO {
            reply.error(libc::EPERM);
            return;
        }

        let path_bytes = match self.inode_to_path(ino) {
            Some(p) => p,
            None => {
                reply.error(ENOENT);
                return;
            }
        };

        let path = self.path_from_bytes(path_bytes);
        let name_str = name.to_str().unwrap_or("");
        let client_pid = self.get_client_pid(req);

        match self.core.xattr_remove(&client_pid, path, name_str) {
            Ok(()) => reply.ok(),
            Err(FsError::NotFound) => reply.error(libc::ENODATA),
            Err(_) => reply.error(EIO),
        }
    }

    fn fallocate(
        &mut self,
        req: &Request,
        ino: u64,
        fh: u64,
        offset: i64,
        length: i64,
        mode: i32,
        reply: ReplyEmpty,
    ) {
        if ino == CONTROL_FILE_INO {
            reply.error(libc::EPERM);
            return;
        }
        if offset < 0 || length < 0 {
            reply.error(libc::EINVAL);
            return;
        }
        let punch = mode & libc::FALLOC_FL_PUNCH_HOLE;
        let zero_range = mode & libc::FALLOC_FL_ZERO_RANGE;
        let unsupported = mode
            & !(libc::FALLOC_FL_PUNCH_HOLE
                | libc::FALLOC_FL_KEEP_SIZE
                | libc::FALLOC_FL_ZERO_RANGE);
        if unsupported != 0 {
            reply.error(libc::EOPNOTSUPP);
            return;
        }
        let mut fallocate_mode = FallocateMode::Allocate;
        if punch != 0 {
            if (mode & libc::FALLOC_FL_KEEP_SIZE) == 0 {
                reply.error(libc::EOPNOTSUPP);
                return;
            }
            fallocate_mode = FallocateMode::PunchHole;
        } else if zero_range != 0 {
            fallocate_mode = FallocateMode::Allocate;
        }

        let client_pid = self.get_client_pid(req);
        let handle_id = HandleId(fh);
        match self.core.fallocate(
            &client_pid,
            handle_id,
            fallocate_mode,
            offset as u64,
            length as u64,
        ) {
            Ok(()) => reply.ok(),
            Err(FsError::AccessDenied) => reply.error(EACCES),
            Err(FsError::InvalidArgument) => reply.error(EINVAL),
            Err(FsError::Unsupported) => reply.error(libc::ENOTSUP),
            Err(_) => reply.error(EIO),
        }
    }

    fn copy_file_range(
        &mut self,
        req: &Request,
        ino_in: u64,
        fh_in: u64,
        offset_in: i64,
        ino_out: u64,
        fh_out: u64,
        offset_out: i64,
        len: u64,
        flags: u32,
        reply: ReplyWrite,
    ) {
        debug!(
            target: "agentfs::fuse",
            "copy_file_range ino_in={} ino_out={} fh_in={} fh_out={} len={}",
            ino_in,
            ino_out,
            fh_in,
            fh_out,
            len
        );
        if ino_in == CONTROL_FILE_INO || ino_out == CONTROL_FILE_INO {
            reply.error(libc::EPERM);
            return;
        }
        if flags != 0 || offset_in < 0 || offset_out < 0 {
            reply.error(libc::EOPNOTSUPP);
            return;
        }
        let client_pid = self.get_client_pid(req);
        let src_handle = HandleId(fh_in);
        let dst_handle = HandleId(fh_out);
        match self.core.copy_file_range(
            &client_pid,
            src_handle,
            dst_handle,
            offset_in as u64,
            offset_out as u64,
            len,
        ) {
            Ok(bytes) => {
                if bytes > u32::MAX as u64 {
                    reply.error(EIO);
                } else {
                    reply.written(bytes as u32);
                }
            }
            Err(FsError::AccessDenied) => reply.error(EACCES),
            Err(FsError::InvalidArgument) => reply.error(EINVAL),
            Err(FsError::Unsupported) => reply.error(libc::ENOSYS),
            Err(_) => reply.error(EIO),
        }
    }

    fn setattr(
        &mut self,
        req: &Request,
        ino: u64,
        mode: Option<u32>,
        uid: Option<u32>,
        gid: Option<u32>,
        size: Option<u64>,
        atime: Option<TimeOrNow>,
        mtime: Option<TimeOrNow>,
        _ctime: Option<SystemTime>,
        fh: Option<u64>,
        _crtime: Option<SystemTime>,
        _chgtime: Option<SystemTime>,
        _bkuptime: Option<SystemTime>,
        _flags: Option<u32>,
        reply: ReplyAttr,
    ) {
        // Resolve path for inode
        let path_bytes = match self.inode_to_path(ino) {
            Some(p) => p,
            None => {
                reply.error(ENOENT);
                return;
            }
        };

        let canonical_path = path_bytes.to_vec();

        if self.is_whiteouted(&canonical_path) {
            reply.error(ENOENT);
            return;
        }

        let path = self.path_from_bytes(&canonical_path);
        let client_pid = self.get_client_pid(req);
        let metadata_lock = Arc::clone(&self.metadata_lock);
        let _metadata_guard = metadata_lock.lock().unwrap();
        let mut metadata_changed = false;

        let needs_materialization = mode.is_some()
            || uid.is_some()
            || gid.is_some()
            || size.is_some()
            || atime.is_some()
            || mtime.is_some();

        if self.overlay_enabled() && needs_materialization {
            let upper_exists = match self.core.has_upper_entry(&self.internal_pid, path) {
                Ok(exists) => exists,
                Err(err) => {
                    warn!(
                        target: "agentfs::fuse",
                        path = %path.display(),
                        ?err,
                        "has_upper_entry failed during setattr"
                    );
                    reply.error(errno_from_fs_error(&err));
                    return;
                }
            };
            if !upper_exists {
                if self.lower_entry_exists(path) {
                    let internal_pid = self.internal_pid;
                    if let Err(err) = self.copy_lower_to_upper(&internal_pid, path, &canonical_path)
                    {
                        reply.error(match err {
                            FsError::AccessDenied => EACCES,

                            FsError::OperationNotPermitted => EPERM,
                            FsError::NotFound => ENOENT,
                            _ => EIO,
                        });
                        return;
                    }
                } else {
                    reply.error(ENOENT);
                    return;
                }
            }
        }

        // Apply size (truncate)
        if let Some(new_size) = size {
            if let Some(fh_val) = fh {
                let h = HandleId(fh_val);
                if let Err(e) = self.core.ftruncate(&client_pid, h, new_size) {
                    reply.error(match e {
                        FsError::NotFound => ENOENT,
                        FsError::AccessDenied => EACCES,

                        FsError::OperationNotPermitted => EPERM,
                        FsError::InvalidArgument => EINVAL,
                        FsError::Unsupported => libc::ENOTSUP,
                        _ => EIO,
                    });
                    return;
                }
                metadata_changed = true;
            } else {
                // Path-based truncate: open for write, then ftruncate
                let opts = OpenOptions {
                    write: true,
                    ..OpenOptions::default()
                };
                match self.core.open(&client_pid, path, &opts) {
                    Ok(h) => {
                        if let Err(e) = self.core.ftruncate(&client_pid, h, new_size) {
                            reply.error(match e {
                                FsError::NotFound => ENOENT,
                                FsError::AccessDenied => EACCES,

                                FsError::OperationNotPermitted => EPERM,
                                FsError::InvalidArgument => EINVAL,
                                FsError::Unsupported => libc::ENOTSUP,
                                _ => EIO,
                            });
                            return;
                        }
                        let _ = self.core.close(&client_pid, h);
                        metadata_changed = true;
                    }
                    Err(e) => {
                        reply.error(match e {
                            FsError::NotFound => ENOENT,
                            FsError::AccessDenied => EACCES,

                            FsError::OperationNotPermitted => EPERM,
                            _ => EIO,
                        });
                        return;
                    }
                }
            }
        }

        // Apply mode (chmod)
        if let Some(new_mode) = mode {
            let chmod_target_node = if let Some(fh_val) = fh {
                self.core.describe_handle_node(HandleId(fh_val)).ok()
            } else {
                self.core
                    .resolve_path_public(&client_pid, path)
                    .ok()
                    .map(|(node_id, _)| node_id)
            };
            debug!(
                target: "agentfs::fuse",
                event = "setattr_chmod",
                path = %path.display(),
                mode = format_args!("{:#o}", new_mode),
                fh = fh.unwrap_or(u64::MAX),
                by_handle = fh.is_some(),
                target_node = ?chmod_target_node
            );
            if let Some(fh_val) = fh {
                if let Err(e) = self.core.fchmod(&client_pid, HandleId(fh_val), new_mode) {
                    reply.error(match e {
                        FsError::NotFound => ENOENT,
                        FsError::AccessDenied => EACCES,

                        FsError::OperationNotPermitted => EPERM,
                        _ => EIO,
                    });
                    return;
                }
                metadata_changed = true;
            } else if let Err(e) = self.core.fchmodat(&client_pid, path, new_mode, 0) {
                reply.error(match e {
                    FsError::NotFound => ENOENT,
                    FsError::AccessDenied => EACCES,

                    FsError::OperationNotPermitted => EPERM,
                    _ => EIO,
                });
                return;
            } else {
                metadata_changed = true;
            }
        }

        // Apply ownership (chown)
        if uid.is_some() || gid.is_some() {
            let new_uid = uid.unwrap_or(u32::MAX);
            let new_gid = gid.unwrap_or(u32::MAX);
            debug!(
                target: "agentfs::fuse",
                event = "setattr_chown_start",
                path = %path.display(),
                req_uid = req.uid(),
                req_gid = req.gid(),
                new_uid,
                new_gid,
                fh = fh.unwrap_or(u64::MAX)
            );
            if let Err(err) = self.core.getattr(&client_pid, path) {
                match err {
                    FsError::AccessDenied => {
                        debug!(
                            target: "agentfs::fuse",
                            event = "chown_getattr_denied",
                            path = %path.display()
                        );
                        reply.error(EACCES);
                        return;
                    }
                    FsError::NotFound => {
                        // allow copy-up flow to attempt materialization
                    }
                    _ => {
                        warn!(
                            target: "agentfs::fuse",
                            event = "chown_getattr_failed",
                            path = %path.display(),
                            new_uid,
                            new_gid,
                            errno = errno_from_fs_error(&err),
                            ?err
                        );
                        reply.error(errno_from_fs_error(&err));
                        return;
                    }
                }
            }
            if let Some(parent_path) = path.parent() {
                let client_attr = self.core.getattr(&client_pid, parent_path);
                let internal_attr = self.core.getattr(&self.internal_pid, parent_path);
                let parent_inode = self
                    .core
                    .resolve_path_public(&self.internal_pid, parent_path)
                    .map(|(ino, _)| ino)
                    .ok();
                debug!(
                    target: "agentfs::fuse",
                    event = "chown_parent_exec_check",
                    child = %path.display(),
                    parent = %parent_path.display(),
                    parent_ino = parent_inode.unwrap_or(0),
                    client_mode = client_attr.as_ref().map(|attr| format!("{:#o}", attr.mode_bits)).unwrap_or_else(|_| "ERR".into()),
                    internal_mode = internal_attr
                        .as_ref()
                        .map(|attr| format!("{:#o}", attr.mode_bits))
                        .unwrap_or_else(|_| "ERR".into())
                );
                if let Err(err) =
                    self.core.check_path_access(&client_pid, parent_path, false, false, true)
                {
                    let errno = match err {
                        FsError::AccessDenied => EACCES,
                        FsError::NotFound => ENOENT,
                        _ => errno_from_fs_error(&err),
                    };
                    debug!(
                        target: "agentfs::fuse",
                        event = "chown_parent_exec_check_failed",
                        path = %path.display(),
                        parent = %parent_path.display(),
                        errno,
                        ?err
                    );
                    reply.error(errno);
                    return;
                }
            } else {
                debug!(
                    target: "agentfs::fuse",
                    event = "chown_parent_missing",
                    path = %path.display()
                );
            }
            let core = Arc::clone(&self.core);
            let attempt_chown = |pid: &PID| -> FsResult<()> {
                if let Some(fh_val) = fh {
                    core.fchown(pid, HandleId(fh_val), new_uid, new_gid)
                } else {
                    core.fchownat(pid, path, new_uid, new_gid, 0)
                }
            };

            match attempt_chown(&client_pid) {
                Ok(()) => {}
                Err(FsError::NotFound)
                    if self.overlay_enabled() && self.lower_entry_exists(path) =>
                {
                    let internal_pid = self.internal_pid;
                    if let Err(err) = self.copy_lower_to_upper(&internal_pid, path, &canonical_path)
                    {
                        let errno = match err {
                            FsError::AccessDenied => EACCES,

                            FsError::OperationNotPermitted => EPERM,
                            FsError::NotFound => ENOENT,
                            _ => EIO,
                        };
                        debug!(
                            target: "agentfs::fuse",
                            event = "copy_up_failed_before_chown",
                            path = %path.display(),
                            errno,
                            ?err
                        );
                        reply.error(errno);
                        return;
                    }

                    if let Err(e) = attempt_chown(&client_pid) {
                        warn!(
                            target: "agentfs::fuse",
                            path = %path.display(),
                            requested_uid = new_uid,
                            requested_gid = new_gid,
                            ?e,
                            "fchown retry failed after copy-up"
                        );
                        let errno = match e {
                            FsError::NotFound => ENOENT,
                            FsError::AccessDenied => EACCES,

                            FsError::OperationNotPermitted => EPERM,
                            _ => EIO,
                        };
                        debug!(
                            target: "agentfs::fuse",
                            event = "chown_retry_failed",
                            path = %path.display(),
                            errno,
                            ?e
                        );
                        reply.error(errno);
                        return;
                    }
                }
                Err(e) => {
                    let errno = match e {
                        FsError::NotFound => ENOENT,
                        FsError::AccessDenied => EACCES,

                        FsError::OperationNotPermitted => EPERM,
                        _ => EIO,
                    };
                    warn!(
                        target: "agentfs::fuse",
                        event = "chown_failed",
                        path = %path.display(),
                        requested_uid = new_uid,
                        requested_gid = new_gid,
                        errno,
                        ?e,
                        "fchown operation failed during setattr"
                    );
                    debug!(
                        target: "agentfs::fuse",
                        event = "chown_failed",
                        path = %path.display(),
                        errno,
                        ?e
                    );
                    reply.error(errno);
                    return;
                }
            }
            metadata_changed = true;
        }

        // Apply timestamps (utimens)
        if atime.is_some() || mtime.is_some() {
            let to_ts = |tor: Option<TimeOrNow>| -> TimespecData {
                match tor {
                    Some(TimeOrNow::Now) => TimespecData {
                        tv_sec: 0,
                        tv_nsec: libc::UTIME_NOW as u32,
                    },
                    Some(TimeOrNow::SpecificTime(t)) => {
                        let dur = t.duration_since(UNIX_EPOCH).unwrap_or_default();
                        TimespecData {
                            tv_sec: dur.as_secs(),
                            tv_nsec: dur.subsec_nanos(),
                        }
                    }
                    None => TimespecData {
                        tv_sec: 0,
                        tv_nsec: libc::UTIME_OMIT as u32,
                    },
                }
            };

            let a_ts = to_ts(atime);
            let m_ts = to_ts(mtime);

            if let Some(fh_val) = fh {
                if let Err(e) =
                    self.core.futimens(&client_pid, HandleId(fh_val), Some((a_ts, m_ts)))
                {
                    reply.error(match e {
                        FsError::NotFound => ENOENT,
                        FsError::AccessDenied => EACCES,

                        FsError::OperationNotPermitted => EPERM,
                        _ => EIO,
                    });
                    return;
                }
                metadata_changed = true;
            } else if let Err(e) = self.core.utimensat(&client_pid, path, Some((a_ts, m_ts)), 0) {
                reply.error(match e {
                    FsError::NotFound => ENOENT,
                    FsError::AccessDenied => EACCES,

                    FsError::OperationNotPermitted => EPERM,
                    _ => EIO,
                });
                return;
            } else {
                metadata_changed = true;
            }
        }

        if metadata_changed {
            if let Some((parent_ino, name)) =
                self.resolve_parent_and_name(&canonical_path, None, None)
            {
                self.invalidate_entry(parent_ino, name.as_os_str().as_bytes());
            }
            self.invalidate_inode_metadata(ino);
        }

        // Return updated attributes
        match self.core.getattr(&client_pid, path) {
            Ok(attr) => {
                let fuse_attr = self.attr_to_fuse(&attr, ino);
                reply.attr(&self.attr_ttl, &fuse_attr);
            }
            Err(FsError::NotFound) => reply.error(ENOENT),
            Err(_) => reply.error(EIO),
        }
    }
}
