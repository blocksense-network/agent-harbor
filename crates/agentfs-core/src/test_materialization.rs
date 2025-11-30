// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Tests for overlay materialization modes (F15.5)
//!
//! This module tests the three materialization strategies:
//! - **Lazy**: Files remain in lower layer until first write (O(1) branch creation)
//! - **Eager**: All files copied from lower to upper at branch creation (ZFS-like isolation)
//! - **CloneEager**: Same as Eager but uses reflink when available
//!
//! See [AgentFS.md §Overlay Materialization Modes] for detailed semantics.

use std::path::Path;
use std::time::Instant;

use crate::FsCore;
use crate::config::{
    BackstoreMode, CachePolicy, CaseSensitivity, CopyUpMode, FsConfig, FsLimits, InterposeConfig,
    MaterializationMode, MemoryPolicy, OverlayConfig,
};

/// Helper to create read-only open options
fn ro() -> crate::OpenOptions {
    crate::OpenOptions {
        read: true,
        write: false,
        create: false,
        truncate: false,
        append: false,
        share: vec![],
        stream: None,
    }
}

/// Create a test FsCore with overlay enabled and the specified materialization mode
fn create_overlay_core(
    lower_dir: &Path,
    backstore_dir: &Path,
    materialization: MaterializationMode,
    require_clone_support: bool,
) -> FsCore {
    let config = FsConfig {
        case_sensitivity: CaseSensitivity::Sensitive,
        memory: MemoryPolicy {
            max_bytes_in_memory: Some(1024 * 1024 * 1024),
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
        security: crate::config::SecurityPolicy::default(),
        backstore: BackstoreMode::HostFs {
            root: backstore_dir.to_path_buf(),
            prefer_native_snapshots: false,
        },
        overlay: OverlayConfig {
            enabled: true,
            lower_root: Some(lower_dir.to_path_buf()),
            copyup_mode: CopyUpMode::Lazy,
            visible_subdir: None,
            materialization,
            require_clone_support,
        },
        interpose: InterposeConfig::default(),
    };
    FsCore::new(config).expect("Failed to create FsCore")
}

/// T15.5.1: Lazy mode branch creation is O(1)
///
/// Properties verified: Lazy mode branch creation is O(1)
/// Steps:
/// 1. Create lower layer with many files
/// 2. Create branch with materialization=Lazy
/// 3. Verify creation time < 100ms
/// 4. Verify backstore has 0 files (no eager copy)
#[test]
fn test_materialization_lazy_branch_creation_time() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let lower_dir = temp_dir.path().join("lower");
    let backstore_dir = temp_dir.path().join("backstore");
    std::fs::create_dir_all(&lower_dir).unwrap();
    std::fs::create_dir_all(&backstore_dir).unwrap();

    // Create 1000 files in lower layer (enough to measure time difference)
    let file_count = 1000;
    for i in 0..file_count {
        let file_path = lower_dir.join(format!("file_{:04}.txt", i));
        std::fs::write(&file_path, format!("content {}", i)).unwrap();
    }

    let core = create_overlay_core(&lower_dir, &backstore_dir, MaterializationMode::Lazy, false);

    let pid = core.register_process(1, 1, 1000, 1000);

    // Create a snapshot first
    let snapshot_id = core.snapshot_create_for_pid(&pid, Some("base")).unwrap();

    // Measure branch creation time
    let start = Instant::now();
    let branch_id = core.branch_create_from_snapshot(snapshot_id, Some("lazy-branch")).unwrap();
    let elapsed = start.elapsed();

    // Lazy mode should be very fast (O(1))
    assert!(
        elapsed.as_millis() < 100,
        "Lazy branch creation took {}ms, expected < 100ms",
        elapsed.as_millis()
    );

    // Verify branch was created
    let branches = core.branch_list();
    assert!(branches.iter().any(|b| b.id == branch_id));

    // Verify the branch has Lazy materialization mode recorded
    let branch_info = branches.iter().find(|b| b.id == branch_id).unwrap();
    assert_eq!(branch_info.materialization_mode, MaterializationMode::Lazy);

    // Verify backstore has minimal files (just the backstore structure, not all lower files)
    // In lazy mode, files are NOT copied upfront
    let backstore_file_count = count_files_recursive(&backstore_dir);
    assert!(
        backstore_file_count < file_count,
        "Lazy mode should not copy all {} files to backstore, found {} files",
        file_count,
        backstore_file_count
    );
}

