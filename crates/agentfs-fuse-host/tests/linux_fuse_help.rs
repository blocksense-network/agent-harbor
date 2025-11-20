// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

#![allow(clippy::disallowed_methods)]

#[cfg(target_os = "linux")]
mod linux_tests {
    use std::path::PathBuf;
    use std::process::Command;

    #[test]
    fn fuse_host_binary_help_runs() {
        // Locate workspace root via CARGO_MANIFEST_DIR (this file lives in crates/agentfs-fuse-host)
        let crate_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let workspace_root = crate_dir
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root")
            .to_path_buf();

        // The Justfile builds the binary before running tests on Linux
        let bin_path = workspace_root.join("target").join("debug").join("agentfs-fuse-host");
        if !bin_path.exists() {
            eprintln!(
                "Skipping FUSE help test: binary not found at {}",
                bin_path.display()
            );
            return;
        }

        // Running with --help should not attempt a mount and must succeed
        let status = Command::new(&bin_path)
            .arg("--help")
            .status()
            .expect("able to execute agentfs-fuse-host");

        assert!(status.success(), "--help should succeed");
    }
}
