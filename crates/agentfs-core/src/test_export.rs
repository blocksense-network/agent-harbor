// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

use crate::{
    CachePolicy, CaseSensitivity, FsConfig, FsCore, FsLimits, MemoryPolicy, PID,
    config::{BackstoreMode, InterposeConfig, OverlayConfig},
};

#[test]
fn test_export_snapshot_overlay() {
    let temp_dir = tempfile::TempDir::new().unwrap();

    // Create lower filesystem with test files
    let lower_dir = temp_dir.path().join("lower");
    std::fs::create_dir_all(&lower_dir).unwrap();
    std::fs::write(lower_dir.join("marker.txt"), b"marker content").unwrap();

    // Create export target directory
    let export_dir = temp_dir.path().join("export");

    let config = FsConfig {
        case_sensitivity: CaseSensitivity::Sensitive,
        memory: MemoryPolicy {
            max_bytes_in_memory: Some(1024 * 1024),
            spill_directory: None,
        },
        limits: FsLimits {
            max_open_handles: 1000,
            max_branches: 100,
            max_snapshots: 1000,
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
        backstore: BackstoreMode::InMemory,
        overlay: OverlayConfig {
            enabled: true,
            lower_root: Some(lower_dir.clone()),
            copyup_mode: crate::config::CopyUpMode::Lazy,
            visible_subdir: None,
        },
        interpose: InterposeConfig {
            enabled: true,
            max_copy_bytes: 64 * 1024 * 1024,
            require_reflink: false,
            allow_windows_reparse: false,
        },
    };

    let core = FsCore::new(config).unwrap();
    let pid = PID::new(1);

    // Create a snapshot
    let snapshot_id = core.snapshot_create_for_pid(&pid, Some("snap1")).unwrap();

    // Export the snapshot
    core.export_snapshot(snapshot_id, &export_dir).unwrap();

    // Verify marker exists in export
    let exported_marker = export_dir.join("marker.txt");
    assert!(
        exported_marker.exists(),
        "Marker file should exist in export"
    );
    let content = std::fs::read_to_string(exported_marker).unwrap();
    assert_eq!(content, "marker content");
}

#[test]
fn test_export_snapshot_upper_layer() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let lower_dir = temp_dir.path().join("lower");
    std::fs::create_dir_all(&lower_dir).unwrap();
    let export_dir = temp_dir.path().join("export");

    let config = FsConfig {
        case_sensitivity: CaseSensitivity::Sensitive,
        memory: MemoryPolicy {
            max_bytes_in_memory: Some(1024 * 1024),
            spill_directory: None,
        },
        limits: FsLimits {
            max_open_handles: 1000,
            max_branches: 100,
            max_snapshots: 1000,
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
        backstore: BackstoreMode::InMemory,
        overlay: OverlayConfig {
            enabled: true,
            lower_root: Some(lower_dir.clone()),
            copyup_mode: crate::config::CopyUpMode::Lazy,
            visible_subdir: None,
        },
        interpose: InterposeConfig {
            enabled: true,
            max_copy_bytes: 64 * 1024 * 1024,
            require_reflink: false,
            allow_windows_reparse: false,
        },
    };

    let core = FsCore::new(config).unwrap();
    let pid = PID::new(1);

    // Create a file in the upper layer using FsCore methods
    let path = std::path::Path::new("/upper.txt");
    let handle = core
        .create(
            &pid,
            path,
            &crate::OpenOptions {
                read: true,
                write: true,
                create: true,
                truncate: false,
                append: false,
                share: vec![],
                stream: None,
            },
        )
        .unwrap();

    core.write(&pid, handle, 0, b"upper content").unwrap();
    core.close(&pid, handle).unwrap();

    // Create a snapshot
    let snapshot_id = core.snapshot_create_for_pid(&pid, Some("snap1")).unwrap();

    // Export the snapshot
    core.export_snapshot(snapshot_id, &export_dir).unwrap();

    // Verify upper file exists in export
    let exported_file = export_dir.join("upper.txt");
    assert!(exported_file.exists(), "Upper file should exist in export");
    let content = std::fs::read_to_string(exported_file).unwrap();
    assert_eq!(content, "upper content");
}
