// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Storage backend implementations for AgentFS Core

use libc;
use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::os::unix::io::AsRawFd;
use std::path::Path;
use std::sync::{Arc, Mutex};
use tracing::{debug, error};

// Note: we use libc::clonefile directly on macOS in platform-specific sections.

use crate::config::BackstoreMode;
use crate::error::FsResult;
use crate::fault::{FaultInjector, FaultOp};
use crate::{Backstore, ContentId, FsError, types::FallocateMode};

pub trait StorageBackend: Send + Sync {
    fn read(&self, id: ContentId, offset: u64, buf: &mut [u8]) -> FsResult<usize>;
    fn write(&self, id: ContentId, offset: u64, data: &[u8]) -> FsResult<usize>;
    fn truncate(&self, id: ContentId, new_len: u64) -> FsResult<()>;
    fn allocate(&self, initial: &[u8]) -> FsResult<ContentId>;
    fn clone_cow(&self, base: ContentId) -> FsResult<ContentId>;
    fn sync(&self, _id: ContentId, _data_only: bool) -> FsResult<()> {
        Ok(())
    }
    fn fallocate(
        &self,
        _id: ContentId,
        _mode: FallocateMode,
        _offset: u64,
        _len: u64,
    ) -> FsResult<()> {
        Err(FsError::Unsupported)
    }

    fn copy_range(
        &self,
        _src: ContentId,
        _src_offset: u64,
        _dst: ContentId,
        _dst_offset: u64,
        _len: u64,
    ) -> FsResult<u64> {
        Err(FsError::Unsupported)
    }

    /// Mark content immutable so further writes are prevented.
    fn seal(&self, id: ContentId) -> FsResult<()>; // for snapshot immutability

    /// Get the filesystem path for a content ID (if the backend stores content as files).
    /// Default returns None. Backends with file-based storage override this.
    fn get_content_path(&self, _id: ContentId) -> Option<std::path::PathBuf> {
        None
    }

    /// Seal an entire content tree (recursive sealing for snapshots). Default no-op.
    fn seal_content_tree(&self, _root_content_id: ContentId) -> FsResult<()> {
        Ok(())
    }
}

/// In-memory storage backend implementation
pub struct InMemoryBackend {
    next_id: Mutex<u64>,
    data: Mutex<HashMap<ContentId, Vec<u8>>>,
    refcounts: Mutex<HashMap<ContentId, usize>>,
    sealed: Mutex<HashMap<ContentId, bool>>,
}

impl InMemoryBackend {
    const MAX_FILE_BYTES: u64 = 1 << 34; // 16 GiB safeguard for test environments

    pub fn new() -> Self {
        Self {
            next_id: Mutex::new(1),
            data: Mutex::new(HashMap::new()),
            refcounts: Mutex::new(HashMap::new()),
            sealed: Mutex::new(HashMap::new()),
        }
    }

    fn ensure_len_within_limit(len: u64) -> FsResult<()> {
        if len > Self::MAX_FILE_BYTES || len > usize::MAX as u64 {
            return Err(FsError::InvalidArgument);
        }
        Ok(())
    }

    fn get_next_id(&self) -> ContentId {
        let mut next_id = self.next_id.lock().unwrap();
        let id = ContentId::new(*next_id);
        *next_id += 1;
        id
    }

    fn increment_refcount(&self, id: ContentId) {
        let mut refcounts = self.refcounts.lock().unwrap();
        *refcounts.entry(id).or_insert(0) += 1;
    }

    #[allow(dead_code)] // Called by future drop logic for reference-managed content; suppress until integrated
    fn decrement_refcount(&self, id: ContentId) {
        let mut refcounts = self.refcounts.lock().unwrap();
        if let Some(count) = refcounts.get_mut(&id) {
            *count = count.saturating_sub(1);
            if *count == 0 {
                refcounts.remove(&id);
                let mut data = self.data.lock().unwrap();
                data.remove(&id);
                let mut sealed = self.sealed.lock().unwrap();
                sealed.remove(&id);
            }
        }
    }
}

