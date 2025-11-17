// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! AgentFS FUSE adapter implementation
//!
//! Maps FUSE operations to AgentFS Core calls.

#[cfg(not(all(feature = "fuse", target_os = "linux")))]
compile_error!("This module requires the 'fuse' feature on Linux");

use agentfs_core::{
    Attributes, FsConfig, FsCore, FsError, HandleId, OpenOptions, error::FsResult, vfs::PID,
};
use agentfs_proto::messages::{StatData, TimespecData};
use agentfs_proto::*;
use crossbeam_queue::SegQueue;
use fuser::{
    FUSE_ROOT_ID, FileAttr, FileType, ReplyAttr, ReplyCreate, ReplyData, ReplyDirectory,
    ReplyEmpty, ReplyEntry, ReplyOpen, ReplyStatfs, ReplyWrite, ReplyXattr, Request, TimeOrNow,
};
use libc::{
    EACCES, EBADF, EBUSY, EEXIST, EINVAL, EIO, EISDIR, ELOOP, ENAMETOOLONG, ENOENT, ENOSYS,
    ENOTDIR, ENOTEMPTY, ENOTSUP, EPERM, c_int,
};
use ssz::{Decode, Encode};
use std::collections::{HashMap, HashSet};
use std::ffi::OsStr;
use std::fs::File;
use std::io::Read;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{FileExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::{
    Arc, Condvar, Mutex,
    atomic::{AtomicBool, AtomicU64, Ordering},
};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tracing::{debug, error, info, warn};

/// Special inode for the .agentfs directory
const AGENTFS_DIR_INO: u64 = FUSE_ROOT_ID + 1;

/// Special inode for the .agentfs/control file
const CONTROL_FILE_INO: u64 = FUSE_ROOT_ID + 2;

/// IOCTL command for AgentFS control operations (matches _IOWR('A','F', 4096))
const AGENTFS_IOCTL_CMD: u32 = 0xD000_4146;

/// Maximum single path component length to guard against overly long names
const NAME_MAX: usize = 255;

/// File-handle space reserved for lower pass-through handles
const LOWER_HANDLE_BASE: u64 = 1u64 << 60;

struct LowerHandle {
    file: File,
    _ino: u64,
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
    reply: ReplyWrite,
    trace: Option<WriteTrace>,
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
                            match result {
                                Ok(bytes_written) => job.reply.written(bytes_written as u32),
                                Err(FsError::NotFound) => job.reply.error(ENOENT),
                                Err(FsError::AccessDenied) => job.reply.error(EACCES),
                                Err(_) => job.reply.error(EIO),
                            }
                            if let Some(trace) = job.trace.take() {
                                trace.finish();
                            }
                        }
                        None => {
                            let (lock, cvar) = &*signal_clone;
                            let mut guard = lock.lock().unwrap();
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

    fn submit(&self, job: WriteJob) -> Result<(), WriteJob> {
        self.queue.push(job);
        let (lock, cvar) = &*self.signal;
        if let Ok(mut pending) = lock.lock() {
            *pending = true;
            cvar.notify_one();
        }
        Ok(())
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

/// AgentFS FUSE filesystem adapter
pub struct AgentFsFuse {
    /// Core filesystem instance
    core: Arc<FsCore>,
    /// Configuration
    config: FsConfig,
    /// TTL for attribute cache responses
    attr_ttl: Duration,
    /// TTL for directory entry cache responses
    entry_ttl: Duration,
    /// TTL for negative lookups (future use)
    _negative_ttl: Duration,
    /// Cache of inode to path mappings for control operations
    inodes: HashMap<u64, Vec<u8>>, // inode -> canonical path
    /// Reverse mapping from path to inode
    paths: HashMap<Vec<u8>, u64>, // path -> inode
    /// Next available inode number
    next_inode: u64,
    /// Open handles tracked per inode for fh-less operations
    inode_handles: HashMap<u64, HashSet<u64>>,
    /// Map handle IDs to (inode, owning pid)
    handle_index: HashMap<u64, (u64, u32)>,
    /// Lower pass-through handles
    lower_handles: HashMap<u64, LowerHandle>,
    /// Next pass-through handle id
    next_lower_fh: u64,
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
}

impl AgentFsFuse {
    /// Create a new FUSE adapter with the given configuration
    pub fn new(mut config: FsConfig) -> FsResult<Self> {
        config.security.enforce_posix_permissions = true;
        // pjdfstest expects uid 0 to bypass sticky/exec bits so cleanup steps succeed.
        config.security.root_bypass_permissions = true;
        let core = Arc::new(FsCore::new(config.clone())?);
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

        let inflight_writes = Arc::new(AtomicU64::new(0));
        let max_write_depth = Arc::new(AtomicU64::new(0));
        let write_dispatcher = WriteDispatcher::new(Arc::clone(&core), write_worker_count());

        Ok(Self {
            core,
            config,
            attr_ttl,
            entry_ttl,
            _negative_ttl: negative_ttl,
            inodes,
            paths,
            next_inode: CONTROL_FILE_INO + 1,
            inode_handles: HashMap::new(),
            handle_index: HashMap::new(),
            lower_handles: HashMap::new(),
            next_lower_fh: LOWER_HANDLE_BASE,
            whiteouts: HashSet::new(),
            trace_writes,
            inflight_writes,
            max_write_depth,
            write_dispatcher,
        })
    }

    /// Get the path for a given inode
    fn inode_to_path(&self, ino: u64) -> Option<&[u8]> {
        self.inodes.get(&ino).map(|p| p.as_slice())
    }

    /// Gets the AgentFS PID for the client process making the request.
    /// This registers the process with its correct UID/GID if not seen before.
    fn get_client_pid(&self, req: &Request) -> PID {
        let client_pid = req.pid();
        let client_uid = req.uid();
        let client_gid = req.gid();
        debug!(
            target: "agentfs::metadata",
            fuse_pid = client_pid,
            client_uid,
            client_gid,
            "registering client process"
        );

        // The FsCore register_process function expects a parent_pid.
        // The fuser::Request does not provide this.
        // Based on the agentfs-core tests, passing the client_pid as its own
        // parent is a valid convention for registering a new process tree root.
        //
        // This is VASTLY superior to the previous hardcoded (host_pid, 1, 0, 0).
        let groups = Self::load_process_groups(client_pid, client_gid);

        self.core.register_process_with_groups(
            client_pid,
            client_pid,
            client_uid,
            client_gid,
            groups.as_slice(),
        )
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

        if groups.is_empty() {
            groups.push(primary_gid);
        } else if !groups.contains(&primary_gid) {
            groups.push(primary_gid);
        }

        groups
    }

    /// Allocate a new inode for a path
    fn alloc_inode(&mut self, path: &[u8]) -> u64 {
        if let Some(&existing_inode) = self.paths.get(path) {
            return existing_inode;
        }

        let inode = self.next_inode;
        self.next_inode += 1;
        self.record_path_for_inode(path.to_vec(), inode);
        inode
    }

    /// Get or allocate inode for a path
    fn get_or_alloc_inode(&mut self, path: &[u8]) -> u64 {
        if let Some(&inode) = self.paths.get(path) {
            inode
        } else {
            self.alloc_inode(path)
        }
    }

    /// Associate a path with an inode, preserving the original canonical path.
    fn record_path_for_inode(&mut self, path: Vec<u8>, inode: u64) {
        self.whiteouts.remove(&path);
        self.paths.insert(path.clone(), inode);
        self.inodes.insert(inode, path);
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

    fn alloc_lower_handle(&mut self, ino: u64, file: File) -> u64 {
        let fh = self.next_lower_fh;
        self.next_lower_fh = self.next_lower_fh.saturating_add(1);
        self.lower_handles.insert(fh, LowerHandle { file, _ino: ino });
        fh
    }

    fn ensure_upper_parent_dirs(&mut self, pid: &PID, path: &Path) -> FsResult<()> {
        if let Some(parent) = path.parent() {
            use std::path::Component;
            let mut current = PathBuf::from("/");
            for component in parent.components() {
                if let Component::Normal(name) = component {
                    current.push(name);
                    let upper_exists = self.core.has_upper_entry(pid, &current).unwrap_or(false);
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
            ..Default::default()
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
        }

        self.clear_whiteout(canonical);
        Ok(())
    }

    /// Remove a single path mapping and update canonical bookkeeping.
    fn remove_path_mapping(&mut self, path: &[u8]) -> Option<u64> {
        if let Some(inode) = self.paths.remove(path) {
            let removed_was_canonical =
                self.inodes.get(&inode).map(|p| p.as_slice() == path).unwrap_or(false);

            if removed_was_canonical {
                if let Some((replacement, _)) = self.paths.iter().find(|(_, &ino)| ino == inode) {
                    self.inodes.insert(inode, replacement.clone());
                }
            }

            Some(inode)
        } else {
            None
        }
    }

    fn track_handle(&mut self, ino: u64, fh: u64, pid: &PID) {
        self.inode_handles.entry(ino).or_default().insert(fh);
        self.handle_index.insert(fh, (ino, pid.as_u32()));
    }

    fn untrack_handle(&mut self, ino: u64, fh: u64) {
        if let Some(handles) = self.inode_handles.get_mut(&ino) {
            handles.remove(&fh);
            if handles.is_empty() {
                self.inode_handles.remove(&ino);
            }
        }
        self.handle_index.remove(&fh);
    }

    fn handle_for_inode(&self, ino: u64) -> Option<(u64, PID)> {
        self.inode_handles
            .get(&ino)
            .and_then(|handles| handles.iter().next())
            .and_then(|fh| self.handle_index.get(fh).map(|(_, pid)| (*fh, PID::new(*pid))))
    }

    fn forget_inode(&mut self, inode: u64) {
        if inode == FUSE_ROOT_ID || inode == AGENTFS_DIR_INO || inode == CONTROL_FILE_INO {
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
    fn attr_to_fuse(&self, attr: &Attributes, ino: u64) -> FileAttr {
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

        FileAttr {
            ino,
            size: attr.len,
            blocks: (attr.len + 511) / 512, // 512-byte blocks
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
        }
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

    /// Convert FUSE flags to OpenOptions
    fn fuse_flags_to_options(&self, flags: i32) -> OpenOptions {
        use libc::{O_APPEND, O_CREAT, O_RDWR, O_TRUNC, O_WRONLY};

        let mut options = OpenOptions::default();

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

        let request_bytes = Self::extract_framed_payload(data).inspect_err(|code| {
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
        let mut fuse = AgentFsFuse::new(FsConfig::default()).expect("fuse init");

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

impl fuser::Filesystem for AgentFsFuse {
    fn init(&mut self, _req: &Request, config: &mut fuser::KernelConfig) -> Result<(), c_int> {
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

    fn forget(&mut self, _req: &Request, ino: u64, _nlookup: u64) {
        self.forget_inode(ino);
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
                uid: 0,
                gid: 0,
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
                uid: 0,
                gid: 0,
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
        let client_pid = self.get_client_pid(req);
        match self.core.getattr(&client_pid, path) {
            Ok(attr) => {
                let ino = self.get_or_alloc_inode(&full_path);
                let fuse_attr = self.attr_to_fuse(&attr, ino);
                reply.entry(&self.entry_ttl, &fuse_attr, 0);
            }
            Err(FsError::NotFound) => reply.error(ENOENT),
            Err(_) => reply.error(EIO),
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
                uid: 0,
                gid: 0,
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
                uid: 0,
                gid: 0,
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
                uid: 0,
                gid: 0,
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
            reply.opened(0, 0); // fh=0 for control file
            return;
        }

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
        let options = self.fuse_flags_to_options(flags);
        let client_pid = self.get_client_pid(req);

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
                self.track_handle(ino, handle_id.0, &client_pid);
                reply.opened(handle_id.0, 0);
            }
            Err(FsError::NotFound) | Err(FsError::Unsupported) => {
                if !self.overlay_enabled() || !self.lower_entry_exists(path) {
                    reply.error(ENOENT);
                    return;
                }

                if options.write || options.create || options.truncate || options.append {
                    if let Err(err) = self.copy_lower_to_upper(&client_pid, path, &canonical_path) {
                        error!("failed to materialize lower entry: {:?}", err);
                        reply.error(EIO);
                        return;
                    }
                    match self.core.open(&client_pid, path, &options) {
                        Ok(handle_id) => {
                            self.track_handle(ino, handle_id.0, &client_pid);
                            reply.opened(handle_id.0, 0);
                        }
                        Err(FsError::AccessDenied) => reply.error(EACCES),
                        Err(FsError::NotFound) => reply.error(ENOENT),
                        Err(_) => reply.error(EIO),
                    }
                } else if options.read {
                    if let Some(lower_path) = self.lower_full_path(path) {
                        match File::open(&lower_path) {
                            Ok(file) => {
                                let fh = self.alloc_lower_handle(ino, file);
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
            Err(FsError::AccessDenied) => reply.error(EACCES),
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

        let client_pid = self.get_client_pid(req);
        match self.core.mknod(&client_pid, path, final_mode, rdev as u64) {
            Ok(()) => match self.core.getattr(&client_pid, path) {
                Ok(attr) => {
                    let ino = self.get_or_alloc_inode(&full_path);
                    let fuse_attr = self.attr_to_fuse(&attr, ino);
                    reply.entry(&self.entry_ttl, &fuse_attr, 0);
                }
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

        let job = WriteJob {
            pid: client_pid,
            handle_id,
            offset: offset as u64,
            data: buffer,
            reply,
            trace,
        };

        if let Err(mut failed_job) = self.write_dispatcher.submit(job) {
            if let Some(trace) = failed_job.trace.take() {
                trace.finish();
            } else if self.trace_writes {
                self.inflight_writes.fetch_sub(1, Ordering::SeqCst);
            }
            failed_job.reply.error(EIO);
        }
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

                    let entry_ino = self.get_or_alloc_inode(&entry_path);

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
        _mode: u32,
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
                    Ok(attr) => {
                        let ino = self.get_or_alloc_inode(&full_path);
                        let fuse_attr = self.attr_to_fuse(&attr, ino);
                        self.track_handle(ino, handle_id.0 as u64, &client_pid);
                        reply.created(&self.entry_ttl, &fuse_attr, 0, handle_id.0 as u64, 0);
                    }
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
                Ok(attr) => {
                    let ino = self.get_or_alloc_inode(&full_path);
                    let fuse_attr = self.attr_to_fuse(&attr, ino);
                    reply.entry(&self.entry_ttl, &fuse_attr, 0);
                }
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
                self.remove_path_mapping(&full_path);
                reply.ok();
            }
            Err(FsError::NotFound) => {
                if self.overlay_enabled() && self.lower_entry_exists(path) {
                    self.record_whiteout(full_path.clone());
                    self.remove_path_mapping(&full_path);
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
                self.remove_path_mapping(&full_path);
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

        let old_path_obj = self.path_from_bytes(&old_path);
        let new_path_obj = self.path_from_bytes(&new_path);
        let client_pid = self.get_client_pid(req);

        match self.core.rename(&client_pid, old_path_obj, new_path_obj) {
            Ok(()) => {
                if let Some(inode) = self.remove_path_mapping(&old_path) {
                    self.record_path_for_inode(new_path.clone(), inode);
                }
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
                    let ino = self.get_or_alloc_inode(&linkpath);
                    let fuse_attr = self.attr_to_fuse(&attr, ino);
                    reply.entry(&self.entry_ttl, &fuse_attr, 0);
                }
                Err(_) => reply.error(EIO),
            },
            Err(FsError::AlreadyExists) => reply.error(EEXIST),
            Err(FsError::NotFound) => reply.error(ENOENT),
            Err(FsError::NotADirectory) => reply.error(ENOTDIR),
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
                    self.record_path_for_inode(new_path.clone(), ino);
                    let fuse_attr = self.attr_to_fuse(&attr, ino);
                    reply.entry(&self.entry_ttl, &fuse_attr, 0);
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

        match self.core.fsync(&client_pid, handle_id, datasync) {
            Ok(()) => reply.ok(),
            Err(FsError::AccessDenied) => reply.error(EACCES),
            Err(FsError::InvalidArgument) => reply.error(EINVAL),
            Err(FsError::NotFound) => reply.error(ENOENT),
            Err(_) => reply.error(EIO),
        }
    }

    fn flush(&mut self, _req: &Request, ino: u64, _fh: u64, _lock_owner: u64, reply: ReplyEmpty) {
        if ino == CONTROL_FILE_INO {
            reply.ok(); // No-op for control file
            return;
        }

        // For now, flush is not implemented - no-op
        reply.ok();
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
        _req: &Request,
        ino: u64,
        _fh: u64,
        _offset: i64,
        _length: i64,
        _mode: i32,
        reply: ReplyEmpty,
    ) {
        if ino == CONTROL_FILE_INO {
            reply.error(libc::EPERM);
            return;
        }

        // For now, we don't implement fallocate - return ENOTSUP
        reply.error(libc::ENOTSUP);
    }

    fn copy_file_range(
        &mut self,
        _req: &Request,
        ino_in: u64,
        _fh_in: u64,
        _offset_in: i64,
        ino_out: u64,
        _fh_out: u64,
        _offset_out: i64,
        _len: u64,
        _flags: u32,
        reply: ReplyWrite,
    ) {
        if ino_in == CONTROL_FILE_INO || ino_out == CONTROL_FILE_INO {
            reply.error(libc::EPERM);
            return;
        }

        // For now, we don't implement copy_file_range - return ENOTSUP
        reply.error(libc::ENOTSUP);
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

        let needs_materialization = mode.is_some()
            || uid.is_some()
            || gid.is_some()
            || size.is_some()
            || atime.is_some()
            || mtime.is_some();

        if self.overlay_enabled() && needs_materialization {
            let upper_exists = self.core.has_upper_entry(&client_pid, path).unwrap_or(false);
            if !upper_exists {
                if self.lower_entry_exists(path) {
                    if let Err(err) = self.copy_lower_to_upper(&client_pid, path, &canonical_path) {
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
            } else {
                // Path-based truncate: open for write, then ftruncate
                let opts = OpenOptions {
                    write: true,
                    ..Default::default()
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
            } else if let Err(e) = self.core.fchmodat(&client_pid, path, new_mode, 0) {
                reply.error(match e {
                    FsError::NotFound => ENOENT,
                    FsError::AccessDenied => EACCES,

                    FsError::OperationNotPermitted => EPERM,
                    _ => EIO,
                });
                return;
            }
        }

        // Apply ownership (chown)
        if uid.is_some() || gid.is_some() {
            let new_uid = uid.unwrap_or(u32::MAX);
            let new_gid = gid.unwrap_or(u32::MAX);
            if let Some(fh_val) = fh {
                if let Err(e) = self.core.fchown(&client_pid, HandleId(fh_val), new_uid, new_gid) {
                    reply.error(match e {
                        FsError::NotFound => ENOENT,
                        FsError::AccessDenied => EACCES,

                        FsError::OperationNotPermitted => EPERM,
                        _ => EIO,
                    });
                    return;
                }
            } else if let Err(e) = self.core.fchownat(&client_pid, path, new_uid, new_gid, 0) {
                reply.error(match e {
                    FsError::NotFound => ENOENT,
                    FsError::AccessDenied => EACCES,

                    FsError::OperationNotPermitted => EPERM,
                    _ => EIO,
                });
                return;
            }
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
            } else if let Err(e) = self.core.utimensat(&client_pid, path, Some((a_ts, m_ts)), 0) {
                reply.error(match e {
                    FsError::NotFound => ENOENT,
                    FsError::AccessDenied => EACCES,

                    FsError::OperationNotPermitted => EPERM,
                    _ => EIO,
                });
                return;
            }
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
        assert_eq!(fuse._negative_ttl, Duration::from_millis(3500));
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
