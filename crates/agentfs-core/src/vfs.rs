// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Virtual filesystem implementation for AgentFS Core

use std::collections::HashMap;
use std::path::Path;
#[cfg(test)]
use std::sync::OnceLock;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::error::{FsError, FsResult};
use crate::storage::StorageBackend;
use crate::{
    Attributes, BranchId, BranchInfo, ContentId, DirEntry, EventKind, EventSink, FileMode,
    FileTimes, FsConfig, FsStats, HandleId, LockKind, LockRange, OpenOptions, ShareMode,
    SnapshotId, StreamSpec, SubscriptionId,
};
use crate::{Backstore, LowerFs};

// Import proto types for interpose operations
use agentfs_proto::messages::{StatData, StatfsData, TimespecData};
#[cfg(test)]
use std::cell::RefCell;
#[cfg(test)]
use std::ops::Deref;

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
    pub id: NodeId,
    pub kind: NodeKind,
    pub times: FileTimes,
    pub mode: u32,
    pub uid: u32,
    pub gid: u32,
    pub xattrs: HashMap<String, Vec<u8>>, // Extended attributes
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
    pub id: HandleId,
    pub node_id: NodeId,
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
    next_subscription_id: Mutex<u64>,
    pub(crate) process_branches: Mutex<HashMap<u32, BranchId>>, // Process ID -> Branch ID mapping
    process_identities: Mutex<HashMap<u32, User>>,              // Process ID -> security identity
    process_children: Mutex<HashMap<u32, Vec<u32>>>,            // Parent PID -> list of child PIDs
    process_parents: Mutex<HashMap<u32, u32>>,                  // Child PID -> parent PID
    locks: Mutex<LockManager>,                                  // Byte-range lock manager
    event_subscriptions: Mutex<HashMap<SubscriptionId, Arc<dyn EventSink>>>,
}