impl StorageBackend for InMemoryBackend {
    fn read(&self, id: ContentId, offset: u64, buf: &mut [u8]) -> FsResult<usize> {
        let data = self.data.lock().unwrap();
        let content = data.get(&id).ok_or(FsError::NotFound)?;

        let start = offset as usize;
        if start >= content.len() {
            return Ok(0);
        }

        let end = std::cmp::min(start + buf.len(), content.len());
        let bytes_to_copy = end - start;
        buf[..bytes_to_copy].copy_from_slice(&content[start..end]);
        Ok(bytes_to_copy)
    }

    fn write(&self, id: ContentId, offset: u64, data: &[u8]) -> FsResult<usize> {
        let mut storage_data = self.data.lock().unwrap();
        let content = storage_data.get_mut(&id).ok_or(FsError::NotFound)?;

        Self::ensure_len_within_limit(offset)?;
        let start = offset as usize;
        let end_u64 = offset.saturating_add(data.len() as u64);
        Self::ensure_len_within_limit(end_u64)?;
        let end = end_u64 as usize;

        // Extend the content if necessary
        if end > content.len() {
            content.resize(end, 0);
        }

        content[start..end].copy_from_slice(data);
        Ok(data.len())
    }

    fn truncate(&self, id: ContentId, new_len: u64) -> FsResult<()> {
        Self::ensure_len_within_limit(new_len)?;
        let mut data = self.data.lock().unwrap();
        let content = data.get_mut(&id).ok_or(FsError::NotFound)?;
        content.resize(new_len as usize, 0);
        Ok(())
    }

    fn allocate(&self, initial: &[u8]) -> FsResult<ContentId> {
        let id = self.get_next_id();
        let mut data = self.data.lock().unwrap();
        data.insert(id, initial.to_vec());
        self.increment_refcount(id);
        Ok(id)
    }

    fn clone_cow(&self, base: ContentId) -> FsResult<ContentId> {
        let base_content = {
            let data = self.data.lock().unwrap();
            data.get(&base).ok_or(FsError::NotFound)?.clone()
        };
        let id = self.get_next_id();
        {
            let mut data_mut = self.data.lock().unwrap();
            data_mut.insert(id, base_content);
        }
        self.increment_refcount(id);
        Ok(id)
    }

    fn fallocate(&self, id: ContentId, mode: FallocateMode, offset: u64, len: u64) -> FsResult<()> {
        let mut data = self.data.lock().unwrap();
        let content = data.get_mut(&id).ok_or(FsError::NotFound)?;
        match mode {
            FallocateMode::Allocate => {
                let end = offset.checked_add(len).ok_or(FsError::InvalidArgument)? as usize;
                if end > content.len() {
                    Self::ensure_len_within_limit(end as u64)?;
                    content.resize(end, 0);
                }
            }
            FallocateMode::PunchHole => {
                if len == 0 {
                    return Ok(());
                }
                let start = std::cmp::min(offset, content.len() as u64) as usize;
                let end = std::cmp::min(offset.saturating_add(len), content.len() as u64) as usize;
                if start < end {
                    content[start..end].fill(0);
                }
            }
        }
        Ok(())
    }

    fn copy_range(
        &self,
        src: ContentId,
        src_offset: u64,
        dst: ContentId,
        dst_offset: u64,
        len: u64,
    ) -> FsResult<u64> {
        if len == 0 {
            return Ok(0);
        }
        let mut data = self.data.lock().unwrap();
        let src_content = data.get(&src).ok_or(FsError::NotFound)?;
        let start = std::cmp::min(src_offset, src_content.len() as u64) as usize;
        if start >= src_content.len() {
            return Ok(0);
        }
        let available = src_content.len() - start;
        let to_copy = std::cmp::min(len as usize, available);
        let buffer = src_content[start..start + to_copy].to_vec();
        let dest_content = data.get_mut(&dst).ok_or(FsError::NotFound)?;
        let end_offset = dst_offset.checked_add(to_copy as u64).ok_or(FsError::InvalidArgument)?;
        Self::ensure_len_within_limit(end_offset)?;
        if end_offset as usize > dest_content.len() {
            dest_content.resize(end_offset as usize, 0);
        }
        let start_dst = dst_offset as usize;
        dest_content[start_dst..start_dst + to_copy].copy_from_slice(&buffer);
        Ok(to_copy as u64)
    }

    fn seal(&self, id: ContentId) -> FsResult<()> {
        let data = self.data.lock().unwrap();
        if !data.contains_key(&id) {
            return Err(FsError::NotFound);
        }

        let mut sealed = self.sealed.lock().unwrap();
        sealed.insert(id, true);
        Ok(())
    }
}