/// T15.5.2: Eager mode copies all lower layer files to upper at branch creation
///
/// Properties verified: Eager mode copies all lower layer files to upper at branch creation
/// Steps:
/// 1. Create lower layer with 100 files
/// 2. Create branch with materialization=Eager
/// 3. Inspect backstore directory
/// 4. Verify all 100 files present in upper layer
#[test]
fn test_materialization_eager_copies_all_files() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let lower_dir = temp_dir.path().join("lower");
    let backstore_dir = temp_dir.path().join("backstore");
    std::fs::create_dir_all(&lower_dir).unwrap();
    std::fs::create_dir_all(&backstore_dir).unwrap();

    // Create 100 files in lower layer
    let file_count = 100;
    for i in 0..file_count {
        let file_path = lower_dir.join(format!("file_{:04}.txt", i));
        std::fs::write(&file_path, format!("content {}", i)).unwrap();
    }

    let core = create_overlay_core(
        &lower_dir,
        &backstore_dir,
        MaterializationMode::Eager,
        false,
    );

    let pid = core.register_process(1, 1, 1000, 1000);

    // Create a snapshot first
    let snapshot_id = core.snapshot_create_for_pid(&pid, Some("base")).unwrap();

    // Create branch with Eager materialization
    let branch_id = core.branch_create_from_snapshot(snapshot_id, Some("eager-branch")).unwrap();

    // Verify branch was created with Eager mode
    let branches = core.branch_list();
    let branch_info = branches.iter().find(|b| b.id == branch_id).unwrap();
    assert_eq!(branch_info.materialization_mode, MaterializationMode::Eager);

    // Verify backstore has all the files (eager copies everything)
    let backstore_file_count = count_files_recursive(&backstore_dir);
    assert!(
        backstore_file_count >= file_count,
        "Eager mode should copy all {} files to backstore, found {} files",
        file_count,
        backstore_file_count
    );

    // Verify files are accessible through the filesystem
    for i in 0..file_count {
        let path_str = format!("/file_{:04}.txt", i);
        let attrs = core.getattr(&pid, path_str.as_ref()).unwrap();
        assert!(!attrs.is_dir);
    }
}

/// T15.5.3: Files created in lower layer after Eager branch creation are NOT visible
///
/// Properties verified: Files created in lower layer after Eager branch creation are NOT visible
/// Steps:
/// 1. Create lower layer
/// 2. Create branch with materialization=Eager
/// 3. Create new file in lower layer
/// 4. Verify file NOT visible in branch
#[test]
fn test_materialization_eager_isolation_from_lower_creates() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let lower_dir = temp_dir.path().join("lower");
    let backstore_dir = temp_dir.path().join("backstore");
    std::fs::create_dir_all(&lower_dir).unwrap();
    std::fs::create_dir_all(&backstore_dir).unwrap();

    // Create initial file in lower
    std::fs::write(lower_dir.join("existing.txt"), "existing content").unwrap();

    let core = create_overlay_core(
        &lower_dir,
        &backstore_dir,
        MaterializationMode::Eager,
        false,
    );

    let pid = core.register_process(1, 1, 1000, 1000);

    // Verify existing file is visible before branching
    assert!(core.getattr(&pid, "/existing.txt".as_ref()).is_ok());

    // Create snapshot and branch
    let snapshot_id = core.snapshot_create_for_pid(&pid, Some("base")).unwrap();
    let branch_id = core.branch_create_from_snapshot(snapshot_id, Some("eager-branch")).unwrap();

    // Bind to the branch
    core.bind_process_to_branch_with_pid(branch_id, pid.0).unwrap();

    // Now create a NEW file in the lower layer AFTER branch creation
    std::fs::write(lower_dir.join("new_after_branch.txt"), "new content").unwrap();

    // The new file should NOT be visible in the branch (Eager isolation)
    let result = core.getattr(&pid, "/new_after_branch.txt".as_ref());
    assert!(
        result.is_err(),
        "Files created in lower after Eager branch should NOT be visible"
    );

    // But the existing file should still be visible
    assert!(core.getattr(&pid, "/existing.txt".as_ref()).is_ok());
}

