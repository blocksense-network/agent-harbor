#![cfg(test)]
// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

use anyhow::Context;
#[cfg(all(feature = "agentfs", target_os = "macos"))]
use fs_snapshots_test_harness::assert_interpose_shim_exists;
use fs_snapshots_test_harness::{assert_driver_exists, scenarios};
#[cfg(feature = "btrfs")]
use fs_snapshots_test_harness::{btrfs_available, btrfs_is_root};
#[cfg(feature = "zfs")]
use fs_snapshots_test_harness::{zfs_available, zfs_is_root};
use std::process::Command as StdCommand;
use tracing::info;

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
    use ah_logging::test_utils::strip_ansi_codes;

    let driver = assert_driver_exists()?;
    let output = tokio::process::Command::new(driver)
        .arg("shim-smoke")
        .env("RUST_LOG", "info")
        .output()
        .await
        .context("failed to run harness driver shim-smoke scenario")?;

    assert!(
        output.status.success(),
        "driver exited with status {:?}",
        output.status.code()
    );

    let stdout = String::from_utf8(output.stdout)?;

    // Find the line containing "shim located" in the tracing output
    let shim_line = stdout
        .lines()
        .find(|line| line.contains("shim located"))
        .unwrap_or_default()
        .trim();

    assert!(
        shim_line.contains("shim located"),
        "shim location not found in output: {}",
        stdout
    );

    // Strip ANSI color codes from the line
    let clean_line = strip_ansi_codes(shim_line);

    // Extract the path from the structured logging output format
    // The format is: timestamp LEVEL target: shim located path=/path/to/shim
    // Split by "path=" and take the second part
    let path_parts: Vec<&str> = clean_line.split("path=").collect();
    let shim_path = path_parts
        .get(1)
        .unwrap_or_else(|| {
            panic!("Could not find 'path=' in cleaned line: {}", clean_line);
        })
        .trim();
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
    // Skip this test in CI environments where git snapshots may not work properly
    if std::env::var("CI").is_ok() {
        info!("Skipping git snapshot scenario test in CI environment");
        return Ok(());
    }

    if !ah_repo::test_helpers::git_available() {
        info!("Skipping Git snapshot scenario: git command not available");
        return Ok(());
    }

    let driver = assert_driver_exists()?;
    let output = StdCommand::new(driver)
        .arg("git-snapshot")
        .env("GIT_AUTHOR_NAME", "test")
        .env("GIT_AUTHOR_EMAIL", "test@example.com")
        .env("GIT_COMMITTER_NAME", "test")
        .env("GIT_COMMITTER_EMAIL", "test@example.com")
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
    // Skip this test in CI environments where git provider matrix may not work properly
    if std::env::var("CI").is_ok() {
        info!("Skipping git provider matrix test in CI environment");
        return Ok(());
    }

    if !ah_repo::test_helpers::git_available() {
        info!("Skipping Git provider matrix: git command not available");
        return Ok(());
    }

    let driver = assert_driver_exists()?;
    let output = StdCommand::new(driver)
        .arg("provider-matrix")
        .arg("--provider")
        .arg("git")
        .env("GIT_AUTHOR_NAME", "test")
        .env("GIT_AUTHOR_EMAIL", "test@example.com")
        .env("GIT_COMMITTER_NAME", "test")
        .env("GIT_COMMITTER_EMAIL", "test@example.com")
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
    // Skip this test in CI environments where git snapshot scenario matching may not work properly
    if std::env::var("CI").is_ok() {
        info!("Skipping git snapshot scenario matches legacy checks test in CI environment");
        return Ok(());
    }

    if !ah_repo::test_helpers::git_available() {
        info!("Skipping Git snapshot scenario: git command not available");
        return Ok(());
    }

    std::env::set_var("GIT_AUTHOR_NAME", "test");
    std::env::set_var("GIT_AUTHOR_EMAIL", "test@example.com");
    std::env::set_var("GIT_COMMITTER_NAME", "test");
    std::env::set_var("GIT_COMMITTER_EMAIL", "test@example.com");

    scenarios::git_snapshot_scenario()
}

#[cfg(feature = "zfs")]
#[test]
fn zfs_snapshot_scenario_runs_successfully() -> anyhow::Result<()> {
    if !zfs_is_root() {
        info!("Skipping ZFS snapshot scenario: requires root privileges");
        return Ok(());
    }

    if !zfs_available() {
        info!("Skipping ZFS snapshot scenario: ZFS tooling not available");
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
        info!("Skipping ZFS provider matrix: requires root privileges");
        return Ok(());
    }

    if !zfs_available() {
        info!("Skipping ZFS provider matrix: ZFS tooling not available");
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
        info!("Skipping ZFS snapshot scenario: requires root privileges");
        return Ok(());
    }

    if !zfs_available() {
        info!("Skipping ZFS snapshot scenario: ZFS tooling not available");
        return Ok(());
    }

    scenarios::zfs_snapshot_scenario()
}

#[cfg(feature = "btrfs")]
#[test]
fn btrfs_snapshot_scenario_runs_successfully() -> anyhow::Result<()> {
    if !btrfs_is_root() {
        info!("Skipping Btrfs snapshot scenario: requires root privileges");
        return Ok(());
    }

    if !btrfs_available() {
        info!("Skipping Btrfs snapshot scenario: Btrfs tooling not available");
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
        info!("Skipping Btrfs provider matrix: requires root privileges");
        return Ok(());
    }

    if !btrfs_available() {
        info!("Skipping Btrfs provider matrix: Btrfs tooling not available");
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
        info!("Skipping Btrfs snapshot scenario: requires root privileges");
        return Ok(());
    }

    if !btrfs_available() {
        info!("Skipping Btrfs snapshot scenario: Btrfs tooling not available");
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
