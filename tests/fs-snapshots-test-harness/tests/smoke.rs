#![cfg(test)]
// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

use anyhow::Context;
#[cfg(feature = "agentfs")]
use fs_snapshots_test_harness::assert_interpose_shim_exists;
use fs_snapshots_test_harness::{assert_driver_exists, scenarios};
#[cfg(feature = "btrfs")]
use fs_snapshots_test_harness::{btrfs_available, btrfs_is_root};
#[cfg(feature = "zfs")]
use fs_snapshots_test_harness::{zfs_available, zfs_is_root};
use std::process::Command as StdCommand;

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
    socket_dir
}

#[cfg(target_os = "macos")]
#[tokio::test(flavor = "multi_thread")]
async fn harness_driver_sets_dyld_insert_libraries() -> anyhow::Result<()> {
    let driver = assert_driver_exists()?;
    let output = tokio::process::Command::new(driver)
        .arg("shim-smoke")
        .output()
        .await
        .context("failed to run harness driver shim-smoke scenario")?;

    assert!(
        output.status.success(),
        "driver exited with status {:?}",
        output.status.code()
    );

    let stdout = String::from_utf8(output.stdout)?;
    let shim_line = stdout.lines().next().unwrap_or_default().trim();
    assert!(
        shim_line.starts_with("shim located at "),
        "unexpected shim smoke output: {}",
        shim_line
    );
    let shim_path = shim_line["shim located at ".len()..].trim();
    assert!(
        std::path::Path::new(shim_path).exists(),
        "reported shim path does not exist: {}",
        shim_path
    );

    Ok(())
}

#[cfg(not(target_os = "macos"))]
#[test]
fn harness_driver_reports_unsupported() -> anyhow::Result<()> {
    let driver = assert_driver_exists()?;
    let status = StdCommand::new(driver)
        .arg("shim-smoke")
        .status()
        .context("failed to run harness driver shim-smoke scenario")?;

    assert!(
        status.success(),
        "driver exited with status {:?}",
        status.code()
    );

    Ok(())
}

#[test]
fn git_snapshot_scenario_runs_successfully() -> anyhow::Result<()> {
    if !ah_repo::test_helpers::git_available() {
        eprintln!("Skipping Git snapshot scenario: git command not available");
        return Ok(());
    }

    let driver = assert_driver_exists()?;
    let output = StdCommand::new(driver)
        .arg("git-snapshot")
        .output()
        .context("failed to run harness driver git-snapshot scenario")?;

    assert!(
        output.status.success(),
        "driver exited with status {:?}",
        output.status.code()
    );

    let stdout = String::from_utf8(output.stdout)?;
    assert!(
        stdout.contains("Provider: Git"),
        "expected provider information in stdout, got:\n{}",
        stdout
    );
    assert!(
        stdout.contains("Readonly mount:")
            && stdout.contains("Git snapshot scenario completed successfully"),
        "expected success details in stdout, got:\n{}",
        stdout
    );

    Ok(())
}

#[test]
fn git_provider_matrix_runs_successfully() -> anyhow::Result<()> {
    if !ah_repo::test_helpers::git_available() {
        eprintln!("Skipping Git provider matrix: git command not available");
        return Ok(());
    }

    let driver = assert_driver_exists()?;
    let output = StdCommand::new(driver)
        .arg("provider-matrix")
        .arg("--provider")
        .arg("git")
        .output()
        .context("failed to run harness driver provider-matrix git")?;

    assert!(
        output.status.success(),
        "driver exited with status {:?}",
        output.status.code()
    );

    let stdout = String::from_utf8(output.stdout)?;
    assert!(
        stdout.contains("Git provider matrix completed successfully"),
        "expected matrix success message in stdout, got:\n{}",
        stdout
    );

    Ok(())
}

#[test]
fn git_snapshot_scenario_matches_legacy_checks() -> anyhow::Result<()> {
    if !ah_repo::test_helpers::git_available() {
        eprintln!("Skipping Git snapshot scenario: git command not available");
        return Ok(());
    }

    scenarios::git_snapshot_scenario()
}

#[cfg(feature = "zfs")]
#[test]
fn zfs_snapshot_scenario_runs_successfully() -> anyhow::Result<()> {
    if !zfs_is_root() {
        eprintln!("Skipping ZFS snapshot scenario: requires root privileges");
        return Ok(());
    }

    if !zfs_available() {
        eprintln!("Skipping ZFS snapshot scenario: ZFS tooling not available");
        return Ok(());
    }

    let driver = assert_driver_exists()?;
    let output = StdCommand::new(driver)
        .arg("zfs-snapshot")
        .output()
        .context("failed to run harness driver zfs-snapshot scenario")?;

    assert!(
        output.status.success(),
        "driver exited with status {:?}",
        output.status.code()
    );

    let stdout = String::from_utf8(output.stdout)?;
    assert!(
        stdout.contains("ZFS snapshot scenario completed successfully"),
        "expected ZFS success message in stdout, got:\n{}",
        stdout
    );

    Ok(())
}

