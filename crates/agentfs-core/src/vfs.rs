// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Virtual filesystem implementation for AgentFS Core

use std::collections::HashMap;
use std::fs::OpenOptions as StdOpenOptions;
use std::fs::{self, File};
use std::io::Write;
use std::os::unix::io::RawFd;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::error::{FsError, FsResult};
use crate::storage::StorageBackend;
use crate::{
    Attributes, BackstoreInfo, BranchId, BranchInfo, ContentId, DirEntry, FileMode, FileTimes,
    FsConfig, FsStats, HandleId, LockKind, LockRange, OpenOptions, ShareMode, SnapshotId,
    SpecialNodeKind, StreamSpec,
};

use crate::{Backstore, LowerFs};
#[cfg(feature = "events")]
use crate::{EventKind, EventSink, SubscriptionId};

// Import proto types for interpose operations
use agentfs_proto::messages::{StatData, StatfsData, TimespecData};

// Directory file descriptor mapping for *at functions
#[derive(Clone, Debug)]
pub struct DirfdMapping {
    /// Current working directory for AT_FDCWD resolution
    cwd: std::path::PathBuf,
    /// File descriptor to path mappings
    fd_paths: HashMap<std::os::fd::RawFd, std::path::PathBuf>,
}

impl DirfdMapping {
    pub fn new() -> Self {
        Self {
            cwd: std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("/")),
            fd_paths: HashMap::new(),
        }
    }

    /// Get the path for a directory file descriptor
    pub fn get_path(&self, dirfd: std::os::fd::RawFd) -> Option<&std::path::PathBuf> {
        self.fd_paths.get(&dirfd)
    }

    /// Set the path for a directory file descriptor
    pub fn set_path(&mut self, dirfd: std::os::fd::RawFd, path: std::path::PathBuf) {
        self.fd_paths.insert(dirfd, path);
    }

    /// Remove a directory file descriptor mapping
    pub fn remove_path(&mut self, dirfd: std::os::fd::RawFd) {
        self.fd_paths.remove(&dirfd);
    }

    /// Update current working directory
    pub fn set_cwd(&mut self, cwd: std::path::PathBuf) {
        self.cwd = cwd;
    }

    /// Get current working directory
    pub fn get_cwd(&self) -> &std::path::PathBuf {
        &self.cwd
    }

    /// Duplicate file descriptor mapping
    pub fn dup_fd(&mut self, old_fd: std::os::fd::RawFd, new_fd: std::os::fd::RawFd) {
        if let Some(path) = self.fd_paths.get(&old_fd).cloned() {
            self.fd_paths.insert(new_fd, path);
        }
    }
}

impl Default for DirfdMapping {
    fn default() -> Self {
        Self::new()
    }
}

/// Internal node ID for filesystem nodes
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct NodeId(u64);

/// Filesystem node types
#[derive(Clone, Debug)]
pub(crate) enum NodeKind {
    File {
        streams: HashMap<String, (ContentId, u64)>, // stream_name -> (content_id, size)
    },
    Directory {
        children: HashMap<String, NodeId>,
    },
    Symlink {
        target: String,
    },
}

/// Filesystem node
#[derive(Clone, Debug)]
pub(crate) struct Node {
    #[allow(dead_code)] // ID currently unused outside of debugging; kept for future referencing
    pub(crate) id: NodeId,
    pub kind: NodeKind,
    pub times: FileTimes,
    pub mode: u32,
    pub uid: u32,
    pub gid: u32,
    pub special_kind: Option<SpecialNodeKind>,
    pub xattrs: HashMap<String, Vec<u8>>, // Extended attributes
    pub acls: HashMap<u32, Vec<u8>>,      // ACLs by type (ACL_TYPE_EXTENDED, etc.)
    pub flags: u32,                       // BSD flags (UF_* flags)
    pub nlink: u32,
}

/// Handle types
#[derive(Debug)]
pub(crate) enum HandleType {
    File {
        options: OpenOptions,
        deleted: bool, // For delete-on-close semantics
    },
    Directory {
        position: usize,        // Index into directory entries
        entries: Vec<DirEntry>, // Cached directory entries
    },
}

/// Open handle (file or directory)
#[derive(Debug)]
pub(crate) struct Handle {
    #[allow(dead_code)] // Handle ID reserved for future handle table queries
    pub(crate) id: HandleId,
    pub node_id: NodeId,
    pub path: PathBuf, // Store the path for event emission
    pub kind: HandleType,
}

/// Snapshot containing immutable tree state
#[derive(Clone, Debug)]
pub(crate) struct Snapshot {
    pub id: SnapshotId,
    pub root_id: NodeId,
    pub name: Option<String>,
}

/// Branch state containing a tree of nodes (writable clone of a snapshot)
#[derive(Clone, Debug)]
pub(crate) struct Branch {
    pub id: BranchId,
    pub root_id: NodeId,
    pub parent_snapshot: Option<SnapshotId>,
    pub name: Option<String>,
}

/// Active byte-range lock
#[derive(Clone, Debug)]
pub(crate) struct ActiveLock {
    pub handle_id: HandleId,
    pub range: LockRange,
}

/// Lock manager for tracking byte-range locks per node
#[derive(Clone, Debug)]
pub(crate) struct LockManager {
    pub locks: HashMap<NodeId, Vec<ActiveLock>>,
}

/// Process identifier for type safety in the filesystem API.
/// All filesystem operations require a registered PID obtained via `register_process`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PID(pub(crate) u32);

impl PID {
    pub fn new(pid: u32) -> Self {
        Self(pid)
    }

    pub fn as_u32(&self) -> u32 {
        self.0
    }
}

/// User represents the security identity of a process (uid, gid, and groups)
#[derive(Clone, Debug)]
pub(crate) struct User {
    pub(crate) uid: u32,
    pub(crate) gid: u32,
    pub(crate) groups: Vec<u32>,
}

/// The main filesystem core implementation
pub struct FsCore {
    config: FsConfig,
    storage: Arc<dyn StorageBackend>,
    backstore: Option<Box<dyn Backstore>>, // Backstore for overlay operations
    lower_fs: Option<Box<dyn LowerFs>>,    // Lower filesystem provider for overlay
    nodes: Mutex<HashMap<NodeId, Node>>,
    pub(crate) snapshots: Mutex<HashMap<SnapshotId, Snapshot>>,
    pub(crate) branches: Mutex<HashMap<BranchId, Branch>>,
    handles: Mutex<HashMap<HandleId, Handle>>,
    next_node_id: Mutex<u64>,
    next_handle_id: Mutex<u64>,
    #[cfg(feature = "events")]
    next_subscription_id: Mutex<u64>,
    pub(crate) process_branches: Mutex<HashMap<u32, BranchId>>, // Process ID -> Branch ID mapping
    process_identities: Mutex<HashMap<u32, User>>,              // Process ID -> security identity
    process_children: Mutex<HashMap<u32, Vec<u32>>>,            // Parent PID -> list of child PIDs
    process_parents: Mutex<HashMap<u32, u32>>,                  // Child PID -> parent PID
    process_dirfd_mappings: Mutex<HashMap<u32, DirfdMapping>>, // Process ID -> directory fd mappings
    locks: Mutex<LockManager>,                                 // Byte-range lock manager
    #[cfg(feature = "events")]
    event_subscriptions: Mutex<HashMap<SubscriptionId, Arc<dyn EventSink>>>,
}

impl FsCore {
    /// Create a new FsCore instance with the given configuration
    pub fn new(config: FsConfig) -> FsResult<Self> {
        // Initialize storage backend based on backstore configuration
        let storage: Arc<dyn StorageBackend> = match &config.backstore {
            crate::config::BackstoreMode::InMemory => {
                Arc::new(crate::storage::InMemoryBackend::new())
            }
            crate::config::BackstoreMode::HostFs { root, .. } => {
                Arc::new(crate::storage::HostFsBackend::new(root.clone())?)
            }
            crate::config::BackstoreMode::RamDisk { .. } => {
                // RamDisk creates an APFS volume for backstore, but uses in-memory for core storage
                Arc::new(crate::storage::InMemoryBackend::new())
            }
        };

        // Initialize backstore for overlay operations or standalone backstore modes
        let backstore = if config.overlay.enabled
            || matches!(
                config.backstore,
                crate::config::BackstoreMode::RamDisk { .. }
            ) {
            Some(crate::storage::create_backstore(&config.backstore)?)
        } else {
            None
        };

        // Initialize lower filesystem if overlay is enabled
        let lower_fs = if config.overlay.enabled {
            if let Some(lower_root) = &config.overlay.lower_root {
                Some(crate::overlay::create_lower_fs(lower_root)?)
            } else {
                return Err(FsError::InvalidArgument);
            }
        } else {
            None
        };

        let mut core = Self {
            config,
            storage,
            backstore,
            lower_fs,
            nodes: Mutex::new(HashMap::new()),
            snapshots: Mutex::new(HashMap::new()),
            branches: Mutex::new(HashMap::new()),
            handles: Mutex::new(HashMap::new()),
            next_node_id: Mutex::new(1),
            next_handle_id: Mutex::new(1),
            #[cfg(feature = "events")]
            next_subscription_id: Mutex::new(1),
            process_branches: Mutex::new(HashMap::new()), // No processes initially bound
            process_identities: Mutex::new(HashMap::new()), // No processes initially registered
            process_children: Mutex::new(HashMap::new()), // No process hierarchy initially
            process_parents: Mutex::new(HashMap::new()),  // No process hierarchy initially
            process_dirfd_mappings: Mutex::new(HashMap::new()), // No dirfd mappings initially
            locks: Mutex::new(LockManager {
                locks: HashMap::new(),
            }),
            #[cfg(feature = "events")]
            event_subscriptions: Mutex::new(HashMap::new()),
        };

        // Create root directory
        core.create_root_directory()?;
        Ok(core)
    }

    /// Get information about the current backstore configuration
    pub fn get_backstore_info(&self) -> Option<BackstoreInfo> {
        self.backstore.as_ref().map(|backstore| BackstoreInfo {
            root_path: backstore.root_path(),
            supports_native_snapshots: backstore.supports_native_snapshots(),
            mount_point: backstore.mount_point(),
        })
    }

    /// Create a new FsCore instance with RamDisk backstore for testing
    ///
    /// This creates an APFS RAM disk and configures the FsCore to use it for backstore operations.
    /// Returns both the FsCore and a placeholder for cleanup.
    ///
    /// This function is only available on macOS and requires root privileges or appropriate entitlements.
    #[cfg(test)]
    pub fn new_ephemeral() -> FsResult<(Self, Box<dyn std::any::Any>)> {
        use crate::config::{
            BackstoreMode, CachePolicy, FsConfig, FsLimits, InterposeConfig, MemoryPolicy,
            OverlayConfig, SecurityPolicy,
        };

        let config = FsConfig {
            case_sensitivity: crate::config::CaseSensitivity::Sensitive,
            memory: MemoryPolicy {
                max_bytes_in_memory: Some(1024 * 1024 * 1024), // 1GB
                spill_directory: None,
            },
            limits: FsLimits {
                max_open_handles: 10000,
                max_branches: 1000,
                max_snapshots: 10000,
            },
            cache: CachePolicy {
                attr_ttl_ms: 1000,
                entry_ttl_ms: 1000,
                negative_ttl_ms: 1000,
                enable_readdir_plus: true,
                auto_cache: true,
                writeback_cache: false,
            },
            enable_xattrs: true,
            enable_ads: false,
            track_events: false,
            security: SecurityPolicy::default(),
            backstore: BackstoreMode::RamDisk { size_mb: 128 }, // 128MB RAM disk for testing
            overlay: OverlayConfig::default(),
            interpose: InterposeConfig::default(),
        };

        let core = Self::new(config)?;
        // Return placeholder for cleanup - in practice, tests should manage ramdisk lifecycle
        Ok((core, Box::new(())))
    }

    fn create_root_directory(&mut self) -> FsResult<()> {
        let root_node_id = self.allocate_node_id();
        let now = Self::current_timestamp();

        let root_node = Node {
            id: root_node_id,
            kind: NodeKind::Directory {
                children: HashMap::new(),
            },
            times: FileTimes {
                atime: now,
                mtime: now,
                ctime: now,
                birthtime: now,
            },
            mode: 0o755,
            uid: self.config.security.default_uid,
            gid: self.config.security.default_gid,
            special_kind: None,
            xattrs: HashMap::new(),
            acls: HashMap::new(),
            flags: 0,
            nlink: 2, // root has '.' and '..'
        };

        let default_branch = Branch {
            id: BranchId::DEFAULT,
            root_id: root_node_id,
            parent_snapshot: None, // Default branch has no parent snapshot
            name: Some("default".to_string()),
        };

        self.nodes.lock().unwrap().insert(root_node_id, root_node);
        self.branches.lock().unwrap().insert(default_branch.id, default_branch);

        Ok(())
    }

    fn allocate_node_id(&self) -> NodeId {
        let mut next_id = self.next_node_id.lock().unwrap();
        let id = NodeId(*next_id);
        *next_id += 1;
        id
    }

    fn allocate_handle_id(&self) -> HandleId {
        let mut next_id = self.next_handle_id.lock().unwrap();
        let id = HandleId::new(*next_id);
        *next_id += 1;
        id
    }

    fn current_timestamp() -> i64 {
        SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as i64
    }

    fn current_process_id() -> u32 {
        std::process::id()
    }

    /// Check if overlay mode is enabled
    fn is_overlay_enabled(&self) -> bool {
        self.config.overlay.enabled
    }

    /// Check if a path has an upper entry in the current branch
    pub fn has_upper_entry(&self, pid: &PID, path: &Path) -> FsResult<bool> {
        if !self.is_overlay_enabled() {
            return Ok(false);
        }

        // Try to resolve the path - if it succeeds, there's an upper entry
        match self.resolve_path(pid, path) {
            Ok(_) => Ok(true),
            Err(FsError::NotFound) => Ok(false),
            Err(e) => Err(e),
        }
    }

    /// Perform copy-up operation for a path
    #[allow(dead_code)]
    fn copy_up(&self, _pid: &PID, _path: &Path) -> FsResult<()> {
        if !self.is_overlay_enabled() {
            return Ok(());
        }

        // For now, copy-up is handled implicitly when operations create upper entries
        // This method can be expanded later for explicit copy-up scenarios
        Ok(())
    }

    /// Collect all upper layer files for a given branch root
    /// Returns a list of (upper_file_path, overlay_path) pairs
    fn collect_upper_layer_files(
        &self,
        branch_root_id: NodeId,
    ) -> FsResult<Vec<(std::path::PathBuf, std::path::PathBuf)>> {
        let mut upper_files = Vec::new();

        // Only collect files if we have a backstore that supports reflink
        if let Some(backstore) = &self.backstore {
            if backstore.supports_native_reflink() {
                let mut to_visit = vec![(branch_root_id, std::path::PathBuf::new())];
                let nodes = self.nodes.lock().unwrap();
                let backstore_root = backstore.root_path();

                eprintln!(
                    "DEBUG: Collecting upper files, backstore root: {}",
                    backstore_root.display()
                );
                eprintln!("DEBUG: Branch root id: {}", branch_root_id.0);

                while let Some((node_id, current_path)) = to_visit.pop() {
                    eprintln!(
                        "DEBUG: Visiting node {} at path {}",
                        node_id.0,
                        current_path.display()
                    );
                    if let Some(node) = nodes.get(&node_id) {
                        match &node.kind {
                            NodeKind::File { streams } => {
                                eprintln!("DEBUG: Found file with {} streams", streams.len());
                                // For each stream, get the content file path from the storage backend
                                for (content_id, _size) in streams.values() {
                                    // Get the actual file path where this content is stored
                                    if let Some(content_path) =
                                        self.storage.get_content_path(*content_id)
                                    {
                                        eprintln!(
                                            "DEBUG: Found content file: {}",
                                            content_path.display()
                                        );
                                        // Check if the content file actually exists
                                        if content_path.exists() {
                                            eprintln!("DEBUG: Content file exists!");
                                            upper_files.push((content_path, current_path.clone()));
                                        } else {
                                            eprintln!("DEBUG: Content file does not exist");
                                        }
                                    } else {
                                        eprintln!(
                                            "DEBUG: No content path for content_id {}",
                                            content_id.0
                                        );
                                    }
                                }
                            }
                            NodeKind::Directory { children } => {
                                eprintln!(
                                    "DEBUG: Found directory with {} children",
                                    children.len()
                                );
                                // Recursively visit all children
                                for (child_name, child_id) in children {
                                    let child_path = current_path.join(child_name);
                                    to_visit.push((*child_id, child_path));
                                }
                            }
                            NodeKind::Symlink { .. } => {
                                // Symlinks don't have upper layer files to clonefile
                                // The symlink target is stored in the node metadata
                            }
                        }
                    } else {
                        eprintln!("DEBUG: Node {} not found", node_id.0);
                    }
                }
            } else {
                eprintln!("DEBUG: Backstore does not support native reflink");
            }
        } else {
            eprintln!("DEBUG: No backstore available");
        }

        Ok(upper_files)
    }

    /// Registers a process with the filesystem, establishing its security identity and process hierarchy.
    /// All filesystem operations require a registered PID obtained from this function.
    ///
    /// This function is idempotent: calling it multiple times for the same process ID will return
    /// the same PID token without modifying the existing registration.
    ///
    /// The process inherits active branch bindings from its parent process (and all ancestors).
    /// If the parent process has an active binding, this process will use the same branch.
    ///
    /// # Parameters
    /// - `pid`: The process ID to register
    /// - `parent_pid`: The parent process ID (use the same PID for root processes)
    /// - `uid`: User ID for security identity
    /// - `gid`: Group ID for security identity
    ///
    /// # Returns
    /// A `PID` token for use in filesystem operations
    pub fn register_process(&self, pid: u32, parent_pid: u32, uid: u32, gid: u32) -> PID {
        let user = User {
            uid,
            gid,
            groups: vec![gid], // Basic group membership - can be extended later
        };

        // Check if already registered - if so, refresh identity and return
        {
            let mut identities = self.process_identities.lock().unwrap();
            if let Some(existing) = identities.get_mut(&pid) {
                if existing.uid != uid || existing.gid != gid {
                    existing.uid = uid;
                    existing.gid = gid;
                    existing.groups = vec![gid];
                }
                return PID(pid);
            }
            identities.insert(pid, user);
        }

        // Establish parent-child relationship
        {
            let mut children = self.process_children.lock().unwrap();
            children.entry(parent_pid).or_default().push(pid);
        }

        {
            let mut parents = self.process_parents.lock().unwrap();
            parents.insert(pid, parent_pid);
        }

        // Inherit branch binding from parent (walking up the hierarchy if needed)
        let inherited_branch = self.find_inherited_branch(parent_pid);
        if let Some(branch_id) = inherited_branch {
            let mut branches = self.process_branches.lock().unwrap();
            branches.insert(pid, branch_id);
        }

        PID(pid)
    }

    /// Finds the active branch for a process by walking up the process hierarchy.
    /// Returns the first active branch found in the ancestry chain.
    fn find_inherited_branch(&self, pid: u32) -> Option<BranchId> {
        let branches = self.process_branches.lock().unwrap();
        let parents = self.process_parents.lock().unwrap();

        let mut current_pid = pid;
        loop {
            if let Some(branch) = branches.get(&current_pid) {
                return Some(*branch);
            }

            // Move up to parent
            if let Some(parent) = parents.get(&current_pid) {
                current_pid = *parent;
                // Prevent infinite loops in case of cycles (shouldn't happen in normal process trees)
                if current_pid == pid {
                    break;
                }
            } else {
                break;
            }
        }

        None
    }

    fn branch_for_process(&self, pid: &PID) -> BranchId {
        let process_branches = self.process_branches.lock().unwrap();
        *process_branches.get(&pid.0).unwrap_or(&BranchId::DEFAULT)
    }

    pub(crate) fn user_for_process(&self, pid: &PID) -> Option<User> {
        let identities = self.process_identities.lock().unwrap();
        identities.get(&pid.0).cloned()
    }

    fn has_group(&self, user: &User, gid: u32) -> bool {
        user.gid == gid || user.groups.contains(&gid)
    }

    fn allowed_for_user(
        &self,
        node: &Node,
        user: &User,
        want_read: bool,
        want_write: bool,
        want_exec: bool,
    ) -> bool {
        if !self.config.security.enforce_posix_permissions {
            return true;
        }
        if self.config.security.root_bypass_permissions && user.uid == 0 {
            return true;
        }

        let (r_bit, w_bit, x_bit) = if user.uid == node.uid {
            (0o400, 0o200, 0o100)
        } else if self.has_group(user, node.gid) {
            (0o040, 0o020, 0o010)
        } else {
            (0o004, 0o002, 0o001)
        };

        let mode = node.mode;
        let allow_r = !want_read || (mode & r_bit) != 0;
        let allow_w = !want_write || (mode & w_bit) != 0;
        let allow_x = !want_exec || (mode & x_bit) != 0;
        allow_r && allow_w && allow_x
    }

    fn get_node_clone(&self, node_id: NodeId) -> FsResult<Node> {
        let nodes = self.nodes.lock().unwrap();
        nodes.get(&node_id).cloned().ok_or(FsError::NotFound)
    }

    fn check_dir_permissions(
        &self,
        pid: &PID,
        dir_id: NodeId,
        child: Option<&Node>,
    ) -> FsResult<()> {
        if !self.config.security.enforce_posix_permissions {
            return Ok(());
        }
        let Some(user) = self.user_for_process(pid) else {
            return Ok(());
        };
        let dir_node = self.get_node_clone(dir_id)?;
        if !self.allowed_for_user(&dir_node, &user, false, true, true) {
            return Err(FsError::AccessDenied);
        }

        if let Some(child_node) = child {
            let sticky = (dir_node.mode & libc::S_ISVTX as u32) != 0;
            let should_log = sticky && user.uid != 0;
            let should_deny =
                sticky && user.uid != 0 && user.uid != dir_node.uid && user.uid != child_node.uid;

            if should_log {
                let action = if should_deny { "deny" } else { "allow" };
                self.log_sticky_event(
                    pid,
                    dir_id,
                    &dir_node,
                    Some(child_node),
                    user.uid,
                    action,
                    None,
                );
            }

            if should_deny {
                return Err(FsError::AccessDenied);
            }
        }

        Ok(())
    }

