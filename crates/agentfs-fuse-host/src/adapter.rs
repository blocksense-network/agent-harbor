// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! AgentFS FUSE adapter implementation
//!
//! Maps FUSE operations to AgentFS Core calls.

#[cfg(not(feature = "fuse"))]
compile_error!("This module requires the 'fuse' feature to be enabled");

use agentfs_core::{
    Attributes, DirEntry, FileTimes, FsConfig, FsCore, FsError, HandleId, LockRange, OpenOptions,
    ShareMode, error::FsResult, vfs::PID,
};
use agentfs_proto::*;
use fuser::{
    FUSE_ROOT_ID, FileAttr, FileType, ReplyAttr, ReplyCreate, ReplyData, ReplyDirectory,
    ReplyEmpty, ReplyEntry, ReplyLock, ReplyOpen, ReplyStatfs, ReplyWrite, ReplyXattr, Request,
    TimeOrNow,
};
use libc::{
    EACCES, EBUSY, EEXIST, EINVAL, EIO, EISDIR, ENAMETOOLONG, ENOENT, ENOTDIR, ENOTEMPTY, ENOTSUP,
    c_int,
};
use ssz::{Decode, Encode};
use std::collections::HashMap;
use std::ffi::OsStr;
use std::io::Read;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tracing::{debug, error, info, warn};

/// Special inode for the .agentfs directory
const AGENTFS_DIR_INO: u64 = FUSE_ROOT_ID + 1;

/// Special inode for the .agentfs/control file
const CONTROL_FILE_INO: u64 = FUSE_ROOT_ID + 2;

/// IOCTL command for AgentFS control operations
const AGENTFS_IOCTL_CMD: u32 = 0x8000_4146; // 'AF' in hex with 0x8000 bit set

/// AgentFS FUSE filesystem adapter
pub struct AgentFsFuse {
    /// Core filesystem instance
    core: FsCore,
    /// Configuration
    config: FsConfig,
    /// Cache of inode to path mappings for control operations
    inodes: HashMap<u64, Vec<u8>>, // inode -> path
    /// Reverse mapping from path to inode
    paths: HashMap<Vec<u8>, u64>, // path -> inode
    /// Next available inode number
    next_inode: u64,
}

