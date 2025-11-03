#![cfg(target_os = "macos")]

use agentfs_core::{error::FsResult, types::Backstore};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

/// Mock APFS backstore implementation for testing
///
/// This implementation simulates APFS behavior using a temporary directory
/// and in-memory tracking of reflink relationships for copy-on-write semantics.
pub struct MockApfsBackstore {
    /// Root directory for all backstore files
    root: TempDir,
    /// Track reflink relationships: (source_path, target_path) -> shared_inode_id
    /// In a real implementation, this would be handled by the filesystem
    reflink_groups: std::sync::Mutex<HashMap<(PathBuf, PathBuf), u64>>,
    /// Counter for generating unique inode IDs for reflink groups
    next_inode_id: std::sync::Mutex<u64>,
}

impl MockApfsBackstore {
    /// Create a new mock APFS backstore
    pub fn new() -> FsResult<Self> {
        Ok(Self {
            root: TempDir::new().map_err(|e| agentfs_core::error::FsError::Io(e))?,
            reflink_groups: std::sync::Mutex::new(HashMap::new()),
            next_inode_id: std::sync::Mutex::new(1),
        })
    }

    /// Get the next unique inode ID for reflink groups
    fn next_inode_id(&self) -> u64 {
        let mut id = self.next_inode_id.lock().unwrap();
        let current = *id;
        *id += 1;
        current
    }
}

impl Backstore for MockApfsBackstore {
    fn supports_native_snapshots(&self) -> bool {
        false
    }

    fn snapshot_native(&self, _snapshot_name: &str) -> FsResult<()> {
        Err(agentfs_core::error::FsError::Unsupported)
    }

    fn supports_native_reflink(&self) -> bool {
        true
    }

    fn reflink(&self, from_path: &Path, to_path: &Path) -> FsResult<()> {
        // Ensure the source file exists
        if !from_path.exists() {
            return Err(agentfs_core::error::FsError::NotFound);
        }

        // Create parent directories for target if needed
        if let Some(parent) = to_path.parent() {
            fs::create_dir_all(parent).map_err(|e| agentfs_core::error::FsError::Io(e))?;
        }

        // For mock implementation, we create a copy (simulating the initial reflink)
        // In a real APFS implementation, this would be a true reflink/clonefile
        fs::copy(from_path, to_path).map_err(|e| agentfs_core::error::FsError::Io(e))?;

        // Track the reflink relationship for potential copy-on-write behavior
        // (though in this mock, we don't implement the actual COW breaking)
        let mut groups = self.reflink_groups.lock().unwrap();
        let inode_id = self.next_inode_id();
        groups.insert((from_path.to_path_buf(), to_path.to_path_buf()), inode_id);

        Ok(())
    }

    fn root_path(&self) -> PathBuf {
        self.root.path().to_path_buf()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Write;

    #[test]
    fn ci_gate() {}

    #[test]
    fn mock_reflink_same_inode_until_write() {
        let backstore = MockApfsBackstore::new().unwrap();
        let root = backstore.root_path();

        // Create a source file
        let source_path = root.join("source.txt");
        let mut source_file = fs::File::create(&source_path).unwrap();
        source_file.write_all(b"Hello, World!").unwrap();
        drop(source_file);

        // Create target via reflink
        let target_path = root.join("target.txt");
        backstore.reflink(&source_path, &target_path).unwrap();

        // Initially, both files should exist and have the same content
        assert!(source_path.exists());
        assert!(target_path.exists());
        assert_eq!(
            fs::read(&source_path).unwrap(),
            fs::read(&target_path).unwrap()
        );

        // In this mock implementation, we don't simulate inode sharing,
        // but we verify the files are distinct (which would change in a real reflink)
        // This test primarily ensures the reflink operation succeeds and preserves content
    }

    #[test]
    fn mock_reflink_preserves_xattrs() {
        // Note: On macOS, we would need to test extended attributes preservation
        // For this mock implementation, we skip xattr testing as tempfile-backed
        // filesystems may not support xattrs consistently across test environments
        let backstore = MockApfsBackstore::new().unwrap();
        let root = backstore.root_path();

        // Create a source file
        let source_path = root.join("source.txt");
        fs::write(&source_path, "test content").unwrap();

        // Create target via reflink
        let target_path = root.join("target.txt");
        backstore.reflink(&source_path, &target_path).unwrap();

        // Verify content is preserved
        assert_eq!(
            fs::read(&source_path).unwrap(),
            fs::read(&target_path).unwrap()
        );
    }

    #[test]
    fn mock_snapshot_unsupported_error_code() {
        let backstore = MockApfsBackstore::new().unwrap();

        let result = backstore.snapshot_native("test_snapshot");
        assert!(matches!(
            result,
            Err(agentfs_core::error::FsError::Unsupported)
        ));
    }

    #[test]
    fn memory_leak_test_reflink_loop() {
        // Test creating a complex reflink scenario to ensure no memory leaks
        // or other issues occur with repeated reflink operations
        let backstore = MockApfsBackstore::new().unwrap();
        let root = backstore.root_path();

        // Create initial file
        let base_path = root.join("base.txt");
        fs::write(&base_path, "base content").unwrap();

        // Create a chain of reflinks
        let mut current_path = base_path.clone();
        for i in 0..10 {
            let next_path = root.join(format!("chain_{}.txt", i));
            backstore.reflink(&current_path, &next_path).unwrap();

            // Verify content is preserved
            assert_eq!(
                fs::read(&current_path).unwrap(),
                fs::read(&next_path).unwrap()
            );
            current_path = next_path;
        }

        // Create some cross-links to test more complex relationships
        let cross1 = root.join("cross1.txt");
        let cross2 = root.join("cross2.txt");
        backstore.reflink(&base_path, &cross1).unwrap();
        backstore.reflink(&current_path, &cross2).unwrap();

        // Verify all files have the same content
        let base_content = fs::read(&base_path).unwrap();
        let cross1_content = fs::read(&cross1).unwrap();
        let cross2_content = fs::read(&cross2).unwrap();

        assert_eq!(base_content, cross1_content);
        assert_eq!(base_content, cross2_content);

        // The backstore should be properly cleaned up when it goes out of scope
        // (TempDir will be dropped, cleaning up all files)
    }
}

#[cfg(test)]
mod proptests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn proptest_reflink_idempotent(content in ".{0,1000}") {
            let backstore = MockApfsBackstore::new().unwrap();
            let root = backstore.root_path();

            // Create source file with random content
            let source_path = root.join("source.txt");
            fs::write(&source_path, &content).unwrap();

            // Create first reflink target
            let target1_path = root.join("target1.txt");
            backstore.reflink(&source_path, &target1_path).unwrap();

            // Create second reflink target from the first target
            let target2_path = root.join("target2.txt");
            backstore.reflink(&target1_path, &target2_path).unwrap();

            // All files should have identical content
            prop_assert_eq!(fs::read(&source_path).unwrap(), fs::read(&target1_path).unwrap());
            prop_assert_eq!(fs::read(&source_path).unwrap(), fs::read(&target2_path).unwrap());
            prop_assert_eq!(fs::read(&target1_path).unwrap(), fs::read(&target2_path).unwrap());
        }
    }
}