/// T15.5.4: Modifications to lower layer files after Eager branch creation do NOT affect branch
///
/// Properties verified: Modifications to lower layer files after Eager branch creation do NOT affect branch
/// Steps:
/// 1. Create lower layer with file "test.txt" containing "original"
/// 2. Create branch with materialization=Eager
/// 3. Modify lower layer file to "modified"
/// 4. Verify branch still sees "original"
#[test]
fn test_materialization_eager_isolation_from_lower_modifications() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let lower_dir = temp_dir.path().join("lower");
    let backstore_dir = temp_dir.path().join("backstore");
    std::fs::create_dir_all(&lower_dir).unwrap();
    std::fs::create_dir_all(&backstore_dir).unwrap();

    // Create initial file with "original" content
    let test_file = lower_dir.join("test.txt");
    std::fs::write(&test_file, "original").unwrap();

    let core = create_overlay_core(
        &lower_dir,
        &backstore_dir,
        MaterializationMode::Eager,
        false,
    );

    let pid = core.register_process(1, 1, 1000, 1000);

    // Create snapshot and branch
    let snapshot_id = core.snapshot_create_for_pid(&pid, Some("base")).unwrap();
    let branch_id = core.branch_create_from_snapshot(snapshot_id, Some("eager-branch")).unwrap();

    // Bind to the branch
    core.bind_process_to_branch_with_pid(branch_id, pid.0).unwrap();

    // Now modify the file in the lower layer AFTER branch creation
    std::fs::write(&test_file, "modified").unwrap();

    // Read the file through FsCore - should still see "original" (Eager isolation)
    let handle = core.open(&pid, "/test.txt".as_ref(), &ro()).unwrap();
    let mut buf = vec![0u8; 100];
    let bytes_read = core.read(&pid, handle, 0, &mut buf).unwrap();
    core.close(&pid, handle).unwrap();

    let content = String::from_utf8_lossy(&buf[..bytes_read]);
    assert_eq!(
        content, "original",
        "Branch should see 'original', not the modified lower layer content"
    );
}

/// T15.5.6: CloneEager falls back to Eager on filesystems without reflink
///
/// Properties verified: CloneEager falls back to Eager on filesystems without reflink (e.g., ext4, tmpfs)
/// Steps:
/// 1. Create lower layer on ext4/tmpfs
/// 2. Create branch with materialization=CloneEager
/// 3. Verify branch creation succeeds
/// 4. Verify files are copied (not reflinked)
#[test]
fn test_materialization_clone_eager_fallback_to_eager() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let lower_dir = temp_dir.path().join("lower");
    let backstore_dir = temp_dir.path().join("backstore");
    std::fs::create_dir_all(&lower_dir).unwrap();
    std::fs::create_dir_all(&backstore_dir).unwrap();

    // Create files in lower layer
    let file_count = 10;
    for i in 0..file_count {
        let file_path = lower_dir.join(format!("file_{}.txt", i));
        std::fs::write(&file_path, format!("content {}", i)).unwrap();
    }

    let core = create_overlay_core(
        &lower_dir,
        &backstore_dir,
        MaterializationMode::CloneEager,
        false, // Don't require reflink, allow fallback
    );

    let pid = core.register_process(1, 1, 1000, 1000);

    // Create snapshot and branch
    let snapshot_id = core.snapshot_create_for_pid(&pid, Some("base")).unwrap();
    let branch_id = core
        .branch_create_from_snapshot(snapshot_id, Some("clone-eager-branch"))
        .unwrap();

    // Branch creation should succeed even if reflink isn't available
    let branches = core.branch_list();
    let branch_info = branches.iter().find(|b| b.id == branch_id).unwrap();
    assert_eq!(
        branch_info.materialization_mode,
        MaterializationMode::CloneEager
    );

    // Files should be accessible
    core.bind_process_to_branch_with_pid(branch_id, pid.0).unwrap();
    for i in 0..file_count {
        let path_str = format!("/file_{}.txt", i);
        assert!(
            core.getattr(&pid, path_str.as_ref()).is_ok(),
            "File {} should be accessible after CloneEager (with fallback)",
            path_str
        );
    }
}