impl AgentFsFuse {
    /// Create a new FUSE adapter with the given configuration
    pub fn new(config: FsConfig) -> FsResult<Self> {
        let core = FsCore::new(config.clone())?;
        let mut inodes = HashMap::new();
        let mut paths = HashMap::new();

        // Pre-populate special inodes
        inodes.insert(FUSE_ROOT_ID, b"/".to_vec());
        paths.insert(b"/".to_vec(), FUSE_ROOT_ID);
        inodes.insert(AGENTFS_DIR_INO, b"/.agentfs".to_vec());
        paths.insert(b"/.agentfs".to_vec(), AGENTFS_DIR_INO);
        inodes.insert(CONTROL_FILE_INO, b"/.agentfs/control".to_vec());
        paths.insert(b"/.agentfs/control".to_vec(), CONTROL_FILE_INO);

        Ok(Self {
            core,
            config,
            inodes,
            paths,
            next_inode: CONTROL_FILE_INO + 1,
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

        // The FsCore register_process function expects a parent_pid.
        // The fuser::Request does not provide this.
        // Based on the agentfs-core tests, passing the client_pid as its own
        // parent is a valid convention for registering a new process tree root.
        //
        // This is VASTLY superior to the previous hardcoded (host_pid, 1, 0, 0).
        self.core.register_process(client_pid, client_pid, client_uid, client_gid)
    }

    /// Allocate a new inode for a path
    fn alloc_inode(&mut self, path: &[u8]) -> u64 {
        if let Some(&existing_inode) = self.paths.get(path) {
            return existing_inode;
        }

        let inode = self.next_inode;
        self.next_inode += 1;

        let path_vec = path.to_vec();
        self.inodes.insert(inode, path_vec.clone());
        self.paths.insert(path_vec, inode);

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

    /// Deallocate an inode (when file/directory is removed)
    fn dealloc_inode(&mut self, inode: u64) {
        if let Some(path) = self.inodes.remove(&inode) {
            self.paths.remove(&path);
        }
    }

    /// Convert a path slice to a Path
    fn path_from_bytes<'a>(&self, path: &'a [u8]) -> &'a Path {
        Path::new(OsStr::from_bytes(path))
    }

    /// Convert FsCore Attributes to FUSE FileAttr
    fn attr_to_fuse(&self, attr: &Attributes, ino: u64) -> FileAttr {
        let kind = if attr.is_dir {
            FileType::Directory
        } else if attr.is_symlink {
            FileType::Symlink
        } else {
            FileType::RegularFile
        };

        FileAttr {
            ino,
            size: attr.len,
            blocks: (attr.len + 511) / 512, // 512-byte blocks
            atime: SystemTime::UNIX_EPOCH + Duration::from_secs(attr.times.atime as u64),
            mtime: SystemTime::UNIX_EPOCH + Duration::from_secs(attr.times.mtime as u64),
            ctime: SystemTime::UNIX_EPOCH + Duration::from_secs(attr.times.ctime as u64),
            crtime: SystemTime::UNIX_EPOCH + Duration::from_secs(attr.times.birthtime as u64),
            kind,
            perm: attr.mode() as u16,
            nlink: 1, // Hardcoded for now - TODO: implement proper nlink counting
            uid: attr.uid,
            gid: attr.gid,
            rdev: 0, // Hardcoded for now - TODO: implement device files if needed
            blksize: 512,
            flags: 0, // macOS specific
        }
    }

    /// Convert FUSE flags to OpenOptions
    fn fuse_flags_to_options(&self, flags: i32) -> OpenOptions {
        use libc::{O_APPEND, O_CREAT, O_EXCL, O_RDONLY, O_RDWR, O_TRUNC, O_WRONLY};

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

        let request: Request = <Request as Decode>::from_ssz_bytes(data).map_err(|e| {
            error!("Failed to decode SSZ control request: {:?}", e);
            EINVAL
        })?;

        // Validate request structure
        if let Err(e) = validate_request(&request) {
            error!("Request validation failed: {}", e);
            let response = Response::error(format!("{}", e), Some(EINVAL as u32));
            return Ok(<Response as Encode>::as_ssz_bytes(&response));
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
                        Ok(<Response as Encode>::as_ssz_bytes(&response))
                    }
                    Err(e) => {
                        let errno = match e {
                            FsError::NotFound => ENOENT,
                            FsError::AlreadyExists => EEXIST,
                            FsError::AccessDenied => EACCES,
                            FsError::InvalidArgument => EINVAL,
                            _ => EIO,
                        };
                        let response = Response::error(format!("{:?}", e), Some(errno as u32));
                        Ok(<Response as Encode>::as_ssz_bytes(&response))
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
                Ok(response.as_ssz_bytes())
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
                        Ok(<Response as Encode>::as_ssz_bytes(&response))
                    }
                    Err(e) => {
                        let errno = match e {
                            FsError::NotFound => ENOENT,
                            FsError::AlreadyExists => EEXIST,
                            FsError::AccessDenied => EACCES,
                            FsError::InvalidArgument => EINVAL,
                            _ => EIO,
                        };
                        let response = Response::error(format!("{:?}", e), Some(errno as u32));
                        Ok(<Response as Encode>::as_ssz_bytes(&response))
                    }
                }
            }
            Request::BranchBind((_, req)) => {
                let pid = req.pid.unwrap_or_else(|| std::process::id());
                let branch_str = String::from_utf8_lossy(&req.branch).to_string();
                let branch_id = branch_str.parse().map_err(|_| EINVAL)?;

                match self.core.bind_process_to_branch_with_pid(branch_id, pid) {
                    Ok(()) => {
                        let response = Response::branch_bind(req.branch.clone(), pid);
                        Ok(<Response as Encode>::as_ssz_bytes(&response))
                    }
                    Err(e) => {
                        let errno = match e {
                            FsError::NotFound => ENOENT,
                            FsError::AlreadyExists => EEXIST,
                            FsError::AccessDenied => EACCES,
                            FsError::InvalidArgument => EINVAL,
                            _ => EIO,
                        };
                        let response = Response::error(format!("{:?}", e), Some(errno as u32));
                        Ok(<Response as Encode>::as_ssz_bytes(&response))
                    }
                }
            }
            _ => {
                // For now, only handle the basic control operations mentioned in the milestone
                let response =
                    Response::error("Unsupported operation".to_string(), Some(ENOTSUP as u32));
                Ok(response.as_ssz_bytes())
            }
        }
    }
}