#[cfg(feature = "zfs")]
#[test]
fn zfs_provider_matrix_runs_successfully() -> anyhow::Result<()> {
    if !zfs_is_root() {
        eprintln!("Skipping ZFS provider matrix: requires root privileges");
        return Ok(());
    }

    if !zfs_available() {
        eprintln!("Skipping ZFS provider matrix: ZFS tooling not available");
        return Ok(());
    }

    let driver = assert_driver_exists()?;
    let output = StdCommand::new(driver)
        .arg("provider-matrix")
        .arg("--provider")
        .arg("zfs")
        .output()
        .context("failed to run harness driver provider-matrix zfs")?;

    assert!(
        output.status.success(),
        "driver exited with status {:?}",
        output.status.code()
    );

    let stdout = String::from_utf8(output.stdout)?;
    assert!(
        stdout.contains("ZFS provider matrix completed successfully"),
        "expected matrix success message in stdout, got:\n{}",
        stdout
    );

    Ok(())
}

#[cfg(feature = "zfs")]
#[test]
fn zfs_snapshot_scenario_matches_legacy_checks() -> anyhow::Result<()> {
    if !zfs_is_root() {
        eprintln!("Skipping ZFS snapshot scenario: requires root privileges");
        return Ok(());
    }

    if !zfs_available() {
        eprintln!("Skipping ZFS snapshot scenario: ZFS tooling not available");
        return Ok(());
    }

    scenarios::zfs_snapshot_scenario()
}

#[cfg(feature = "btrfs")]
#[test]
fn btrfs_snapshot_scenario_runs_successfully() -> anyhow::Result<()> {
    if !btrfs_is_root() {
        eprintln!("Skipping Btrfs snapshot scenario: requires root privileges");
        return Ok(());
    }

    if !btrfs_available() {
        eprintln!("Skipping Btrfs snapshot scenario: Btrfs tooling not available");
        return Ok(());
    }

    let driver = assert_driver_exists()?;
    let output = StdCommand::new(driver)
        .arg("btrfs-snapshot")
        .output()
        .context("failed to run harness driver btrfs-snapshot scenario")?;

    assert!(
        output.status.success(),
        "driver exited with status {:?}",
        output.status.code()
    );

    let stdout = String::from_utf8(output.stdout)?;
    assert!(
        stdout.contains("Btrfs snapshot scenario completed successfully"),
        "expected Btrfs success message in stdout, got:\n{}",
        stdout
    );

    Ok(())
}

#[cfg(feature = "btrfs")]
#[test]
fn btrfs_provider_matrix_runs_successfully() -> anyhow::Result<()> {
    if !btrfs_is_root() {
        eprintln!("Skipping Btrfs provider matrix: requires root privileges");
        return Ok(());
    }

    if !btrfs_available() {
        eprintln!("Skipping Btrfs provider matrix: Btrfs tooling not available");
        return Ok(());
    }

    let driver = assert_driver_exists()?;
    let output = StdCommand::new(driver)
        .arg("provider-matrix")
        .arg("--provider")
        .arg("btrfs")
        .output()
        .context("failed to run harness driver provider-matrix btrfs")?;

    assert!(
        output.status.success(),
        "driver exited with status {:?}",
        output.status.code()
    );

    let stdout = String::from_utf8(output.stdout)?;
    assert!(
        stdout.contains("Btrfs provider matrix completed successfully"),
        "expected matrix success message in stdout, got:\n{}",
        stdout
    );

    Ok(())
}

#[cfg(feature = "btrfs")]
#[test]
fn btrfs_snapshot_scenario_matches_legacy_checks() -> anyhow::Result<()> {
    if !btrfs_is_root() {
        eprintln!("Skipping Btrfs snapshot scenario: requires root privileges");
        return Ok(());
    }

    if !btrfs_available() {
        eprintln!("Skipping Btrfs snapshot scenario: Btrfs tooling not available");
        return Ok(());
    }

    scenarios::btrfs_snapshot_scenario()
}

#[cfg(all(feature = "agentfs", target_os = "macos"))]
#[test]
fn agentfs_provider_matrix_runs_successfully() -> anyhow::Result<()> {
    let shim_path = assert_interpose_shim_exists()?;

    let driver = assert_driver_exists()?;
    let mut command = StdCommand::new(&driver);
    command
        .arg("provider-matrix")
        .arg("--provider")
        .arg("agentfs")
        .env("DYLD_INSERT_LIBRARIES", shim_path);
    let _socket_dir = configure_agentfs_env(&mut command, driver.as_path());
    let output = command
        .output()
        .context("failed to run harness driver provider-matrix agentfs")?;

    assert!(
        output.status.success(),
        "driver exited with status {:?}",
        output.status.code()
    );

    let stdout = String::from_utf8(output.stdout)?;
    assert!(
        stdout.contains("AgentFS provider matrix completed successfully"),
        "expected matrix success message in stdout, got:\n{}",
        stdout
    );

    Ok(())
}

#[cfg(all(feature = "agentfs", not(target_os = "macos")))]
#[test]
fn agentfs_provider_matrix_reports_unsupported() -> anyhow::Result<()> {
    let driver = assert_driver_exists()?;
    let output = StdCommand::new(driver)
        .arg("provider-matrix")
        .arg("--provider")
        .arg("agentfs")
        .output()
        .context("failed to run harness driver provider-matrix agentfs")?;

    assert!(
        output.status.success(),
        "driver exited with status {:?}",
        output.status.code()
    );

    Ok(())
}