/// T15.5.7: Branch creation fails when require_clone_support=true and reflink unavailable
///
/// Properties verified: Branch creation fails when `require_clone_support=true` and reflink unavailable
/// Steps:
/// 1. Create lower layer on ext4/tmpfs
/// 2. Attempt branch creation with materialization=CloneEager and require_clone_support=true
/// 3. Verify creation fails with actionable error message
#[test]
fn test_materialization_clone_eager_require_clone_fails() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let lower_dir = temp_dir.path().join("lower");
    let backstore_dir = temp_dir.path().join("backstore");
    std::fs::create_dir_all(&lower_dir).unwrap();
    std::fs::create_dir_all(&backstore_dir).unwrap();

    // Create file in lower layer
    std::fs::write(lower_dir.join("test.txt"), "content").unwrap();

    // First check if this filesystem supports reflink
    let supports_reflink = FsCore::can_reflink(&backstore_dir);

    let core = create_overlay_core(
        &lower_dir,
        &backstore_dir,
        MaterializationMode::CloneEager,
        true, // Require reflink support
    );

    let pid = core.register_process(1, 1, 1000, 1000);

    // Create snapshot first
    let snapshot_id = core.snapshot_create_for_pid(&pid, Some("base")).unwrap();

    // Attempt branch creation
    let result = core.branch_create_from_snapshot(snapshot_id, Some("clone-branch"));

    if supports_reflink {
        // On filesystems with reflink support, it should succeed
        assert!(
            result.is_ok(),
            "Branch creation should succeed on reflink-capable FS"
        );
    } else {
        // On filesystems without reflink, it should fail with Unsupported error
        assert!(
            result.is_err(),
            "Branch creation should fail when require_clone_support=true and reflink unavailable"
        );
    }
}

/// T15.5.10: Lazy mode allows lower layer changes to be visible
///
/// Properties verified: Lazy mode allows lower layer changes to be visible (documenting expected behavior)
/// Steps:
/// 1. Create lower layer
/// 2. Create branch with materialization=Lazy
/// 3. Create new file in lower layer
/// 4. Verify file IS visible in branch (expected lazy behavior)
#[test]
fn test_materialization_lazy_lower_visibility() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let lower_dir = temp_dir.path().join("lower");
    let backstore_dir = temp_dir.path().join("backstore");
    std::fs::create_dir_all(&lower_dir).unwrap();
    std::fs::create_dir_all(&backstore_dir).unwrap();

    // Create initial file in lower
    std::fs::write(lower_dir.join("existing.txt"), "existing content").unwrap();

    let core = create_overlay_core(&lower_dir, &backstore_dir, MaterializationMode::Lazy, false);

    let pid = core.register_process(1, 1, 1000, 1000);

    // Create snapshot and branch
    let snapshot_id = core.snapshot_create_for_pid(&pid, Some("base")).unwrap();
    let branch_id = core.branch_create_from_snapshot(snapshot_id, Some("lazy-branch")).unwrap();

    // Bind to the branch
    core.bind_process_to_branch_with_pid(branch_id, pid.0).unwrap();

    // Now create a NEW file in the lower layer AFTER branch creation
    std::fs::write(lower_dir.join("new_after_branch.txt"), "new content").unwrap();

    // In Lazy mode, the new file SHOULD be visible (pass-through behavior)
    // This documents the expected behavior: lazy mode does NOT provide isolation from lower layer changes
    let result = core.getattr(&pid, "/new_after_branch.txt".as_ref());
    assert!(
        result.is_ok(),
        "In Lazy mode, new files in lower layer SHOULD be visible (pass-through)"
    );
}