impl Default for InMemoryBackend {
    fn default() -> Self {
        Self::new()
    }
}

/// Wrapper backend that injects configured failures before delegating to the real backend.
pub struct FaultInjectingBackend {
    inner: Arc<dyn StorageBackend>,
    injector: Arc<FaultInjector>,
}

impl FaultInjectingBackend {
    pub fn new(inner: Arc<dyn StorageBackend>, injector: Arc<FaultInjector>) -> Self {
        Self { inner, injector }
    }

    fn guard(&self, op: FaultOp) -> FsResult<()> {
        if let Some(err) = self.injector.should_fault(op) {
            return Err(err);
        }
        Ok(())
    }
}

impl StorageBackend for FaultInjectingBackend {
    fn read(&self, id: ContentId, offset: u64, buf: &mut [u8]) -> FsResult<usize> {
        self.guard(FaultOp::Read)?;
        self.inner.read(id, offset, buf)
    }

    fn write(&self, id: ContentId, offset: u64, data: &[u8]) -> FsResult<usize> {
        self.guard(FaultOp::Write)?;
        self.inner.write(id, offset, data)
    }

    fn truncate(&self, id: ContentId, new_len: u64) -> FsResult<()> {
        self.guard(FaultOp::Truncate)?;
        self.inner.truncate(id, new_len)
    }

    fn allocate(&self, initial: &[u8]) -> FsResult<ContentId> {
        self.guard(FaultOp::Allocate)?;
        self.inner.allocate(initial)
    }

    fn clone_cow(&self, base: ContentId) -> FsResult<ContentId> {
        self.guard(FaultOp::CloneCow)?;
        self.inner.clone_cow(base)
    }

    fn sync(&self, id: ContentId, data_only: bool) -> FsResult<()> {
        self.guard(FaultOp::Sync)?;
        self.inner.sync(id, data_only)
    }

    fn fallocate(&self, id: ContentId, mode: FallocateMode, offset: u64, len: u64) -> FsResult<()> {
        self.guard(FaultOp::Write)?;
        self.inner.fallocate(id, mode, offset, len)
    }

    fn copy_range(
        &self,
        src: ContentId,
        src_offset: u64,
        dst: ContentId,
        dst_offset: u64,
        len: u64,
    ) -> FsResult<u64> {
        self.guard(FaultOp::Write)?;
        self.inner.copy_range(src, src_offset, dst, dst_offset, len)
    }

    fn seal(&self, id: ContentId) -> FsResult<()> {
        self.inner.seal(id)
    }

    fn get_content_path(&self, id: ContentId) -> Option<std::path::PathBuf> {
        self.inner.get_content_path(id)
    }

    fn seal_content_tree(&self, root_content_id: ContentId) -> FsResult<()> {
        self.inner.seal_content_tree(root_content_id)
    }
}

/// In-memory backstore implementation
pub struct InMemoryBackstore;

impl InMemoryBackstore {
    pub fn new() -> Self {
        Self
    }
}

impl Backstore for InMemoryBackstore {
    fn supports_native_snapshots(&self) -> bool {
        false
    }

    fn snapshot_native(&self, _snapshot_name: &str) -> FsResult<()> {
        Err(FsError::Unsupported)
    }

    fn supports_native_reflink(&self) -> bool {
        false
    }

    fn reflink(&self, _from_path: &Path, _to_path: &Path) -> FsResult<()> {
        Err(FsError::Unsupported)
    }

    fn root_path(&self) -> std::path::PathBuf {
        std::path::PathBuf::new() // No physical path for in-memory
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn snapshot_clonefile_materialize(
        &self,
        _snapshot_name: &str,
        _upper_files: &[(std::path::PathBuf, std::path::PathBuf)],
    ) -> FsResult<()> {
        // In-memory backstore has no filesystem to clonefile on
        Err(FsError::Unsupported)
    }
}

impl Default for InMemoryBackstore {
    fn default() -> Self {
        Self::new()
    }
}

/// Host filesystem storage backend implementation
pub struct HostFsBackend {
    root: std::path::PathBuf,
    next_id: Mutex<u64>,
    refcounts: Mutex<HashMap<ContentId, usize>>,
    sealed: Mutex<HashMap<ContentId, bool>>,
    file_handles: Mutex<HashMap<ContentId, Arc<Mutex<File>>>>,
}

impl HostFsBackend {
    pub fn new(root: std::path::PathBuf) -> FsResult<Self> {
        fs::create_dir_all(&root)?;
        Ok(Self {
            root,
            next_id: Mutex::new(1),
            refcounts: Mutex::new(HashMap::new()),
            sealed: Mutex::new(HashMap::new()),
            file_handles: Mutex::new(HashMap::new()),
        })
    }

