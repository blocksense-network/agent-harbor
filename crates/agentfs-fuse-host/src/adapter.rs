//! AgentFS FUSE adapter implementation
//!
//! Maps FUSE operations to AgentFS Core calls.

#[cfg(not(feature = "fuse"))]
compile_error!("This module requires the 'fuse' feature to be enabled");

use agentfs_core::{
    Attributes, DirEntry, FileTimes, FsConfig, FsCore, FsError, FsResult, HandleId, LockRange,
    OpenOptions, ShareMode,
};
use agentfs_proto::*;
use fuser::{
    FUSE_ROOT_ID, FileAttr, FileType, ReplyAttr, ReplyBMap, ReplyCreate, ReplyData, ReplyDirectory,
    ReplyEmpty, ReplyEntry, ReplyLSeek, ReplyLock, ReplyOpen, ReplyStatfs, ReplyWrite, ReplyXattr,
    Request, TimeOrNow,
};
use libc::{
    EACCES, EBUSY, EEXIST, EINVAL, EIO, EISDIR, ENAMETOOLONG, ENOENT, ENOTDIR, ENOTEMPTY, ENOTSUP,
    c_int,
};
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
    /// Cache of inode to path mappings for control operations
    inodes: HashMap<u64, Vec<u8>>, // inode -> path
}

impl AgentFsFuse {
    /// Create a new FUSE adapter with the given configuration
    pub fn new(config: FsConfig) -> FsResult<Self> {
        let core = FsCore::new(config)?;
        let mut inodes = HashMap::new();

        // Pre-populate special inodes
        inodes.insert(FUSE_ROOT_ID, b"/".to_vec());
        inodes.insert(AGENTFS_DIR_INO, b"/.agentfs".to_vec());
        inodes.insert(CONTROL_FILE_INO, b"/.agentfs/control".to_vec());

        Ok(Self { core, inodes })
    }

    /// Get the path for a given inode
    fn inode_to_path(&self, ino: u64) -> Option<&[u8]> {
        self.inodes.get(&ino).map(|p| p.as_slice())
    }

    /// Convert a path slice to a Path
    fn path_from_bytes(&self, path: &[u8]) -> &Path {
        Path::new(OsStr::from_bytes(path))
    }

    /// Convert FsCore Attributes to FUSE FileAttr
    fn attr_to_fuse(&self, attr: &Attributes, ino: u64) -> FileAttr {
        let kind = match attr.file_type {
            agentfs_core::FileType::File => FileType::RegularFile,
            agentfs_core::FileType::Directory => FileType::Directory,
            agentfs_core::FileType::Symlink => FileType::Symlink,
        };

        FileAttr {
            ino,
            size: attr.size,
            blocks: (attr.size + 511) / 512, // 512-byte blocks
            atime: attr.atime.into(),
            mtime: attr.mtime.into(),
            ctime: attr.ctime.into(),
            crtime: attr.crtime.into(),
            kind,
            perm: attr.mode as u16,
            nlink: attr.nlink as u32,
            uid: attr.uid,
            gid: attr.gid,
            rdev: attr.rdev as u32,
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
        if flags & O_EXCL != 0 {
            options.create_new = true;
        }
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

        let request: Request = Request::from_ssz_bytes(data).map_err(|e| {
            error!("Failed to decode SSZ control request: {:?}", e);
            EINVAL
        })?;

        // Validate request structure
        if let Err(e) = validate_request(&request) {
            error!("Request validation failed: {}", e);
            let response = Response::error(format!("{}", e), Some(EINVAL as u32));
            return response.as_ssz_bytes().map_err(|_| EIO);
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
                        response.as_ssz_bytes().map_err(|_| EIO)
                    }
                    Err(e) => {
                        let response = Response::error(format!("{:?}", e), Some(e as u32));
                        response.as_ssz_bytes().map_err(|_| EIO)
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
                response.as_ssz_bytes().map_err(|_| EIO)
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
                        response.as_ssz_bytes().map_err(|_| EIO)
                    }
                    Err(e) => {
                        let response = Response::error(format!("{:?}", e), Some(e as u32));
                        response.as_ssz_bytes().map_err(|_| EIO)
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
                        response.as_ssz_bytes().map_err(|_| EIO)
                    }
                    Err(e) => {
                        let response = Response::error(format!("{:?}", e), Some(e as u32));
                        response.as_ssz_bytes().map_err(|_| EIO)
                    }
                }
            }
        }
    }
}

impl fuser::Filesystem for AgentFsFuse {
    fn init(&mut self, _req: &Request, config: &mut fuser::KernelConfig) -> Result<(), c_int> {
        // Map cache configuration from FsConfig to fuse_config
        let core_config = self.core.config();
        config.set_attr_timeout(Duration::from_secs_f64(core_config.cache.attr_timeout));
        config.set_entry_timeout(Duration::from_secs_f64(core_config.cache.entry_timeout));
        config.set_negative_timeout(Duration::from_secs_f64(core_config.cache.negative_timeout));

        if core_config.cache.use_ino {
            // Note: fuser doesn't expose use_ino directly, but we can track inodes
        }

        info!("AgentFS FUSE adapter initialized");
        Ok(())
    }