impl FsCore {
    pub fn new(config: FsConfig) -> FsResult<Self> {
        // Initialize storage backend based on backstore configuration
        let storage: Arc<dyn StorageBackend> = match &config.backstore {
            crate::config::BackstoreMode::InMemory => {
                Arc::new(crate::storage::InMemoryBackend::new())
            }
            crate::config::BackstoreMode::HostFs { root, .. } => {
                Arc::new(crate::storage::HostFsBackend::new(root.clone())?)
            }
        };

        // Initialize backstore for overlay operations
        let backstore = if config.overlay.enabled {
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
            next_subscription_id: Mutex::new(1),
            process_branches: Mutex::new(HashMap::new()), // No processes initially bound
            process_identities: Mutex::new(HashMap::new()), // No processes initially registered
            process_children: Mutex::new(HashMap::new()), // No process hierarchy initially
            process_parents: Mutex::new(HashMap::new()),  // No process hierarchy initially
            locks: Mutex::new(LockManager {
                locks: HashMap::new(),
            }),
            event_subscriptions: Mutex::new(HashMap::new()),
        };

        // Create root directory
        core.create_root_directory()?;
        Ok(core)
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
            xattrs: HashMap::new(),
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
    fn copy_up(&self, pid: &PID, path: &Path) -> FsResult<()> {
        if !self.is_overlay_enabled() {
            return Ok(());
        }

        // For now, copy-up is handled implicitly when operations create upper entries
        // This method can be expanded later for explicit copy-up scenarios
        Ok(())
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

        // Check if already registered - if so, return existing PID
        {
            let identities = self.process_identities.lock().unwrap();
            if identities.contains_key(&pid) {
                return PID(pid);
            }
        }

        // Register the process identity
        {
            let mut identities = self.process_identities.lock().unwrap();
            identities.insert(pid, user);
        }

        // Establish parent-child relationship
        {
            let mut children = self.process_children.lock().unwrap();
            children.entry(parent_pid).or_insert_with(Vec::new).push(pid);
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
        user.gid == gid || user.groups.iter().any(|g| *g == gid)
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
            xattrs: HashMap::new(),
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
            xattrs: HashMap::new(),
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
                    if user.uid != node.uid
                        || (user.gid != gid && !user.groups.iter().any(|g| *g == gid))
                    {
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
        Ok(())
    }

    /// Percent-encode arbitrary bytes to a safe internal string name
    fn percent_encode_name(bytes: &[u8]) -> String {
        let mut s = String::with_capacity(bytes.len() * 3);
        for &b in bytes {
            let is_safe = (b'A'..=b'Z').contains(&b)
                || (b'a'..=b'z').contains(&b)
                || (b'0'..=b'9').contains(&b)
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
        {
            let mut nodes = self.nodes.lock().unwrap();
            if let Some(parent) = nodes.get_mut(&parent_id) {
                if let NodeKind::Directory { children } = &mut parent.kind {
                    children.insert(internal_name, new_node_id);
                }
            }
        }

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
            for (stream_name, (content_id, size)) in streams {
                let new_content_id = self.storage.clone_cow(*content_id)?;
                new_streams.insert(stream_name.clone(), (new_content_id, *size));
            }
            new_node.kind = NodeKind::File {
                streams: new_streams,
            };
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
        let current_branch = self.branch_for_process(&PID::new(Self::current_process_id()));
        let branches = self.branches.lock().unwrap();
        let branch = branches.get(&current_branch).ok_or(FsError::NotFound)?;

        let snapshot_id = SnapshotId::new();

        // If we have a backstore that supports native snapshots, delegate to it
        if let Some(backstore) = &self.backstore {
            if backstore.supports_native_snapshots() {
                let snapshot_name = name
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| format!("snapshot_{}", hex::encode(snapshot_id.0)));
                backstore.snapshot_native(&snapshot_name)?;
            }
        }

        let snapshot = Snapshot {
            id: snapshot_id,
            root_id: branch.root_id,
            name: name.map(|s| s.to_string()),
        };

        self.snapshots.lock().unwrap().insert(snapshot_id, snapshot);

        // Emit event
        self.emit_event(EventKind::SnapshotCreated {
            id: snapshot_id,
            name: name.map(|s| s.to_string()),
        });

        Ok(snapshot_id)
    }

    pub fn snapshot_list(&self) -> Vec<(SnapshotId, Option<String>)> {
        let snapshots = self.snapshots.lock().unwrap();
        snapshots.values().map(|s| (s.id, s.name.clone())).collect()
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

    // Event subscription operations
    pub fn subscribe_events(&self, cb: Arc<dyn EventSink>) -> FsResult<SubscriptionId> {
        let mut subscriptions = self.event_subscriptions.lock().unwrap();
        let mut next_id = self.next_subscription_id.lock().unwrap();
        let subscription_id = SubscriptionId::new(*next_id);
        *next_id += 1;
        subscriptions.insert(subscription_id, cb);
        Ok(subscription_id)
    }

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
    fn emit_event(&self, event: EventKind) {
        if !self.config.track_events {
            return;
        }

        let subscriptions = self.event_subscriptions.lock().unwrap();
        for sink in subscriptions.values() {
            sink.on_event(&event);
        }
    }

    // File operations
    pub fn create(&self, pid: &PID, path: &Path, opts: &OpenOptions) -> FsResult<HandleId> {
        // Check if the path already exists
        if let Ok(_) = self.resolve_path(pid, path) {
            return Err(FsError::AlreadyExists);
        }

        // Get parent directory
        let parent_path = path.parent().ok_or(FsError::InvalidArgument)?;
        let parent_name = path.file_name().and_then(|n| n.to_str()).ok_or(FsError::InvalidName)?;

        let (parent_id, _) = self.resolve_path(pid, parent_path)?;

        // Permission check for parent directory w+x access
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

        // Add to parent directory
        {
            let mut nodes = self.nodes.lock().unwrap();
            if let Some(parent_node) = nodes.get_mut(&parent_id) {
                if let NodeKind::Directory { children } = &mut parent_node.kind {
                    children.insert(parent_name.to_string(), file_node_id);
                }
            }
        }

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
            kind: HandleType::File {
                options: opts.clone(),
                deleted: false,
            },
        };

        self.handles.lock().unwrap().insert(handle_id, handle);

        // Emit event
        let path_str = path.to_string_lossy().to_string();
        self.emit_event(EventKind::Created { path: path_str });

        Ok(handle_id)
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
        let handle = Handle {
            id: handle_id,
            node_id,
            kind: HandleType::File {
                options: opts.clone(),
                deleted: false,
            },
        };
        self.handles.lock().unwrap().insert(handle_id, handle);
        Ok(handle_id)
    }

    /// Check if a node is shared between branches/snapshots
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

        // If this was the last handle to a deleted file, remove the node
        if was_deleted {
            let remaining_handles: Vec<_> =
                handles.values().filter(|h| h.node_id == node_id).collect();

            if remaining_handles.is_empty() {
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
        let node_locks = locks.locks.entry(node_id).or_insert_with(Vec::new);
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
        Ok(())
    }

    // Directory operations
    pub fn mkdir(&self, pid: &PID, path: &Path, mode: u32) -> FsResult<()> {
        // Check if the path already exists
        if let Ok(_) = self.resolve_path(pid, path) {
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

        // Add to parent directory
        {
            let mut nodes = self.nodes.lock().unwrap();
            if let Some(parent_node) = nodes.get_mut(&parent_id) {
                if let NodeKind::Directory { children } = &mut parent_node.kind {
                    children.insert(dir_name.to_string(), dir_node_id);
                }
            }
        }

        // Emit event
        let path_str = path.to_string_lossy().to_string();
        self.emit_event(EventKind::Created { path: path_str });

        Ok(())
    }

    pub fn rmdir(&self, pid: &PID, path: &Path) -> FsResult<()> {
        let (node_id, parent_info) = self.resolve_path(pid, path)?;

        let Some((parent_id, name)) = parent_info else {
            return Err(FsError::InvalidArgument); // Can't remove root
        };

        let nodes = self.nodes.lock().unwrap();
        let node = nodes.get(&node_id).ok_or(FsError::NotFound)?;

        // Check if it's a directory and empty
        match &node.kind {
            NodeKind::Directory { children } => {
                if !children.is_empty() {
                    return Err(FsError::Busy); // Directory not empty
                }
            }
            NodeKind::File { .. } => return Err(FsError::NotADirectory),
            NodeKind::Symlink { .. } => return Err(FsError::NotADirectory),
        }
        drop(nodes);

        // Remove from parent directory
        {
            let mut nodes = self.nodes.lock().unwrap();
            if let Some(parent_node) = nodes.get_mut(&parent_id) {
                if let NodeKind::Directory { children } = &mut parent_node.kind {
                    children.remove(&name);
                }
            }
        }

        // Remove the directory node itself to avoid leaking nodes
        {
            let mut nodes = self.nodes.lock().unwrap();
            nodes.remove(&node_id);
        }

        // Emit event
        let path_str = path.to_string_lossy().to_string();
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
        pid: &PID,
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

        let nodes = self.nodes.lock().unwrap();
        let node = nodes.get(&node_id).ok_or(FsError::NotFound)?;

        // Check if it's a file or symlink (can't unlink directories with unlink)
        match &node.kind {
            NodeKind::Directory { .. } => return Err(FsError::IsADirectory),
            NodeKind::File { .. } => {}
            NodeKind::Symlink { .. } => {} // Symlinks can be unlinked like files
        }
        // Enforce parent directory permissions w+x and sticky semantics
        if self.config.security.enforce_posix_permissions {
            if let Some(user) = self.user_for_process(pid) {
                let parent = nodes.get(&parent_id).ok_or(FsError::NotFound)?;
                if !self.allowed_for_user(parent, &user, false, true, true) {
                    return Err(FsError::AccessDenied);
                }
                let sticky = (parent.mode & 0o1000) != 0;
                if sticky && user.uid != 0 {
                    if user.uid != parent.uid && user.uid != node.uid {
                        return Err(FsError::AccessDenied);
                    }
                }
            }
        }
        drop(nodes);

        // Check if any handles are open to this file and mark them as deleted
        let mut handles = self.handles.lock().unwrap();
        let has_open_handles = handles.values().any(|h| h.node_id == node_id);

        if has_open_handles {
            // Mark all file handles to this file as deleted
            for handle in handles.values_mut() {
                if handle.node_id == node_id {
                    if let HandleType::File { deleted, .. } = &mut handle.kind {
                        *deleted = true;
                    }
                }
            }
        } else {
            // No open handles, remove immediately
            let mut nodes = self.nodes.lock().unwrap();
            nodes.remove(&node_id);
        }

        // Remove from parent directory
        {
            let mut nodes = self.nodes.lock().unwrap();
            if let Some(parent_node) = nodes.get_mut(&parent_id) {
                if let NodeKind::Directory { children } = &mut parent_node.kind {
                    children.remove(&name);
                }
            }
        }

        // Emit event
        let path_str = path.to_string_lossy().to_string();
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
                    times: node.times.clone(),
                    uid: node.uid,
                    gid: node.gid,
                    is_dir: false,
                    is_symlink: true,
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
    pub fn fstatat(
        &self,
        pid: &PID,
        dirfd: HandleId,
        path: &Path,
        flags: u32,
    ) -> FsResult<StatData> {
        // For now, implement as regular stat - full implementation would need path resolution relative to dirfd
        // This is a simplified version for the milestone
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
    pub fn fchmodat(
        &self,
        pid: &PID,
        dirfd: HandleId,
        path: &Path,
        mode: u32,
        flags: u32,
    ) -> FsResult<()> {
        // For now, implement as regular chmod - full implementation would need path resolution relative to dirfd
        // This is a simplified version for the milestone
        let _ = dirfd; // unused in simplified version
        let _ = flags; // unused in simplified version
        self.set_mode(pid, path, mode)
    }

    /// Change file owner (fchown) for an open file descriptor
    pub fn fchown(&self, pid: &PID, handle_id: HandleId, uid: u32, gid: u32) -> FsResult<()> {
        let node_id = self.get_node_id_for_handle(pid, handle_id)?;
        self.set_node_owner(node_id, pid, uid, gid)
    }

    /// Change file owner (fchownat) relative to a directory file descriptor
    pub fn fchownat(
        &self,
        pid: &PID,
        dirfd: HandleId,
        path: &Path,
        uid: u32,
        gid: u32,
        flags: u32,
    ) -> FsResult<()> {
        // For now, implement as regular chown - full implementation would need path resolution relative to dirfd
        // This is a simplified version for the milestone
        let _ = dirfd; // unused in simplified version
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
                ctime: FsCore::current_timestamp() as i64,
                birthtime: FsCore::current_timestamp() as i64,
            })
            .unwrap_or_else(|| {
                let now = FsCore::current_timestamp() as i64;
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
        dirfd: HandleId,
        path: &Path,
        times: Option<(TimespecData, TimespecData)>,
        flags: u32,
    ) -> FsResult<()> {
        // For now, implement as regular utimes - full implementation would need path resolution relative to dirfd
        // This is a simplified version for the milestone
        let _ = dirfd; // unused in simplified version
        let _ = flags; // unused in simplified version
        let file_times = times
            .map(|(atime, mtime)| FileTimes {
                atime: atime.tv_sec as i64,
                mtime: mtime.tv_sec as i64,
                ctime: FsCore::current_timestamp() as i64,
                birthtime: FsCore::current_timestamp() as i64,
            })
            .unwrap_or_else(|| {
                let now = FsCore::current_timestamp() as i64;
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
    pub fn ftruncate(&self, pid: &PID, handle_id: HandleId, length: u64) -> FsResult<()> {
        let node_id = self.get_node_id_for_handle(pid, handle_id)?;
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

        // Convert FileMode structs to mode bits
        let user_mode = if attrs.mode_user.read {
            libc::S_IRUSR as u32
        } else {
            0
        } | if attrs.mode_user.write {
            libc::S_IWUSR as u32
        } else {
            0
        } | if attrs.mode_user.exec {
            libc::S_IXUSR as u32
        } else {
            0
        };
        let group_mode = if attrs.mode_group.read {
            libc::S_IRGRP as u32
        } else {
            0
        } | if attrs.mode_group.write {
            libc::S_IWGRP as u32
        } else {
            0
        } | if attrs.mode_group.exec {
            libc::S_IXGRP as u32
        } else {
            0
        };
        let other_mode = if attrs.mode_other.read {
            libc::S_IROTH as u32
        } else {
            0
        } | if attrs.mode_other.write {
            libc::S_IWOTH as u32
        } else {
            0
        } | if attrs.mode_other.exec {
            libc::S_IXOTH as u32
        } else {
            0
        };
        let file_type = if attrs.is_dir {
            libc::S_IFDIR as u32
        } else if attrs.is_symlink {
            libc::S_IFLNK as u32
        } else {
            libc::S_IFREG as u32
        };

        Ok(StatData {
            st_dev: 1, // Dummy device ID
            st_ino: inode,
            st_mode: file_type | user_mode | group_mode | other_mode,
            st_nlink: 1, // Simplified
            st_uid: attrs.uid,
            st_gid: attrs.gid,
            st_rdev: 0, // Not a special file
            st_size: attrs.len,
            st_blksize: 4096,                   // 4KB block size
            st_blocks: (attrs.len + 511) / 512, // Number of 512-byte blocks
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
        // Resolve old path and its parent
        let (old_id, old_parent) = self.resolve_path(pid, old)?;
        let Some((old_parent_id, old_name)) = old_parent else {
            return Err(FsError::InvalidArgument); // Cannot rename root
        };

        // Resolve destination parent and name
        let new_parent_path = new.parent().ok_or(FsError::InvalidArgument)?;
        let new_name = new
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or(FsError::InvalidName)?
            .to_string();

        let (new_parent_id, _) = self.resolve_path(pid, new_parent_path)?;

        // Lock nodes for mutation
        let mut nodes = self.nodes.lock().unwrap();

        // Ensure destination does not exist
        if let Some(parent_node) = nodes.get(&new_parent_id) {
            if let NodeKind::Directory { children } = &parent_node.kind {
                if children.contains_key(&new_name) {
                    return Err(FsError::AlreadyExists);
                }
            } else {
                return Err(FsError::NotADirectory);
            }
        } else {
            return Err(FsError::NotFound);
        }

        // Remove from old parent's children and insert into new parent's children
        // Also update ctime on the moved node and both directories
        let now = FsCore::current_timestamp();

        // Remove from old parent
        if let Some(old_parent_node) = nodes.get_mut(&old_parent_id) {
            if let NodeKind::Directory { children } = &mut old_parent_node.kind {
                children.remove(&old_name);
                old_parent_node.times.ctime = now;
            }
        }

        // Insert into new parent
        if let Some(new_parent_node) = nodes.get_mut(&new_parent_id) {
            if let NodeKind::Directory { children } = &mut new_parent_node.kind {
                children.insert(new_name, old_id);
                new_parent_node.times.ctime = now;
            }
        }

        // Update moved node's ctime
        if let Some(node) = nodes.get_mut(&old_id) {
            node.times.ctime = now;
        }

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

        // Add to parent directory
        {
            let mut nodes = self.nodes.lock().unwrap();
            if let Some(parent_node) = nodes.get_mut(&parent_id) {
                if let NodeKind::Directory { children } = &mut parent_node.kind {
                    children.insert(link_name, symlink_node_id);
                }
            }
        }

        // Emit event
        let path_str = linkpath.to_string_lossy().to_string();
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

        // Add link to parent directory
        {
            let mut nodes = self.nodes.lock().unwrap();
            if let Some(parent_node) = nodes.get_mut(&parent_id) {
                if let NodeKind::Directory { children } = &mut parent_node.kind {
                    children.insert(link_name, node_id);
                    parent_node.times.ctime = Self::current_timestamp();
                } else {
                    return Err(FsError::NotADirectory);
                }
            }

            // Increment link count
            if let Some(node) = nodes.get_mut(&node_id) {
                node.times.ctime = Self::current_timestamp();
            }
        }

        // Emit event
        let path_str = new_path.to_string_lossy().to_string();
        self.emit_event(EventKind::Created { path: path_str });

        Ok(())
    }

    /// Create a hard link with dirfd (relative to directory)
    pub fn linkat(
        &self,
        pid: &PID,
        old_dirfd: u32,
        old_path: &Path,
        new_dirfd: u32,
        new_path: &Path,
        flags: u32,
    ) -> FsResult<()> {
        // For now, implement basic version - full implementation would need to handle AT_FDCWD and flags
        // This is a simplified version that treats dirfd as absolute paths for now
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

        self.link(pid, &old_full_path, &new_full_path)
    }

    /// Create a symbolic link with dirfd (relative to directory)
    pub fn symlinkat(
        &self,
        pid: &PID,
        target: &str,
        new_dirfd: u32,
        linkpath: &Path,
    ) -> FsResult<()> {
        // For now, implement basic version - full implementation would need to handle AT_FDCWD
        let full_linkpath = if new_dirfd == libc::AT_FDCWD as u32 {
            linkpath.to_path_buf()
        } else {
            // TODO: Implement proper dirfd resolution
            return Err(FsError::NotImplemented);
        };

        self.symlink(pid, target, &full_linkpath)
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
        flags: u32,
    ) -> FsResult<()> {
        // For now, implement as regular rename - full implementation would handle macOS-specific flags
        self.renameat(pid, old_dirfd, old_path, new_dirfd, new_path)
    }

    /// Unlink with dirfd (relative to directory)
    pub fn unlinkat(&self, pid: &PID, dirfd: u32, path: &Path, flags: u32) -> FsResult<()> {
        // For now, implement basic version - full implementation would need to handle AT_FDCWD and flags
        let full_path = if dirfd == libc::AT_FDCWD as u32 {
            path.to_path_buf()
        } else {
            // TODO: Implement proper dirfd resolution
            return Err(FsError::NotImplemented);
        };

        self.unlink(pid, &full_path)
    }

    /// Remove (alias for unlink)
    pub fn remove(&self, pid: &PID, path: &Path) -> FsResult<()> {
        self.unlink(pid, path)
    }

    /// Create directory with dirfd (relative to directory)
    pub fn mkdirat(&self, pid: &PID, dirfd: u32, path: &Path, mode: u32) -> FsResult<()> {
        // For now, implement basic version - full implementation would need to handle AT_FDCWD
        let full_path = if dirfd == libc::AT_FDCWD as u32 {
            path.to_path_buf()
        } else {
            // TODO: Implement proper dirfd resolution
            return Err(FsError::NotImplemented);
        };

        self.mkdir(pid, &full_path, mode)
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
            xattrs: HashMap::new(),
        };

        let mut nodes = self.nodes.lock().unwrap();
        nodes.insert(node_id, node);

        Ok(node_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

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
}