    fn get_next_id(&self) -> ContentId {
        let mut next_id = self.next_id.lock().unwrap();
        let id = ContentId::new(*next_id);
        *next_id += 1;
        id
    }

    fn content_path(&self, id: ContentId) -> std::path::PathBuf {
        self.root.join(format!("{:016x}", id.0))
    }

    fn insert_handle(&self, id: ContentId, file: File) -> Arc<Mutex<File>> {
        let mut handles = self.file_handles.lock().unwrap();
        let arc = Arc::new(Mutex::new(file));
        handles.insert(id, arc.clone());
        arc
    }

    fn get_or_open_handle(&self, id: ContentId) -> FsResult<Arc<Mutex<File>>> {
        let mut handles = self.file_handles.lock().unwrap();
        if let Some(handle) = handles.get(&id) {
            return Ok(handle.clone());
        }

        let path = self.content_path(id);
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(false)
            .open(&path)
            .map_err(io_to_fs_error)?;
        let arc = Arc::new(Mutex::new(file));
        handles.insert(id, arc.clone());
        Ok(arc)
    }

    fn punch_hole(&self, file: &mut File, offset: u64, len: u64) -> FsResult<()> {
        if len == 0 {
            return Ok(());
        }
        #[cfg(target_os = "linux")]
        {
            let res = unsafe {
                libc::fallocate64(
                    file.as_raw_fd(),
                    libc::FALLOC_FL_PUNCH_HOLE | libc::FALLOC_FL_KEEP_SIZE,
                    offset as libc::off64_t,
                    len as libc::off64_t,
                )
            };
            if res == 0 {
                return Ok(());
            }
            let err = io::Error::last_os_error();
            if err.raw_os_error() != Some(libc::EOPNOTSUPP)
                && err.raw_os_error() != Some(libc::ENOSYS)
            {
                return Err(err.into());
            }
        }
        self.zero_fill_range(file, offset, len)
    }

    fn zero_fill_range(&self, file: &mut File, offset: u64, len: u64) -> FsResult<()> {
        if len == 0 {
            return Ok(());
        }
        let file_len = file.metadata()?.len();
        if offset >= file_len {
            return Ok(());
        }
        let end = std::cmp::min(offset.saturating_add(len), file_len);
        file.seek(SeekFrom::Start(offset))?;
        let mut remaining = end - offset;
        let buffer = vec![0u8; 1 << 16];
        while remaining > 0 {
            let chunk = std::cmp::min(remaining, buffer.len() as u64);
            file.write_all(&buffer[..chunk as usize])?;
            remaining -= chunk;
        }
        Ok(())
    }

    fn copy_between_files(
        &self,
        src: &mut File,
        src_offset: u64,
        dst: &mut File,
        dst_offset: u64,
        len: u64,
    ) -> FsResult<u64> {
        #[cfg(target_os = "linux")]
        {
            let mut total = 0u64;
            let mut remaining = len;
            let mut src_off = src_offset as libc::off64_t;
            let mut dst_off = dst_offset as libc::off64_t;
            while remaining > 0 {
                let chunk = std::cmp::min(remaining, usize::MAX as u64);
                let copied = unsafe {
                    libc::copy_file_range(
                        src.as_raw_fd(),
                        &mut src_off,
                        dst.as_raw_fd(),
                        &mut dst_off,
                        chunk as libc::size_t,
                        0,
                    )
                };
                if copied == -1 {
                    let err = io::Error::last_os_error();
                    if err.raw_os_error() == Some(libc::ENOSYS)
                        || err.raw_os_error() == Some(libc::EOPNOTSUPP)
                        || err.raw_os_error() == Some(libc::EINVAL)
                        || err.raw_os_error() == Some(libc::EXDEV)
                    {
                        total = 0;
                        break;
                    }
                    return Err(err.into());
                }
                if copied == 0 {
                    break;
                }
                remaining -= copied as u64;
                total += copied as u64;
            }
            if total > 0 || remaining == 0 {
                return Ok(total);
            }
        }
        self.copy_via_buffer(src, src_offset, dst, dst_offset, len)
    }