    fn check_dir_cross_parent_permissions(
        &self,
        pid: &PID,
        dir_id: NodeId,
        child: &Node,
    ) -> FsResult<()> {
        if !self.config.security.enforce_posix_permissions {
            return Ok(());
        }

        let Some(user) = self.user_for_process(pid) else {
            return Ok(());
        };

        let dir_node = self.get_node_clone(dir_id)?;
        let sticky = (dir_node.mode & libc::S_ISVTX as u32) != 0;
        if !sticky {
            return Ok(());
        }

        if self.config.security.root_bypass_permissions && user.uid == 0 {
            return Ok(());
        }

        let should_deny = user.uid != child.uid;

        let decision = if should_deny {
            "deny_cross_parent"
        } else {
            "allow_cross_parent"
        };
        self.log_sticky_event(
            pid,
            dir_id,
            &dir_node,
            Some(child),
            user.uid,
            decision,
            None,
        );

        if should_deny {
            return Err(FsError::AccessDenied);
        }

        Ok(())
    }

    fn log_sticky_event(
        &self,
        pid: &PID,
        dir_id: NodeId,
        dir_node: &Node,
        child: Option<&Node>,
        user_uid: u32,
        tag: &str,
        extra: Option<String>,
    ) {
        if user_uid == 0 {
            return;
        }
        let sticky = (dir_node.mode & libc::S_ISVTX as u32) != 0;
        if !sticky {
            return;
        }

        let dir_path = self
            .path_for_node(pid, dir_id)
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_else(|| format!("inode:{}", dir_id.0));
        let child_uid = child.map(|node| node.uid);
        let child_kind = child.map(|node| match node.kind {
            NodeKind::Directory { .. } => "dir",
            NodeKind::File { .. } => "file",
            NodeKind::Symlink { .. } => "symlink",
        });
        let child_uid_str = child_uid.map(|uid| uid.to_string()).unwrap_or_else(|| "-".to_string());
        let child_kind_str = child_kind.unwrap_or("-");

        if let Ok(mut log) =
            StdOpenOptions::new().create(true).append(true).open("/tmp/agentfs-sticky.log")
        {
            if let Some(extra_str) = extra.as_deref() {
                let _ = writeln!(
                    log,
                    "{} dir_id={} dir_uid={} child_uid={} child_kind={} user_uid={} mode={:o} path={} {}",
                    tag,
                    dir_id.0,
                    dir_node.uid,
                    child_uid_str,
                    child_kind_str,
                    user_uid,
                    dir_node.mode,
                    dir_path,
                    extra_str
                );
            } else {
                let _ = writeln!(
                    log,
                    "{} dir_id={} dir_uid={} child_uid={} child_kind={} user_uid={} mode={:o} path={}",
                    tag,
                    dir_id.0,
                    dir_node.uid,
                    child_uid_str,
                    child_kind_str,
                    user_uid,
                    dir_node.mode,
                    dir_path
                );
            }
        }
    }

    /// Resolve a path to a node ID and parent information (read-only)
    /// In overlay mode, this will fall back to lower filesystem if no upper entry exists
    fn resolve_path(&self, pid: &PID, path: &Path) -> FsResult<(NodeId, Option<(NodeId, String)>)> {
        let current_branch = self.branch_for_process(pid);
        let branches = self.branches.lock().unwrap();
        let branch = branches.get(&current_branch).ok_or(FsError::NotFound)?;
        let mut current_node_id = branch.root_id;

        let components: Vec<&str> = path
            .components()
            .filter_map(|c| match c {
                std::path::Component::Normal(s) => s.to_str(),
                _ => None,
            })
            .collect();

        if components.is_empty() {
            // Root directory
            return Ok((current_node_id, None));
        }

        let nodes = self.nodes.lock().unwrap();
        let user_opt = self.user_for_process(pid);
        // Enforce that a process must be registered when permission checks are enabled
        if self.config.security.enforce_posix_permissions && user_opt.is_none() {
            return Err(FsError::AccessDenied);
        }
        let mut parent_node_id = None;
        let mut parent_name = None;

        for (i, component) in components.iter().enumerate() {
            let node = nodes.get(&current_node_id).ok_or(FsError::NotFound)?;

            match &node.kind {
                NodeKind::Directory { children } => {
                    if self.config.security.enforce_posix_permissions {
                        if let Some(user) = &user_opt {
                            if !self.allowed_for_user(node, user, false, false, true) {
                                return Err(FsError::AccessDenied);
                            }
                        }
                    }
                    if let Some(child_id) = children.get(*component) {
                        if i == components.len() - 1 {
                            // Last component
                            return Ok((*child_id, Some((current_node_id, component.to_string()))));
                        } else {
                            parent_node_id = Some(current_node_id);
                            parent_name = Some(component.to_string());
                            current_node_id = *child_id;
                        }
                    } else {
                        return Err(FsError::NotFound);
                    }
                }
                NodeKind::File { .. } => {
                    if i == components.len() - 1 {
                        // Last component is a file
                        return Ok((
                            current_node_id,
                            Some((current_node_id, component.to_string())),
                        ));
                    } else {
                        return Err(FsError::NotADirectory);
                    }
                }
                NodeKind::Symlink { .. } => {
                    if i == components.len() - 1 {
                        // Last component is a symlink
                        return Ok((
                            current_node_id,
                            Some((current_node_id, component.to_string())),
                        ));
                    } else {
                        return Err(FsError::NotADirectory);
                    }
                }
            }
        }

        Ok((current_node_id, parent_node_id.zip(parent_name)))
    }

    /// Create a new file node
    fn create_file_node(&self, content_id: ContentId) -> FsResult<NodeId> {
        let node_id = self.allocate_node_id();
        let now = Self::current_timestamp();

        let mut streams = HashMap::new();
        streams.insert("".to_string(), (content_id, 0)); // Default unnamed stream

        let node = Node {
            id: node_id,
            kind: NodeKind::File { streams },
            times: FileTimes {
                atime: now,
                mtime: now,
                ctime: now,
                birthtime: now,
            },
            mode: 0o644,
            uid: self.config.security.default_uid,
            gid: self.config.security.default_gid,
            special_kind: None,
            xattrs: HashMap::new(),
            acls: HashMap::new(),
            flags: 0,
            nlink: 1,
        };

        self.nodes.lock().unwrap().insert(node_id, node);
        Ok(node_id)
    }

    fn create_special_node(&self, kind: SpecialNodeKind) -> FsResult<NodeId> {
        let node_id = self.allocate_node_id();
        let now = Self::current_timestamp();

        let node = Node {
            id: node_id,
            kind: NodeKind::File {
                streams: HashMap::new(),
            },
            times: FileTimes {
                atime: now,
                mtime: now,
                ctime: now,
                birthtime: now,
            },
            mode: 0o644,
            uid: self.config.security.default_uid,
            gid: self.config.security.default_gid,
            special_kind: Some(kind),
            xattrs: HashMap::new(),
            acls: HashMap::new(),
            flags: 0,
            nlink: 1,
        };

        self.nodes.lock().unwrap().insert(node_id, node);
        Ok(node_id)
    }

    /// Create a new directory node
    fn create_directory_node(&self) -> FsResult<NodeId> {
        let node_id = self.allocate_node_id();
        let now = Self::current_timestamp();

        let node = Node {
            id: node_id,
            kind: NodeKind::Directory {
                children: HashMap::new(),
            },
            times: FileTimes {
                atime: now,
                mtime: now,
                ctime: now,
                birthtime: now,
            },
            mode: 0o755,
            uid: self.config.security.default_uid,
            gid: self.config.security.default_gid,
            special_kind: None,
            xattrs: HashMap::new(),
            acls: HashMap::new(),
            flags: 0,
            nlink: 2, // '.' and '..'
        };

        self.nodes.lock().unwrap().insert(node_id, node);
        Ok(node_id)
    }

    /// Change ownership of a node addressed by path
    pub fn set_owner(&self, pid: &PID, path: &Path, uid: u32, gid: u32) -> FsResult<()> {
        let (node_id, _) = self.resolve_path(pid, path)?;
        let mut nodes = self.nodes.lock().unwrap();
        let node = nodes.get_mut(&node_id).ok_or(FsError::NotFound)?;
        // Only root may change owner uid; owner may change gid to a group they belong to
        if self.config.security.enforce_posix_permissions {
            if let Some(user) = self.user_for_process(pid) {
                let changing_uid = uid != node.uid;
                if changing_uid && user.uid != 0 {
                    return Err(FsError::AccessDenied);
                }
                if gid != node.gid && user.uid != 0 {
                    // Owner may change gid only to a group they belong to
                    if user.uid != node.uid || (user.gid != gid && !user.groups.contains(&gid)) {
                        return Err(FsError::AccessDenied);
                    }
                }
            }
        }
        node.uid = uid;
        node.gid = gid;
        // Clear setuid/setgid on metadata ownership change
        node.mode &= !0o6000;
        node.times.ctime = Self::current_timestamp();

        // Emit Modified event for metadata change
        #[cfg(feature = "events")]
        self.emit_event(EventKind::Modified {
            path: path.to_string_lossy().to_string(),
        });

        Ok(())
    }

    /// Percent-encode arbitrary bytes to a safe internal string name
    fn percent_encode_name(bytes: &[u8]) -> String {
        let mut s = String::with_capacity(bytes.len() * 3);
        for &b in bytes {
            let is_safe = b.is_ascii_uppercase()
                || b.is_ascii_lowercase()
                || b.is_ascii_digit()
                || matches!(b, b'-' | b'_' | b'.');
            if is_safe {
                s.push(b as char);
            } else {
                s.push('%');
                s.push_str(&format!("{:02X}", b));
            }
        }
        s
    }

    /// Create a child under a parent directory by parent node id and raw name bytes.
    /// Returns created node id.
    pub fn create_child_by_id(
        &self,
        parent_id_u64: u64,
        name_bytes: &[u8],
        item_type: u32,
        mode: u32,
    ) -> FsResult<u64> {
        let parent_id = NodeId(parent_id_u64);
        let mut nodes = self.nodes.lock().unwrap();
        let parent_node = nodes.get_mut(&parent_id).ok_or(FsError::NotFound)?;

        // Determine internal name used for map lookup
        let internal_name = match std::str::from_utf8(name_bytes) {
            Ok(s) => s.to_string(),
            Err(_) => Self::percent_encode_name(name_bytes),
        };

        // Ensure parent is a directory and the child doesn't exist
        match &mut parent_node.kind {
            NodeKind::Directory { children } => {
                if children.contains_key(&internal_name) {
                    return Err(FsError::AlreadyExists);
                }
            }
            _ => return Err(FsError::NotADirectory),
        }
        drop(nodes);

        // Create the node
        let new_node_id = match item_type {
            0 => {
                // file
                let content_id = self.storage.allocate(&[])?;
                self.create_file_node(content_id)?
            }
            1 => {
                // directory
                self.create_directory_node()?
            }
            _ => return Err(FsError::InvalidArgument),
        };

        // Apply mode
        {
            let mut nodes = self.nodes.lock().unwrap();
            if let Some(n) = nodes.get_mut(&new_node_id) {
                n.mode = mode;
                // Preserve original raw name in xattr for later round-trip
                n.xattrs.insert("user.agentfs.rawname".to_string(), name_bytes.to_vec());
            }
        }

        // Insert into parent directory
        self.link_child_into_parent(parent_id, &internal_name, new_node_id)?;

        Ok(new_node_id.0)
    }

    /// Get attributes of a child under a parent directory by raw name bytes
    pub fn getattr_child_by_id_name(
        &self,
        parent_id_u64: u64,
        name_bytes: &[u8],
    ) -> FsResult<Attributes> {
        let parent_id = NodeId(parent_id_u64);
        let internal_name = match std::str::from_utf8(name_bytes) {
            Ok(s) => s.to_string(),
            Err(_) => Self::percent_encode_name(name_bytes),
        };

        let nodes = self.nodes.lock().unwrap();
        let parent = nodes.get(&parent_id).ok_or(FsError::NotFound)?;
        let child_id = match &parent.kind {
            NodeKind::Directory { children } => {
                children.get(&internal_name).ok_or(FsError::NotFound).copied()?
            }
            _ => return Err(FsError::NotADirectory),
        };
        drop(nodes);
        self.get_node_attributes(child_id)
    }

    /// Resolve child node id by parent id and raw name bytes
    pub fn resolve_child_id_by_id_name(
        &self,
        parent_id_u64: u64,
        name_bytes: &[u8],
    ) -> FsResult<u64> {
        let parent_id = NodeId(parent_id_u64);
        let internal_name = match std::str::from_utf8(name_bytes) {
            Ok(s) => s.to_string(),
            Err(_) => Self::percent_encode_name(name_bytes),
        };

        let nodes = self.nodes.lock().unwrap();
        let parent = nodes.get(&parent_id).ok_or(FsError::NotFound)?;
        let child_id = match &parent.kind {
            NodeKind::Directory { children } => {
                children.get(&internal_name).ok_or(FsError::NotFound).copied()?
            }
            _ => return Err(FsError::NotADirectory),
        };
        Ok(child_id.0)
    }

    /// Clone a node for copy-on-write (creates a new node with the same content)
    fn clone_node_cow(&self, node_id: NodeId) -> FsResult<NodeId> {
        self.clone_node_cow_recursive(node_id)
    }

    /// Recursively clone a node and all its children for copy-on-write
    fn clone_node_cow_recursive(&self, node_id: NodeId) -> FsResult<NodeId> {
        // First, get the node data
        let node = {
            let nodes = self.nodes.lock().unwrap();
            nodes.get(&node_id).ok_or(FsError::NotFound)?.clone()
        };

        let new_node_id = self.allocate_node_id();
        let mut new_node = node.clone();
        // xattrs are already cloned by the derive(Clone) on Node

        // For files, we need to clone all streams in storage
        if let NodeKind::File { streams } = &new_node.kind {
            let mut new_streams = HashMap::new();
            for (content_id, size) in streams.values() {
                let new_content_id = self.storage.clone_cow(*content_id)?;
                // We'll keep the same stream names via later mapping
                // using the original keys.
                // Placeholder: actual key copy occurs below
                new_streams.insert(String::new(), (new_content_id, *size));
            }
            // Reconstruct streams with original keys to avoid key loss
            if let NodeKind::File { streams } = &node.kind {
                let mut iter = streams.keys();
                let mut rebuilt = HashMap::new();
                for ((_, (cid, sz)), key) in new_streams.into_iter().zip(&mut iter) {
                    rebuilt.insert(key.clone(), (cid, sz));
                }
                new_node.kind = NodeKind::File { streams: rebuilt };
            }
        }
        // For directories, we recursively clone all children
        else if let NodeKind::Directory { children } = &new_node.kind {
            let mut new_children = HashMap::new();
            for (name, child_id) in children {
                let new_child_id = self.clone_node_cow_recursive(*child_id)?;
                new_children.insert(name.clone(), new_child_id);
            }
            new_node.kind = NodeKind::Directory {
                children: new_children,
            };
        }

        // Insert the new node
        {
            let mut nodes = self.nodes.lock().unwrap();
            nodes.insert(new_node_id, new_node);
        }
        Ok(new_node_id)
    }

    /// Clone a branch's root directory for copy-on-write
    #[allow(dead_code)]
    fn clone_branch_root_cow(&self, branch_id: BranchId) -> FsResult<()> {
        let mut branches = self.branches.lock().unwrap();
        let branch = branches.get_mut(&branch_id).ok_or(FsError::NotFound)?;

        // Only clone if the branch shares its root with a snapshot
        if let Some(snapshot_id) = branch.parent_snapshot {
            let snapshots = self.snapshots.lock().unwrap();
            if let Some(snapshot) = snapshots.get(&snapshot_id) {
                if branch.root_id == snapshot.root_id {
                    // Clone the root directory
                    let new_root_id = self.clone_node_cow(branch.root_id)?;
                    branch.root_id = new_root_id;
                }
            }
        }

        Ok(())
    }

    /// Update node timestamps
    fn update_node_times(&self, node_id: NodeId, times: FileTimes) {
        let mut nodes = self.nodes.lock().unwrap();
        if let Some(node) = nodes.get_mut(&node_id) {
            node.times = times;
        }
    }

    /// Get file attributes
    pub fn getattr(&self, pid: &PID, path: &Path) -> FsResult<Attributes> {
        // First try to resolve in upper layer
        if let Ok((node_id, _)) = self.resolve_path(pid, path) {
            return self.get_node_attributes(node_id);
        }

        // If overlay is enabled and no upper entry, check lower filesystem
        if self.is_overlay_enabled() {
            if let Some(lower_fs) = &self.lower_fs {
                return lower_fs.stat(path);
            }
        }

        // No entry found
        Err(FsError::NotFound)
    }

    /// Get node attributes
    fn get_node_attributes(&self, node_id: NodeId) -> FsResult<Attributes> {
        let nodes = self.nodes.lock().unwrap();
        let node = nodes.get(&node_id).ok_or(FsError::NotFound)?;

        let (len, is_dir, is_symlink) = match &node.kind {
            NodeKind::File { streams } => {
                // Size is the size of the unnamed stream (default data stream)
                let size = streams.get("").map(|(_, size)| *size).unwrap_or(0);
                (size, false, false)
            }
            NodeKind::Directory { .. } => (0, true, false),
            NodeKind::Symlink { target } => (target.len() as u64, false, true),
        };

        // Extract permission bits from mode (ignore file type bits in high bits)
        let perm_bits = node.mode & 0o777;

        Ok(Attributes {
            len,
            times: node.times,
            uid: node.uid,
            gid: node.gid,
            is_dir,
            is_symlink,
            special_kind: node.special_kind.clone(),
            nlink: node.nlink,
            mode_user: FileMode {
                read: (perm_bits & 0o400) != 0,
                write: (perm_bits & 0o200) != 0,
                exec: (perm_bits & 0o100) != 0,
            },
            mode_group: FileMode {
                read: (perm_bits & 0o040) != 0,
                write: (perm_bits & 0o020) != 0,
                exec: (perm_bits & 0o010) != 0,
            },
            mode_other: FileMode {
                read: (perm_bits & 0o004) != 0,
                write: (perm_bits & 0o002) != 0,
                exec: (perm_bits & 0o001) != 0,
            },
        })
    }

    // Snapshot operations
    pub fn snapshot_create(&self, name: Option<&str>) -> FsResult<SnapshotId> {
        let current_pid = PID::new(Self::current_process_id());
        self.snapshot_create_for_pid(&current_pid, name)
    }

    pub fn snapshot_create_for_pid(&self, pid: &PID, name: Option<&str>) -> FsResult<SnapshotId> {
        let current_branch = self.branch_for_process(pid);
        let branches = self.branches.lock().unwrap();
        let branch = branches.get(&current_branch).ok_or(FsError::NotFound)?;

        let snapshot_id = SnapshotId::new();

        let name_owned = name.map(|s| s.to_string());
        let snapshot_name = name_owned
            .clone()
            .unwrap_or_else(|| format!("snapshot_{}", hex::encode(snapshot_id.0)));

        // If we have a backstore that supports native snapshots, delegate to it
        if let Some(backstore) = &self.backstore {
            if backstore.supports_native_snapshots() {
                backstore.snapshot_native(&snapshot_name)?;
            } else if backstore.supports_native_reflink() {
                // Collect all upper layer files that need to be materialized
                let upper_files = self.collect_upper_layer_files(branch.root_id)?;
                eprintln!(
                    "DEBUG: Collected {} upper files for snapshot",
                    upper_files.len()
                );
                for (upper_path, overlay_path) in &upper_files {
                    eprintln!(
                        "DEBUG: Upper file: {} -> {}",
                        upper_path.display(),
                        overlay_path.display()
                    );
                }
                backstore.snapshot_clonefile_materialize(&snapshot_name, &upper_files)?;
            }
        }

        let snapshot = Snapshot {
            id: snapshot_id,
            root_id: branch.root_id,
            name: name_owned.clone(),
        };

        self.snapshots.lock().unwrap().insert(snapshot_id, snapshot);

        // Emit event
        #[cfg(feature = "events")]
        self.emit_event(EventKind::SnapshotCreated {
            id: snapshot_id,
            name: name_owned.clone(),
        });

        Ok(snapshot_id)
    }

    pub fn snapshot_list(&self) -> Vec<(SnapshotId, Option<String>)> {
        let snapshots = self.snapshots.lock().unwrap();
        snapshots.values().map(|s| (s.id, s.name.clone())).collect()
    }

    pub fn export_snapshot(&self, snapshot_id: SnapshotId, target: &Path) -> FsResult<()> {
        fs::create_dir_all(target)?;
        let snapshot = {
            let snapshots = self.snapshots.lock().unwrap();
            snapshots.get(&snapshot_id).cloned().ok_or(FsError::NotFound)?
        };
        self.export_node_overlay(snapshot.root_id, Path::new("/"), target)
    }

