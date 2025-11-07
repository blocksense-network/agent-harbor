// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Storage backend implementations for AgentFS Core

use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;

#[cfg(target_os = "macos")]
extern "C" {
    fn clonefile(
        src: *const libc::c_char,
        dst: *const libc::c_char,
        flags: libc::c_int,
    ) -> libc::c_int;
}

use crate::config::BackstoreMode;
use crate::error::FsResult;
use crate::{Backstore, ContentId, FsError};

/// Storage backend trait for content-addressable storage with copy-on-write
pub trait StorageBackend: Send + Sync {
    fn read(&self, id: ContentId, offset: u64, buf: &mut [u8]) -> FsResult<usize>;
    fn write(&self, id: ContentId, offset: u64, data: &[u8]) -> FsResult<usize>;
    fn truncate(&self, id: ContentId, new_len: u64) -> FsResult<()>;
    fn allocate(&self, initial: &[u8]) -> FsResult<ContentId>;
    fn clone_cow(&self, base: ContentId) -> FsResult<ContentId>;
    fn seal(&self, id: ContentId) -> FsResult<()>; // for snapshot immutability

    /// Get the filesystem path for a content ID (if the backend stores content as files)
    fn get_content_path(&self, id: ContentId) -> Option<std::path::PathBuf> {
        // Default implementation returns None (for in-memory backends)
        None
    }

    /// Seal an entire content tree (recursive sealing for snapshots)
    fn seal_content_tree(&self, _root_content_id: ContentId) -> FsResult<()> {
        // For now, this is a no-op. In a real implementation, this would
        // recursively seal all content IDs in the tree
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
    pub fn new() -> Self {
        Self {
            next_id: Mutex::new(1),
            data: Mutex::new(HashMap::new()),
            refcounts: Mutex::new(HashMap::new()),
            sealed: Mutex::new(HashMap::new()),
        }
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

        let start = offset as usize;
        let end = start + data.len();

        // Extend the content if necessary
        if end > content.len() {
            content.resize(end, 0);
        }

        content[start..end].copy_from_slice(data);
        Ok(data.len())
    }

    fn truncate(&self, id: ContentId, new_len: u64) -> FsResult<()> {
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
}

impl HostFsBackend {
    pub fn new(root: std::path::PathBuf) -> FsResult<Self> {
        std::fs::create_dir_all(&root)?;
        Ok(Self {
            root,
            next_id: Mutex::new(1),
            refcounts: Mutex::new(HashMap::new()),
            sealed: Mutex::new(HashMap::new()),
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
}

impl StorageBackend for HostFsBackend {
    fn get_content_path(&self, id: ContentId) -> Option<std::path::PathBuf> {
        Some(self.content_path(id))
    }
    fn read(&self, id: ContentId, offset: u64, buf: &mut [u8]) -> FsResult<usize> {
        let path = self.content_path(id);
        let mut file = std::fs::File::open(&path)?;
        use std::io::{Read, Seek};
        file.seek(std::io::SeekFrom::Start(offset))?;
        let n = file.read(buf)?;
        Ok(n)
    }

    fn write(&self, id: ContentId, offset: u64, data: &[u8]) -> FsResult<usize> {
        let path = self.content_path(id);
        let mut file = std::fs::OpenOptions::new().write(true).create(true).open(&path)?;
        use std::io::{Seek, Write};
        file.seek(std::io::SeekFrom::Start(offset))?;
        let n = file.write(data)?;
        file.flush()?;
        Ok(n)
    }

    fn truncate(&self, id: ContentId, new_len: u64) -> FsResult<()> {
        let path = self.content_path(id);
        let file = std::fs::File::open(&path)?;
        file.set_len(new_len)?;
        Ok(())
    }

    fn allocate(&self, initial: &[u8]) -> FsResult<ContentId> {
        let id = self.get_next_id();
        let path = self.content_path(id);

        // Write initial data
        std::fs::write(&path, initial)?;

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
        if let Err(_) = self.reflink(&base_path, &new_path) {
            std::fs::copy(&base_path, &new_path)?;
        }

        // Initialize refcount
        let mut refcounts = self.refcounts.lock().unwrap();
        refcounts.insert(new_id, 1);

        Ok(new_id)
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
        std::fs::copy(from_path, to_path)?;
        Ok(())
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
                eprintln!(
                    "DEBUG: Checking potential mount point: {}",
                    parent.display()
                );
                if let Ok(output) =
                    Command::new("diskutil").args(&["info", &parent.to_string_lossy()]).output()
                {
                    let output_str = String::from_utf8_lossy(&output.stdout);
                    eprintln!(
                        "DEBUG: diskutil output contains 'apfs': {}",
                        output_str.contains("apfs")
                    );
                    if output_str.contains("File System:") && output_str.contains("apfs") {
                        eprintln!(
                            "DEBUG: APFS detected at {}, returning true",
                            parent.display()
                        );
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
                        eprintln!("DEBUG: Found APFS mount containing our path");
                        return true;
                    }
                }
            }
        }

        // Default fallback: no native reflink support
        eprintln!("DEBUG: No APFS detected, returning false");
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
                            std::fs::copy(from_path, to_path)?;
                            Ok(())
                        }
                        libc::ENOSPC => {
                            // No space left, fall back to copy
                            std::fs::copy(from_path, to_path)?;
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
                    std::fs::copy(upper_path, &snapshot_path)?;
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
        assert_eq!(*backend.sealed.lock().unwrap().get(&id).unwrap(), true);
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