impl fuser::Filesystem for AgentFsFuse {
    fn init(&mut self, _req: &Request, _config: &mut fuser::KernelConfig) -> Result<(), c_int> {
        // Cache configuration is handled via mount options in main.rs
        info!("AgentFS FUSE adapter initialized");
        Ok(())
    }

    fn destroy(&mut self) {
        info!("AgentFS FUSE adapter destroyed");
    }

    fn lookup(&mut self, req: &Request, parent: u64, name: &OsStr, reply: ReplyEntry) {
        let name_bytes = name.as_bytes();

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
            reply.entry(&Duration::from_secs(1), &attr, 0);
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
            reply.entry(&Duration::from_secs(1), &attr, 0);
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

        let path = self.path_from_bytes(&full_path);
        let client_pid = self.get_client_pid(req);
        match self.core.getattr(&client_pid, path) {
            Ok(attr) => {
                let ino = self.get_or_alloc_inode(&full_path);
                let fuse_attr = self.attr_to_fuse(&attr, ino);
                reply.entry(&Duration::from_secs(1), &fuse_attr, 0);
            }
            Err(FsError::NotFound) => reply.error(ENOENT),
            Err(_) => reply.error(EIO),
        }
    }

    fn getattr(&mut self, req: &Request, ino: u64, _fh: Option<u64>, reply: ReplyAttr) {
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
            reply.attr(&Duration::from_secs(1), &attr);
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
            reply.attr(&Duration::from_secs(1), &attr);
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
            reply.attr(&Duration::from_secs(1), &attr);
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
        let client_pid = self.get_client_pid(req);
        match self.core.getattr(&client_pid, path) {
            Ok(attr) => {
                let fuse_attr = self.attr_to_fuse(&attr, ino);
                reply.attr(&Duration::from_secs(1), &fuse_attr);
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

        let path = self.path_from_bytes(path_bytes);
        let options = self.fuse_flags_to_options(flags);
        let client_pid = self.get_client_pid(req);

        match self.core.open(&client_pid, path, &options) {
            Ok(handle_id) => {
                reply.opened(handle_id.0, 0);
            }
            Err(FsError::NotFound) => reply.error(ENOENT),
            Err(FsError::AccessDenied) => reply.error(EACCES),
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

        let handle_id = HandleId(fh);
        let client_pid = self.get_client_pid(req);

        match self.core.write(&client_pid, handle_id, offset as u64, data) {
            Ok(bytes_written) => {
                reply.written(bytes_written as u32);
            }
            Err(FsError::NotFound) => reply.error(ENOENT),
            Err(_) => reply.error(EIO),
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

        let handle_id = HandleId(fh);
        let client_pid = self.get_client_pid(req);

        match self.core.close(&client_pid, handle_id) {
            Ok(()) => reply.ok(),
            Err(_) => reply.error(EIO),
        }
    }

    fn readdir(
        &mut self,
        req: &Request,
        ino: u64,
        fh: u64,
        offset: i64,
        mut reply: ReplyDirectory,
    ) {
        if ino == AGENTFS_DIR_INO {
            // List the .agentfs directory contents
            if offset == 0 {
                if reply.add(CONTROL_FILE_INO, 1, FileType::RegularFile, "control") {
                    return;
                }
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
        mode: u32,
        _umask: u32,
        flags: i32,
        reply: ReplyCreate,
    ) {
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

        match self.core.create(&client_pid, path, &options) {
            Ok(handle_id) => match self.core.getattr(&client_pid, path) {
                Ok(attr) => {
                    let ino = self.get_or_alloc_inode(&full_path);
                    let fuse_attr = self.attr_to_fuse(&attr, ino);
                    reply.created(
                        &Duration::from_secs(1),
                        &fuse_attr,
                        0,
                        handle_id.0 as u64,
                        0,
                    );
                }
                Err(_) => reply.error(EIO),
            },
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

        match self.core.mkdir(&client_pid, path, mode) {
            Ok(()) => match self.core.getattr(&client_pid, path) {
                Ok(attr) => {
                    let ino = self.get_or_alloc_inode(&full_path);
                    let fuse_attr = self.attr_to_fuse(&attr, ino);
                    reply.entry(&Duration::from_secs(1), &fuse_attr, 0);
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
                // Deallocate inode
                if let Some(inode) = self.paths.get(&full_path) {
                    self.dealloc_inode(*inode);
                }
                reply.ok();
            }
            Err(FsError::NotFound) => reply.error(ENOENT),
            Err(FsError::AccessDenied) => reply.error(EACCES),
            Err(_) => reply.error(EIO),
        }
    }

    fn rmdir(&mut self, req: &Request, parent: u64, name: &OsStr, reply: ReplyEmpty) {
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
                // Deallocate inode
                if let Some(inode) = self.paths.get(&full_path) {
                    self.dealloc_inode(*inode);
                }
                reply.ok();
            }
            Err(FsError::NotFound) => reply.error(ENOENT),
            Err(FsError::Busy) => reply.error(ENOTEMPTY),
            Err(FsError::AccessDenied) => reply.error(EACCES),
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
                // Update inode mapping for the renamed file
                if let Some(inode) = self.paths.remove(&old_path) {
                    self.paths.insert(new_path.clone(), inode);
                    if let Some(path_vec) = self.inodes.get_mut(&inode) {
                        *path_vec = new_path;
                    }
                }
                reply.ok();
            }
            Err(FsError::NotFound) => reply.error(ENOENT),
            Err(FsError::AlreadyExists) => reply.error(EEXIST),
            Err(FsError::AccessDenied) => reply.error(EACCES),
            Err(_) => reply.error(EIO),
        }
    }

    fn ioctl(
        &mut self,
        _req: &Request,
        ino: u64,
        fh: u64,
        flags: u32,
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

    fn fsync(&mut self, _req: &Request, ino: u64, fh: u64, datasync: bool, reply: ReplyEmpty) {
        if ino == CONTROL_FILE_INO {
            reply.ok(); // No-op for control file
            return;
        }

        // For now, fsync is not implemented - no-op (assume data is durable)
        reply.ok();
    }

    fn flush(&mut self, _req: &Request, ino: u64, fh: u64, lock_owner: u64, reply: ReplyEmpty) {
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
        position: u32,
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
        let create = flags == libc::XATTR_CREATE as i32;
        let replace = flags == libc::XATTR_REPLACE as i32;

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

        // For now, we don't implement fallocate - return ENOTSUP
        reply.error(libc::ENOTSUP);
    }

    fn copy_file_range(
        &mut self,
        _req: &Request,
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
        if ino_in == CONTROL_FILE_INO || ino_out == CONTROL_FILE_INO {
            reply.error(libc::EPERM);
            return;
        }

        // For now, we don't implement copy_file_range - return ENOTSUP
        reply.error(libc::ENOTSUP);
    }
}
