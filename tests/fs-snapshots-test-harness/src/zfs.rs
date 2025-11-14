// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! ZFS-specific helpers used by the harness driver.
//!
//! This module mirrors the legacy ZFS test utilities so that we can stand up
//! temporary pools inside the external harness process and exercise the ZFS
//! snapshot provider end-to-end in the same way the original Rust tests did.

#![cfg(feature = "zfs")]

use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::TempDir;

/// ZFS harness environment that manages ephemeral pools and handles cleanup.
pub struct ZfsHarnessEnvironment {
    /// Base directory used for device images and mountpoints.
    pub test_dir: PathBuf,
    pools: Vec<ZfsPoolInfo>,
    _temp_dir: TempDir,
}

#[derive(Debug)]
struct ZfsPoolInfo {
    pool_name: String,
    device_file: PathBuf,
}

impl ZfsHarnessEnvironment {
    /// Create a new harness environment backed by a temporary directory.
    pub fn new() -> Result<Self> {
        let temp_dir = TempDir::new().context("failed to create temporary directory")?;
        let test_dir = temp_dir.path().to_path_buf();

        Ok(Self {
            test_dir,
            pools: Vec::new(),
            _temp_dir: temp_dir,
        })
    }

    /// Create a ZFS pool backed by a sparse file, returning the dataset mountpoint.
    pub fn create_zfs_test_pool(&mut self, pool_name: &str, size_mb: u32) -> Result<PathBuf> {
        let device_file = self.test_dir.join(format!("{pool_name}_device.img"));
        let mount_point = self.test_dir.join(format!("{pool_name}_mount"));

        // Create sparse device file
        let dd_status = Command::new("dd")
            .arg("if=/dev/zero")
            .arg(format!("of={}", device_file.display()))
            .arg("bs=1M")
            .arg(format!("count={size_mb}"))
            .status()
            .context("failed to invoke dd")?;
        if !dd_status.success() {
            anyhow::bail!("dd failed while creating ZFS device image");
        }

        // Create the pool
        let zpool_status = Command::new("zpool")
            .arg("create")
            .arg("-f")
            .arg(pool_name)
            .arg(device_file.display().to_string())
            .status()
            .context("failed to invoke zpool create")?;
        if !zpool_status.success() {
            anyhow::bail!("zpool create failed");
        }

        // Create a dataset inside the pool
        let dataset_name = format!("{pool_name}/test_dataset");
        let zfs_status = Command::new("zfs")
            .arg("create")
            .arg(&dataset_name)
            .status()
            .context("failed to invoke zfs create")?;
        if !zfs_status.success() {
            let _ = Command::new("zpool").arg("destroy").arg(pool_name).status();
            anyhow::bail!("zfs create failed");
        }

        // Set mount point to the temp directory
        let mount_status = Command::new("zfs")
            .arg("set")
            .arg(format!("mountpoint={}", mount_point.display()))
            .arg(&dataset_name)
            .status()
            .context("failed to invoke zfs set mountpoint")?;
        if !mount_status.success() {
            let _ = Command::new("zfs").arg("destroy").arg("-r").arg(&dataset_name).status();
            let _ = Command::new("zpool").arg("destroy").arg(pool_name).status();
            anyhow::bail!("zfs set mountpoint failed");
        }

        self.pools.push(ZfsPoolInfo {
            pool_name: pool_name.to_owned(),
            device_file,
        });

        Ok(mount_point)
    }

    /// Helper to compute the used space for a mount point (best effort).
    pub fn get_used_space(&self, mount_point: &Path) -> Result<u64> {
        let output = Command::new("df")
            .arg("-B1")
            .arg(mount_point)
            .output()
            .context("failed to invoke df")?;
        if !output.status.success() {
            return Ok(0);
        }

        let stdout = String::from_utf8(output.stdout).context("df output not valid utf-8")?;
        let mut lines = stdout.lines();
        let _header = lines.next();
        if let Some(data) = lines.next() {
            let fields: Vec<&str> = data.split_whitespace().collect();
            if fields.len() >= 3 {
                return fields[2]
                    .parse::<u64>()
                    .map_err(|e| anyhow::anyhow!("failed to parse df used field: {e}"));
            }
        }
        Ok(0)
    }

    fn cleanup_pool(&self, pool: &ZfsPoolInfo) -> Result<()> {
        let dataset = format!("{}/test_dataset", pool.pool_name);
        let _ = Command::new("zfs").arg("destroy").arg("-r").arg(&dataset).status();

        Command::new("zpool")
            .arg("destroy")
            .arg("-f")
            .arg(&pool.pool_name)
            .status()
            .context("failed to invoke zpool destroy")?;

        Ok(())
    }
}

impl Drop for ZfsHarnessEnvironment {
    fn drop(&mut self) {
        for pool in &self.pools {
            let _ = self.cleanup_pool(pool);
            let _ = fs::remove_file(&pool.device_file);
        }
        self.pools.clear();
    }
}

/// Return true when the test is running with elevated privileges.
pub fn is_root() -> bool {
    unsafe { libc::geteuid() == 0 }
}

/// Backwards-compatible helper exposed under the legacy name used by older tests.
pub fn zfs_is_root() -> bool {
    is_root()
}

/// Check whether the ZFS tooling is present on the PATH.
pub fn zfs_available() -> bool {
    Command::new("which").arg("zfs").output().map_or(false, |o| o.status.success())
        && Command::new("which")
            .arg("zpool")
            .output()
            .map_or(false, |o| o.status.success())
}
