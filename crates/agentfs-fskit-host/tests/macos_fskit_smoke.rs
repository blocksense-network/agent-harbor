// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

#[cfg(target_os = "macos")]
mod macos_tests {
    use agentfs_core::{CaseSensitivity, FsConfig};
    use agentfs_fskit_host::{FsKitAdapter, FsKitConfig};
    use std::path::PathBuf;

    #[test]
    fn fskit_mount_unmount_smoke() {
        // Minimal FsConfig
        let fs_config = FsConfig {
            case_sensitivity: CaseSensitivity::InsensitivePreserving,
            ..Default::default()
        };

        // Use a temporary directory path for mount point (no real mount performed by stub)
        let mount_point = PathBuf::from("/tmp/agentfs-fskit-smoke-test");

        let cfg = FsKitConfig {
            fs_config,
            mount_point: mount_point.to_string_lossy().to_string(),
            xpc_service_name: None,
        };

        // Adapter should construct and mount/unmount without requiring a real FSKit extension
        let adapter = FsKitAdapter::new(cfg).expect("adapter constructed");

        // The current FSKit implementation is a stub that prints; ensure methods return Ok
        adapter.mount().expect("mount returned Ok");
        adapter.unmount().expect("unmount returned Ok");
    }
}