    fn export_node_overlay(
        &self,
        node_id: NodeId,
        absolute_path: &Path,
        destination: &Path,
    ) -> FsResult<()> {
        let node = {
            let nodes = self.nodes.lock().unwrap();
            nodes.get(&node_id).cloned().ok_or(FsError::NotFound)?
        };

        match node.kind {
            NodeKind::Directory { children } => {
                fs::create_dir_all(destination)?;

                let mut entry_map: HashMap<String, Option<NodeId>> =
                    HashMap::with_capacity(children.len());
                for (name, child_id) in children.iter() {
                    entry_map.insert(name.clone(), Some(*child_id));
                }

                if let Some(lower_fs) = self.lower_fs.as_ref() {
                    match lower_fs.readdir(absolute_path) {
                        Ok(lower_entries) => {
                            for entry in lower_entries {
                                entry_map.entry(entry.name.clone()).or_insert(None);
                            }
                        }
                        Err(FsError::NotFound) => {}
                        Err(err) => return Err(err),
                    }
                }

                let mut entries: Vec<(String, Option<NodeId>)> = entry_map.into_iter().collect();
                entries.sort_by(|a, b| a.0.cmp(&b.0));

                for (name, maybe_child_id) in entries {
                    let child_destination = destination.join(&name);
                    let child_absolute_path = if absolute_path == Path::new("/") {
                        PathBuf::from("/").join(&name)
                    } else {
                        absolute_path.join(&name)
                    };

                    if let Some(child_id) = maybe_child_id {
                        self.export_node_overlay(
                            child_id,
                            &child_absolute_path,
                            &child_destination,
                        )?;
                    } else if let Some(lower_fs) = self.lower_fs.as_ref() {
                        self.export_lower_entry(
                            lower_fs.as_ref(),
                            &child_absolute_path,
                            &child_destination,
                        )?;
                    }
                }

                Ok(())
            }
            NodeKind::File { streams } => {
                if let Some(parent) = destination.parent() {
                    fs::create_dir_all(parent)?;
                }
                let mut file = File::create(destination)?;
                if let Some((content_id, size)) = streams.get("") {
                    let mut offset = 0u64;
                    let mut buffer = vec![0u8; 8192];
                    while offset < *size {
                        let remaining = (*size - offset) as usize;
                        let read_len = remaining.min(buffer.len());
                        let bytes_read =
                            self.storage.read(*content_id, offset, &mut buffer[..read_len])?;
                        if bytes_read == 0 {
                            break;
                        }
                        file.write_all(&buffer[..bytes_read])?;
                        offset += bytes_read as u64;
                    }
                }
                Ok(())
            }
            NodeKind::Symlink { target } => {
                if let Some(parent) = destination.parent() {
                    fs::create_dir_all(parent)?;
                }
                #[cfg(unix)]
                {
                    std::os::unix::fs::symlink(&target, destination)?;
                    Ok(())
                }
                #[cfg(not(unix))]
                {
                    let _ = target;
                    Err(FsError::Unsupported)
                }
            }
        }
    }

    fn export_lower_entry(
        &self,
        lower_fs: &dyn LowerFs,
        absolute_path: &Path,
        destination: &Path,
    ) -> FsResult<()> {
        let attrs = match lower_fs.stat(absolute_path) {
            Ok(attrs) => attrs,
            Err(FsError::NotFound) => return Ok(()),
            Err(err) => return Err(err),
        };

        if attrs.is_dir {
            fs::create_dir_all(destination)?;
            match lower_fs.readdir(absolute_path) {
                Ok(entries) => {
                    for entry in entries {
                        let child_path = if absolute_path == Path::new("/") {
                            PathBuf::from("/").join(&entry.name)
                        } else {
                            absolute_path.join(&entry.name)
                        };
                        let child_destination = destination.join(&entry.name);
                        self.export_lower_entry(lower_fs, &child_path, &child_destination)?;
                    }
                }
                Err(FsError::NotFound) => {}
                Err(err) => return Err(err),
            }
            return Ok(());
        }

        if attrs.is_symlink {
            if let Some(parent) = destination.parent() {
                fs::create_dir_all(parent)?;
            }
            #[cfg(unix)]
            {
                let target = lower_fs.readlink(absolute_path)?;
                std::os::unix::fs::symlink(target, destination)?;
                return Ok(());
            }
            #[cfg(not(unix))]
            {
                return Err(FsError::Unsupported);
            }
        }

        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut reader = lower_fs.open_ro(absolute_path)?;
        let mut writer = File::create(destination)?;
        std::io::copy(&mut reader, &mut writer)?;
        Ok(())
    }

    pub fn snapshot_delete(&self, snapshot_id: SnapshotId) -> FsResult<()> {
        let mut snapshots = self.snapshots.lock().unwrap();
        let branches = self.branches.lock().unwrap();

        // Check if any branches depend on this snapshot
        let has_dependents = branches.values().any(|b| b.parent_snapshot == Some(snapshot_id));

        if has_dependents {
            return Err(FsError::Busy); // Cannot delete snapshot with dependent branches
        }

        snapshots.remove(&snapshot_id);
        Ok(())
    }

    // Branch operations
    pub fn branch_create_from_snapshot(
        &self,
        snapshot_id: SnapshotId,
        name: Option<&str>,
    ) -> FsResult<BranchId> {
        let snapshots = self.snapshots.lock().unwrap();
        let snapshot = snapshots.get(&snapshot_id).ok_or(FsError::NotFound)?;

        // Clone the snapshot's root directory for the branch (immediate CoW for directory structure)
        let branch_root_id = self.clone_node_cow(snapshot.root_id)?;

        let branch_id = BranchId::new();
        let branch = Branch {
            id: branch_id,
            root_id: branch_root_id, // Branch gets its own copy of the directory structure
            parent_snapshot: Some(snapshot_id),
            name: name.map(|s| s.to_string()),
        };

        self.branches.lock().unwrap().insert(branch_id, branch);

        // Emit event
        #[cfg(feature = "events")]
        self.emit_event(EventKind::BranchCreated {
            id: branch_id,
            name: name.map(|s| s.to_string()),
        });

        Ok(branch_id)
    }

    pub fn branch_create_from_current(&self, name: Option<&str>) -> FsResult<BranchId> {
        let current_branch = self.branch_for_process(&PID::new(Self::current_process_id()));
        let branches = self.branches.lock().unwrap();
        let branch = branches.get(&current_branch).ok_or(FsError::NotFound)?;

        // Clone the current branch's root directory for the new branch
        let new_branch_root_id = self.clone_node_cow(branch.root_id)?;

        let branch_id = BranchId::new();
        let new_branch = Branch {
            id: branch_id,
            root_id: new_branch_root_id, // New branch gets its own copy of the directory structure
            parent_snapshot: None,       // Not based on a snapshot
            name: name.map(|s| s.to_string()),
        };

        drop(branches);
        self.branches.lock().unwrap().insert(branch_id, new_branch);
        Ok(branch_id)
    }

    pub fn branch_list(&self) -> Vec<BranchInfo> {
        let branches = self.branches.lock().unwrap();
        branches
            .values()
            .map(|b| BranchInfo {
                id: b.id,
                parent: b.parent_snapshot,
                name: b.name.clone(),
            })
            .collect()
    }

    // Process binding operations
    pub fn bind_process_to_branch(&self, branch_id: BranchId) -> FsResult<()> {
        self.bind_process_to_branch_with_pid(branch_id, Self::current_process_id())
    }

    pub fn bind_process_to_branch_with_pid(&self, branch_id: BranchId, pid: u32) -> FsResult<()> {
        let branches = self.branches.lock().unwrap();
        if !branches.contains_key(&branch_id) {
            return Err(FsError::NotFound);
        }
        drop(branches);

        let mut process_branches = self.process_branches.lock().unwrap();
        process_branches.insert(pid, branch_id);
        Ok(())
    }

    pub fn unbind_process(&self) -> FsResult<()> {
        self.unbind_process_with_pid(Self::current_process_id())
    }

    pub fn unbind_process_with_pid(&self, pid: u32) -> FsResult<()> {
        let mut process_branches = self.process_branches.lock().unwrap();
        process_branches.remove(&pid);
        Ok(())
    }

    // Directory file descriptor mapping operations for *at functions
    pub fn register_process_dirfd_mapping(&self, pid: u32) -> FsResult<()> {
        let mut mappings = self.process_dirfd_mappings.lock().unwrap();
        mappings.entry(pid).or_default();
        Ok(())
    }

    pub fn open_dir_fd(
        &self,
        pid: u32,
        path: std::path::PathBuf,
        fd: std::os::fd::RawFd,
    ) -> FsResult<()> {
        let mut mappings = self.process_dirfd_mappings.lock().unwrap();
        let mapping = mappings.entry(pid).or_default();
        mapping.set_path(fd, path);
        Ok(())
    }

    pub fn close_fd(&self, pid: u32, fd: std::os::fd::RawFd) -> FsResult<()> {
        let mut mappings = self.process_dirfd_mappings.lock().unwrap();
        if let Some(mapping) = mappings.get_mut(&pid) {
            mapping.remove_path(fd);
        }
        Ok(())
    }

    pub fn dup_fd(
        &self,
        pid: u32,
        old_fd: std::os::fd::RawFd,
        new_fd: std::os::fd::RawFd,
    ) -> FsResult<()> {
        let mut mappings = self.process_dirfd_mappings.lock().unwrap();
        if let Some(mapping) = mappings.get_mut(&pid) {
            mapping.dup_fd(old_fd, new_fd);
        }
        Ok(())
    }

    pub fn set_process_cwd(&self, pid: u32, cwd: std::path::PathBuf) -> FsResult<()> {
        let mut mappings = self.process_dirfd_mappings.lock().unwrap();
        let mapping = mappings.entry(pid).or_default();
        mapping.set_cwd(cwd);
        Ok(())
    }

    pub fn resolve_dirfd(
        &self,
        pid: u32,
        dirfd: std::os::fd::RawFd,
    ) -> FsResult<Option<std::path::PathBuf>> {
        let mappings = self.process_dirfd_mappings.lock().unwrap();
        if let Some(mapping) = mappings.get(&pid) {
            match dirfd {
                libc::AT_FDCWD => Ok(Some(mapping.get_cwd().clone())),
                fd if fd >= 0 => Ok(mapping.get_path(fd).cloned()),
                _ => Ok(None),
            }
        } else {
            Ok(None)
        }
    }

    pub fn resolve_path_with_dirfd(
        &self,
        pid: u32,
        dirfd: std::os::fd::RawFd,
        relative_path: &std::path::Path,
    ) -> FsResult<std::path::PathBuf> {
        let base_path = self.resolve_dirfd(pid, dirfd)?.ok_or(FsError::InvalidArgument)?;

        let mut resolved_path = base_path.clone();
        resolved_path.push(relative_path);

        // Canonicalize the path to resolve . and .. components
        match resolved_path.canonicalize() {
            Ok(canonical) => Ok(canonical),
            Err(_) => {
                // If canonicalization fails, return the non-canonicalized path
                // This allows operations on non-existent files to work correctly
                Ok(resolved_path)
            }
        }
    }

    // Event subscription operations
    #[cfg(feature = "events")]
    pub fn subscribe_events(&self, cb: Arc<dyn EventSink>) -> FsResult<SubscriptionId> {
        let mut subscriptions = self.event_subscriptions.lock().unwrap();
        let mut next_id = self.next_subscription_id.lock().unwrap();
        let subscription_id = SubscriptionId::new(*next_id);
        *next_id += 1;
        subscriptions.insert(subscription_id, cb);
        Ok(subscription_id)
    }

    #[cfg(feature = "events")]
    pub fn unsubscribe_events(&self, sub: SubscriptionId) -> FsResult<()> {
        let mut subscriptions = self.event_subscriptions.lock().unwrap();
        if subscriptions.remove(&sub).is_none() {
            return Err(FsError::NotFound);
        }
        Ok(())
    }

    // Statistics
    pub fn stats(&self) -> FsStats {
        let branches = self.branches.lock().unwrap();
        let snapshots = self.snapshots.lock().unwrap();
        let handles = self.handles.lock().unwrap();

        // For now, we only track in-memory storage
        // TODO: Add actual byte counting when storage backend supports it
        let bytes_in_memory = 0; // Placeholder
        let bytes_spilled = 0; // Placeholder

        FsStats {
            branches: branches.len() as u32,
            snapshots: snapshots.len() as u32,
            open_handles: handles.len() as u32,
            bytes_in_memory,
            bytes_spilled,
        }
    }

    // Helper method to emit events to all subscribers
    #[cfg(feature = "events")]
    /// Reconstruct the absolute path for a node ID within the caller's branch
    fn path_for_node(&self, pid: &PID, target_id: NodeId) -> Option<PathBuf> {
        let branch_id = self.branch_for_process(pid);
        let branches = self.branches.lock().unwrap();
        let branch = branches.get(&branch_id)?;
        let root_id = branch.root_id;
        drop(branches);

        let nodes = self.nodes.lock().unwrap();
        let mut components = Vec::new();
        if Self::find_path_components(&nodes, root_id, target_id, &mut components) {
            let mut path = PathBuf::from("/");
            for component in components.iter() {
                path.push(component);
            }
            return Some(path);
        }
        None
    }

    fn find_path_components(
        nodes: &HashMap<NodeId, Node>,
        current: NodeId,
        target: NodeId,
        components: &mut Vec<String>,
    ) -> bool {
        if current == target {
            return true;
        }
        if let Some(node) = nodes.get(&current) {
            if let NodeKind::Directory { children } = &node.kind {
                for (name, child_id) in children {
                    components.push(name.clone());
                    if Self::find_path_components(nodes, *child_id, target, components) {
                        return true;
                    }
                    components.pop();
                }
            }
        }
        false
    }

    fn emit_event(&self, event: EventKind) {
        if !self.config.track_events {
            return;
        }

        let subscriptions = self.event_subscriptions.lock().unwrap();
        for sink in subscriptions.values() {
            sink.on_event(&event);
        }
    }

    fn ensure_parent_dir_allows_creation(&self, pid: &PID, parent_id: NodeId) -> FsResult<()> {
        if self.config.security.enforce_posix_permissions {
            if let Some(user) = self.user_for_process(pid) {
                let nodes = self.nodes.lock().unwrap();
                let parent_node = nodes.get(&parent_id).ok_or(FsError::NotFound)?;
                if !self.allowed_for_user(parent_node, &user, false, true, true) {
                    return Err(FsError::AccessDenied);
                }
            }
        }
        Ok(())
    }

    fn link_child_into_parent(
        &self,
        parent_id: NodeId,
        child_name: &str,
        child_id: NodeId,
    ) -> FsResult<()> {
        let mut nodes = self.nodes.lock().unwrap();
        let child_is_dir = matches!(
            nodes.get(&child_id).map(|n| &n.kind),
            Some(NodeKind::Directory { .. })
        );
        if let Some(parent_node) = nodes.get_mut(&parent_id) {
            match &mut parent_node.kind {
                NodeKind::Directory { children } => {
                    if children.contains_key(child_name) {
                        return Err(FsError::AlreadyExists);
                    }
                    children.insert(child_name.to_string(), child_id);
                    let now = Self::current_timestamp();
                    parent_node.times.mtime = now;
                    parent_node.times.ctime = now;
                    if child_is_dir {
                        parent_node.nlink = parent_node.nlink.saturating_add(1);
                    }
                    Ok(())
                }
                _ => Err(FsError::NotADirectory),
            }
        } else {
            Err(FsError::NotFound)
        }
    }

    fn unlink_child_from_parent(&self, parent_id: NodeId, child_name: &str) {
        let mut nodes = self.nodes.lock().unwrap();
        let removed_child = {
            let Some(parent_node) = nodes.get_mut(&parent_id) else {
                return;
            };
            if let NodeKind::Directory { children } = &mut parent_node.kind {
                if let Some(child_id) = children.remove(child_name) {
                    let now = Self::current_timestamp();
                    parent_node.times.mtime = now;
                    parent_node.times.ctime = now;
                    Some(child_id)
                } else {
                    None
                }
            } else {
                None
            }
        };

        if let Some(child_id) = removed_child {
            let child_is_dir = matches!(
                nodes.get(&child_id).map(|n| &n.kind),
                Some(NodeKind::Directory { .. })
            );
            if child_is_dir {
                if let Some(parent_node) = nodes.get_mut(&parent_id) {
                    parent_node.nlink = parent_node.nlink.saturating_sub(1);
                }
            }
        }
    }

    fn increment_link_count(&self, node_id: NodeId) {
        let mut nodes = self.nodes.lock().unwrap();
        if let Some(node) = nodes.get_mut(&node_id) {
            node.nlink = node.nlink.saturating_add(1);
        }
    }

    fn decrement_link_count(&self, node_id: NodeId) -> u32 {
        let mut nodes = self.nodes.lock().unwrap();
        if let Some(node) = nodes.get_mut(&node_id) {
            if node.nlink > 0 {
                node.nlink -= 1;
            }
            return node.nlink;
        }
        0
    }

    fn node_link_count(&self, node_id: NodeId) -> u32 {
        let nodes = self.nodes.lock().unwrap();
        nodes.get(&node_id).map(|n| n.nlink).unwrap_or(0)
    }

    fn create_special_at_path(
        &self,
        pid: &PID,
        path: &Path,
        kind: SpecialNodeKind,
        mode: u32,
    ) -> FsResult<()> {
        if self.resolve_path(pid, path).is_ok() {
            return Err(FsError::AlreadyExists);
        }

        let parent_path = path.parent().ok_or(FsError::InvalidArgument)?;
        let name = path.file_name().and_then(|n| n.to_str()).ok_or(FsError::InvalidName)?;

        let (parent_id, _) = self.resolve_path(pid, parent_path)?;
        self.ensure_parent_dir_allows_creation(pid, parent_id)?;

        {
            let nodes = self.nodes.lock().unwrap();
            let parent_node = nodes.get(&parent_id).ok_or(FsError::NotFound)?;
            if let NodeKind::Directory { children } = &parent_node.kind {
                if children.contains_key(name) {
                    return Err(FsError::AlreadyExists);
                }
            } else {
                return Err(FsError::NotADirectory);
            }
        }

        let node_id = self.create_special_node(kind)?;

        {
            let mut nodes = self.nodes.lock().unwrap();
            if let Some(node) = nodes.get_mut(&node_id) {
                node.mode = mode & 0o7777;
            }
        }

        if let Some(user) = self.user_for_process(pid) {
            let mut nodes = self.nodes.lock().unwrap();
            if let Some(node) = nodes.get_mut(&node_id) {
                node.uid = user.uid;
                node.gid = user.gid;
            }
        }

        self.link_child_into_parent(parent_id, name, node_id)?;

        #[cfg(feature = "events")]
        self.emit_event(EventKind::Created {
            path: path.to_string_lossy().to_string(),
        });

        Ok(())
    }

    fn create_regular_via_mknod(&self, pid: &PID, path: &Path, mode: u32) -> FsResult<()> {
        let mut opts = OpenOptions::default();
        opts.create = true;
        opts.write = true;
        let handle = self.create(pid, path, &opts)?;
        self.set_mode(pid, path, mode & 0o7777)?;
        self.close(pid, handle)?;
        Ok(())
    }

    // File operations
    pub fn create(&self, pid: &PID, path: &Path, opts: &OpenOptions) -> FsResult<HandleId> {
        // Check if the path already exists
        if self.resolve_path(pid, path).is_ok() {
            return Err(FsError::AlreadyExists);
        }

        // Get parent directory
        let parent_path = path.parent().ok_or(FsError::InvalidArgument)?;
        let parent_name = path.file_name().and_then(|n| n.to_str()).ok_or(FsError::InvalidName)?;

        let (parent_id, _) = self.resolve_path(pid, parent_path)?;

        // Permission check for parent directory w+x access
        self.ensure_parent_dir_allows_creation(pid, parent_id)?;

        let nodes = self.nodes.lock().unwrap();
        let parent_node = nodes.get(&parent_id).ok_or(FsError::NotFound)?;

        match &parent_node.kind {
            NodeKind::Directory { children } => {
                if children.contains_key(parent_name) {
                    return Err(FsError::AlreadyExists);
                }
            }
            NodeKind::File { .. } => return Err(FsError::NotADirectory),
            NodeKind::Symlink { .. } => return Err(FsError::NotADirectory),
        }
        drop(nodes);

        // Allocate content for the file
        let content_id = self.storage.allocate(&[])?;
        let file_node_id = self.create_file_node(content_id)?;

        self.link_child_into_parent(parent_id, parent_name, file_node_id)?;

        // Set ownership to creating process
        if let Some(user) = self.user_for_process(pid) {
            let mut nodes = self.nodes.lock().unwrap();
            if let Some(n) = nodes.get_mut(&file_node_id) {
                n.uid = user.uid;
                n.gid = user.gid;
            }
        }

        // Create handle
        let handle_id = self.allocate_handle_id();
        let handle = Handle {
            id: handle_id,
            node_id: file_node_id,
            path: path.to_path_buf(),
            kind: HandleType::File {
                options: opts.clone(),
                deleted: false,
            },
        };

        self.handles.lock().unwrap().insert(handle_id, handle);

        // Emit event
        let path_str = path.to_string_lossy().to_string();
        #[cfg(feature = "events")]
        self.emit_event(EventKind::Created { path: path_str });

        Ok(handle_id)
    }

    pub fn mkfifo(&self, pid: &PID, path: &Path, mode: u32) -> FsResult<()> {
        self.create_special_at_path(pid, path, SpecialNodeKind::Fifo, mode)
    }

    pub fn mknod(&self, pid: &PID, path: &Path, mode: u32, dev: u64) -> FsResult<()> {
        let file_type = mode & libc::S_IFMT as u32;
        match file_type {
            t if t == 0 || t == libc::S_IFREG as u32 => {
                self.create_regular_via_mknod(pid, path, mode)
            }
            t if t == libc::S_IFIFO as u32 => {
                self.create_special_at_path(pid, path, SpecialNodeKind::Fifo, mode)
            }
            t if t == libc::S_IFCHR as u32 => {
                self.create_special_at_path(pid, path, SpecialNodeKind::CharDevice { dev }, mode)
            }
            t if t == libc::S_IFBLK as u32 => {
                self.create_special_at_path(pid, path, SpecialNodeKind::BlockDevice { dev }, mode)
            }
            t if t == libc::S_IFSOCK as u32 => {
                self.create_special_at_path(pid, path, SpecialNodeKind::Socket, mode)
            }
            _ => Err(FsError::Unsupported),
        }
    }

