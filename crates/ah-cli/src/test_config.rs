// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Shared test configuration constants.
//!
//! This module centralizes configuration constants used for testing,
//! corresponding to the values defined in scripts/test-filesystems-config.sh.
//!
//! These constants ensure consistency across different test modules and avoid
//! duplication of hardcoded paths.

use std::path::PathBuf;

/// Cache directory for test filesystem backing files
pub fn cache_dir() -> PathBuf {
    std::env::var("HOME")
        .map(|home| PathBuf::from(home).join(".cache/agent-harbor"))
        .unwrap_or_else(|_| PathBuf::from("/tmp/agent-harbor-cache"))
}

/// ZFS configuration
pub mod zfs {
    use super::cache_dir;

    /// ZFS backing file path
    pub fn backing_file() -> std::path::PathBuf {
        cache_dir().join("zfs_backing.img")
    }

    /// ZFS pool name
    pub const POOL_NAME: &str = "AH_test_zfs";

    /// ZFS dataset name within the pool
    pub const DATASET_NAME: &str = "test_dataset";

    /// Full ZFS dataset path (pool/dataset)
    pub fn dataset_path() -> String {
        format!("{}/{}", POOL_NAME, DATASET_NAME)
    }
}

/// Btrfs configuration
pub mod btrfs {
    use super::cache_dir;

    /// Btrfs backing file path
    pub fn backing_file() -> std::path::PathBuf {
        cache_dir().join("btrfs_backing.img")
    }

    /// Btrfs loop device path
    pub const LOOP_DEVICE: &str = "/dev/loop99";
}

/// Get the mount point of the ZFS test filesystem.
///
/// This function determines the platform-specific mount point of the shared ZFS test filesystem
/// created by the test setup scripts. On macOS, ZFS mounts under /Volumes/, while on Linux
/// it mounts under /mnt/ or other locations.
///
/// # Returns
/// The mount point path if the ZFS dataset exists and is mounted, otherwise an error.
pub fn get_zfs_test_mount_point() -> Result<std::path::PathBuf, anyhow::Error> {
    use std::process::Command;

    let dataset_path = zfs::dataset_path();

    // Get the mountpoint using zfs get command (same as check-test-filesystems.sh)
    let output = Command::new("zfs")
        .args(["get", "-H", "-o", "value", "mountpoint", &dataset_path])
        .output()?;

    if !output.status.success() {
        return Err(anyhow::anyhow!(
            "Failed to get ZFS mountpoint for dataset {}: {}",
            dataset_path,
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    let mountpoint = String::from_utf8(output.stdout)?.trim().to_string();

    if mountpoint.is_empty() || mountpoint == "-" {
        return Err(anyhow::anyhow!(
            "ZFS dataset {} is not mounted or mountpoint is invalid",
            dataset_path
        ));
    }

    Ok(std::path::PathBuf::from(mountpoint))
}