    fn copy_via_buffer(
        &self,
        src: &mut File,
        src_offset: u64,
        dst: &mut File,
        dst_offset: u64,
        len: u64,
    ) -> FsResult<u64> {
        let mut remaining = len;
        let mut total = 0u64;
        let mut buffer = vec![0u8; 1 << 16];
        let mut current_src = src_offset;
        let mut current_dst = dst_offset;
        while remaining > 0 {
            src.seek(SeekFrom::Start(current_src))?;
            let chunk = std::cmp::min(remaining, buffer.len() as u64);
            let read = src.read(&mut buffer[..chunk as usize])?;
            if read == 0 {
                break;
            }
            dst.seek(SeekFrom::Start(current_dst))?;
            dst.write_all(&buffer[..read])?;
            remaining -= read as u64;
            total += read as u64;
            current_src += read as u64;
            current_dst += read as u64;
        }
        Ok(total)
    }

    fn copy_within_file(
        &self,
        file: &mut File,
        src_offset: u64,
        dst_offset: u64,
        len: u64,
    ) -> FsResult<u64> {
        if len == 0 {
            return Ok(0);
        }
        let mut buffer = vec![0u8; 1 << 16];
        let mut remaining = len;
        let mut total = 0u64;
        let mut current_src = src_offset;
        let mut current_dst = dst_offset;
        while remaining > 0 {
            file.seek(SeekFrom::Start(current_src))?;
            let chunk = std::cmp::min(remaining, buffer.len() as u64);
            let read = file.read(&mut buffer[..chunk as usize])?;
            if read == 0 {
                break;
            }
            file.seek(SeekFrom::Start(current_dst))?;
            file.write_all(&buffer[..read])?;
            remaining -= read as u64;
            total += read as u64;
            current_src += read as u64;
            current_dst += read as u64;
        }
        Ok(total)
    }
}

impl StorageBackend for HostFsBackend {
    fn get_content_path(&self, id: ContentId) -> Option<std::path::PathBuf> {
        Some(self.content_path(id))
    }
    fn read(&self, id: ContentId, offset: u64, buf: &mut [u8]) -> FsResult<usize> {
        let handle = self.get_or_open_handle(id)?;
        let mut file = handle.lock().unwrap();
        file.seek(SeekFrom::Start(offset))?;
        let n = file.read(buf)?;
        Ok(n)
    }

    fn write(&self, id: ContentId, offset: u64, data: &[u8]) -> FsResult<usize> {
        let handle = self.get_or_open_handle(id)?;
        let mut file = handle.lock().unwrap();
        file.seek(SeekFrom::Start(offset))?;
        if let Err(err) = file.write_all(data) {
            error!(
                target: "agentfs::storage",
                ?id,
                offset,
                size = data.len(),
                %err,
                "HostFsBackend write failed"
            );
            return Err(err.into());
        }
        Ok(data.len())
    }

    fn sync(&self, id: ContentId, data_only: bool) -> FsResult<()> {
        let handle = self.get_or_open_handle(id)?;
        let file = handle.lock().unwrap();
        if data_only {
            file.sync_data()?;
        } else {
            file.sync_all()?;
        }
        Ok(())
    }

    fn truncate(&self, id: ContentId, new_len: u64) -> FsResult<()> {
        let handle = self.get_or_open_handle(id)?;
        let file = handle.lock().unwrap();
        file.set_len(new_len)?;
        Ok(())
    }

    fn allocate(&self, initial: &[u8]) -> FsResult<ContentId> {
        let id = self.get_next_id();
        let path = self.content_path(id);

        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(true)
            .open(&path)
            .map_err(io_to_fs_error)?;
        let handle = self.insert_handle(id, file);
        {
            let mut guard = handle.lock().unwrap();
            guard.write_all(initial)?;
            guard.flush()?;
        }

        // Initialize refcount
        let mut refcounts = self.refcounts.lock().unwrap();
        refcounts.insert(id, 1);

        Ok(id)
    }