    fn destroy(&mut self) {
        info!("AgentFS FUSE adapter destroyed");
    }

    fn lookup(&mut self, _req: &Request, parent: u64, name: &OsStr, reply: ReplyEntry) {
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
        match self.core.getattr(path) {
            Ok(attr) => {
                let ino = full_path.len() as u64 + 1000; // Simple inode generation
                self.inodes.insert(ino, full_path);
                let fuse_attr = self.attr_to_fuse(&attr, ino);
                reply.entry(&Duration::from_secs(1), &fuse_attr, 0);
            }
            Err(FsError::NotFound) => reply.error(ENOENT),
            Err(_) => reply.error(EIO),
        }
    }

    fn getattr(&mut self, _req: &Request, ino: u64, reply: ReplyAttr) {
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
        match self.core.getattr(path) {
            Ok(attr) => {
                let fuse_attr = self.attr_to_fuse(&attr, ino);
                reply.attr(&Duration::from_secs(1), &fuse_attr);
            }
            Err(FsError::NotFound) => reply.error(ENOENT),
            Err(_) => reply.error(EIO),
        }
    }

    fn open(&mut self, _req: &Request, ino: u64, flags: i32, reply: ReplyOpen) {
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

        match self.core.open(path, &options) {
            Ok(handle_id) => {
                reply.opened(handle_id.0 as u64, 0);
            }
            Err(FsError::NotFound) => reply.error(ENOENT),
            Err(FsError::PermissionDenied) => reply.error(EACCES),
            Err(_) => reply.error(EIO),
        }
    }

