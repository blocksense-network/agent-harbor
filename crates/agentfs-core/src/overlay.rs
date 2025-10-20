//! Overlay filesystem implementation for AgentFS Core
//!
//! This module provides overlay functionality that allows AgentFS to operate
//! as an overlay on top of an existing filesystem, with copy-on-write semantics.

use std::io::Read;
use std::path::Path;
use crate::error::{FsError, FsResult};
use crate::{Attributes, DirEntry, LowerFs};

/// Host filesystem implementation of LowerFs trait
/// This provides access to the underlying host filesystem for overlay operations.
pub struct HostLowerFs {
    root: std::path::PathBuf,
}

impl HostLowerFs {
    pub fn new(root: std::path::PathBuf) -> FsResult<Self> {
        // Ensure the root exists and is a directory
        let metadata = std::fs::metadata(&root)?;
        if !metadata.is_dir() {
            return Err(FsError::NotADirectory);
        }
        Ok(Self { root })
    }
}

impl LowerFs for HostLowerFs {
    fn stat(&self, abs_path: &Path) -> FsResult<Attributes> {
        // Convert overlay path to host path
        let host_path = self.root.join(abs_path.strip_prefix("/").unwrap_or(abs_path));
        let metadata = std::fs::metadata(&host_path)?;

        // Convert std::fs::Metadata to our Attributes
        let file_type = metadata.file_type();
        let len = metadata.len();
        let times = crate::FileTimes {
            atime: metadata
                .accessed()
                .unwrap_or(std::time::SystemTime::UNIX_EPOCH)
                .duration_since(std::time::SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_secs() as i64,
            mtime: metadata
                .modified()
                .unwrap_or(std::time::SystemTime::UNIX_EPOCH)
                .duration_since(std::time::SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_secs() as i64,
            ctime: metadata
                .created()
                .unwrap_or(std::time::SystemTime::UNIX_EPOCH)
                .duration_since(std::time::SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_secs() as i64,
            birthtime: metadata
                .created()
                .unwrap_or(std::time::SystemTime::UNIX_EPOCH)
                .duration_since(std::time::SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_secs() as i64,
        };

        Ok(Attributes {
            len,
            times,
            uid: 0, // Host FS doesn't have meaningful uid/gid
            gid: 0,
            is_dir: file_type.is_dir(),
            is_symlink: file_type.is_symlink(),
            mode_user: crate::FileMode {
                read: true, // Assume readable for simplicity
                write: file_type.is_dir() || !metadata.permissions().readonly(),
                exec: true, // Assume executable
            },
            mode_group: crate::FileMode {
                read: true,
                write: file_type.is_dir() || !metadata.permissions().readonly(),
                exec: true,
            },
            mode_other: crate::FileMode {
                read: true,
                write: false, // Conservative for other
                exec: file_type.is_dir(),
            },
        })
    }

    fn open_ro(&self, abs_path: &Path) -> FsResult<Box<dyn Read + Send>> {
        let host_path = self.root.join(abs_path.strip_prefix("/").unwrap_or(abs_path));
        let file = std::fs::File::open(&host_path)?;
        Ok(Box::new(file))
    }

    fn readdir(&self, abs_dir: &Path) -> FsResult<Vec<DirEntry>> {
        let host_path = self.root.join(abs_dir.strip_prefix("/").unwrap_or(abs_dir));
        let entries = std::fs::read_dir(&host_path)?;

        let mut result = Vec::new();
        for entry in entries {
            let entry = entry?;
            let file_name = entry.file_name().to_string_lossy().to_string();
            let metadata = entry.metadata()?;

            result.push(DirEntry {
                name: file_name,
                is_dir: metadata.is_dir(),
                is_symlink: metadata.is_symlink(),
                len: metadata.len(),
            });
        }

        Ok(result)
    }

    fn readlink(&self, abs_path: &Path) -> FsResult<std::path::PathBuf> {
        let host_path = self.root.join(abs_path.strip_prefix("/").unwrap_or(abs_path));
        Ok(std::fs::read_link(&host_path)?)
    }

    fn getxattr(&self, _abs_path: &Path, _name: &str) -> FsResult<Vec<u8>> {
        // Simplified implementation for testing - xattrs not supported
        Err(FsError::Unsupported)
    }

    fn listxattr(&self, _abs_path: &Path) -> FsResult<Vec<String>> {
        // Simplified implementation for testing - xattrs not supported
        Err(FsError::Unsupported)
    }
}

/// Create a LowerFs instance from overlay configuration
pub fn create_lower_fs(lower_root: &Path) -> FsResult<Box<dyn LowerFs>> {
    Ok(Box::new(HostLowerFs::new(lower_root.to_path_buf())?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_host_lower_fs_basic() {
        let temp_dir = TempDir::new().unwrap();
        let test_file = temp_dir.path().join("test.txt");

        // Create a test file
        std::fs::write(&test_file, b"lower content").unwrap();

        let lower_fs = HostLowerFs::new(temp_dir.path().to_path_buf()).unwrap();

        // Test stat
        let attrs = lower_fs.stat(Path::new("/test.txt")).unwrap();
        assert_eq!(attrs.len, 13);
        assert!(!attrs.is_dir);
        assert!(!attrs.is_symlink);

        // Test open_ro
        let mut reader = lower_fs.open_ro(Path::new("/test.txt")).unwrap();
        let mut content = String::new();
        reader.read_to_string(&mut content).unwrap();
        assert_eq!(content, "lower content");

        // Test readdir
        let entries = lower_fs.readdir(Path::new("/")).unwrap();
        assert!(entries.iter().any(|e| e.name == "test.txt"));
    }
}