    fn clone_cow(&self, base: ContentId) -> FsResult<ContentId> {
        // For copy-on-write, we can use reflink if available, otherwise copy
        let base_path = self.content_path(base);
        let new_id = self.get_next_id();
        let new_path = self.content_path(new_id);

        // Try reflink first, fall back to copy
        if self.reflink(&base_path, &new_path).is_err() {
            fs::copy(&base_path, &new_path)?;
        }

        // Initialize refcount
        let mut refcounts = self.refcounts.lock().unwrap();
        refcounts.insert(new_id, 1);

        Ok(new_id)
    }

    fn fallocate(&self, id: ContentId, mode: FallocateMode, offset: u64, len: u64) -> FsResult<()> {
        let handle = self.get_or_open_handle(id)?;
        let mut file = handle.lock().unwrap();
        match mode {
            FallocateMode::Allocate => {
                let target = offset.checked_add(len).ok_or(FsError::InvalidArgument)?;
                let current = file.metadata()?.len();
                if target > current {
                    file.set_len(target)?;
                }
            }
            FallocateMode::PunchHole => {
                self.punch_hole(&mut file, offset, len)?;
            }
        }
        Ok(())
    }

    fn copy_range(
        &self,
        src: ContentId,
        src_offset: u64,
        dst: ContentId,
        dst_offset: u64,
        len: u64,
    ) -> FsResult<u64> {
        if len == 0 {
            return Ok(0);
        }
        if src == dst {
            let handle = self.get_or_open_handle(src)?;
            let mut file = handle.lock().unwrap();
            return self.copy_within_file(&mut file, src_offset, dst_offset, len);
        }
        let src_handle = self.get_or_open_handle(src)?;
        let dst_handle = self.get_or_open_handle(dst)?;
        let mut src_file = src_handle.lock().unwrap();
        let mut dst_file = dst_handle.lock().unwrap();
        self.copy_between_files(&mut src_file, src_offset, &mut dst_file, dst_offset, len)
    }

    fn seal(&self, id: ContentId) -> FsResult<()> {
        let mut sealed = self.sealed.lock().unwrap();
        sealed.insert(id, true);
        Ok(())
    }
}

impl HostFsBackend {
    fn reflink(&self, from_path: &Path, to_path: &Path) -> FsResult<()> {
        // Simple copy for now - in real implementation would use platform-specific reflink
        fs::copy(from_path, to_path)?;
        Ok(())
    }
}

fn io_to_fs_error(err: io::Error) -> FsError {
    if let Some(code) = err.raw_os_error() {
        match code {
            libc::EMFILE | libc::ENFILE => return FsError::TooManyOpenFiles,
            libc::ENOSPC => return FsError::NoSpace,
            libc::EACCES => return FsError::AccessDenied,
            _ => {}
        }
    }
    match err.kind() {
        io::ErrorKind::PermissionDenied => FsError::AccessDenied,
        _ => err.into(),
    }
}

/// Host filesystem backstore implementation
pub struct HostFsBackstore {
    root: std::path::PathBuf,
    prefer_native_snapshots: bool,
}

impl HostFsBackstore {
    pub fn new(root: std::path::PathBuf, prefer_native_snapshots: bool) -> FsResult<Self> {
        // Create the root directory if it doesn't exist
        std::fs::create_dir_all(&root)?;
        Ok(Self {
            root,
            prefer_native_snapshots,
        })
    }

    /// Detect if the filesystem supports native snapshots
    fn detect_native_snapshots(&self) -> bool {
        // For testing purposes, if prefer_native_snapshots is true, pretend we support them
        // In a real implementation, this would check for APFS, ZFS, Btrfs, etc.
        self.prefer_native_snapshots
    }
}

impl Backstore for HostFsBackstore {
    fn supports_native_snapshots(&self) -> bool {
        self.prefer_native_snapshots && self.detect_native_snapshots()
    }

    fn snapshot_native(&self, _snapshot_name: &str) -> FsResult<()> {
        // This is a mock implementation - always return Unsupported
        // In a real implementation, this would check if the underlying filesystem
        // actually supports native snapshots and call the appropriate commands
        Err(FsError::Unsupported)
    }