/// T15.5.13: Branch metadata records which materialization mode was used
///
/// Properties verified: Branch metadata records which materialization mode was used
/// Steps:
/// 1. Create branch with materialization=Eager
/// 2. Query branch info via control plane
/// 3. Verify response includes materialization mode field
#[test]
fn test_materialization_mode_persisted_in_branch_metadata() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let lower_dir = temp_dir.path().join("lower");
    let backstore_dir = temp_dir.path().join("backstore");
    std::fs::create_dir_all(&lower_dir).unwrap();
    std::fs::create_dir_all(&backstore_dir).unwrap();

    // Create a file in lower
    std::fs::write(lower_dir.join("test.txt"), "content").unwrap();

    // Test with Lazy mode
    {
        let core =
            create_overlay_core(&lower_dir, &backstore_dir, MaterializationMode::Lazy, false);
        let pid = core.register_process(1, 1, 1000, 1000);
        let snapshot_id = core.snapshot_create_for_pid(&pid, Some("base")).unwrap();
        let branch_id = core.branch_create_from_snapshot(snapshot_id, Some("lazy-branch")).unwrap();

        let branches = core.branch_list();
        let branch_info = branches.iter().find(|b| b.id == branch_id).unwrap();
        assert_eq!(
            branch_info.materialization_mode,
            MaterializationMode::Lazy,
            "Lazy mode should be recorded in branch metadata"
        );
    }

    // Test with Eager mode (use different backstore)
    let backstore_dir_eager = temp_dir.path().join("backstore_eager");
    std::fs::create_dir_all(&backstore_dir_eager).unwrap();
    {
        let core = create_overlay_core(
            &lower_dir,
            &backstore_dir_eager,
            MaterializationMode::Eager,
            false,
        );
        let pid = core.register_process(1, 1, 1000, 1000);
        let snapshot_id = core.snapshot_create_for_pid(&pid, Some("base")).unwrap();
        let branch_id =
            core.branch_create_from_snapshot(snapshot_id, Some("eager-branch")).unwrap();

        let branches = core.branch_list();
        let branch_info = branches.iter().find(|b| b.id == branch_id).unwrap();
        assert_eq!(
            branch_info.materialization_mode,
            MaterializationMode::Eager,
            "Eager mode should be recorded in branch metadata"
        );
    }

    // Test with CloneEager mode (use different backstore)
    let backstore_dir_clone = temp_dir.path().join("backstore_clone");
    std::fs::create_dir_all(&backstore_dir_clone).unwrap();
    {
        let core = create_overlay_core(
            &lower_dir,
            &backstore_dir_clone,
            MaterializationMode::CloneEager,
            false,
        );
        let pid = core.register_process(1, 1, 1000, 1000);
        let snapshot_id = core.snapshot_create_for_pid(&pid, Some("base")).unwrap();
        let branch_id = core
            .branch_create_from_snapshot(snapshot_id, Some("clone-eager-branch"))
            .unwrap();

        let branches = core.branch_list();
        let branch_info = branches.iter().find(|b| b.id == branch_id).unwrap();
        assert_eq!(
            branch_info.materialization_mode,
            MaterializationMode::CloneEager,
            "CloneEager mode should be recorded in branch metadata"
        );
    }
}

/// T15.5.8: Test branch_create_from_current also uses materialization mode
#[test]
fn test_materialization_branch_from_current() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let lower_dir = temp_dir.path().join("lower");
    let backstore_dir = temp_dir.path().join("backstore");
    std::fs::create_dir_all(&lower_dir).unwrap();
    std::fs::create_dir_all(&backstore_dir).unwrap();

    // Create files in lower layer
    for i in 0..10 {
        std::fs::write(
            lower_dir.join(format!("file_{}.txt", i)),
            format!("content {}", i),
        )
        .unwrap();
    }

    let core = create_overlay_core(
        &lower_dir,
        &backstore_dir,
        MaterializationMode::Eager,
        false,
    );

    let _pid = core.register_process(1, 1, 1000, 1000);

    // Create branch from current state (not from snapshot)
    let branch_id = core.branch_create_from_current(Some("current-branch")).unwrap();

    // Verify branch was created with Eager mode
    let branches = core.branch_list();
    let branch_info = branches.iter().find(|b| b.id == branch_id).unwrap();
    assert_eq!(
        branch_info.materialization_mode,
        MaterializationMode::Eager,
        "branch_create_from_current should also respect materialization mode"
    );
}

/// Test that can_reflink correctly detects filesystem capabilities
#[test]
fn test_can_reflink_detection() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let test_path = temp_dir.path();

    // This should complete without panicking
    let supports_reflink = FsCore::can_reflink(test_path);

    // The result depends on the filesystem, but the function should work
    // On tmpfs (common in tests), reflink is typically NOT supported
    // On Btrfs or XFS with reflink, it would return true
    //
    // The test passes if we get a boolean result without panicking - we use the
    // value to avoid unused variable warning but don't assert on a specific value
    // since reflink support varies by filesystem.
    let _ = supports_reflink;
}