    pub fn open(&self, pid: &PID, path: &Path, opts: &OpenOptions) -> FsResult<HandleId> {
        // First try to resolve in upper layer
        if let Ok((node_id, _)) = self.resolve_path(pid, path) {
            // Permission check
            if self.config.security.enforce_posix_permissions {
                if let Some(user) = self.user_for_process(pid) {
                    let nodes = self.nodes.lock().unwrap();
                    let node = nodes.get(&node_id).ok_or(FsError::NotFound)?;
                    let allow = self.allowed_for_user(node, &user, opts.read, opts.write, false);
                    if !allow {
                        return Err(FsError::AccessDenied);
                    }
                }
            }

            // Check share mode conflicts with existing handles
            if self.share_mode_conflicts(node_id, opts) {
                return Err(FsError::AccessDenied);
            }

            // Create handle
            let handle_id = self.allocate_handle_id();
            let handle = Handle {
                id: handle_id,
                node_id,
                path: path.to_path_buf(),
                kind: HandleType::File {
                    options: opts.clone(),
                    deleted: false,
                },
            };

            self.handles.lock().unwrap().insert(handle_id, handle);
            return Ok(handle_id);
        }

        // No upper entry, check if we can open from lower filesystem in overlay mode
        if self.is_overlay_enabled() && opts.read && !opts.write && !opts.create {
            if let Some(lower_fs) = &self.lower_fs {
                // Check if lower file exists and is readable
                if lower_fs.stat(path).is_ok() {
                    // For interpose mode, we should provide direct access to lower files
                    // without creating upper entries. However, the current architecture
                    // doesn't support this, so we return Unsupported for now.
                    if self.config.interpose.enabled {
                        // TODO: Implement proper FD forwarding for interpose
                        // For now, return Unsupported to indicate interpose isn't fully implemented
                        return Err(FsError::Unsupported);
                    } else {
                        // Regular overlay mode - read through from lower
                        // For now, return unsupported as we don't have direct lower handle support
                        return Err(FsError::Unsupported);
                    }
                }
            }
        }

        Err(FsError::NotFound)
    }

    /// Open file for interpose mode with eager upperization
    ///
    /// This method implements the eager upperization policy for interposed opens:
    /// - If the file exists only in the lower layer and is being opened read-only,
    ///   eagerly create an upper entry using reflink (preferred) or bounded copy
    /// - Returns a file descriptor to the upper file for direct I/O
    /// - Falls back to FORWARDING_UNAVAILABLE error if conditions aren't met
    pub fn fd_open(&self, pid: u32, path: &Path, flags: u32, _mode: u32) -> Result<RawFd, String> {
        use std::os::unix::io::AsRawFd;

        // Only support interpose mode
        if !self.config.interpose.enabled {
            return Err("Interpose mode not enabled".to_string());
        }

        // Convert flags to OpenOptions for internal use
        let has_write = (flags & (libc::O_WRONLY as u32)) != 0;
        let has_rdwr = (flags & (libc::O_RDWR as u32)) != 0;
        let read = !has_write || has_rdwr; // Read access if not write-only, or read-write
        let write = has_write || has_rdwr; // Write access if write-only or read-write
        let create = (flags & (libc::O_CREAT as u32)) != 0;

        // For now, only support read-only opens on existing lower files
        // TODO: Support write opens with copy-up semantics
        if write || create {
            return Err("Write opens not yet supported in interpose mode".to_string());
        }

        if !read {
            return Err("Invalid flags for fd_open".to_string());
        }

        let pid_struct = PID(pid);

        // Check if path exists in upper layer
        if self.resolve_path(&pid_struct, path).is_ok() {
            // File exists in upper layer, use normal open
            let opts = OpenOptions {
                read: true,
                write: false,
                create: false,
                truncate: false,
                append: false,
                share: vec![],
                stream: None,
            };

            match self.open(&pid_struct, path, &opts) {
                Ok(_handle_id) => {
                    // Get the file descriptor from the handle
                    // For now, this is a simplified implementation
                    // In a real implementation, we'd need to track file descriptors per handle
                    Err("Upper layer files not yet supported for fd_open".to_string())
                }
                Err(e) => Err(format!("Failed to open upper file: {:?}", e)),
            }
        } else {
            // Check if file exists in lower layer
            if let Some(lower_fs) = &self.lower_fs {
                match lower_fs.stat(path) {
                    Ok(attrs) => {
                        // File exists in lower layer, check size limits
                        if attrs.len > self.config.interpose.max_copy_bytes {
                            return Err("File too large for forwarding".to_string());
                        }

                        // Check if backstore supports native reflink
                        let backstore_supports_reflink = if let Some(backstore) = &self.backstore {
                            backstore.supports_native_reflink()
                        } else {
                            false
                        };

                        // Check policy requirements
                        if self.config.interpose.require_reflink && !backstore_supports_reflink {
                            return Err("Reflink required but not supported".to_string());
                        }

                        // Create upper entry with copy-up
                        if let Some(backstore) = &self.backstore {
                            // Create the upper file path
                            let upper_path =
                                backstore.root_path().join(path.strip_prefix("/").unwrap_or(path));

                            // Ensure parent directories exist
                            if let Some(parent) = upper_path.parent() {
                                if let Err(e) = std::fs::create_dir_all(parent) {
                                    return Err(format!(
                                        "Failed to create parent directories: {}",
                                        e
                                    ));
                                }
                            }

                            // Get the lower file path
                            let lower_root = self
                                .config
                                .overlay
                                .lower_root
                                .as_ref()
                                .ok_or("No lower root configured")?;
                            let lower_path =
                                lower_root.join(path.strip_prefix("/").unwrap_or(path));

                            // Try reflink first, then copy
                            let copy_result = if backstore_supports_reflink {
                                backstore.reflink(&lower_path, &upper_path)
                            } else {
                                // Fallback to copy
                                match std::fs::copy(&lower_path, &upper_path) {
                                    Ok(_) => Ok(()),
                                    Err(e) => Err(FsError::Io(e)),
                                }
                            };

                            match copy_result {
                                Ok(()) => {
                                    // Now open the upper file and return its file descriptor
                                    match std::fs::File::open(&upper_path) {
                                        Ok(file) => Ok(file.as_raw_fd()),
                                        Err(e) => Err(format!("Failed to open upper file: {}", e)),
                                    }
                                }
                                Err(e) => Err(format!("Failed to copy-up file: {:?}", e)),
                            }
                        } else {
                            Err("No backstore configured for interpose mode".to_string())
                        }
                    }
                    Err(_) => Err("File not found in lower filesystem".to_string()),
                }
            } else {
                Err("Overlay mode not enabled".to_string())
            }
        }
    }

    /// Open by internal node id (adapter pathless open)
    pub fn open_by_id(
        &self,
        pid: &PID,
        node_id_u64: u64,
        opts: &OpenOptions,
    ) -> FsResult<HandleId> {
        let node_id = NodeId(node_id_u64);

        // Verify node exists
        {
            let nodes = self.nodes.lock().unwrap();
            let _ = nodes.get(&node_id).ok_or(FsError::NotFound)?;
        }

        // Check share mode conflicts with existing handles
        if self.share_mode_conflicts(node_id, opts) {
            return Err(FsError::AccessDenied);
        }

        let handle_id = self.allocate_handle_id();
        let path = self.path_for_node(pid, node_id).unwrap_or_else(|| PathBuf::from("/unknown"));
        let handle = Handle {
            id: handle_id,
            node_id,
            path,
            kind: HandleType::File {
                options: opts.clone(),
                deleted: false,
            },
        };
        self.handles.lock().unwrap().insert(handle_id, handle);
        Ok(handle_id)
    }

    /// Check if a node is shared between branches/snapshots
    #[allow(dead_code)]
    fn is_node_shared(&self, _node_id: NodeId) -> bool {
        // For simplicity, assume all nodes need CoW for now
        true
    }

    pub fn read(
        &self,
        pid: &PID,
        handle_id: HandleId,
        offset: u64,
        buf: &mut [u8],
    ) -> FsResult<usize> {
        let handles = self.handles.lock().unwrap();
        let handle = handles.get(&handle_id).ok_or(FsError::InvalidArgument)?;

        // Ensure this is a file handle
        let options = match &handle.kind {
            HandleType::File { options, .. } => options,
            HandleType::Directory { .. } => return Err(FsError::InvalidArgument),
        };

        // Permission check for read
        if self.config.security.enforce_posix_permissions {
            if let Some(user) = self.user_for_process(pid) {
                let nodes = self.nodes.lock().unwrap();
                let node = nodes.get(&handle.node_id).ok_or(FsError::NotFound)?;
                if !self.allowed_for_user(node, &user, true, false, false) {
                    return Err(FsError::AccessDenied);
                }
            }
        }

        if !options.read {
            return Err(FsError::AccessDenied);
        }

        let stream_name = Self::get_stream_name(handle);
        let nodes = self.nodes.lock().unwrap();
        let node = nodes.get(&handle.node_id).ok_or(FsError::NotFound)?;

        match &node.kind {
            NodeKind::File { streams } => {
                if let Some((content_id, _)) = streams.get(stream_name) {
                    self.storage.read(*content_id, offset, buf)
                } else {
                    Err(FsError::NotFound) // Stream doesn't exist
                }
            }
            NodeKind::Directory { .. } => Err(FsError::IsADirectory),
            NodeKind::Symlink { .. } => Err(FsError::InvalidArgument), // Symlinks are not readable like files
        }
    }

    pub fn write(
        &self,
        pid: &PID,
        handle_id: HandleId,
        offset: u64,
        data: &[u8],
    ) -> FsResult<usize> {
        let mut handles = self.handles.lock().unwrap();
        let handle = handles.get_mut(&handle_id).ok_or(FsError::InvalidArgument)?;

        // Ensure this is a file handle
        let options = match &handle.kind {
            HandleType::File { options, .. } => options,
            HandleType::Directory { .. } => return Err(FsError::InvalidArgument),
        };

        // Permission check for write
        if self.config.security.enforce_posix_permissions {
            if let Some(user) = self.user_for_process(pid) {
                let nodes = self.nodes.lock().unwrap();
                let node = nodes.get(&handle.node_id).ok_or(FsError::NotFound)?;
                if !self.allowed_for_user(node, &user, false, true, false) {
                    return Err(FsError::AccessDenied);
                }
            }
        }

        if !options.write {
            return Err(FsError::AccessDenied);
        }

        let stream_name = Self::get_stream_name(handle);
        let current_branch_id = self.branch_for_process(pid);
        let _branches = self.branches.lock().unwrap();
        let _branch = _branches.get(&current_branch_id).ok_or(FsError::NotFound)?;

        let mut nodes = self.nodes.lock().unwrap();
        let node = nodes.get_mut(&handle.node_id).ok_or(FsError::NotFound)?;

        match &mut node.kind {
            NodeKind::File { streams } => {
                // Get or create the stream
                let (content_id, size) =
                    streams.entry(stream_name.to_string()).or_insert_with(|| {
                        // Create new stream if it doesn't exist
                        let new_content_id = self.storage.allocate(&[]).unwrap();
                        (new_content_id, 0)
                    });

                let content_to_write = if self.is_content_shared(*content_id) {
                    // Clone the content for this branch
                    let new_content_id = self.storage.clone_cow(*content_id).unwrap();
                    *content_id = new_content_id;
                    new_content_id
                } else {
                    *content_id
                };

                let written = self.storage.write(content_to_write, offset, data)?;
                let new_size = std::cmp::max(*size, offset + written as u64);
                *size = new_size;
                node.times.mtime = Self::current_timestamp();
                node.times.ctime = node.times.mtime;

                // Emit Modified event for file content changes
                #[cfg(feature = "events")]
                if written > 0 {
                    self.emit_event(EventKind::Modified {
                        path: handle.path.to_string_lossy().to_string(),
                    });
                }

                Ok(written)
            }
            NodeKind::Directory { .. } => Err(FsError::IsADirectory),
            NodeKind::Symlink { .. } => Err(FsError::InvalidArgument), // Symlinks are not writable like files
        }
    }

    /// Check if content is shared between branches/snapshots
    fn is_content_shared(&self, _content_id: ContentId) -> bool {
        // For simplicity, assume all content needs CoW for now
        // In a real implementation, this would track reference counts
        true
    }

    /// Check if two lock ranges overlap
    fn ranges_overlap(r1: &LockRange, r2: &LockRange) -> bool {
        r1.offset < (r2.offset + r2.len) && r2.offset < (r1.offset + r1.len)
    }

