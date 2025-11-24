// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Integration tests that exercise filesystem snapshot providers via the external
//! harness driver. These tests mirror the legacy in-process checks while ensuring
//! providers are validated through the same launch path the CLI will use.

use fs_snapshots_test_harness::assert_driver_exists;
#[cfg(all(feature = "agentfs", target_os = "macos"))]
use fs_snapshots_test_harness::assert_interpose_shim_exists;
use std::io::{self, Write};
use std::process::Command as StdCommand;

#[cfg(feature = "btrfs")]
use fs_snapshots_test_harness::{btrfs_available, btrfs_is_root};
#[cfg(feature = "zfs")]
use fs_snapshots_test_harness::{zfs_available, zfs_is_root};

#[cfg(all(feature = "agentfs", target_os = "macos"))]
fn configure_agentfs_env(
    command: &mut StdCommand,
    driver_path: &std::path::Path,
) -> tempfile::TempDir {
    use std::fs;
    let socket_dir = tempfile::Builder::new()
        .prefix("agentfs-driver-")
        .tempdir()
        .expect("failed to create temporary directory for AgentFS socket");
    let socket_path = socket_dir.path().join("agentfs.sock");
    let _ = fs::remove_file(&socket_path);
    command.env("AGENTFS_INTERPOSE_SOCKET", &socket_path);
    command.env("AGENTFS_INTERPOSE_EXE", driver_path);

    // Ensure we use the agentfs-daemon from the same directory as the driver
    if let Some(parent) = driver_path.parent() {
        let daemon_path = parent.join("agentfs-daemon");
        if daemon_path.exists() {
            command.env("AGENTFS_INTERPOSE_DAEMON_BIN", daemon_path);
        }
    }

    socket_dir
}

/// Integration test for ZFS snapshot providers executed via the harness driver.
#[cfg(feature = "zfs")]
#[test]
fn test_zfs_snapshot_integration() {
    if !zfs_is_root() {
        let _ = writeln!(
            io::stdout(),
            "Skipping ZFS integration test: requires root privileges"
        );
        return;
    }

    if !zfs_available() {
        let _ = writeln!(
            io::stdout(),
            "Skipping ZFS integration test: ZFS tooling not available"
        );
        return;
    }

    let driver = assert_driver_exists().expect("harness driver binary not found");
    let output = StdCommand::new(driver)
        .arg("zfs-snapshot")
        .output()
        .expect("failed to execute fs-snapshots harness zfs-snapshot scenario");

    assert!(
        output.status.success(),
        "zfs-snapshot scenario exited with status {:?}",
        output.status.code()
    );

    let stdout = String::from_utf8(output.stdout).expect("stdout not valid utf-8");
    assert!(
        stdout.contains("ZFS snapshot scenario completed successfully"),
        "expected ZFS success message in stdout, got:\n{}",
        stdout
    );
}

/// Stubbed variant used when the ZFS feature is disabled for this crate.
#[cfg(not(feature = "zfs"))]
#[test]
fn test_zfs_snapshot_integration() {
    let _ = writeln!(
        io::stdout(),
        "Skipping ZFS integration test: zfs feature disabled"
    );
}

/// Integration test for Git snapshot providers via the harness driver.
#[test]
#[ignore = "TODO: Add support for running this in GitHub Actions CI"]
fn test_git_snapshot_integration() {
    if !git_available() {
        let _ = writeln!(
            io::stdout(),
            "Skipping Git integration test: git command not available"
        );
        return;
    }

    let driver = assert_driver_exists().expect("harness driver binary not found");
    let output = StdCommand::new(driver)
        .arg("git-snapshot")
        .output()
        .expect("failed to execute fs-snapshots harness git-snapshot scenario");

    assert!(
        output.status.success(),
        "git-snapshot scenario exited with status {:?}",
        output.status.code()
    );

    let stdout = String::from_utf8(output.stdout).expect("stdout not valid utf-8");
    assert!(
        stdout.contains("Provider: Git"),
        "expected provider information in stdout, got:\n{}",
        stdout
    );
    assert!(
        stdout.contains("Readonly mount:"),
        "expected readonly mount output, got:\n{}",
        stdout
    );
    assert!(
        stdout.contains("Git snapshot scenario completed successfully"),
        "expected success message in stdout, got:\n{}",
        stdout
    );
}

#[cfg(feature = "btrfs")]
#[test]
fn test_btrfs_snapshot_integration() {
    if !btrfs_is_root() {
        let _ = writeln!(
            io::stdout(),
            "Skipping Btrfs integration test: requires root privileges"
        );
        return;
    }

    if !btrfs_available() {
        let _ = writeln!(
            io::stdout(),
            "Skipping Btrfs integration test: Btrfs tooling not available"
        );
        return;
    }

    let driver = assert_driver_exists().expect("harness driver binary not found");
    let output = StdCommand::new(driver)
        .arg("btrfs-snapshot")
        .output()
        .expect("failed to execute fs-snapshots harness btrfs-snapshot scenario");

    assert!(
        output.status.success(),
        "btrfs-snapshot scenario exited with status {:?}",
        output.status.code()
    );

    let stdout = String::from_utf8(output.stdout).expect("stdout not valid utf-8");
    assert!(
        stdout.contains("Btrfs snapshot scenario completed successfully"),
        "expected Btrfs success message in stdout, got:\n{}",
        stdout
    );
}