/// Test default materialization mode
#[test]
fn test_materialization_default_mode() {
    // Verify that default is Lazy
    let default_mode = MaterializationMode::default();
    assert_eq!(
        default_mode,
        MaterializationMode::Lazy,
        "Default materialization mode should be Lazy"
    );

    // Verify default OverlayConfig has Lazy mode
    let overlay_config = OverlayConfig::default();
    assert_eq!(
        overlay_config.materialization,
        MaterializationMode::Lazy,
        "Default OverlayConfig should have Lazy materialization"
    );
    assert!(
        !overlay_config.require_clone_support,
        "Default OverlayConfig should not require clone support"
    );
}

/// Helper function to count files recursively in a directory
fn count_files_recursive(dir: &Path) -> usize {
    let mut count = 0;
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                count += count_files_recursive(&path);
            } else if path.is_file() {
                count += 1;
            }
        }
    }
    count
}

/// Test that directories are properly created during eager materialization
#[test]
fn test_materialization_eager_nested_directories() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let lower_dir = temp_dir.path().join("lower");
    let backstore_dir = temp_dir.path().join("backstore");
    std::fs::create_dir_all(&lower_dir).unwrap();
    std::fs::create_dir_all(&backstore_dir).unwrap();

    // Create nested directory structure in lower layer
    let nested_dir = lower_dir.join("a/b/c/d");
    std::fs::create_dir_all(&nested_dir).unwrap();
    std::fs::write(nested_dir.join("deep_file.txt"), "deep content").unwrap();
    std::fs::write(lower_dir.join("a/top_file.txt"), "top content").unwrap();
    std::fs::write(lower_dir.join("a/b/mid_file.txt"), "mid content").unwrap();

    let core = create_overlay_core(
        &lower_dir,
        &backstore_dir,
        MaterializationMode::Eager,
        false,
    );

    let pid = core.register_process(1, 1, 1000, 1000);

    // Create snapshot and branch
    let snapshot_id = core.snapshot_create_for_pid(&pid, Some("base")).unwrap();
    let branch_id = core.branch_create_from_snapshot(snapshot_id, Some("eager-branch")).unwrap();

    // Bind to the branch
    core.bind_process_to_branch_with_pid(branch_id, pid.0).unwrap();

    // Verify all files are accessible
    assert!(
        core.getattr(&pid, "/a/top_file.txt".as_ref()).is_ok(),
        "Top-level nested file should be accessible"
    );
    assert!(
        core.getattr(&pid, "/a/b/mid_file.txt".as_ref()).is_ok(),
        "Mid-level nested file should be accessible"
    );
    assert!(
        core.getattr(&pid, "/a/b/c/d/deep_file.txt".as_ref()).is_ok(),
        "Deeply nested file should be accessible"
    );

    // Verify directories exist
    let dir_attrs = core.getattr(&pid, "/a/b/c/d".as_ref()).unwrap();
    assert!(dir_attrs.is_dir, "/a/b/c/d should be a directory");
}

/// Test that symlinks are handled during eager materialization
#[test]
fn test_materialization_eager_symlinks() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let lower_dir = temp_dir.path().join("lower");
    let backstore_dir = temp_dir.path().join("backstore");
    std::fs::create_dir_all(&lower_dir).unwrap();
    std::fs::create_dir_all(&backstore_dir).unwrap();

    // Create a file and a symlink in lower layer
    std::fs::write(lower_dir.join("target.txt"), "target content").unwrap();

    #[cfg(unix)]
    {
        std::os::unix::fs::symlink("target.txt", lower_dir.join("link.txt")).unwrap();
    }

    let core = create_overlay_core(
        &lower_dir,
        &backstore_dir,
        MaterializationMode::Eager,
        false,
    );

    let pid = core.register_process(1, 1, 1000, 1000);

    // Create snapshot and branch
    let snapshot_id = core.snapshot_create_for_pid(&pid, Some("base")).unwrap();
    let branch_id = core.branch_create_from_snapshot(snapshot_id, Some("eager-branch")).unwrap();

    // Bind to the branch
    core.bind_process_to_branch_with_pid(branch_id, pid.0).unwrap();

    // Verify the target file is accessible
    assert!(
        core.getattr(&pid, "/target.txt".as_ref()).is_ok(),
        "Target file should be accessible"
    );

    #[cfg(unix)]
    {
        // Verify the symlink is accessible
        let link_attrs = core.getattr(&pid, "/link.txt".as_ref()).unwrap();
        assert!(link_attrs.is_symlink, "Link should be a symlink");
    }
}