    fn read(
        &mut self,
        _req: &Request,
        ino: u64,
        fh: u64,
        offset: i64,
        size: u32,
        _flags: i32,
        reply: ReplyData,
    ) {
        // Special handling for control file - no data to read
        if ino == CONTROL_FILE_INO {
            reply.data(&[]);
            return;
        }

        let handle_id = HandleId(fh as usize);
        let mut buf = vec![0u8; size as usize];

        match self.core.read(handle_id, offset as u64, &mut buf) {
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
        _req: &Request,
        ino: u64,
        fh: u64,
        offset: i64,
        data: &[u8],
        _write_flags: u32,
        reply: ReplyWrite,
    ) {
        // Control file is not writable
        if ino == CONTROL_FILE_INO {
            reply.error(EACCES);
            return;
        }

        let handle_id = HandleId(fh as usize);

        match self.core.write(handle_id, offset as u64, data) {
            Ok(bytes_written) => {
                reply.written(bytes_written as u32);
            }
            Err(FsError::NotFound) => reply.error(ENOENT),
            Err(_) => reply.error(EIO),
        }
    }

    fn release(
        &mut self,
        _req: &Request,
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

        let handle_id = HandleId(fh as usize);

        match self.core.close(handle_id) {
            Ok(()) => reply.ok(),
            Err(_) => reply.error(EIO),
        }
    }

    fn readdir(
        &mut self,
        _req: &Request,
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

        match self.core.readdir(path) {
            Ok(entries) => {
                for (i, entry) in entries.iter().enumerate().skip(offset as usize) {
                    let entry_ino = path_bytes.len() as u64 + 1000 + i as u64; // Simple inode generation
                    let mut full_path = path_bytes.to_vec();
                    full_path.push(b'/');
                    full_path.extend_from_slice(entry.name.as_bytes());
                    self.inodes.insert(entry_ino, full_path);

                    let file_type = match entry.file_type {
                        agentfs_core::FileType::File => FileType::RegularFile,
                        agentfs_core::FileType::Directory => FileType::Directory,
                        agentfs_core::FileType::Symlink => FileType::Symlink,
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
        _req: &Request,
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

        match self.core.create(path, &options) {
            Ok(handle_id) => match self.core.getattr(path) {
                Ok(attr) => {
                    let ino = full_path.len() as u64 + 1000;
                    self.inodes.insert(ino, full_path);
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
            Err(FsError::PermissionDenied) => reply.error(EACCES),
            Err(_) => reply.error(EIO),
        }
    }

    fn mkdir(
        &mut self,
        _req: &Request,
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

        match self.core.mkdir(path, mode) {
            Ok(()) => match self.core.getattr(path) {
                Ok(attr) => {
                    let ino = full_path.len() as u64 + 1000;
                    self.inodes.insert(ino, full_path);
                    let fuse_attr = self.attr_to_fuse(&attr, ino);
                    reply.entry(&Duration::from_secs(1), &fuse_attr, 0);
                }
                Err(_) => reply.error(EIO),
            },
            Err(FsError::NotFound) => reply.error(ENOENT),
            Err(FsError::AlreadyExists) => reply.error(EEXIST),
            Err(FsError::PermissionDenied) => reply.error(EACCES),
            Err(_) => reply.error(EIO),
        }
    }

    fn unlink(&mut self, _req: &Request, parent: u64, name: &OsStr, reply: ReplyEmpty) {
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

        match self.core.unlink(path) {
            Ok(()) => {
                // Remove from inode cache
                self.inodes.retain(|_, p| p != &full_path);
                reply.ok();
            }
            Err(FsError::NotFound) => reply.error(ENOENT),
            Err(FsError::PermissionDenied) => reply.error(EACCES),
            Err(_) => reply.error(EIO),
        }
    }

    fn rmdir(&mut self, _req: &Request, parent: u64, name: &OsStr, reply: ReplyEmpty) {
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

        match self.core.rmdir(path) {
            Ok(()) => {
                // Remove from inode cache
                self.inodes.retain(|_, p| p != &full_path);
                reply.ok();
            }
            Err(FsError::NotFound) => reply.error(ENOENT),
            Err(FsError::NotEmpty) => reply.error(ENOTEMPTY),
            Err(FsError::PermissionDenied) => reply.error(EACCES),
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
        reply: ReplyData,
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
                    reply.data(&response_data);
                }
            }
            Err(errno) => reply.error(errno),
        }
    }

    fn truncate(
        &mut self,
        _req: &Request,
        ino: u64,
        fh: Option<u64>,
        size: u64,
        reply: ReplyEmpty,
    ) {
        if ino == CONTROL_FILE_INO {
            reply.error(libc::EACCES);
            return;
        }

        // For now, truncate is not implemented - return ENOTSUP
        reply.error(libc::ENOTSUP);
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

    fn getxattr(&mut self, _req: &Request, ino: u64, name: &OsStr, size: u32, reply: ReplyXattr) {
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

        match self.core.xattr_get(path, name_str) {
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
        _req: &Request,
        ino: u64,
        name: &OsStr,
        value: &[u8],
        flags: u32,
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

        // Handle flags (XATTR_CREATE, XATTR_REPLACE)
        let create = flags == libc::XATTR_CREATE as u32;
        let replace = flags == libc::XATTR_REPLACE as u32;

        match self.core.xattr_set(path, name_str, value) {
            Ok(()) => reply.ok(),
            Err(FsError::AlreadyExists) if create => reply.error(libc::EEXIST),
            Err(FsError::NotFound) if replace => reply.error(libc::ENODATA),
            Err(FsError::NotFound) => reply.error(ENOENT),
            Err(_) => reply.error(EIO),
        }
    }

    fn listxattr(&mut self, _req: &Request, ino: u64, size: u32, reply: ReplyXattr) {
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

        match self.core.xattr_list(path) {
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

    fn removexattr(&mut self, _req: &Request, ino: u64, name: &OsStr, reply: ReplyEmpty) {
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

        match self.core.xattr_set(path, name_str, &[]) {
            Ok(()) => reply.ok(),
            Err(FsError::NotFound) => reply.error(libc::ENODATA),
            Err(_) => reply.error(EIO),
        }
    }

    fn utimens(
        &mut self,
        _req: &Request,
        ino: u64,
        atime: Option<TimeOrNow>,
        mtime: Option<TimeOrNow>,
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

        let atime = match atime {
            Some(TimeOrNow::SpecificTime(timespec)) => {
                FileTimes::from_mtimes(timespec.sec as u64, timespec.nsec as u32)
            }
            Some(TimeOrNow::Now) => FileTimes::now(),
            None => FileTimes::now(),
        };

        let mtime = match mtime {
            Some(TimeOrNow::SpecificTime(timespec)) => {
                FileTimes::from_mtimes(timespec.sec as u64, timespec.nsec as u32)
            }
            Some(TimeOrNow::Now) => FileTimes::now(),
            None => FileTimes::now(),
        };

        let times = FileTimes {
            atime,
            mtime,
            ctime: FileTimes::now(),
            crtime: FileTimes::now(), // This should be preserved from existing attrs
        };

        match self.core.set_times(path, times) {
            Ok(()) => reply.ok(),
            Err(FsError::NotFound) => reply.error(ENOENT),
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
        mode: u32,
        reply: ReplyEmpty,
    ) {
        if ino == CONTROL_FILE_INO {
            reply.error(libc::EPERM);
            return;
        }

        // For now, we don't implement fallocate - return ENOTSUP
        reply.error(libc::ENOTSUP);
    }

    fn lseek(
        &mut self,
        _req: &Request,
        ino: u64,
        fh: u64,
        offset: i64,
        whence: u32,
        reply: ReplyLSeek,
    ) {
        if ino == CONTROL_FILE_INO {
            reply.error(libc::EPERM);
            return;
        }

        // For now, we don't implement lseek - return ENOTSUP
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