#[cfg(not(feature = "btrfs"))]
#[test]
fn test_btrfs_snapshot_integration() {
    let _ = writeln!(
        io::stdout(),
        "Skipping Btrfs integration test: btrfs feature disabled"
    );
}

#[test]
#[ignore = "TODO: Add support for running this in GitHub Actions CI"]
fn test_git_provider_matrix() {
    if !git_available() {
        let _ = writeln!(
            io::stdout(),
            "Skipping Git provider matrix: git command not available"
        );
        return;
    }

    let driver = assert_driver_exists().expect("harness driver binary not found");
    let output = StdCommand::new(driver)
        .arg("provider-matrix")
        .arg("--provider")
        .arg("git")
        .output()
        .expect("failed to execute fs-snapshots harness provider-matrix git scenario");

    assert!(
        output.status.success(),
        "provider-matrix git scenario exited with status {:?}",
        output.status.code()
    );

    let stdout = String::from_utf8(output.stdout).expect("stdout not valid utf-8");
    assert!(
        stdout.contains("Git provider matrix completed successfully"),
        "expected git matrix success message in stdout, got:\n{}",
        stdout
    );
}

#[cfg(feature = "zfs")]
#[test]
fn test_zfs_provider_matrix() {
    if !zfs_is_root() {
        let _ = writeln!(
            io::stdout(),
            "Skipping ZFS provider matrix: requires root privileges"
        );
        return;
    }

    if !zfs_available() {
        let _ = writeln!(
            io::stdout(),
            "Skipping ZFS provider matrix: ZFS tooling not available"
        );
        return;
    }

    let driver = assert_driver_exists().expect("harness driver binary not found");
    let output = StdCommand::new(driver)
        .arg("provider-matrix")
        .arg("--provider")
        .arg("zfs")
        .output()
        .expect("failed to execute fs-snapshots harness provider-matrix zfs scenario");

    assert!(
        output.status.success(),
        "provider-matrix zfs scenario exited with status {:?}",
        output.status.code()
    );

    let stdout = String::from_utf8(output.stdout).expect("stdout not valid utf-8");
    assert!(
        stdout.contains("ZFS provider matrix completed successfully"),
        "expected zfs matrix success message in stdout, got:\n{}",
        stdout
    );
}

#[cfg(feature = "btrfs")]
#[test]
fn test_btrfs_provider_matrix() {
    if !btrfs_is_root() {
        let _ = writeln!(
            io::stdout(),
            "Skipping Btrfs provider matrix: requires root privileges"
        );
        return;
    }

    if !btrfs_available() {
        let _ = writeln!(
            io::stdout(),
            "Skipping Btrfs provider matrix: Btrfs tooling not available"
        );
        return;
    }

    let driver = assert_driver_exists().expect("harness driver binary not found");
    let output = StdCommand::new(driver)
        .arg("provider-matrix")
        .arg("--provider")
        .arg("btrfs")
        .output()
        .expect("failed to execute fs-snapshots harness provider-matrix btrfs scenario");

    assert!(
        output.status.success(),
        "provider-matrix btrfs scenario exited with status {:?}",
        output.status.code()
    );

    let stdout = String::from_utf8(output.stdout).expect("stdout not valid utf-8");
    assert!(
        stdout.contains("Btrfs provider matrix completed successfully"),
        "expected btrfs matrix success message in stdout, got:\n{}",
        stdout
    );
}

#[cfg(all(feature = "agentfs", target_os = "macos"))]
#[serial_test::file_serial(agentfs)]
#[test]
fn test_agentfs_provider_matrix() {
    let shim_path = assert_interpose_shim_exists().expect("interpose shim not found");

    let driver = assert_driver_exists().expect("harness driver binary not found");
    let mut command = StdCommand::new(&driver);
    command
        .arg("provider-matrix")
        .arg("--provider")
        .arg("agentfs")
        .env("DYLD_INSERT_LIBRARIES", shim_path);
    let _socket_dir = configure_agentfs_env(&mut command, driver.as_path());
    let output = command
        .output()
        .expect("failed to execute fs-snapshots harness provider-matrix agentfs scenario");

    assert!(
        output.status.success(),
        "provider-matrix agentfs scenario exited with status {:?}\nSTDOUT:\n{}\nSTDERR:\n{}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).expect("stdout not valid utf-8");
    assert!(
        stdout.contains("AgentFS provider matrix completed successfully"),
        "expected agentfs matrix success message in stdout, got:\n{}",
        stdout
    );
}

// Use git helpers from ah-repo
use ah_repo::test_helpers::git_available;
