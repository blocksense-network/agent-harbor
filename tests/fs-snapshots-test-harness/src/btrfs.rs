// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Btrfs-specific helpers used by the harness driver.
//!
//! This module stands up a temporary Btrfs filesystem backed by a sparse file
//! and exposes convenience helpers so tests can exercise the Btrfs snapshot
//! provider end-to-end in the same way the legacy in-process tests did.

#![cfg(feature = "btrfs")]

use anyhow::{Context, Result};
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use tempfile::TempDir;

struct BtrfsVolumeInfo {
    image_file: PathBuf,
    loop_device: String,
    mount_point: PathBuf,
    subvolumes: Vec<PathBuf>,
}

/// Btrfs harness environment that manages temporary loop-backed filesystems.
pub struct BtrfsHarnessEnvironment {
    pub test_dir: PathBuf,
    volumes: Vec<BtrfsVolumeInfo>,
    _temp_dir: TempDir,
}

impl BtrfsHarnessEnvironment {
    /// Create a new harness environment backed by a temporary directory.
    pub fn new() -> Result<Self> {
        let temp_dir = TempDir::new().context("failed to create temporary directory")?;
        let test_dir = temp_dir.path().to_path_buf();

        Ok(Self {
            test_dir,
            volumes: Vec::new(),
            _temp_dir: temp_dir,
        })
    }

    /// Create a Btrfs filesystem backed by a sparse file and return a subvolume path.
    pub fn create_btrfs_test_subvolume(&mut self, name: &str, size_mb: u32) -> Result<PathBuf> {
        let image_file = self.test_dir.join(format!("{name}_device.img"));
        let mount_point = self.test_dir.join(format!("{name}_mount"));
        let subvolume_path = mount_point.join(format!("{name}_repo"));
        let image_path = image_file.as_path();

        // Create sparse backing file using dd for portability.
        let dd_status = Command::new("dd")
            .arg("if=/dev/zero")
            .arg(format!("of={}", image_path.display()))
            .arg("bs=1M")
            .arg(format!("count={size_mb}"))
            .status()
            .context("failed to invoke dd when creating Btrfs backing file")?;
        if !dd_status.success() {
            anyhow::bail!("dd failed while creating Btrfs device image");
        }

        // Attach the image to a loop device.
        let loop_output = Command::new("losetup")
            .arg("--find")
            .arg("--show")
            .arg(image_path)
            .output()
            .context("failed to invoke losetup")?;
        if !loop_output.status.success() {
            anyhow::bail!("losetup failed while creating loop device for Btrfs image");
        }
        let loop_device = String::from_utf8(loop_output.stdout)
            .context("losetup output not utf-8")?
            .trim()
            .to_string();

        // Format the loop device as Btrfs.
        let mkfs_status = Command::new("mkfs.btrfs")
            .arg("-f")
            .arg(&loop_device)
            .status()
            .context("failed to invoke mkfs.btrfs")?;
        if !mkfs_status.success() {
            let _ = Command::new("losetup").arg("-d").arg(&loop_device).status();
            anyhow::bail!("mkfs.btrfs failed for loop device {loop_device}");
        }

        fs::create_dir_all(&mount_point).with_context(|| {
            format!(
                "failed to create Btrfs mount point {}",
                mount_point.display()
            )
        })?;

        let mount_status = Command::new("mount")
            .arg(&loop_device)
            .arg(&mount_point)
            .status()
            .context("failed to invoke mount for Btrfs loop device")?;
        if !mount_status.success() {
            let _ = Command::new("losetup").arg("-d").arg(&loop_device).status();
            anyhow::bail!("mount failed for loop device {loop_device}");
        }

        let subvolume_status = Command::new("btrfs")
            .args(["subvolume", "create", subvolume_path.to_str().unwrap()])
            .status()
            .context("failed to invoke btrfs subvolume create")?;
        if !subvolume_status.success() {
            let _ = Command::new("umount").arg(&mount_point).status();
            let _ = Command::new("losetup").arg("-d").arg(&loop_device).status();
            anyhow::bail!(
                "btrfs subvolume create failed at {}",
                subvolume_path.display()
            );
        }

        self.volumes.push(BtrfsVolumeInfo {
            image_file,
            loop_device,
            mount_point,
            subvolumes: vec![subvolume_path.clone()],
        });

        Ok(subvolume_path)
    }
}

impl Drop for BtrfsHarnessEnvironment {
    fn drop(&mut self) {
        for volume in &mut self.volumes {
            for subvol in volume.subvolumes.iter().rev() {
                let _ = Command::new("btrfs")
                    .args(["subvolume", "delete", subvol.to_str().unwrap()])
                    .status();
            }
            let _ = Command::new("umount").arg(&volume.mount_point).status();
            let _ = Command::new("losetup").arg("-d").arg(&volume.loop_device).status();
            let _ = fs::remove_file(&volume.image_file);
            let _ = fs::remove_dir_all(&volume.mount_point);
        }
        self.volumes.clear();
    }
}

/// Return true when the test is running with elevated privileges.
pub fn is_root() -> bool {
    unsafe { libc::geteuid() == 0 }
}

/// Backwards-compatible helper mirroring the historical naming convention.
pub fn btrfs_is_root() -> bool {
    is_root()
}

/// Check whether the required Btrfs tooling is present on the PATH.
pub fn btrfs_available() -> bool {
    Command::new("which")
        .arg("btrfs")
        .output()
        .map_or(false, |o| o.status.success())
        && Command::new("which")
            .arg("mkfs.btrfs")
            .output()
            .map_or(false, |o| o.status.success())
        && Command::new("which")
            .arg("losetup")
            .output()
            .map_or(false, |o| o.status.success())
        && Command::new("which")
            .arg("mount")
            .output()
            .map_or(false, |o| o.status.success())
}