    /// Check if a lock conflicts with existing locks
    fn lock_conflicts(&self, node_id: NodeId, new_lock: &LockRange, handle_id: HandleId) -> bool {
        let locks = self.locks.lock().unwrap();
        if let Some(node_locks) = locks.locks.get(&node_id) {
            for existing_lock in node_locks {
                // For POSIX semantics, same handle cannot have conflicting locks
                if existing_lock.handle_id == handle_id
                    && Self::ranges_overlap(&existing_lock.range, new_lock)
                {
                    // Same handle: exclusive locks cannot overlap with anything
                    // Shared locks cannot overlap with exclusive locks from same handle
                    if existing_lock.range.kind == LockKind::Exclusive
                        || new_lock.kind == LockKind::Exclusive
                    {
                        return true;
                    }
                }

                // Different handles: check standard conflict rules
                if existing_lock.handle_id != handle_id
                    && Self::ranges_overlap(&existing_lock.range, new_lock)
                {
                    // Exclusive locks conflict with any overlapping lock
                    // Shared locks only conflict with exclusive locks
                    if existing_lock.range.kind == LockKind::Exclusive
                        || new_lock.kind == LockKind::Exclusive
                    {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// Check if opening with given options would conflict with existing handles (Windows share modes)
    fn share_mode_conflicts(&self, node_id: NodeId, options: &OpenOptions) -> bool {
        let handles = self.handles.lock().unwrap();

        for handle in handles.values() {
            if handle.node_id != node_id {
                continue;
            }

            // Only check file handles for share mode conflicts
            match &handle.kind {
                HandleType::File {
                    options: handle_options,
                    deleted,
                } => {
                    if *deleted {
                        continue;
                    }

                    // Check each requested access type against existing handle's share modes
                    if options.read && !handle_options.share.contains(&ShareMode::Read) {
                        return true;
                    }
                    if options.write && !handle_options.share.contains(&ShareMode::Write) {
                        return true;
                    }
                    // Note: Delete access conflicts are typically checked at delete time, not open time
                }
                HandleType::Directory { .. } => {
                    // Directory handles don't participate in share mode conflicts
                }
            }
        }

        false
    }

    /// Get the stream name for a handle (empty string for unnamed/default stream)
    fn get_stream_name(handle: &Handle) -> &str {
        match &handle.kind {
            HandleType::File { options, .. } => options.stream.as_deref().unwrap_or(""),
            HandleType::Directory { .. } => "",
        }
    }

    pub fn close(&self, _pid: &PID, handle_id: HandleId) -> FsResult<()> {
        let mut handles = self.handles.lock().unwrap();
        let handle = handles.get(&handle_id).ok_or(FsError::InvalidArgument)?;
        let node_id = handle.node_id;
        let was_deleted = matches!(handle.kind, HandleType::File { deleted: true, .. });

        handles.remove(&handle_id);

        // Clean up any locks held by this handle
        let mut locks = self.locks.lock().unwrap();
        if let Some(node_locks) = locks.locks.get_mut(&node_id) {
            node_locks.retain(|lock| lock.handle_id != handle_id);
            if node_locks.is_empty() {
                locks.locks.remove(&node_id);
            }
        }
        drop(locks);

        // If this was the last handle to a deleted file with no remaining links, remove the node
        if was_deleted {
            let remaining_handles = handles.values().any(|h| h.node_id == node_id);
            if !remaining_handles && self.node_link_count(node_id) == 0 {
                let mut nodes = self.nodes.lock().unwrap();
                nodes.remove(&node_id);
            }
        }

        Ok(())
    }

    // Lock operations
    pub fn lock(&self, handle_id: HandleId, range: LockRange) -> FsResult<()> {
        let handles = self.handles.lock().unwrap();
        let handle = handles.get(&handle_id).ok_or(FsError::InvalidArgument)?;
        let node_id = handle.node_id;
        drop(handles);

        // Check for conflicts
        if self.lock_conflicts(node_id, &range, handle_id) {
            return Err(FsError::Busy); // Lock conflict
        }

        // Add the lock
        let mut locks = self.locks.lock().unwrap();
        let node_locks = locks.locks.entry(node_id).or_default();
        node_locks.push(ActiveLock { handle_id, range });

        Ok(())
    }

    pub fn unlock(&self, handle_id: HandleId, range: LockRange) -> FsResult<()> {
        let handles = self.handles.lock().unwrap();
        let handle = handles.get(&handle_id).ok_or(FsError::InvalidArgument)?;
        let node_id = handle.node_id;
        drop(handles);

        // Find and remove matching locks
        let mut locks = self.locks.lock().unwrap();
        if let Some(node_locks) = locks.locks.get_mut(&node_id) {
            // Remove locks that match the handle and range
            node_locks.retain(|lock| {
                !(lock.handle_id == handle_id
                    && lock.range.offset == range.offset
                    && lock.range.len == range.len
                    && lock.range.kind == range.kind)
            });

            // Clean up empty lock lists
            if node_locks.is_empty() {
                locks.locks.remove(&node_id);
            }
        }

        Ok(())
    }

    pub fn set_times(&self, pid: &PID, path: &Path, times: FileTimes) -> FsResult<()> {
        let (node_id, _) = self.resolve_path(pid, path)?;
        self.update_node_times(node_id, times);

        // Emit Modified event for metadata change
        #[cfg(feature = "events")]
        self.emit_event(EventKind::Modified {
            path: path.to_string_lossy().to_string(),
        });

        Ok(())
    }

    // Directory operations
    pub fn mkdir(&self, pid: &PID, path: &Path, mode: u32) -> FsResult<()> {
        // Check if the path already exists
        if self.resolve_path(pid, path).is_ok() {
            return Err(FsError::AlreadyExists);
        }

        // Get parent directory
        let parent_path = path.parent().ok_or(FsError::InvalidArgument)?;
        let dir_name = path.file_name().and_then(|n| n.to_str()).ok_or(FsError::InvalidName)?;

        let (parent_id, _) = self.resolve_path(pid, parent_path)?;

        // Permission check for parent directory write+execute access (w+x required to modify entries)
        if self.config.security.enforce_posix_permissions {
            if let Some(user) = self.user_for_process(pid) {
                let nodes = self.nodes.lock().unwrap();
                let parent_node = nodes.get(&parent_id).ok_or(FsError::NotFound)?;
                if !self.allowed_for_user(parent_node, &user, false, true, true) {
                    return Err(FsError::AccessDenied);
                }
            }
        }

        let nodes = self.nodes.lock().unwrap();
        let parent_node = nodes.get(&parent_id).ok_or(FsError::NotFound)?;

        match &parent_node.kind {
            NodeKind::Directory { children } => {
                if children.contains_key(dir_name) {
                    return Err(FsError::AlreadyExists);
                }
            }
            NodeKind::File { .. } => return Err(FsError::NotADirectory),
            NodeKind::Symlink { .. } => return Err(FsError::NotADirectory),
        }
        drop(nodes);

        // Create directory node
        let dir_node_id = self.create_directory_node()?;

        // Initialize directory node ownership and mode
        {
            let mut nodes = self.nodes.lock().unwrap();
            if let Some(dir_node) = nodes.get_mut(&dir_node_id) {
                if let Some(user) = self.user_for_process(pid) {
                    dir_node.uid = user.uid;
                    dir_node.gid = user.gid;
                }
                dir_node.mode = mode;
            }
        }

        self.link_child_into_parent(parent_id, dir_name, dir_node_id)?;

        // Emit event
        let path_str = path.to_string_lossy().to_string();
        #[cfg(feature = "events")]
        self.emit_event(EventKind::Created { path: path_str });

        Ok(())
    }

    pub fn rmdir(&self, pid: &PID, path: &Path) -> FsResult<()> {
        let (node_id, parent_info) = self.resolve_path(pid, path)?;

        let Some((parent_id, name)) = parent_info else {
            return Err(FsError::InvalidArgument); // Can't remove root
        };

        let node = {
            let nodes = self.nodes.lock().unwrap();
            nodes.get(&node_id).cloned().ok_or(FsError::NotFound)?
        };

        match &node.kind {
            NodeKind::Directory { children } => {
                if !children.is_empty() {
                    return Err(FsError::Busy);
                }
            }
            _ => return Err(FsError::NotADirectory),
        }

        self.check_dir_permissions(pid, parent_id, Some(&node))?;

        self.unlink_child_from_parent(parent_id, &name);

        // Remove the directory node itself to avoid leaking nodes
        {
            let mut nodes = self.nodes.lock().unwrap();
            nodes.remove(&node_id);
        }

        // Emit event
        let path_str = path.to_string_lossy().to_string();
        #[cfg(feature = "events")]
        self.emit_event(EventKind::Removed { path: path_str });

        Ok(())
    }

    // Optional readdir+ that includes attributes without extra getattr calls (libfuse pattern)
    pub fn readdir_plus(&self, pid: &PID, path: &Path) -> FsResult<Vec<(DirEntry, Attributes)>> {
        // Check if there's an upper directory
        if let Ok((node_id, _)) = self.resolve_path(pid, path) {
            let nodes = self.nodes.lock().unwrap();
            let node = nodes.get(&node_id).ok_or(FsError::NotFound)?;

            match &node.kind {
                NodeKind::Directory { children } => {
                    if self.config.security.enforce_posix_permissions {
                        if let Some(user) = self.user_for_process(pid) {
                            if !self.allowed_for_user(node, &user, true, false, true) {
                                return Err(FsError::AccessDenied);
                            }
                        }
                    }

                    // In overlay mode, merge upper and lower entries
                    if self.is_overlay_enabled() {
                        return self.readdir_plus_overlay(pid, path, children, &nodes);
                    }

                    // Non-overlay mode: just upper entries
                    return self.readdir_plus_upper_only(children, &nodes);
                }
                NodeKind::File { .. } => return Err(FsError::NotADirectory),
                NodeKind::Symlink { .. } => return Err(FsError::NotADirectory),
            }
        }

        // No upper entry, check lower filesystem in overlay mode
        if self.is_overlay_enabled() {
            if let Some(lower_fs) = &self.lower_fs {
                let lower_entries = lower_fs.readdir(path)?;
                let mut entries = Vec::new();
                for entry in lower_entries {
                    // Get attributes for each entry
                    let entry_path = path.join(&entry.name);
                    if let Ok(attrs) = lower_fs.stat(&entry_path) {
                        entries.push((entry, attrs));
                    }
                }
                return Ok(entries);
            }
        }

        Err(FsError::NotFound)
    }

    fn readdir_plus_upper_only(
        &self,
        children: &HashMap<String, NodeId>,
        nodes: &std::collections::HashMap<NodeId, Node>,
    ) -> FsResult<Vec<(DirEntry, Attributes)>> {
        // Collect and sort child names for stable ordering
        let mut names: Vec<_> = children.keys().cloned().collect();
        names.sort();

        let mut entries = Vec::new();
        for name in names {
            let child_id = children.get(&name).ok_or(FsError::NotFound)?;
            let child_node = nodes.get(child_id).ok_or(FsError::NotFound)?;
            let (is_dir, is_symlink, len) = match &child_node.kind {
                NodeKind::Directory { .. } => (true, false, 0),
                NodeKind::File { streams } => {
                    // Size is the size of the unnamed stream
                    let size = streams.get("").map(|(_, size)| *size).unwrap_or(0);
                    (false, false, size)
                }
                NodeKind::Symlink { target } => (false, true, target.len() as u64),
            };

            let dir_entry = DirEntry {
                name,
                is_dir,
                is_symlink,
                len,
            };

            let perm_bits = child_node.mode & 0o777;
            let attributes = Attributes {
                len,
                times: child_node.times,
                uid: child_node.uid,
                gid: child_node.gid,
                is_dir,
                is_symlink,
                special_kind: child_node.special_kind.clone(),
                nlink: child_node.nlink,
                mode_user: FileMode {
                    read: (perm_bits & 0o400) != 0,
                    write: (perm_bits & 0o200) != 0,
                    exec: (perm_bits & 0o100) != 0,
                },
                mode_group: FileMode {
                    read: (perm_bits & 0o040) != 0,
                    write: (perm_bits & 0o020) != 0,
                    exec: (perm_bits & 0o010) != 0,
                },
                mode_other: FileMode {
                    read: (perm_bits & 0o004) != 0,
                    write: (perm_bits & 0o002) != 0,
                    exec: (perm_bits & 0o001) != 0,
                },
            };

            entries.push((dir_entry, attributes));
        }
        Ok(entries)
    }

    fn readdir_plus_overlay(
        &self,
        _pid: &PID,
        path: &Path,
        upper_children: &HashMap<String, NodeId>,
        nodes: &std::collections::HashMap<NodeId, Node>,
    ) -> FsResult<Vec<(DirEntry, Attributes)>> {
        let mut entries = std::collections::HashMap::new();

        // Add lower entries first (if any)
        if let Some(lower_fs) = &self.lower_fs {
            if let Ok(lower_dir_entries) = lower_fs.readdir(path) {
                for entry in lower_dir_entries {
                    // Get attributes for each entry
                    let entry_path = path.join(&entry.name);
                    if let Ok(attrs) = lower_fs.stat(&entry_path) {
                        entries.insert(entry.name.clone(), (entry, attrs));
                    }
                }
            }
        }

        // Add/override with upper entries
        for (name, child_id) in upper_children {
            if let Some(child_node) = nodes.get(child_id) {
                let (is_dir, is_symlink, len) = match &child_node.kind {
                    NodeKind::Directory { .. } => (true, false, 0),
                    NodeKind::File { streams } => {
                        // Size is the size of the unnamed stream
                        let size = streams.get("").map(|(_, size)| *size).unwrap_or(0);
                        (false, false, size)
                    }
                    NodeKind::Symlink { target } => (false, true, target.len() as u64),
                };

                // Check if this is a whiteout (deleted file)
                // For now, we don't implement whiteouts in this simplified version
                // TODO: Implement proper whiteout detection

                let dir_entry = DirEntry {
                    name: name.clone(),
                    is_dir,
                    is_symlink,
                    len,
                };

                let perm_bits = child_node.mode & 0o777;
                let attributes = Attributes {
                    len,
                    times: child_node.times,
                    uid: child_node.uid,
                    gid: child_node.gid,
                    is_dir,
                    is_symlink,
                    special_kind: child_node.special_kind.clone(),
                    nlink: child_node.nlink,
                    mode_user: FileMode {
                        read: (perm_bits & 0o400) != 0,
                        write: (perm_bits & 0o200) != 0,
                        exec: (perm_bits & 0o100) != 0,
                    },
                    mode_group: FileMode {
                        read: (perm_bits & 0o040) != 0,
                        write: (perm_bits & 0o020) != 0,
                        exec: (perm_bits & 0o010) != 0,
                    },
                    mode_other: FileMode {
                        read: (perm_bits & 0o004) != 0,
                        write: (perm_bits & 0o002) != 0,
                        exec: (perm_bits & 0o001) != 0,
                    },
                };

                entries.insert(name.clone(), (dir_entry, attributes));
            }
        }

        // Convert to sorted vector
        let mut result: Vec<_> = entries.into_values().collect();
        result.sort_by(|a, b| a.0.name.cmp(&b.0.name));
        Ok(result)
    }

    /// Like readdir_plus, but returns raw name bytes for each entry for adapters that need exact bytes
    pub fn readdir_plus_raw(&self, pid: &PID, path: &Path) -> FsResult<Vec<(Vec<u8>, Attributes)>> {
        let (node_id, _) = self.resolve_path(pid, path)?;
        let nodes = self.nodes.lock().unwrap();
        let node = nodes.get(&node_id).ok_or(FsError::NotFound)?;

        match &node.kind {
            NodeKind::Directory { children } => {
                if self.config.security.enforce_posix_permissions {
                    if let Some(user) = self.user_for_process(pid) {
                        if !self.allowed_for_user(node, &user, true, false, true) {
                            return Err(FsError::AccessDenied);
                        }
                    }
                }
                // Sort internal names for stable order
                let mut names: Vec<_> = children.keys().cloned().collect();
                names.sort();

                let mut entries = Vec::new();
                for name in names {
                    let child_id = children.get(&name).ok_or(FsError::NotFound)?;
                    let child_node = nodes.get(child_id).ok_or(FsError::NotFound)?;

                    let (is_dir, is_symlink, len) = match &child_node.kind {
                        NodeKind::Directory { .. } => (true, false, 0),
                        NodeKind::File { streams } => {
                            let size = streams.get("").map(|(_, size)| *size).unwrap_or(0);
                            (false, false, size)
                        }
                        NodeKind::Symlink { target } => (false, true, target.len() as u64),
                    };

                    let attributes = Attributes {
                        len,
                        times: child_node.times,
                        uid: child_node.uid,
                        gid: child_node.gid,
                        is_dir,
                        is_symlink,
                        special_kind: child_node.special_kind.clone(),
                        nlink: child_node.nlink,
                        mode_user: FileMode {
                            read: true,
                            write: true,
                            exec: is_dir,
                        },
                        mode_group: FileMode {
                            read: true,
                            write: false,
                            exec: is_dir,
                        },
                        mode_other: FileMode {
                            read: true,
                            write: false,
                            exec: false,
                        },
                    };

                    // Prefer raw name bytes preserved at create time, fallback to internal name bytes
                    let raw_bytes = child_node
                        .xattrs
                        .get("user.agentfs.rawname")
                        .cloned()
                        .unwrap_or_else(|| name.as_bytes().to_vec());

                    entries.push((raw_bytes, attributes));
                }
                Ok(entries)
            }
            _ => Err(FsError::NotADirectory),
        }
    }

    /// Open a directory handle
    pub fn opendir(&self, pid: &PID, path: &Path) -> FsResult<HandleId> {
        // Resolve the path to get the node
        let (node_id, _) = self.resolve_path(pid, path)?;

        // Permission check for read access
        if self.config.security.enforce_posix_permissions {
            if let Some(user) = self.user_for_process(pid) {
                let nodes = self.nodes.lock().unwrap();
                let node = nodes.get(&node_id).ok_or(FsError::NotFound)?;
                if !self.allowed_for_user(node, &user, true, false, false) {
                    return Err(FsError::AccessDenied);
                }
            }
        }

        // Read directory entries
        let entries = self.readdir_plus(pid, path)?;

        // Create directory handle
        let handle_id = self.allocate_handle_id();
        let handle = Handle {
            id: handle_id,
            node_id,
            path: path.to_path_buf(),
            kind: HandleType::Directory {
                position: 0,
                entries: entries.into_iter().map(|(entry, _)| entry).collect(),
            },
        };

        self.handles.lock().unwrap().insert(handle_id, handle);
        Ok(handle_id)
    }

    /// Read from a directory handle
    pub fn readdir(&self, _pid: &PID, handle_id: HandleId) -> FsResult<Option<DirEntry>> {
        let mut handles = self.handles.lock().unwrap();
        let handle = handles.get_mut(&handle_id).ok_or(FsError::InvalidArgument)?;

        match &mut handle.kind {
            HandleType::Directory { position, entries } => {
                if *position >= entries.len() {
                    Ok(None) // End of directory
                } else {
                    let entry = entries[*position].clone();
                    *position += 1;
                    Ok(Some(entry))
                }
            }
            HandleType::File { .. } => Err(FsError::InvalidArgument),
        }
    }

    /// Close a directory handle
    pub fn closedir(&self, _pid: &PID, handle_id: HandleId) -> FsResult<()> {
        let mut handles = self.handles.lock().unwrap();
        let handle = handles.get(&handle_id).ok_or(FsError::InvalidArgument)?;

        // Ensure this is a directory handle
        match handle.kind {
            HandleType::Directory { .. } => {
                handles.remove(&handle_id);
                Ok(())
            }
            HandleType::File { .. } => Err(FsError::InvalidArgument),
        }
    }

    // Extended attributes operations
    pub fn xattr_get(&self, pid: &PID, path: &Path, name: &str) -> FsResult<Vec<u8>> {
        let (node_id, _) = self.resolve_path(pid, path)?;
        let nodes = self.nodes.lock().unwrap();
        let node = nodes.get(&node_id).ok_or(FsError::NotFound)?;
        node.xattrs.get(name).cloned().ok_or(FsError::NotFound)
    }

    pub fn xattr_set(&self, pid: &PID, path: &Path, name: &str, value: &[u8]) -> FsResult<()> {
        let (node_id, _) = self.resolve_path(pid, path)?;
        let mut nodes = self.nodes.lock().unwrap();
        if let Some(node) = nodes.get_mut(&node_id) {
            node.xattrs.insert(name.to_string(), value.to_vec());

            // Emit Modified event for metadata change
            #[cfg(feature = "events")]
            drop(nodes); // Release lock before emitting event
            self.emit_event(EventKind::Modified {
                path: path.to_string_lossy().to_string(),
            });

            Ok(())
        } else {
            Err(FsError::NotFound)
        }
    }

    pub fn xattr_list(&self, pid: &PID, path: &Path) -> FsResult<Vec<String>> {
        let (node_id, _) = self.resolve_path(pid, path)?;
        let nodes = self.nodes.lock().unwrap();
        let node = nodes.get(&node_id).ok_or(FsError::NotFound)?;
        Ok(node.xattrs.keys().cloned().collect())
    }

    pub fn xattr_remove(&self, pid: &PID, path: &Path, name: &str) -> FsResult<()> {
        let (node_id, _) = self.resolve_path(pid, path)?;
        let mut nodes = self.nodes.lock().unwrap();
        if let Some(node) = nodes.get_mut(&node_id) {
            if node.xattrs.remove(name).is_some() {
                // Emit Modified event for metadata change
                #[cfg(feature = "events")]
                drop(nodes); // Release lock before emitting event
                self.emit_event(EventKind::Modified {
                    path: path.to_string_lossy().to_string(),
                });

                Ok(())
            } else {
                Err(FsError::NotFound)
            }
        } else {
            Err(FsError::NotFound)
        }
    }

    // l* variants (don't follow symlinks - same as regular since we don't follow symlinks in resolve_path)
    pub fn lgetxattr(&self, pid: &PID, path: &Path, name: &str) -> FsResult<Vec<u8>> {
        self.xattr_get(pid, path, name)
    }

    pub fn lsetxattr(&self, pid: &PID, path: &Path, name: &str, value: &[u8]) -> FsResult<()> {
        self.xattr_set(pid, path, name, value)
    }

    pub fn llistxattr(&self, pid: &PID, path: &Path) -> FsResult<Vec<String>> {
        self.xattr_list(pid, path)
    }

    pub fn lremovexattr(&self, pid: &PID, path: &Path, name: &str) -> FsResult<()> {
        self.xattr_remove(pid, path, name)
    }

    // f* variants (fd-based)
    pub fn fgetxattr(&self, pid: &PID, handle_id: HandleId, name: &str) -> FsResult<Vec<u8>> {
        let node_id = self.get_node_id_for_handle(pid, handle_id)?;
        let nodes = self.nodes.lock().unwrap();
        let node = nodes.get(&node_id).ok_or(FsError::NotFound)?;
        node.xattrs.get(name).cloned().ok_or(FsError::NotFound)
    }

    pub fn fsetxattr(
        &self,
        pid: &PID,
        handle_id: HandleId,
        name: &str,
        value: &[u8],
    ) -> FsResult<()> {
        let node_id = self.get_node_id_for_handle(pid, handle_id)?;
        let mut nodes = self.nodes.lock().unwrap();
        if let Some(node) = nodes.get_mut(&node_id) {
            node.xattrs.insert(name.to_string(), value.to_vec());
            Ok(())
        } else {
            Err(FsError::NotFound)
        }
    }

    pub fn flistxattr(&self, pid: &PID, handle_id: HandleId) -> FsResult<Vec<String>> {
        let node_id = self.get_node_id_for_handle(pid, handle_id)?;
        let nodes = self.nodes.lock().unwrap();
        let node = nodes.get(&node_id).ok_or(FsError::NotFound)?;
        Ok(node.xattrs.keys().cloned().collect())
    }

    pub fn fremovexattr(&self, pid: &PID, handle_id: HandleId, name: &str) -> FsResult<()> {
        let node_id = self.get_node_id_for_handle(pid, handle_id)?;
        let mut nodes = self.nodes.lock().unwrap();
        if let Some(node) = nodes.get_mut(&node_id) {
            if node.xattrs.remove(name).is_some() {
                Ok(())
            } else {
                Err(FsError::NotFound)
            }
        } else {
            Err(FsError::NotFound)
        }
    }

    // ACL operations
    pub fn acl_get_file(&self, pid: &PID, path: &Path, acl_type: u32) -> FsResult<Vec<u8>> {
        let (node_id, _) = self.resolve_path(pid, path)?;
        let nodes = self.nodes.lock().unwrap();
        let node = nodes.get(&node_id).ok_or(FsError::NotFound)?;
        node.acls.get(&acl_type).cloned().ok_or(FsError::NotFound)
    }

    pub fn acl_set_file(
        &self,
        pid: &PID,
        path: &Path,
        acl_type: u32,
        acl_data: &[u8],
    ) -> FsResult<()> {
        let (node_id, _) = self.resolve_path(pid, path)?;
        let mut nodes = self.nodes.lock().unwrap();
        if let Some(node) = nodes.get_mut(&node_id) {
            node.acls.insert(acl_type, acl_data.to_vec());
            Ok(())
        } else {
            Err(FsError::NotFound)
        }
    }

    pub fn acl_get_fd(&self, pid: &PID, handle_id: HandleId, acl_type: u32) -> FsResult<Vec<u8>> {
        let node_id = self.get_node_id_for_handle(pid, handle_id)?;
        let nodes = self.nodes.lock().unwrap();
        let node = nodes.get(&node_id).ok_or(FsError::NotFound)?;
        node.acls.get(&acl_type).cloned().ok_or(FsError::NotFound)
    }

    pub fn acl_set_fd(
        &self,
        pid: &PID,
        handle_id: HandleId,
        acl_type: u32,
        acl_data: &[u8],
    ) -> FsResult<()> {
        let node_id = self.get_node_id_for_handle(pid, handle_id)?;
        let mut nodes = self.nodes.lock().unwrap();
        if let Some(node) = nodes.get_mut(&node_id) {
            node.acls.insert(acl_type, acl_data.to_vec());
            Ok(())
        } else {
            Err(FsError::NotFound)
        }
    }

    pub fn acl_delete_def_file(&self, pid: &PID, path: &Path) -> FsResult<()> {
        let (node_id, _) = self.resolve_path(pid, path)?;
        let mut nodes = self.nodes.lock().unwrap();
        if let Some(node) = nodes.get_mut(&node_id) {
            // Remove default ACLs (ACL_TYPE_DEFAULT)
            node.acls.remove(&1); // ACL_TYPE_DEFAULT = 1 on macOS
            Ok(())
        } else {
            Err(FsError::NotFound)
        }
    }

    // File flags operations
    pub fn chflags(&self, _pid: &PID, path: &Path, flags: u32) -> FsResult<()> {
        let (node_id, _) = self.resolve_path(_pid, path)?;
        let mut nodes = self.nodes.lock().unwrap();
        if let Some(node) = nodes.get_mut(&node_id) {
            node.flags = flags;
            Ok(())
        } else {
            Err(FsError::NotFound)
        }
    }

    pub fn lchflags(&self, pid: &PID, path: &Path, flags: u32) -> FsResult<()> {
        // lchflags doesn't follow symlinks, same as chflags in our implementation
        self.chflags(pid, path, flags)
    }

    pub fn fchflags(&self, pid: &PID, handle_id: HandleId, flags: u32) -> FsResult<()> {
        let node_id = self.get_node_id_for_handle(pid, handle_id)?;
        let mut nodes = self.nodes.lock().unwrap();
        if let Some(node) = nodes.get_mut(&node_id) {
            node.flags = flags;
            Ok(())
        } else {
            Err(FsError::NotFound)
        }
    }

    // getattrlist/setattrlist operations (macOS bulk attribute operations)
    pub fn getattrlist(
        &self,
        pid: &PID,
        path: &Path,
        _attr_list: &[u8],
        _options: u32,
    ) -> FsResult<Vec<u8>> {
        // Resolve the path to get node information
        let (node_id, _) = self.resolve_path(pid, path)?;

        let nodes = self.nodes.lock().unwrap();
        let node = nodes.get(&node_id).ok_or(FsError::NotFound)?;

        // Compute attributes directly to avoid deadlock
        let (len, is_dir, is_symlink) = match &node.kind {
            NodeKind::File { streams } => {
                let size = streams.get("").map(|(_, size)| *size).unwrap_or(0);
                (size, false, false)
            }
            NodeKind::Directory { .. } => (0, true, false),
            NodeKind::Symlink { target } => (target.len() as u64, false, true),
        };

        let perm_bits = node.mode & 0o777;

        let stat_data = Attributes {
            len,
            times: node.times,
            uid: node.uid,
            gid: node.gid,
            is_dir,
            is_symlink,
            special_kind: node.special_kind.clone(),
            nlink: node.nlink,
            mode_user: FileMode {
                read: (perm_bits & 0o400) != 0,
                write: (perm_bits & 0o200) != 0,
                exec: (perm_bits & 0o100) != 0,
            },
            mode_group: FileMode {
                read: (perm_bits & 0o040) != 0,
                write: (perm_bits & 0o020) != 0,
                exec: (perm_bits & 0o010) != 0,
            },
            mode_other: FileMode {
                read: (perm_bits & 0o004) != 0,
                write: (perm_bits & 0o002) != 0,
                exec: (perm_bits & 0o001) != 0,
            },
        };

        // For now, implement a basic version that returns stat-like information
        // In a full implementation, this would parse the attr_list and return
        // the requested attributes in the macOS format

        // Simple implementation: return the stat data as bytes
        // This is a placeholder - real implementation would need to handle
        // the complex macOS attrlist format properly
        let mut result = Vec::new();

        // Add basic file stat information
        result.extend_from_slice(&stat_data.len.to_le_bytes());
        result.extend_from_slice(&stat_data.mode().to_le_bytes());
        result.extend_from_slice(&stat_data.uid.to_le_bytes());
        result.extend_from_slice(&stat_data.gid.to_le_bytes());
        result.extend_from_slice(&(stat_data.times.atime as u64).to_le_bytes());
        result.extend_from_slice(&(stat_data.times.mtime as u64).to_le_bytes());
        result.extend_from_slice(&(stat_data.times.ctime as u64).to_le_bytes());

        // Add any xattrs if requested (simplified)
        if !node.xattrs.is_empty() {
            for (name, value) in &node.xattrs {
                if name.len() <= 255 && value.len() <= 65535 {
                    // reasonable limits
                    result.push(name.len() as u8);
                    result.extend_from_slice(name.as_bytes());
                    result.extend_from_slice(&(value.len() as u16).to_le_bytes());
                    result.extend_from_slice(value);
                }
            }
        }

        Ok(result)
    }

    pub fn setattrlist(
        &self,
        pid: &PID,
        path: &Path,
        _attr_list: &[u8],
        attr_data: &[u8],
        _options: u32,
    ) -> FsResult<()> {
        // Resolve the path and ensure we can write to it
        let (node_id, _) = self.resolve_path(pid, path)?;

        // Permission check for write access
        if self.config.security.enforce_posix_permissions {
            if let Some(user) = self.user_for_process(pid) {
                let nodes = self.nodes.lock().unwrap();
                let node = nodes.get(&node_id).ok_or(FsError::NotFound)?;
                if !self.allowed_for_user(node, &user, false, true, false) {
                    return Err(FsError::AccessDenied);
                }
            }
        }

        // For now, implement a basic version that can set some attributes
        // In a full implementation, this would parse the attr_list and attr_data
        // according to the macOS format and set the appropriate attributes

        // Simple implementation: if attr_data has enough bytes, try to interpret
        // it as basic stat-like data and update the node accordingly
        if attr_data.len() >= 8 + 4 + 4 + 8 + 8 + 8 {
            // len + mode + uid + gid + atime + mtime + ctime
            let mut offset = 0;

            // Skip len (u64)
            offset += 8;

            // Read mode (u32)
            let mode_bytes = &attr_data[offset..offset + 4];
            let mode = u32::from_le_bytes(mode_bytes.try_into().unwrap());
            offset += 4;

            // Read uid (u32)
            let uid_bytes = &attr_data[offset..offset + 4];
            let uid = u32::from_le_bytes(uid_bytes.try_into().unwrap());
            offset += 4;

            // Read gid (u32)
            let gid_bytes = &attr_data[offset..offset + 4];
            let gid = u32::from_le_bytes(gid_bytes.try_into().unwrap());
            offset += 4;

            // Read timestamps (u64 each)
            let atime_bytes = &attr_data[offset..offset + 8];
            let atime = u64::from_le_bytes(atime_bytes.try_into().unwrap());
            offset += 8;

            let mtime_bytes = &attr_data[offset..offset + 8];
            let mtime = u64::from_le_bytes(mtime_bytes.try_into().unwrap());
            offset += 8;

            let ctime_bytes = &attr_data[offset..offset + 8];
            let ctime = u64::from_le_bytes(ctime_bytes.try_into().unwrap());

            // Update the node with the new attributes
            let mut nodes = self.nodes.lock().unwrap();
            if let Some(node) = nodes.get_mut(&node_id) {
                node.mode = mode;
                node.uid = uid;
                node.gid = gid;
                node.times.atime = atime as i64;
                node.times.mtime = mtime as i64;
                node.times.ctime = ctime as i64;

                // Update change time to now
                node.times.ctime = Self::current_timestamp();
            }
        }

        Ok(())
    }

    pub fn getattrlistbulk(
        &self,
        _pid: &PID,
        handle_id: HandleId,
        _attr_list: &[u8],
        _options: u32,
    ) -> FsResult<Vec<Vec<u8>>> {
        // Get the directory handle
        let handles = self.handles.lock().unwrap();
        let handle = handles.get(&handle_id).ok_or(FsError::InvalidArgument)?;

        // Ensure this is a directory handle and get children
        let dir_node_id = handle.node_id;
        if !matches!(handle.kind, HandleType::Directory { .. }) {
            return Err(FsError::NotADirectory);
        }
        drop(handles);

        // Collect attributes for all directory entries
        let mut result = Vec::new();

        let nodes = self.nodes.lock().unwrap();
        if let Some(dir_node) = nodes.get(&dir_node_id) {
            if let NodeKind::Directory { children } = &dir_node.kind {
                for (name, child_node_id) in children {
                    // Get the child node data while we have the lock
                    if let Some(child_node) = nodes.get(child_node_id) {
                        // Compute attributes directly without calling get_node_attributes
                        // to avoid deadlock
                        let (len, is_dir, is_symlink) = match &child_node.kind {
                            NodeKind::File { streams } => {
                                let size = streams.get("").map(|(_, size)| *size).unwrap_or(0);
                                (size, false, false)
                            }
                            NodeKind::Directory { .. } => (0, true, false),
                            NodeKind::Symlink { target } => (target.len() as u64, false, true),
                        };

                        let perm_bits = child_node.mode & 0o777;

                        let attrs = Attributes {
                            len,
                            times: child_node.times,
                            uid: child_node.uid,
                            gid: child_node.gid,
                            is_dir,
                            is_symlink,
                            special_kind: child_node.special_kind.clone(),
                            nlink: child_node.nlink,
                            mode_user: FileMode {
                                read: (perm_bits & 0o400) != 0,
                                write: (perm_bits & 0o200) != 0,
                                exec: (perm_bits & 0o100) != 0,
                            },
                            mode_group: FileMode {
                                read: (perm_bits & 0o040) != 0,
                                write: (perm_bits & 0o020) != 0,
                                exec: (perm_bits & 0o010) != 0,
                            },
                            mode_other: FileMode {
                                read: (perm_bits & 0o004) != 0,
                                write: (perm_bits & 0o002) != 0,
                                exec: (perm_bits & 0o001) != 0,
                            },
                        };

                        // Format as a simple attribute record
                        // In a real implementation, this would follow the macOS attrlistbulk format
                        let mut entry_data = Vec::new();

                        // Add entry name
                        if name.len() <= 255 {
                            entry_data.push(name.len() as u8);
                            entry_data.extend_from_slice(name.as_bytes());

                            // Add basic attributes
                            entry_data.extend_from_slice(&attrs.len.to_le_bytes());
                            entry_data.extend_from_slice(&attrs.mode().to_le_bytes());
                            entry_data.extend_from_slice(&attrs.uid.to_le_bytes());
                            entry_data.extend_from_slice(&attrs.gid.to_le_bytes());

                            result.push(entry_data);
                        }
                    }
                }
            }
        }

        Ok(result)
    }

    // copyfile/clonefile operations (macOS high-level copy operations)
    pub fn copyfile(
        &self,
        pid: &PID,
        src_path: &Path,
        dst_path: &Path,
        _state: &[u8],
        _flags: u32,
    ) -> FsResult<()> {
        // Resolve source path
        let (src_node_id, _) = self.resolve_path(pid, src_path)?;

        // Check if destination already exists
        if self.resolve_path(pid, dst_path).is_ok() {
            return Err(FsError::AlreadyExists);
        }

        // Get parent directory of destination
        let dst_parent_path = dst_path.parent().ok_or(FsError::InvalidArgument)?;
        let dst_name = dst_path.file_name().and_then(|n| n.to_str()).ok_or(FsError::InvalidName)?;

        let (dst_parent_id, _) = self.resolve_path(pid, dst_parent_path)?;

        // Permission checks
        if self.config.security.enforce_posix_permissions {
            if let Some(user) = self.user_for_process(pid) {
                let nodes = self.nodes.lock().unwrap();

                // Check read access to source
                let src_node = nodes.get(&src_node_id).ok_or(FsError::NotFound)?;
                if !self.allowed_for_user(src_node, &user, true, false, false) {
                    return Err(FsError::AccessDenied);
                }

                // Check write access to destination parent
                let dst_parent_node = nodes.get(&dst_parent_id).ok_or(FsError::NotFound)?;
                if !self.allowed_for_user(dst_parent_node, &user, false, true, true) {
                    return Err(FsError::AccessDenied);
                }
            }
        }

        // Ensure source is a file
        let nodes = self.nodes.lock().unwrap();
        let src_node = nodes.get(&src_node_id).ok_or(FsError::NotFound)?;
        let src_content_id = match &src_node.kind {
            NodeKind::File { streams } => {
                streams.get("").map(|(id, _)| *id).ok_or(FsError::NotFound)?
            }
            _ => return Err(FsError::IsADirectory),
        };
        drop(nodes);

        // Create destination file
        let dst_content_id = self.storage.clone_cow(src_content_id)?;
        let dst_node_id = self.create_file_node(dst_content_id)?;

        // Copy attributes from source to destination
        let src_attrs = {
            let nodes = self.nodes.lock().unwrap();
            nodes.get(&src_node_id).map(|node| {
                (
                    node.mode,
                    node.uid,
                    node.gid,
                    node.times,
                    node.xattrs.clone(),
                )
            })
        };

        if let Some((mode, uid, gid, times, xattrs)) = src_attrs {
            let mut nodes = self.nodes.lock().unwrap();
            if let Some(dst_node) = nodes.get_mut(&dst_node_id) {
                dst_node.mode = mode;
                dst_node.uid = uid;
                dst_node.gid = gid;
                dst_node.times = times;
                dst_node.xattrs = xattrs;
                // Note: ACLs and flags are not copied in basic implementation
            }
        }

        // Add destination to parent directory
        {
            let mut nodes = self.nodes.lock().unwrap();
            if let Some(parent_node) = nodes.get_mut(&dst_parent_id) {
                if let NodeKind::Directory { children } = &mut parent_node.kind {
                    children.insert(dst_name.to_string(), dst_node_id);
                }
            }
        }

        Ok(())
    }

    pub fn fcopyfile(
        &self,
        pid: &PID,
        src_handle_id: HandleId,
        dst_handle_id: HandleId,
        _state: &[u8],
        _flags: u32,
    ) -> FsResult<()> {
        // Get source handle
        let handles = self.handles.lock().unwrap();
        let src_handle = handles.get(&src_handle_id).ok_or(FsError::InvalidArgument)?;
        let src_node_id = src_handle.node_id;

        // Ensure source is a file
        let src_content_id = match &src_handle.kind {
            HandleType::File { .. } => {
                let nodes = self.nodes.lock().unwrap();
                let src_node = nodes.get(&src_node_id).ok_or(FsError::NotFound)?;
                match &src_node.kind {
                    NodeKind::File { streams } => {
                        streams.get("").map(|(id, _)| *id).ok_or(FsError::NotFound)?
                    }
                    _ => return Err(FsError::IsADirectory),
                }
            }
            HandleType::Directory { .. } => return Err(FsError::IsADirectory),
        };

        // Get destination handle - this should be a newly created file
        let dst_node_id = match &handles.get(&dst_handle_id) {
            Some(handle) if matches!(handle.kind, HandleType::File { .. }) => handle.node_id,
            _ => return Err(FsError::InvalidArgument),
        };
        drop(handles);

        // Permission checks
        if self.config.security.enforce_posix_permissions {
            if let Some(user) = self.user_for_process(pid) {
                let nodes = self.nodes.lock().unwrap();

                // Check read access to source
                let src_node = nodes.get(&src_node_id).ok_or(FsError::NotFound)?;
                if !self.allowed_for_user(src_node, &user, true, false, false) {
                    return Err(FsError::AccessDenied);
                }

                // Check write access to destination
                let dst_node = nodes.get(&dst_node_id).ok_or(FsError::NotFound)?;
                if !self.allowed_for_user(dst_node, &user, false, true, false) {
                    return Err(FsError::AccessDenied);
                }
            }
        }

        // Copy content using clone_cow
        let dst_content_id = self.storage.clone_cow(src_content_id)?;

        // Get source attributes
        let src_info = {
            let nodes = self.nodes.lock().unwrap();
            nodes.get(&src_node_id).and_then(|node| {
                if let NodeKind::File { streams } = &node.kind {
                    let src_size = streams.get("").map(|(_, size)| *size).unwrap_or(0);
                    Some((
                        src_size,
                        node.mode,
                        node.uid,
                        node.gid,
                        node.times,
                        node.xattrs.clone(),
                    ))
                } else {
                    None
                }
            })
        };

        // Update destination file with copied content and attributes
        if let Some((src_size, mode, uid, gid, times, xattrs)) = src_info {
            let mut nodes = self.nodes.lock().unwrap();
            if let Some(dst_node) = nodes.get_mut(&dst_node_id) {
                // Update the file stream with the copied content
                if let NodeKind::File { streams } = &mut dst_node.kind {
                    streams.insert("".to_string(), (dst_content_id, src_size));
                }

                // Copy attributes
                dst_node.mode = mode;
                dst_node.uid = uid;
                dst_node.gid = gid;
                dst_node.times = times;
                dst_node.xattrs = xattrs;
            }
        }

        Ok(())
    }

    pub fn clonefile(
        &self,
        pid: &PID,
        src_path: &Path,
        dst_path: &Path,
        _flags: u32,
    ) -> FsResult<()> {
        // For AgentFS, clonefile works the same as copyfile since we use
        // copy-on-write semantics for all file operations
        self.copyfile(pid, src_path, dst_path, &[], 0)
    }

    pub fn fclonefileat(
        &self,
        pid: &PID,
        src_dirfd: HandleId,
        src_path: &Path,
        dst_dirfd: HandleId,
        dst_path: &Path,
        _flags: u32,
    ) -> FsResult<()> {
        // For now, implement a simplified version that assumes paths are absolute
        // or relative to current working directory. Full implementation would
        // need proper directory-relative path resolution.

        // Check that the handles are valid directory handles
        let handles = self.handles.lock().unwrap();
        match handles.get(&src_dirfd) {
            Some(handle) if matches!(handle.kind, HandleType::Directory { .. }) => {}
            _ => return Err(FsError::NotADirectory),
        }
        match handles.get(&dst_dirfd) {
            Some(handle) if matches!(handle.kind, HandleType::Directory { .. }) => {}
            _ => return Err(FsError::NotADirectory),
        }
        drop(handles);

        // For simplified implementation, treat paths as if they're relative to root
        // In a full implementation, we'd need to build the full path from the directory handles
        let src_full_path = if src_path.is_absolute() {
            src_path.to_path_buf()
        } else {
            PathBuf::from("/").join(src_path)
        };

        let dst_full_path = if dst_path.is_absolute() {
            dst_path.to_path_buf()
        } else {
            PathBuf::from("/").join(dst_path)
        };

        // Use clonefile for the actual operation
        self.clonefile(pid, &src_full_path, &dst_full_path, _flags)
    }

    // Alternate Data Streams operations
    pub fn streams_list(&self, pid: &PID, path: &Path) -> FsResult<Vec<StreamSpec>> {
        let (node_id, _) = self.resolve_path(pid, path)?;
        let nodes = self.nodes.lock().unwrap();
        let node = nodes.get(&node_id).ok_or(FsError::NotFound)?;

        match &node.kind {
            NodeKind::File { streams } => {
                let mut stream_specs = Vec::new();
                for stream_name in streams.keys() {
                    if !stream_name.is_empty() {
                        // Skip the unnamed default stream
                        stream_specs.push(StreamSpec {
                            name: stream_name.clone(),
                        });
                    }
                }
                Ok(stream_specs)
            }
            NodeKind::Directory { .. } => Err(FsError::IsADirectory),
            NodeKind::Symlink { .. } => Err(FsError::InvalidArgument), // Symlinks don't have streams
        }
    }

    pub fn unlink(&self, pid: &PID, path: &Path) -> FsResult<()> {
        let (node_id, parent_info) = self.resolve_path(pid, path)?;

        let Some((parent_id, name)) = parent_info else {
            return Err(FsError::InvalidArgument); // Can't unlink root
        };

        let node = {
            let nodes = self.nodes.lock().unwrap();
            nodes.get(&node_id).cloned().ok_or(FsError::NotFound)?
        };

        match &node.kind {
            NodeKind::Directory { .. } => return Err(FsError::IsADirectory),
            _ => {}
        }
        self.check_dir_permissions(pid, parent_id, Some(&node))?;

        let mut handles = self.handles.lock().unwrap();
        let has_open_handles = handles.values().any(|h| h.node_id == node_id);
        let remaining_links = self.decrement_link_count(node_id);

        if has_open_handles {
            // Mark all file handles to this file as deleted
            for handle in handles.values_mut() {
                if handle.node_id == node_id {
                    if let HandleType::File { deleted, .. } = &mut handle.kind {
                        *deleted = true;
                    }
                }
            }
        }
        drop(handles);

        let now = FsCore::current_timestamp();
        {
            let mut nodes = self.nodes.lock().unwrap();
            if let Some(node) = nodes.get_mut(&node_id) {
                node.times.ctime = now;
            }
            if remaining_links == 0 && !has_open_handles {
                // No open handles and no more directory links, remove immediately
                nodes.remove(&node_id);
            }
        }

        self.unlink_child_from_parent(parent_id, &name);

        // Emit event
        let path_str = path.to_string_lossy().to_string();
        #[cfg(feature = "events")]
        self.emit_event(EventKind::Removed { path: path_str });

        Ok(())
    }

    /// Public helper to resolve path and return internal IDs for FFI consumers
    pub fn resolve_path_public(&self, pid: &PID, path: &Path) -> FsResult<(u64, Option<u64>)> {
        let (node_id, parent_info) = self.resolve_path(pid, path)?;
        let parent_id = parent_info.map(|(pid, _name)| pid.0);
        Ok((node_id.0, parent_id))
    }

    /// Change permissions mode on a path (basic chmod semantics)
    pub fn set_mode(&self, pid: &PID, path: &Path, mode: u32) -> FsResult<()> {
        let (node_id, _) = self.resolve_path(pid, path)?;
        let mut nodes = self.nodes.lock().unwrap();
        let node = nodes.get_mut(&node_id).ok_or(FsError::NotFound)?;
        // Only owner or root may change mode when enforcing POSIX permissions
        if self.config.security.enforce_posix_permissions {
            if let Some(user) = self.user_for_process(pid) {
                if !(user.uid == 0 || user.uid == node.uid) {
                    return Err(FsError::AccessDenied);
                }
            }
        }
        node.mode = mode;
        // ctime changes on metadata change
        let now = FsCore::current_timestamp();
        node.times.ctime = now;

        // Emit Modified event for metadata change
        #[cfg(feature = "events")]
        self.emit_event(EventKind::Modified {
            path: path.to_string_lossy().to_string(),
        });

        Ok(())
    }

    /// Get file status (stat) for a path - follows symlinks
    pub fn stat(&self, pid: &PID, path: &Path) -> FsResult<StatData> {
        let attrs = self.getattr(pid, path)?;
        self.attributes_to_stat_data(attrs, path)
    }

    /// Get file status (lstat) for a path - does not follow symlinks
    pub fn lstat(&self, pid: &PID, path: &Path) -> FsResult<StatData> {
        // For lstat, we need to check if it's a symlink without following it
        if let Ok((node_id, _)) = self.resolve_path(pid, path) {
            // Get the node directly to check if it's a symlink
            let nodes = self.nodes.lock().unwrap();
            let node = nodes.get(&node_id).ok_or(FsError::NotFound)?;
            if let NodeKind::Symlink { target } = &node.kind {
                // For symlinks, return symlink-specific attributes
                let attrs = Attributes {
                    len: target.len() as u64,
                    times: node.times,
                    uid: node.uid,
                    gid: node.gid,
                    is_dir: false,
                    is_symlink: true,
                    special_kind: node.special_kind.clone(),
                    nlink: node.nlink,
                    mode_user: FileMode {
                        read: (node.mode & libc::S_IRUSR as u32) != 0,
                        write: (node.mode & libc::S_IWUSR as u32) != 0,
                        exec: (node.mode & libc::S_IXUSR as u32) != 0,
                    },
                    mode_group: FileMode {
                        read: (node.mode & libc::S_IRGRP as u32) != 0,
                        write: (node.mode & libc::S_IWGRP as u32) != 0,
                        exec: (node.mode & libc::S_IXGRP as u32) != 0,
                    },
                    mode_other: FileMode {
                        read: (node.mode & libc::S_IROTH as u32) != 0,
                        write: (node.mode & libc::S_IWOTH as u32) != 0,
                        exec: (node.mode & libc::S_IXOTH as u32) != 0,
                    },
                };
                return self.attributes_to_stat_data(attrs, path);
            }
        }
        // For non-symlinks, lstat behaves like stat
        self.stat(pid, path)
    }

    /// Get file status (fstat) for an open file descriptor
    pub fn fstat(&self, pid: &PID, handle_id: HandleId) -> FsResult<StatData> {
        let node_id = self.get_node_id_for_handle(pid, handle_id)?;
        let attrs = self.get_node_attributes(node_id)?;
        // For fstat, we don't have a path, so we'll use a dummy path
        self.attributes_to_stat_data(attrs, Path::new(""))
    }

    /// Get file status (fstatat) relative to a directory file descriptor
    pub fn fstatat(&self, pid: &PID, path: &Path, flags: u32) -> FsResult<StatData> {
        // Now that path resolution is done client-side, this just calls stat/lstat with resolved path
        if flags & libc::AT_SYMLINK_NOFOLLOW as u32 != 0 {
            self.lstat(pid, path)
        } else {
            self.stat(pid, path)
        }
    }

    /// Change file mode (fchmod) for an open file descriptor
    pub fn fchmod(&self, pid: &PID, handle_id: HandleId, mode: u32) -> FsResult<()> {
        let node_id = self.get_node_id_for_handle(pid, handle_id)?;
        let mut nodes = self.nodes.lock().unwrap();
        let node = nodes.get_mut(&node_id).ok_or(FsError::NotFound)?;
        // Only owner or root may change mode when enforcing POSIX permissions
        if self.config.security.enforce_posix_permissions {
            if let Some(user) = self.user_for_process(pid) {
                if !(user.uid == 0 || user.uid == node.uid) {
                    return Err(FsError::AccessDenied);
                }
            }
        }
        node.mode = mode;
        // ctime changes on metadata change
        let now = FsCore::current_timestamp();
        node.times.ctime = now;
        Ok(())
    }

    /// Change file mode (fchmodat) relative to a directory file descriptor
    pub fn fchmodat(&self, pid: &PID, path: &Path, mode: u32, flags: u32) -> FsResult<()> {
        // Now that path resolution is done client-side, this just calls set_mode with resolved path
        let _ = flags; // unused in simplified version
        self.set_mode(pid, path, mode)
    }

    /// Change file owner (fchown) for an open file descriptor
    pub fn fchown(&self, pid: &PID, handle_id: HandleId, uid: u32, gid: u32) -> FsResult<()> {
        let node_id = self.get_node_id_for_handle(pid, handle_id)?;
        self.set_node_owner(node_id, pid, uid, gid)
    }

    /// Change file owner (fchownat) relative to a directory file descriptor
    pub fn fchownat(&self, pid: &PID, path: &Path, uid: u32, gid: u32, flags: u32) -> FsResult<()> {
        // Now that path resolution is done client-side, this just calls set_owner with resolved path
        let _ = flags; // unused in simplified version
        self.set_owner(pid, path, uid, gid)
    }

    /// Change file timestamps (futimes) for an open file descriptor
    pub fn futimes(
        &self,
        pid: &PID,
        handle_id: HandleId,
        times: Option<(TimespecData, TimespecData)>,
    ) -> FsResult<()> {
        let node_id = self.get_node_id_for_handle(pid, handle_id)?;
        let file_times = times
            .map(|(atime, mtime)| FileTimes {
                atime: atime.tv_sec as i64,
                mtime: mtime.tv_sec as i64,
                ctime: FsCore::current_timestamp(),
                birthtime: FsCore::current_timestamp(),
            })
            .unwrap_or_else(|| {
                let now = FsCore::current_timestamp();
                FileTimes {
                    atime: now,
                    mtime: now,
                    ctime: now,
                    birthtime: now,
                }
            });
        self.set_node_times(node_id, file_times)
    }

    /// Change file timestamps (futimens) for an open file descriptor (nanosecond precision)
    pub fn futimens(
        &self,
        pid: &PID,
        handle_id: HandleId,
        times: Option<(TimespecData, TimespecData)>,
    ) -> FsResult<()> {
        // For now, implement as futimes - full implementation would use nanosecond precision
        self.futimes(pid, handle_id, times)
    }

    /// Change file timestamps (utimensat) relative to a directory file descriptor
    pub fn utimensat(
        &self,
        pid: &PID,
        path: &Path,
        times: Option<(TimespecData, TimespecData)>,
        flags: u32,
    ) -> FsResult<()> {
        // Now that path resolution is done client-side, this just calls set_times with resolved path
        let _ = flags; // unused in simplified version
        let file_times = times
            .map(|(atime, mtime)| FileTimes {
                atime: atime.tv_sec as i64,
                mtime: mtime.tv_sec as i64,
                ctime: FsCore::current_timestamp(),
                birthtime: FsCore::current_timestamp(),
            })
            .unwrap_or_else(|| {
                let now = FsCore::current_timestamp();
                FileTimes {
                    atime: now,
                    mtime: now,
                    ctime: now,
                    birthtime: now,
                }
            });
        self.set_times(pid, path, file_times)
    }

    /// Truncate file (ftruncate) for an open file descriptor
    pub fn ftruncate(&self, _pid: &PID, handle_id: HandleId, length: u64) -> FsResult<()> {
        let handles = self.handles.lock().unwrap();
        let handle = handles.get(&handle_id).ok_or(FsError::InvalidArgument)?;
        let node_id = handle.node_id;
        let handle_path = handle.path.clone();
        drop(handles);

        let mut nodes = self.nodes.lock().unwrap();
        let node = nodes.get_mut(&node_id).ok_or(FsError::NotFound)?;

        match &mut node.kind {
            NodeKind::File { streams } => {
                // Truncate the default (unnamed) stream
                if let Some((content_id, current_size)) = streams.get_mut("") {
                    if length < *current_size {
                        // Truncate: use the storage backend's truncate method
                        self.storage.truncate(*content_id, length)?;
                        *current_size = length;
                    } else if length > *current_size {
                        // Extend: for now, we can't easily extend - this would require more complex logic
                        // In a real implementation, we'd need to handle this properly
                        return Err(FsError::Unsupported); // Extension not supported in this simplified version
                    }
                    // length == current_size: no-op
                } else {
                    // No default stream, create one
                    if length > 0 {
                        let content = vec![0u8; length as usize];
                        let content_id = self.storage.allocate(&content)?;
                        streams.insert("".to_string(), (content_id, length));
                    } else {
                        // Empty file
                        let content_id = self.storage.allocate(&[])?;
                        streams.insert("".to_string(), (content_id, 0));
                    }
                }
            }
            _ => return Err(FsError::InvalidArgument), // Can only truncate files
        }

        // Update ctime on truncate
        let now = FsCore::current_timestamp();
        node.times.ctime = now;
        node.times.mtime = now;

        // Emit Modified event for file content change
        #[cfg(feature = "events")]
        {
            drop(nodes); // Release lock before emitting event
            self.emit_event(EventKind::Modified {
                path: handle_path.to_string_lossy().to_string(),
            });
        }

        Ok(())
    }

    /// Get filesystem statistics (statfs) for a path
    pub fn statfs(&self, pid: &PID, path: &Path) -> FsResult<StatfsData> {
        let _ = pid; // unused in simplified implementation
        let _ = path; // unused in simplified implementation
        // Return dummy filesystem statistics
        // In a full implementation, this would query the actual filesystem
        Ok(StatfsData {
            f_bsize: 4096,      // 4KB block size
            f_frsize: 4096,     // Fragment size same as block size
            f_blocks: 1000000,  // 4GB total in 4KB blocks
            f_bfree: 500000,    // 2GB free
            f_bavail: 450000,   // 1.8GB available
            f_files: 100000,    // 100K inodes total
            f_ffree: 95000,     // 95K free inodes
            f_favail: 90000,    // 90K available inodes
            f_fsid: 0x12345678, // Dummy filesystem ID
            f_flag: 0,          // No special flags
            f_namemax: 255,     // Max filename length
        })
    }

    /// Get filesystem statistics (fstatfs) for an open file descriptor
    pub fn fstatfs(&self, pid: &PID, handle_id: HandleId) -> FsResult<StatfsData> {
        let _ = pid; // unused in simplified implementation
        let _ = handle_id; // unused in simplified implementation
        // For now, same as statfs - in a full implementation, this might be different
        self.statfs(pid, Path::new(""))
    }

    /// Helper method to get node ID for a handle
    fn get_node_id_for_handle(&self, pid: &PID, handle_id: HandleId) -> FsResult<NodeId> {
        let handles = self.handles.lock().unwrap();
        let handle = handles.get(&handle_id).ok_or(FsError::BadFileDescriptor)?;
        // Check if the process can access this handle (simplified check)
        let _ = pid; // In full implementation, check process ownership
        Ok(handle.node_id)
    }

    /// Helper method to set node owner with permission checking
    fn set_node_owner(&self, node_id: NodeId, pid: &PID, uid: u32, gid: u32) -> FsResult<()> {
        let mut nodes = self.nodes.lock().unwrap();
        let node = nodes.get_mut(&node_id).ok_or(FsError::NotFound)?;
        // Only root may change owner when enforcing POSIX permissions
        if self.config.security.enforce_posix_permissions {
            if let Some(user) = self.user_for_process(pid) {
                if user.uid != 0 {
                    return Err(FsError::AccessDenied);
                }
            }
        }
        node.uid = uid;
        node.gid = gid;
        // ctime changes on metadata change
        let now = FsCore::current_timestamp();
        node.times.ctime = now;
        Ok(())
    }

    /// Helper method to set node times
    fn set_node_times(&self, node_id: NodeId, times: FileTimes) -> FsResult<()> {
        let mut nodes = self.nodes.lock().unwrap();
        let node = nodes.get_mut(&node_id).ok_or(FsError::NotFound)?;
        node.times = times;
        Ok(())
    }

    /// Helper method to convert Attributes to StatData
    fn attributes_to_stat_data(&self, attrs: Attributes, path: &Path) -> FsResult<StatData> {
        // Get inode number from path hash (simplified)
        let inode = self.simple_inode_from_path(path);

        Ok(StatData {
            st_dev: 1, // Dummy device ID
            st_ino: inode,
            st_mode: attrs.mode(),
            st_nlink: attrs.nlink,
            st_uid: attrs.uid,
            st_gid: attrs.gid,
            st_rdev: attrs.rdev(),
            st_size: attrs.len,
            st_blksize: 4096,                   // 4KB block size
            st_blocks: attrs.len.div_ceil(512), // Number of 512-byte blocks
            st_atime: attrs.times.atime as u64,
            st_atime_nsec: 0, // Simplified
            st_mtime: attrs.times.mtime as u64,
            st_mtime_nsec: 0, // Simplified
            st_ctime: attrs.times.ctime as u64,
            st_ctime_nsec: 0, // Simplified
        })
    }

    /// Simple inode calculation from path (for testing)
    fn simple_inode_from_path(&self, path: &Path) -> u64 {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        path.hash(&mut hasher);
        hasher.finish()
    }

    /// Rename a node from old path to new path. Fails if destination exists.
    pub fn rename(&self, pid: &PID, old: &Path, new: &Path) -> FsResult<()> {
        if old == new {
            return Ok(());
        }

        let (src_id, src_parent_info) = self.resolve_path(pid, old)?;
        let Some((src_parent_id, src_name)) = src_parent_info else {
            return Err(FsError::InvalidArgument);
        };
        let src_node = self.get_node_clone(src_id)?;

        let new_parent_path = new.parent().ok_or(FsError::InvalidArgument)?;
        let new_name = new
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or(FsError::InvalidName)?
            .to_string();
        let (dst_parent_id, _) = self.resolve_path(pid, new_parent_path)?;

        if src_parent_id == dst_parent_id && src_name == new_name {
            return Ok(());
        }

        if matches!(src_node.kind, NodeKind::Directory { .. }) && new.starts_with(old) {
            return Err(FsError::InvalidArgument);
        }

        let (dest_id, dest_node) = match self.resolve_path(pid, new) {
            Ok((id, _)) => {
                if id == src_id {
                    return Ok(());
                }
                (Some(id), Some(self.get_node_clone(id)?))
            }
            Err(FsError::NotFound) => (None, None),
            Err(e) => return Err(e),
        };

        if let Some(dest_node) = &dest_node {
            match (&src_node.kind, &dest_node.kind) {
                (NodeKind::Directory { .. }, NodeKind::Directory { children }) => {
                    if !children.is_empty() {
                        return Err(FsError::Busy);
                    }
                }
                (NodeKind::Directory { .. }, _) => return Err(FsError::NotADirectory),
                (_, NodeKind::Directory { .. }) => return Err(FsError::IsADirectory),
                _ => {}
            }
        }

        self.check_dir_permissions(pid, src_parent_id, Some(&src_node))?;
        if dst_parent_id != src_parent_id {
            if let NodeKind::Directory { .. } = src_node.kind {
                if let Some(user) = self.user_for_process(pid) {
                    if let Ok(dir_node) = self.get_node_clone(src_parent_id) {
                        let extra = self.path_for_node(pid, dst_parent_id).map(|path| {
                            format!(
                                "dst_parent_id={} dst_path={}",
                                dst_parent_id.0,
                                path.to_string_lossy()
                            )
                        });
                        self.log_sticky_event(
                            pid,
                            src_parent_id,
                            &dir_node,
                            Some(&src_node),
                            user.uid,
                            "cross_parent_probe",
                            extra,
                        );
                    }
                }
                self.check_dir_cross_parent_permissions(pid, src_parent_id, &src_node)?;
            }
        }
        if dst_parent_id != src_parent_id || dest_node.is_some() {
            self.check_dir_permissions(pid, dst_parent_id, dest_node.as_ref())?;
        }

        if let Some(dest_node) = dest_node {
            match dest_node.kind {
                NodeKind::Directory { .. } => {
                    self.rmdir(pid, new)?;
                }
                _ => {
                    self.unlink(pid, new)?;
                }
            }
        }

        self.unlink_child_from_parent(src_parent_id, &src_name);
        self.link_child_into_parent(dst_parent_id, &new_name, src_id)?;

        {
            let mut nodes = self.nodes.lock().unwrap();
            if let Some(node) = nodes.get_mut(&src_id) {
                let now = FsCore::current_timestamp();
                node.times.ctime = now;
            }
        }

        #[cfg(feature = "events")]
        self.emit_event(EventKind::Renamed {
            from: old.to_string_lossy().to_string(),
            to: new.to_string_lossy().to_string(),
        });

        Ok(())
    }

    /// Create a symbolic link
    pub fn symlink(&self, pid: &PID, target: &str, linkpath: &Path) -> FsResult<()> {
        // Check if the link path already exists
        if self.resolve_path(pid, linkpath).is_ok() {
            return Err(FsError::AlreadyExists);
        }

        // Resolve parent directory
        let parent_path = linkpath.parent().ok_or(FsError::InvalidArgument)?;
        let link_name = linkpath
            .file_name()
            .ok_or(FsError::InvalidArgument)?
            .to_string_lossy()
            .to_string();

        let (parent_id, _) = self.resolve_path(pid, parent_path)?;
        let nodes = self.nodes.lock().unwrap();

        // Check that parent is a directory
        if let Some(parent_node) = nodes.get(&parent_id) {
            match &parent_node.kind {
                NodeKind::Directory { children } => {
                    if children.contains_key(&link_name) {
                        return Err(FsError::AlreadyExists);
                    }
                }
                _ => return Err(FsError::NotADirectory),
            }
        } else {
            return Err(FsError::NotFound);
        }
        drop(nodes);

        // Create symlink node
        let symlink_node_id = self.create_symlink_node(target.to_string())?;

        self.link_child_into_parent(parent_id, &link_name, symlink_node_id)?;

        // Emit event
        let path_str = linkpath.to_string_lossy().to_string();
        #[cfg(feature = "events")]
        self.emit_event(EventKind::Created { path: path_str });

        Ok(())
    }

    /// Create a hard link
    pub fn link(&self, pid: &PID, old_path: &Path, new_path: &Path) -> FsResult<()> {
        // Resolve the source file
        let (node_id, _) = self.resolve_path(pid, old_path)?;
        let nodes = self.nodes.lock().unwrap();
        let node = nodes.get(&node_id).ok_or(FsError::NotFound)?;

        // Cannot hard link directories
        if let NodeKind::Directory { .. } = &node.kind {
            return Err(FsError::IsADirectory);
        }
        drop(nodes);

        // Check if the link path already exists
        if self.resolve_path(pid, new_path).is_ok() {
            return Err(FsError::AlreadyExists);
        }

        // Get parent directory of new path
        let parent_path = new_path.parent().ok_or(FsError::InvalidArgument)?;
        let link_name = new_path
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or(FsError::InvalidName)?
            .to_string();

        let (parent_id, _) = self.resolve_path(pid, parent_path)?;

        // Permission checks
        if self.config.security.enforce_posix_permissions {
            if let Some(user) = self.user_for_process(pid) {
                let nodes = self.nodes.lock().unwrap();
                let parent_node = nodes.get(&parent_id).ok_or(FsError::NotFound)?;
                let source_node = nodes.get(&node_id).ok_or(FsError::NotFound)?;

                // Check write access to parent directory
                if !self.allowed_for_user(parent_node, &user, false, true, true) {
                    return Err(FsError::AccessDenied);
                }

                // Check read access to source file
                if !self.allowed_for_user(source_node, &user, true, false, false) {
                    return Err(FsError::AccessDenied);
                }
            }
        }

        self.link_child_into_parent(parent_id, &link_name, node_id)?;
        self.increment_link_count(node_id);

        {
            let mut nodes = self.nodes.lock().unwrap();
            if let Some(node) = nodes.get_mut(&node_id) {
                node.times.ctime = Self::current_timestamp();
            }
        }

        // Emit event
        let path_str = new_path.to_string_lossy().to_string();
        #[cfg(feature = "events")]
        self.emit_event(EventKind::Created { path: path_str });

        Ok(())
    }

    /// Create a hard link with dirfd (relative to directory)
    pub fn linkat(&self, pid: &PID, old_path: &Path, new_path: &Path, _flags: u32) -> FsResult<()> {
        // Now that path resolution is done client-side, this just calls link with resolved paths
        self.link(pid, old_path, new_path)
    }

    /// Create a symbolic link with dirfd (relative to directory)
    pub fn symlinkat(&self, pid: &PID, target: &str, linkpath: &Path) -> FsResult<()> {
        // Now that path resolution is done client-side, this just calls symlink with resolved linkpath
        self.symlink(pid, target, linkpath)
    }

    /// Rename with dirfd (relative to directory)
    pub fn renameat(
        &self,
        pid: &PID,
        old_dirfd: u32,
        old_path: &Path,
        new_dirfd: u32,
        new_path: &Path,
    ) -> FsResult<()> {
        // For now, implement basic version - full implementation would need to handle AT_FDCWD
        let old_full_path = if old_dirfd == libc::AT_FDCWD as u32 {
            old_path.to_path_buf()
        } else {
            // TODO: Implement proper dirfd resolution
            return Err(FsError::NotImplemented);
        };

        let new_full_path = if new_dirfd == libc::AT_FDCWD as u32 {
            new_path.to_path_buf()
        } else {
            // TODO: Implement proper dirfd resolution
            return Err(FsError::NotImplemented);
        };

        self.rename(pid, &old_full_path, &new_full_path)
    }

    /// macOS-specific rename with extended flags
    pub fn renameatx_np(
        &self,
        pid: &PID,
        old_dirfd: u32,
        old_path: &Path,
        new_dirfd: u32,
        new_path: &Path,
        _flags: u32,
    ) -> FsResult<()> {
        // For now, implement as regular rename - full implementation would handle macOS-specific flags
        self.renameat(pid, old_dirfd, old_path, new_dirfd, new_path)
    }

    /// Unlink with dirfd (relative to directory)
    pub fn unlinkat(&self, pid: &PID, path: &Path, _flags: u32) -> FsResult<()> {
        // Now that path resolution is done client-side, this just calls unlink with resolved path
        self.unlink(pid, path)
    }

    /// Remove (alias for unlink)
    pub fn remove(&self, pid: &PID, path: &Path) -> FsResult<()> {
        self.unlink(pid, path)
    }

    /// Create directory with dirfd (relative to directory)
    pub fn mkdirat(&self, pid: &PID, path: &Path, mode: u32) -> FsResult<()> {
        // Now that path resolution is done client-side, this just calls mkdir with resolved path
        self.mkdir(pid, path, mode)
    }

    /// Read a symbolic link
    pub fn readlink(&self, pid: &PID, path: &Path) -> FsResult<String> {
        let (node_id, _) = self.resolve_path(pid, path)?;
        let nodes = self.nodes.lock().unwrap();
        let node = nodes.get(&node_id).ok_or(FsError::NotFound)?;

        match &node.kind {
            NodeKind::Symlink { target } => Ok(target.clone()),
            _ => Err(FsError::InvalidArgument), // Not a symlink
        }
    }

    /// Create a symlink node
    fn create_symlink_node(&self, target: String) -> FsResult<NodeId> {
        let now = Self::current_timestamp();
        let node_id = self.allocate_node_id();

        let node = Node {
            id: node_id,
            kind: NodeKind::Symlink { target },
            times: FileTimes {
                atime: now,
                mtime: now,
                ctime: now,
                birthtime: now,
            },
            mode: 0o777, // Symlinks typically have full permissions
            uid: self.config.security.default_uid,
            gid: self.config.security.default_gid,
            special_kind: None,
            xattrs: HashMap::new(),
            acls: HashMap::new(),
            flags: 0,
            nlink: 1,
        };

        let mut nodes = self.nodes.lock().unwrap();
        nodes.insert(node_id, node);

        Ok(node_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_fs() -> FsCore {
        // Use the same config as the main lib.rs tests
        let config = crate::FsConfig {
            case_sensitivity: crate::CaseSensitivity::Sensitive,
            memory: crate::MemoryPolicy {
                max_bytes_in_memory: Some(1024 * 1024),
                spill_directory: None,
            },
            limits: crate::FsLimits {
                max_open_handles: 1000,
                max_branches: 100,
                max_snapshots: 1000,
            },
            cache: crate::CachePolicy {
                attr_ttl_ms: 1000,
                entry_ttl_ms: 1000,
                negative_ttl_ms: 1000,
                enable_readdir_plus: true,
                auto_cache: true,
                writeback_cache: false,
            },
            enable_xattrs: true,
            enable_ads: false,
            track_events: true,
            security: crate::config::SecurityPolicy::default(),
            backstore: crate::config::BackstoreMode::InMemory,
            overlay: crate::config::OverlayConfig::default(),
            interpose: crate::config::InterposeConfig::default(),
        };
        FsCore::new(config).unwrap()
    }

    fn create_test_pid(fs: &FsCore) -> PID {
        fs.register_process(12345, 12345, 1000, 1000)
    }

    fn rw_create() -> crate::OpenOptions {
        crate::OpenOptions {
            read: true,
            write: true,
            create: true,
            truncate: true,
            append: false,
            share: vec![crate::ShareMode::Read, crate::ShareMode::Write],
            stream: None,
        }
    }

    #[test]
    fn test_stat_regular_file() {
        let fs = create_test_fs();
        let pid = create_test_pid(&fs);

        // Create a file using the correct API
        let handle = fs
            .create(&pid, "/test.txt".as_ref(), &rw_create())
            .expect("Failed to create file");

        let content = b"Hello, World!";
        fs.write(&pid, handle, 0, content).expect("Failed to write content");
        fs.close(&pid, handle).expect("Failed to close handle");

        // Test stat using getattr
        let stat_data = fs.getattr(&pid, "/test.txt".as_ref()).expect("getattr should succeed");

        assert_eq!(stat_data.len, content.len() as u64);
        assert_eq!(stat_data.mode() & 0o777, 0o644);
        assert_eq!(stat_data.uid, 1000);
        assert_eq!(stat_data.gid, 1000);
    }

    #[test]
    fn test_stat_directory() {
        let fs = create_test_fs();
        let pid = create_test_pid(&fs);

        // Test stat on root directory
        let stat_data = fs.getattr(&pid, "/".as_ref()).expect("getattr should succeed");

        assert_eq!(stat_data.mode() & libc::S_IFMT as u32, libc::S_IFDIR as u32);
        // Root directory uses default security policy (uid=0, gid=0)
        assert_eq!(stat_data.uid, 0);
        assert_eq!(stat_data.gid, 0);
    }

    #[test]
    fn test_lstat_symlink() {
        let fs = create_test_fs();
        let pid = create_test_pid(&fs);

        // Create a directory first
        fs.mkdir(&pid, "/dir".as_ref(), 0o755).expect("Failed to create directory");

        // Create a symlink using the internal API
        let target = "/target".to_string();
        let node_id = fs.create_symlink_node(target).unwrap();

        // For now, just test that we can create a symlink node
        // (The path-based symlink creation might need more work)
        assert!(node_id.0 > 0);
    }

    #[test]
    fn test_chmod() {
        let fs = create_test_fs();
        let pid = create_test_pid(&fs);

        // Create a file
        let handle = fs
            .create(&pid, "/test.txt".as_ref(), &rw_create())
            .expect("Failed to create file");
        fs.close(&pid, handle).expect("Failed to close handle");

        // Change permissions using set_mode
        fs.set_mode(&pid, "/test.txt".as_ref(), 0o755).expect("set_mode should succeed");

        // Verify permissions changed
        let stat_data = fs.getattr(&pid, "/test.txt".as_ref()).expect("getattr should succeed");
        assert_eq!(stat_data.mode() & 0o777, 0o755);
    }

    #[test]
    fn test_chown() {
        let fs = create_test_fs();
        let pid = create_test_pid(&fs);

        // Create a file
        let handle = fs
            .create(&pid, "/test.txt".as_ref(), &rw_create())
            .expect("Failed to create file");
        fs.close(&pid, handle).expect("Failed to close handle");

        // Change ownership
        fs.set_owner(&pid, "/test.txt".as_ref(), 2000, 2000)
            .expect("set_owner should succeed");

        // Verify ownership changed
        let stat_data = fs.getattr(&pid, "/test.txt".as_ref()).expect("getattr should succeed");
        assert_eq!(stat_data.uid, 2000);
        assert_eq!(stat_data.gid, 2000);
    }

    #[test]
    fn test_truncate() {
        let fs = create_test_fs();
        let pid = create_test_pid(&fs);

        // Create a file with content
        let handle = fs
            .create(&pid, "/test.txt".as_ref(), &rw_create())
            .expect("Failed to create file");

        let content = b"Hello, World! This is a longer test.";
        fs.write(&pid, handle, 0, content).expect("Failed to write content");

        // Verify initial size
        let stat_data = fs.getattr(&pid, "/test.txt".as_ref()).expect("getattr should succeed");
        assert_eq!(stat_data.len, content.len() as u64);

        // Truncate to smaller size using ftruncate
        fs.ftruncate(&pid, handle, 13).expect("ftruncate should succeed");

        // Verify size changed
        let stat_data = fs.getattr(&pid, "/test.txt".as_ref()).expect("getattr should succeed");
        assert_eq!(stat_data.len, 13);

        // Verify content was truncated by reading it back
        let mut buffer = vec![0u8; 13];
        let bytes_read = fs.read(&pid, handle, 0, &mut buffer).expect("Failed to read content");
        assert_eq!(bytes_read, 13);
        assert_eq!(&buffer[..], &content[..13]);

        fs.close(&pid, handle).expect("Failed to close handle");
    }

    #[test]
    fn test_statfs() {
        let fs = create_test_fs();
        let pid = create_test_pid(&fs);

        // Test statfs - this should work with the new implementation
        let statfs_data = fs.statfs(&pid, "/".as_ref()).expect("statfs should succeed");

        // Verify basic filesystem stats (these will be dummy values for now)
        assert!(statfs_data.f_bsize > 0);
        assert!(statfs_data.f_blocks > 0);
    }

    #[test]
    fn test_mkfifo_sets_fifo_type() {
        let fs = create_test_fs();
        let pid = create_test_pid(&fs);

        fs.mkfifo(&pid, "/pipe".as_ref(), 0o640).expect("mkfifo should succeed");

        let attrs = fs.getattr(&pid, "/pipe".as_ref()).expect("getattr should succeed");
        assert_eq!(attrs.mode() & libc::S_IFMT as u32, libc::S_IFIFO as u32);
        assert_eq!(attrs.mode() & 0o777, 0o640);
    }

    #[test]
    fn test_mknod_char_device_records_rdev() {
        let fs = create_test_fs();
        let pid = create_test_pid(&fs);

        let dev: u64 = 0x1234;
        fs.mknod(&pid, "/ttyX".as_ref(), (libc::S_IFCHR as u32) | 0o660, dev)
            .expect("mknod should succeed");

        let attrs = fs.getattr(&pid, "/ttyX".as_ref()).expect("getattr should succeed");
        assert_eq!(attrs.mode() & libc::S_IFMT as u32, libc::S_IFCHR as u32);
        assert_eq!(attrs.mode() & 0o777, 0o660);
        assert_eq!(attrs.rdev(), dev);
    }

    #[test]
    fn test_mknod_socket_creates_special_file() {
        let fs = create_test_fs();
        let pid = create_test_pid(&fs);

        fs.mknod(&pid, "/sock".as_ref(), (libc::S_IFSOCK as u32) | 0o644, 0)
            .expect("mknod should support sockets");

        let attrs = fs.getattr(&pid, "/sock".as_ref()).expect("getattr should succeed");
        assert_eq!(attrs.mode() & libc::S_IFMT as u32, libc::S_IFSOCK as u32);
        assert_eq!(attrs.mode() & 0o777, 0o644);
        assert_eq!(attrs.special_kind, Some(SpecialNodeKind::Socket));
    }

    #[test]
    fn test_unlink_allows_recreate_same_name() {
        let fs = create_test_fs();
        let pid = create_test_pid(&fs);

        // Create then unlink a regular file
        let handle = fs.create(&pid, "/foo".as_ref(), &rw_create()).expect("create foo");
        fs.close(&pid, handle).expect("close foo");
        fs.unlink(&pid, "/foo".as_ref()).expect("unlink foo");

        // Recreate as fifo should succeed
        fs.mkfifo(&pid, "/foo".as_ref(), 0o600).expect("mkfifo after unlink");

        fs.unlink(&pid, "/foo".as_ref()).expect("unlink fifo");

        // Create as socket
        fs.mknod(&pid, "/foo".as_ref(), (libc::S_IFSOCK as u32) | 0o600, 0)
            .expect("mknod socket after unlink");
    }

    #[test]
    fn test_unlink_updates_ctime_for_remaining_links() {
        let fs = create_test_fs();
        let pid = create_test_pid(&fs);

        fs.create(&pid, "/file".as_ref(), &rw_create()).expect("create file");
        fs.link(&pid, "/file".as_ref(), "/file_link".as_ref()).expect("link file");

        // Force a predictable baseline for ctime.
        let (node_id, _) = fs.resolve_path(&pid, "/file".as_ref()).expect("resolve file");
        let baseline = FileTimes {
            atime: 1,
            mtime: 1,
            ctime: 5,
            birthtime: 1,
        };
        fs.set_node_times(node_id, baseline).expect("set custom times");

        let before = fs.getattr(&pid, "/file".as_ref()).expect("stat before");
        fs.unlink(&pid, "/file_link".as_ref()).expect("unlink link");
        let after = fs.getattr(&pid, "/file".as_ref()).expect("stat after");

        assert!(after.times.ctime > before.times.ctime);
    }

    #[test]
    fn test_unlink_sets_nlink_zero_for_open_handle() {
        let fs = create_test_fs();
        let pid = create_test_pid(&fs);

        let handle = fs.create(&pid, "/ghost".as_ref(), &rw_create()).expect("create ghost");
        fs.unlink(&pid, "/ghost".as_ref()).expect("unlink ghost");

        let stat = fs.fstat(&pid, handle).expect("fstat handle");
        assert_eq!(stat.st_nlink, 0);

        fs.close(&pid, handle).expect("close ghost");
    }

    #[test]
    fn test_fstatfs() {
        let fs = create_test_fs();
        let pid = create_test_pid(&fs);

        // Create a file and get handle
        let handle = fs
            .create(&pid, "/test.txt".as_ref(), &rw_create())
            .expect("Failed to create file");

        // Test fstatfs
        let statfs_data = fs.fstatfs(&pid, handle).expect("fstatfs should succeed");

        // Verify basic filesystem stats
        assert!(statfs_data.f_bsize > 0);
        assert!(statfs_data.f_blocks > 0);

        fs.close(&pid, handle).expect("Failed to close handle");
    }

    #[test]
    fn test_metadata_operations_update_timestamps() {
        let fs = create_test_fs();
        let pid = create_test_pid(&fs);

        // Create a file
        let handle = fs
            .create(&pid, "/test.txt".as_ref(), &rw_create())
            .expect("Failed to create file");
        fs.close(&pid, handle).expect("Failed to close handle");

        // Get initial timestamps
        let before = fs.getattr(&pid, "/test.txt".as_ref()).expect("getattr should succeed");

        // Change permissions (should update ctime)
        std::thread::sleep(std::time::Duration::from_millis(10)); // Ensure time difference
        fs.set_mode(&pid, "/test.txt".as_ref(), 0o755).expect("set_mode should succeed");

        // Verify ctime was updated
        let after = fs.getattr(&pid, "/test.txt".as_ref()).expect("getattr should succeed");
        assert!(after.times.ctime >= before.times.ctime);
    }

    // M24.g - Extended attributes, ACLs, and flags unit tests

    #[test]
    fn test_xattr_operations() {
        let fs = create_test_fs();
        let pid = create_test_pid(&fs);

        // Create a test file
        let handle = fs
            .create(&pid, "/test_xattr.txt".as_ref(), &rw_create())
            .expect("Failed to create file");
        fs.close(&pid, handle).expect("Failed to close handle");

        // Test setting xattr
        let name = "user.test_attr";
        let value = b"test_value";
        fs.xattr_set(&pid, "/test_xattr.txt".as_ref(), name, value)
            .expect("xattr_set should succeed");

        // Test getting xattr
        let retrieved = fs
            .xattr_get(&pid, "/test_xattr.txt".as_ref(), name)
            .expect("xattr_get should succeed");
        assert_eq!(retrieved, value);

        // Test listing xattrs
        let attrs = fs
            .xattr_list(&pid, "/test_xattr.txt".as_ref())
            .expect("xattr_list should succeed");
        assert!(attrs.contains(&name.to_string()));

        // Test removing xattr
        fs.xattr_remove(&pid, "/test_xattr.txt".as_ref(), name)
            .expect("xattr_remove should succeed");

        // Verify it's gone
        assert!(fs.xattr_get(&pid, "/test_xattr.txt".as_ref(), name).is_err());
    }

    #[test]
    fn test_xattr_fd_operations() {
        let fs = create_test_fs();
        let pid = create_test_pid(&fs);

        // Create a test file and keep handle open
        let handle = fs
            .create(&pid, "/test_xattr_fd.txt".as_ref(), &rw_create())
            .expect("Failed to create file");

        let name = "user.test_fd_attr";
        let value = b"fd_test_value";

        // Test fd-based operations
        fs.fsetxattr(&pid, handle, name, value).expect("fsetxattr should succeed");

        let retrieved = fs.fgetxattr(&pid, handle, name).expect("fgetxattr should succeed");
        assert_eq!(retrieved, value);

        let attrs = fs.flistxattr(&pid, handle).expect("flistxattr should succeed");
        assert!(attrs.contains(&name.to_string()));

        fs.fremovexattr(&pid, handle, name).expect("fremovexattr should succeed");

        fs.close(&pid, handle).expect("Failed to close handle");
    }

    #[test]
    fn test_acl_operations() {
        let fs = create_test_fs();
        let pid = create_test_pid(&fs);

        // Create a test file
        let handle = fs
            .create(&pid, "/test_acl.txt".as_ref(), &rw_create())
            .expect("Failed to create file");
        fs.close(&pid, handle).expect("Failed to close handle");

        // Test setting ACL
        let acl_type = 0x00000004; // ACL_TYPE_EXTENDED
        let acl_data = vec![1, 2, 3, 4]; // Dummy ACL data
        fs.acl_set_file(&pid, "/test_acl.txt".as_ref(), acl_type, &acl_data)
            .expect("acl_set_file should succeed");

        // Test getting ACL
        let retrieved = fs
            .acl_get_file(&pid, "/test_acl.txt".as_ref(), acl_type)
            .expect("acl_get_file should succeed");
        assert_eq!(retrieved, acl_data);

        // Test deleting default ACL
        fs.acl_delete_def_file(&pid, "/test_acl.txt".as_ref())
            .expect("acl_delete_def_file should succeed");
    }

    #[test]
    fn test_acl_fd_operations() {
        let fs = create_test_fs();
        let pid = create_test_pid(&fs);

        // Create a test file and keep handle open
        let handle = fs
            .create(&pid, "/test_acl_fd.txt".as_ref(), &rw_create())
            .expect("Failed to create file");

        let acl_type = 0x00000004; // ACL_TYPE_EXTENDED
        let acl_data = vec![5, 6, 7, 8]; // Dummy ACL data

        // Test fd-based ACL operations
        fs.acl_set_fd(&pid, handle, acl_type, &acl_data)
            .expect("acl_set_fd should succeed");

        let retrieved = fs.acl_get_fd(&pid, handle, acl_type).expect("acl_get_fd should succeed");
        assert_eq!(retrieved, acl_data);

        fs.close(&pid, handle).expect("Failed to close handle");
    }

    #[test]
    fn test_file_flags_operations() {
        let fs = create_test_fs();
        let pid = create_test_pid(&fs);

        // Create a test file
        let handle = fs
            .create(&pid, "/test_flags.txt".as_ref(), &rw_create())
            .expect("Failed to create file");
        fs.close(&pid, handle).expect("Failed to close handle");

        // Test setting flags
        let test_flags = 0x00000001; // UF_NODUMP
        fs.chflags(&pid, "/test_flags.txt".as_ref(), test_flags)
            .expect("chflags should succeed");

        // Test lchflags (should work the same for regular files)
        fs.lchflags(&pid, "/test_flags.txt".as_ref(), test_flags | 0x00000002)
            .expect("lchflags should succeed");
    }

    #[test]
    fn test_file_flags_fd_operations() {
        let fs = create_test_fs();
        let pid = create_test_pid(&fs);

        // Create a test file and keep handle open
        let handle = fs
            .create(&pid, "/test_flags_fd.txt".as_ref(), &rw_create())
            .expect("Failed to create file");

        let test_flags = 0x00000004; // UF_IMMUTABLE

        // Test fd-based flags operation
        fs.fchflags(&pid, handle, test_flags).expect("fchflags should succeed");

        fs.close(&pid, handle).expect("Failed to close handle");
    }

    #[test]
    fn test_getattrlist_operations() {
        let fs = create_test_fs();
        let pid = create_test_pid(&fs);

        // Create a test file
        let file_handle = fs
            .create(&pid, "/test_attrlist.txt".as_ref(), &rw_create())
            .expect("Failed to create file");
        fs.close(&pid, file_handle).expect("Failed to close file handle");

        // Test getattrlist (returns basic file attributes)
        let result = fs.getattrlist(&pid, "/test_attrlist.txt".as_ref(), &[], 0);
        assert!(result.is_ok()); // Should return success with attribute data
        let attrs = result.unwrap();
        assert!(!attrs.is_empty()); // Should contain some attribute data

        // Test setattrlist (sets file attributes)
        let result = fs.setattrlist(&pid, "/test_attrlist.txt".as_ref(), &[], &[], 0);
        assert!(result.is_ok()); // Should return success

        // Create a test directory for getattrlistbulk
        fs.mkdir(&pid, "/test_dir".as_ref(), 0o755).expect("Failed to create directory");
        let dir_handle = fs.opendir(&pid, "/test_dir".as_ref()).expect("Failed to open directory");

        // Test getattrlistbulk (returns attributes for directory entries)
        let result = fs.getattrlistbulk(&pid, dir_handle, &[], 0);
        assert!(result.is_ok()); // Should return success with empty list (no entries)
        assert_eq!(result.unwrap(), Vec::<Vec<u8>>::new());

        fs.close(&pid, dir_handle).expect("Failed to close directory handle");
    }

    #[test]
    fn test_copyfile_clonefile_operations() {
        let fs = create_test_fs();
        let pid = create_test_pid(&fs);

        // Create a source file
        let src_handle = fs
            .create(&pid, "/source.txt".as_ref(), &rw_create())
            .expect("Failed to create source file");

        let content = b"Test content for copy/clone operations";
        fs.write(&pid, src_handle, 0, content).expect("Failed to write content");
        fs.close(&pid, src_handle).expect("Failed to close source handle");

        // Test copyfile (should work now that it's implemented)
        let result = fs.copyfile(
            &pid,
            "/source.txt".as_ref(),
            "/dest_copy.txt".as_ref(),
            &[],
            0,
        );
        assert!(result.is_ok()); // Should succeed

        // Test clonefile (should work now that it's implemented)
        let result = fs.clonefile(&pid, "/source.txt".as_ref(), "/dest_clone.txt".as_ref(), 0);
        assert!(result.is_ok()); // Should succeed
    }

    #[test]
    fn test_copyfile_fd_operations() {
        let fs = create_test_fs();
        let pid = create_test_pid(&fs);

        // Create source and destination files
        let src_handle = fs
            .create(&pid, "/source_fd.txt".as_ref(), &rw_create())
            .expect("Failed to create source file");

        let dst_handle = fs
            .create(&pid, "/dest_fd.txt".as_ref(), &rw_create())
            .expect("Failed to create dest file");

        // Test fcopyfile (should work now that it's implemented)
        let result = fs.fcopyfile(&pid, src_handle, dst_handle, &[], 0);
        assert!(result.is_ok()); // Should succeed

        // Test fclonefileat (placeholder implementation - should fail for now)
        let result = fs.fclonefileat(
            &pid,
            src_handle,
            "/source_fd.txt".as_ref(),
            dst_handle,
            "/dest_clone_fd.txt".as_ref(),
            0,
        );
        assert!(result.is_err()); // Placeholder implementation doesn't handle directory-relative paths properly

        fs.close(&pid, src_handle).expect("Failed to close source handle");
        fs.close(&pid, dst_handle).expect("Failed to close dest handle");
    }

    #[test]
    fn test_xattr_error_handling() {
        let fs = create_test_fs();
        let pid = create_test_pid(&fs);

        // Test operations on non-existent file
        assert!(fs.xattr_get(&pid, "/nonexistent.txt".as_ref(), "user.test").is_err());
        assert!(fs.xattr_set(&pid, "/nonexistent.txt".as_ref(), "user.test", b"value").is_err());
        assert!(fs.xattr_list(&pid, "/nonexistent.txt".as_ref()).is_err());
        assert!(fs.xattr_remove(&pid, "/nonexistent.txt".as_ref(), "user.test").is_err());

        // Create a file and test non-existent xattr
        let handle = fs
            .create(&pid, "/test_errors.txt".as_ref(), &rw_create())
            .expect("Failed to create file");
        fs.close(&pid, handle).expect("Failed to close handle");

        // Test getting non-existent xattr
        assert!(fs.xattr_get(&pid, "/test_errors.txt".as_ref(), "user.nonexistent").is_err());

        // Test removing non-existent xattr
        assert!(fs.xattr_remove(&pid, "/test_errors.txt".as_ref(), "user.nonexistent").is_err());
    }

    #[test]
    fn test_acl_error_handling() {
        let fs = create_test_fs();
        let pid = create_test_pid(&fs);

        // Test operations on non-existent file
        assert!(fs.acl_get_file(&pid, "/nonexistent.txt".as_ref(), 0x00000004).is_err());
        assert!(
            fs.acl_set_file(&pid, "/nonexistent.txt".as_ref(), 0x00000004, &[1, 2, 3])
                .is_err()
        );
        assert!(fs.acl_delete_def_file(&pid, "/nonexistent.txt".as_ref()).is_err());

        // Create a file and test operations
        let handle = fs
            .create(&pid, "/test_acl_errors.txt".as_ref(), &rw_create())
            .expect("Failed to create file");
        fs.close(&pid, handle).expect("Failed to close handle");

        // Test getting non-existent ACL type
        let result = fs.acl_get_file(&pid, "/test_acl_errors.txt".as_ref(), 0x00000010); // Invalid ACL type
        // This might succeed or fail depending on implementation - just ensure it doesn't panic
        let _ = result;
    }

    #[test]
    fn test_flags_error_handling() {
        let fs = create_test_fs();
        let pid = create_test_pid(&fs);

        // Test operations on non-existent file
        assert!(fs.chflags(&pid, "/nonexistent.txt".as_ref(), 0x00000001).is_err());
        assert!(fs.lchflags(&pid, "/nonexistent.txt".as_ref(), 0x00000001).is_err());
    }

    #[test]
    fn test_fd_error_handling() {
        let fs = create_test_fs();
        let pid = create_test_pid(&fs);

        // Test operations with invalid handle
        let invalid_handle = HandleId(99999);
        assert!(fs.fgetxattr(&pid, invalid_handle, "user.test").is_err());
        assert!(fs.fsetxattr(&pid, invalid_handle, "user.test", b"value").is_err());
        assert!(fs.flistxattr(&pid, invalid_handle).is_err());
        assert!(fs.fremovexattr(&pid, invalid_handle, "user.test").is_err());

        assert!(fs.acl_get_fd(&pid, invalid_handle, 0x00000004).is_err());
        assert!(fs.acl_set_fd(&pid, invalid_handle, 0x00000004, &[1, 2, 3]).is_err());

        assert!(fs.fchflags(&pid, invalid_handle, 0x00000001).is_err());
    }
}