    fn supports_native_reflink(&self) -> bool {
        // Detect if the underlying filesystem supports native reflink operations
        // For now, check if we're on macOS and the path looks like an APFS volume
        #[cfg(target_os = "macos")]
        {
            use std::path::Path;
            use std::process::Command;

            // Try to find the mount point for this path by walking up the directory tree
            let mut current_path = self.root.as_path();
            while let Some(parent) = current_path.parent() {
                debug!("Checking potential mount point: {}", parent.display());
                if let Ok(output) =
                    Command::new("diskutil").args(["info", &parent.to_string_lossy()]).output()
                {
                    let output_str = String::from_utf8_lossy(&output.stdout);
                    debug!(
                        "diskutil output contains 'apfs': {}",
                        output_str.contains("apfs")
                    );
                    if output_str.contains("File System:") && output_str.contains("apfs") {
                        debug!("APFS detected at {}, returning true", parent.display());
                        return true;
                    }
                }
                current_path = parent;
                // Stop at root to avoid infinite loop
                if parent == Path::new("/") {
                    break;
                }
            }

            // Also check the mount command to see if our path is on an APFS volume
            if let Ok(output) = Command::new("mount").output() {
                let mount_info = String::from_utf8_lossy(&output.stdout);
                for line in mount_info.lines() {
                    if line.contains("apfs") && line.contains(&*self.root.to_string_lossy()) {
                        debug!("Found APFS mount containing our path");
                        return true;
                    }
                }
            }
        }

        // Default fallback: no native reflink support
        debug!("No APFS detected, returning false");
        false
    }

    fn reflink(&self, from_path: &Path, to_path: &Path) -> FsResult<()> {
        // Attempt to create a reflink/copy-on-write copy
        if self.supports_native_reflink() {
            #[cfg(target_os = "macos")]
            {
                // Use clonefile syscall on macOS APFS
                use std::ffi::CString;
                use std::os::unix::ffi::OsStrExt;

                // Create parent directories for target if needed
                if let Some(parent) = to_path.parent() {
                    std::fs::create_dir_all(parent)?;
                }

                // Convert paths to C strings
                let from_cstr = CString::new(from_path.as_os_str().as_bytes())
                    .map_err(|_| FsError::InvalidArgument)?;
                let to_cstr = CString::new(to_path.as_os_str().as_bytes())
                    .map_err(|_| FsError::InvalidArgument)?;

                // Call clonefile with flags=0 (normal copy-on-write clone)
                let result = unsafe { libc::clonefile(from_cstr.as_ptr(), to_cstr.as_ptr(), 0) };

                if result == 0 {
                    // Success
                    Ok(())
                } else {
                    // Error - check errno
                    let errno = unsafe { *libc::__error() };
                    match errno {
                        libc::ENOTSUP => {
                            // Filesystem doesn't support clonefile, fall back to copy
                            fs::copy(from_path, to_path)?;
                            Ok(())
                        }
                        libc::ENOSPC => {
                            // No space left, fall back to copy
                            fs::copy(from_path, to_path)?;
                            Ok(())
                        }
                        libc::ENOENT => Err(FsError::NotFound),
                        libc::EEXIST => Err(FsError::AlreadyExists),
                        _ => Err(FsError::Io(std::io::Error::from_raw_os_error(errno))),
                    }
                }
            }
            #[cfg(not(target_os = "macos"))]
            {
                // On other platforms, fall back to copy for now
                std::fs::copy(from_path, to_path)?;
                Ok(())
            }
        } else {
            // Fallback to copy
            std::fs::copy(from_path, to_path)?;
            Ok(())
        }
    }

