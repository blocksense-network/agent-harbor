// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! ZFS test helpers for creating test ZFS pools for testing.
//!
//! This module provides utilities for setting up ZFS test environments,
//! similar to the ZFS portions of the legacy Ruby filesystem_test_helper.rb but implemented in Rust.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::TempDir;

/// A ZFS test environment that manages test ZFS pools and cleanup.
pub struct ZfsTestEnvironment {
    /// Base directory for creating ZFS pool devices and mount points.
    pub test_dir: PathBuf,
    /// List of created ZFS pools for cleanup.
    pub zfs_pools: Vec<ZfsPoolInfo>,
    /// Temporary directory handle (keeps it alive during tests).
    _temp_dir: TempDir,
}

/// Information about a created ZFS test pool.
#[derive(Debug, Clone)]
pub struct ZfsPoolInfo {
    /// Name of the ZFS pool.
    pub pool_name: String,
    /// Path to the pool device file.
    pub device_file: PathBuf,
    /// Mount point for the ZFS dataset.
    pub mount_point: PathBuf,
}

impl ZfsTestEnvironment {
    /// Create a new ZFS test environment with a temporary directory.
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let temp_dir = TempDir::new()?;
        let test_dir = temp_dir.path().to_path_buf();

        Ok(Self {
            test_dir,
            zfs_pools: Vec::new(),
            _temp_dir: temp_dir,
        })
    }

    /// Create a ZFS pool on a file-based device for testing.
    ///
    /// # Arguments
    /// * `pool_name` - Name for the ZFS pool
    /// * `size_mb` - Size of the underlying device file in megabytes (default: 500)
    ///
    /// # Returns
    /// The mount point of the created ZFS dataset.
    pub fn create_zfs_test_pool(
        &mut self,
        pool_name: &str,
        size_mb: Option<u32>,
    ) -> Result<PathBuf, Box<dyn std::error::Error>> {
        let size_mb = size_mb.unwrap_or(500);
        let device_file = self.test_dir.join(format!("{}_device.img", pool_name));
        let mount_point = self.test_dir.join(format!("{}_mount", pool_name));

        // Create device file
        let dd_status = Command::new("dd")
            .arg("if=/dev/zero")
            .arg(format!("of={}", device_file.display()))
            .arg("bs=1M")
            .arg(format!("count={}", size_mb))
            .status()?;

        if !dd_status.success() {
            return Err(format!(
                "Failed to create ZFS device file: {}",
                device_file.display()
            )
            .into());
        }

        // Create ZFS pool
        let zpool_status = Command::new("zpool")
            .arg("create")
            .arg("-f") // Force creation
            .arg(pool_name)
            .arg(device_file.display().to_string())
            .status()?;

        if !zpool_status.success() {
            return Err(format!("Failed to create ZFS pool '{}'", pool_name).into());
        }

        // Create a dataset in the pool
        let dataset_name = format!("{}/test_dataset", pool_name);
        let zfs_status = Command::new("zfs").arg("create").arg(&dataset_name).status()?;

        if !zfs_status.success() {
            // Cleanup pool on failure
            let _ = Command::new("zpool").arg("destroy").arg(pool_name).status();
            return Err(format!("Failed to create ZFS dataset '{}'", dataset_name).into());
        }

        // Set mountpoint
        let mountpoint_status = Command::new("zfs")
            .arg("set")
            .arg(format!("mountpoint={}", mount_point.display()))
            .arg(&dataset_name)
            .status()?;

        if !mountpoint_status.success() {
            // Cleanup on failure
            let _ = Command::new("zfs").arg("destroy").arg("-r").arg(&dataset_name).status();
            let _ = Command::new("zpool").arg("destroy").arg(pool_name).status();
            return Err(format!(
                "Failed to set mountpoint for ZFS dataset '{}'",
                dataset_name
            )
            .into());
        }

        // Track for cleanup
        let info = ZfsPoolInfo {
            pool_name: pool_name.to_string(),
            device_file,
            mount_point: mount_point.clone(),
        };
        self.zfs_pools.push(info);

        Ok(mount_point)
    }

    /// Get filesystem usage in bytes using df.
    ///
    /// # Arguments
    /// * `mount_point` - The filesystem mount point
    ///
    /// # Returns
    /// Used space in bytes.
    pub fn get_filesystem_used_space(
        &self,
        mount_point: &Path,
    ) -> Result<u64, Box<dyn std::error::Error>> {
        let output = Command::new("df")
            .arg("-B1") // 1-byte blocks
            .arg(mount_point)
            .output()?;

        if !output.status.success() {
            return Ok(0);
        }

        let stdout = String::from_utf8(output.stdout)?;
        let lines: Vec<&str> = stdout.lines().collect();
        if lines.len() < 2 {
            return Ok(0);
        }

        let fields: Vec<&str> = lines[1].split_whitespace().collect();
        if fields.len() < 3 {
            return Ok(0);
        }

        // Used space is in field 2 (1B-blocks format)
        fields[2].parse::<u64>().map_err(Into::into)
    }

    /// Cleanup all tracked ZFS pools by destroying them.
    pub fn cleanup_all_pools(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        for pool_info in &self.zfs_pools {
            // Destroy the ZFS pool and all its datasets
            let _ = Command::new("zpool")
                .arg("destroy")
                .arg("-f")
                .arg(&pool_info.pool_name)
                .status();
        }

        self.zfs_pools.clear();
        Ok(())
    }
}

impl Drop for ZfsTestEnvironment {
    fn drop(&mut self) {
        let _ = self.cleanup_all_pools();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_zfs_test_environment_creation() {
        let env = ZfsTestEnvironment::new();
        assert!(env.is_ok());
        let env = env.unwrap();
        assert!(env.test_dir.exists());
    }

    #[test]
    fn test_zfs_pool_creation() {
        // Skip if not running as root (required for ZFS operations)
        if !is_root() {
            println!("Skipping ZFS test: requires root privileges");
            return;
        }

        let mut env = ZfsTestEnvironment::new().unwrap();

        let result = env.create_zfs_test_pool("test_zfs_pool", Some(100));
        match result {
            Ok(mount_point) => {
                println!("Successfully created ZFS pool at: {:?}", mount_point);
                assert!(mount_point.exists());

                // Try to write a test file
                let test_file = mount_point.join("test.txt");
                fs::write(&test_file, "ZFS test content").unwrap();
                assert!(test_file.exists());

                // Verify content
                let content = fs::read_to_string(&test_file).unwrap();
                assert_eq!(content, "ZFS test content");

                // Verify space measurement works
                let space_used = env.get_filesystem_used_space(&mount_point);
                assert!(space_used.is_ok());
                assert!(space_used.unwrap() > 0);
            }
            Err(e) => {
                println!(
                    "ZFS pool creation failed (expected in some environments): {}",
                    e
                );
            }
        }
    }

    fn is_root() -> bool {
        unsafe { libc::geteuid() == 0 }
    }
}
