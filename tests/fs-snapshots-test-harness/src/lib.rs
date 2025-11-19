// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Utilities for launching the filesystem snapshots harness driver and locating
//! build artefacts used by snapshot providers.  The goal is to provide thin
//! helpers that tests (from other crates) can call to spawn the external driver
//! binary that loads the AgentFS interpose shim in a production-like manner.

use anyhow::Result;
use std::path::{Path, PathBuf};
use tracing::debug;

pub mod scenarios;

#[cfg(feature = "zfs")]
pub mod zfs;

#[cfg(feature = "zfs")]
pub use zfs::{ZfsHarnessEnvironment, zfs_available};

#[cfg(feature = "zfs")]
pub fn zfs_is_root() -> bool {
    zfs::is_root()
}

#[cfg(feature = "btrfs")]
pub mod btrfs;

#[cfg(feature = "btrfs")]
pub use btrfs::{BtrfsHarnessEnvironment, btrfs_available};

#[cfg(feature = "btrfs")]
pub fn btrfs_is_root() -> bool {
    btrfs::is_root()
}

/// Helpers for locating artefacts produced by the harness build.
pub mod paths {
    use std::env;
    use std::path::PathBuf;

    /// Return the Cargo build profile for the current test run.
    fn cargo_profile() -> String {
        env::var("PROFILE").unwrap_or_else(|_| "debug".to_string())
    }

    /// Path to the workspace `target/<profile>` directory.
    pub fn workspace_target_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("target")
            .join(cargo_profile())
    }

    /// Path to the harness driver binary (`fs-snapshots-harness-driver`).
    pub fn harness_driver_binary() -> PathBuf {
        if let Ok(path) = env::var("CARGO_BIN_EXE_fs-snapshots-harness-driver") {
            return PathBuf::from(path);
        }
        workspace_target_dir().join("fs-snapshots-harness-driver")
    }
}

/// Convenience function for ensuring the harness driver binary exists before a
/// test attempts to spawn it.
pub fn assert_driver_exists() -> Result<PathBuf> {
    let path = paths::harness_driver_binary();
    if std::env::var("FS_SNAPSHOTS_HARNESS_DEBUG").is_ok() {
        if std::env::var("CARGO_BIN_EXE_fs-snapshots-harness-driver").is_ok() {
            debug!(path = %path.display(), "fs-snapshots harness driver located via CARGO_BIN_EXE");
        } else {
            debug!(path = %path.display(), "fs-snapshots harness driver falling back to workspace target");
        }
    }
    if !Path::new(&path).exists() {
        anyhow::bail!(
            "harness driver not found at {}. Run `just build-rust-test-binaries` before invoking the harness tests.",
            path.display()
        );
    }
    Ok(path)
}

/// Ensure the AgentFS interpose shim dylib is present (macOS only).
#[cfg(target_os = "macos")]
pub fn assert_interpose_shim_exists() -> Result<PathBuf> {
    let path = agentfs_interpose_e2e_tests::find_dylib_path();
    if !Path::new(&path).exists() {
        anyhow::bail!(
            "AgentFS interpose shim not found at {}. Build the agentfs-interpose-shim crate.",
            path.display()
        );
    }
    Ok(path)
}

/// No-op stub for platforms without the interpose shim.
#[cfg(not(target_os = "macos"))]
pub fn assert_interpose_shim_exists() -> Result<PathBuf> {
    anyhow::bail!("AgentFS interpose shim is only available on macOS");
}