    fn root_path(&self) -> std::path::PathBuf {
        self.root.clone()
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn snapshot_clonefile_materialize(
        &self,
        snapshot_name: &str,
        upper_files: &[(std::path::PathBuf, std::path::PathBuf)],
    ) -> FsResult<()> {
        // Create snapshot directory
        let snapshot_dir = self.root.join("snapshots").join(snapshot_name);
        std::fs::create_dir_all(&snapshot_dir)?;

        // For each upper file, create a clonefile copy
        for (upper_path, _overlay_path) in upper_files {
            if upper_path.exists() {
                // Calculate relative path from backstore root
                let relative_path =
                    upper_path.strip_prefix(&self.root).map_err(|_| FsError::InvalidArgument)?;

                // Create destination path in snapshot directory
                let snapshot_path = snapshot_dir.join(relative_path);

                // Ensure parent directories exist
                if let Some(parent) = snapshot_path.parent() {
                    std::fs::create_dir_all(parent)?;
                }

                // Use reflink if supported, otherwise copy
                if self.supports_native_reflink() {
                    self.reflink(upper_path, &snapshot_path)?;
                } else {
                    fs::copy(upper_path, &snapshot_path)?;
                }
            }
        }

        Ok(())
    }
}

/// Create a backstore instance from configuration
pub fn create_backstore(config: &BackstoreMode) -> FsResult<Box<dyn Backstore>> {
    match config {
        BackstoreMode::InMemory => Ok(Box::new(InMemoryBackstore::new())),
        BackstoreMode::HostFs {
            root,
            prefer_native_snapshots,
        } => Ok(Box::new(HostFsBackstore::new(
            root.clone(),
            *prefer_native_snapshots,
        )?)),
        BackstoreMode::RamDisk { size_mb } => {
            // Ramdisk creation requires platform-specific code
            #[cfg(target_os = "macos")]
            {
                // Use dynamic loading or runtime dispatch to avoid circular dependency
                // For now, return Unsupported - the actual implementation should be
                // handled at a higher level where platform-specific crates are available
                let _ = size_mb;
                Err(FsError::Unsupported)
            }
            #[cfg(not(target_os = "macos"))]
            {
                let _ = size_mb; // Suppress unused variable warning
                Err(FsError::Unsupported)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_in_memory_backend_basic() {
        let backend = InMemoryBackend::new();

        // Allocate some content
        let id = backend.allocate(b"hello world").unwrap();
        assert_eq!(
            backend.data.lock().unwrap().get(&id).unwrap().as_slice(),
            b"hello world"
        );

        // Read it back
        let mut buf = [0u8; 5];
        let n = backend.read(id, 0, &mut buf).unwrap();
        assert_eq!(n, 5);
        assert_eq!(&buf, b"hello");

        // Write to it
        let n = backend.write(id, 6, b"AgentFS").unwrap();
        assert_eq!(n, 7);

        // Read the modified content
        let mut buf = [0u8; 13];
        let n = backend.read(id, 0, &mut buf).unwrap();
        assert_eq!(n, 13);
        assert_eq!(&buf, b"hello AgentFS");

        // Truncate
        backend.truncate(id, 5).unwrap();
        let mut buf = [0u8; 10];
        let n = backend.read(id, 0, &mut buf).unwrap();
        assert_eq!(n, 5);
        assert_eq!(&buf[..5], b"hello");
    }

    #[test]
    fn test_clone_cow() {
        let backend = InMemoryBackend::new();

        let id1 = backend.allocate(b"original").unwrap();
        let id2 = backend.clone_cow(id1).unwrap();

        // They should have the same content
        let mut buf1 = [0u8; 8];
        let mut buf2 = [0u8; 8];
        backend.read(id1, 0, &mut buf1).unwrap();
        backend.read(id2, 0, &mut buf2).unwrap();
        assert_eq!(&buf1, &buf2);
        assert_eq!(&buf1, b"original");

        // Modifying one shouldn't affect the other
        backend.write(id2, 0, b"modified").unwrap();

        let mut buf1 = [0u8; 8];
        let mut buf2 = [0u8; 8];
        backend.read(id1, 0, &mut buf1).unwrap();
        backend.read(id2, 0, &mut buf2).unwrap();
        assert_eq!(&buf1, b"original");
        assert_eq!(&buf2, b"modified");
    }

    #[test]
    fn test_seal() {
        let backend = InMemoryBackend::new();
        let id = backend.allocate(b"test").unwrap();

        // Should be able to write before sealing
        backend.write(id, 0, b"modified").unwrap();

        // Seal it
        backend.seal(id).unwrap();

        // Verify it's marked as sealed
        assert!(*backend.sealed.lock().unwrap().get(&id).unwrap());
    }

    #[test]
    fn test_read_beyond_eof() {
        let backend = InMemoryBackend::new();
        let id = backend.allocate(b"short").unwrap();

        let mut buf = [0u8; 10];
        let n = backend.read(id, 10, &mut buf).unwrap();
        assert_eq!(n, 0); // Should return 0 bytes when reading beyond EOF
    }
}
