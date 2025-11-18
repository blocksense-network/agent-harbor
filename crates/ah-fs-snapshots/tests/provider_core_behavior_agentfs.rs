// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! AgentFS control-plane behaviour exercised through the external harness.
//! This test mirrors the legacy Ruby provider-core checks while ensuring the
//! interpose shim is active by launching the harness driver in a child
//! process.

use fs_snapshots_test_harness::assert_driver_exists;
use std::io::{self, Write};

#[cfg(all(feature = "agentfs", target_os = "macos"))]
use {
    fs_snapshots_test_harness::assert_interpose_shim_exists,
    std::env,
    std::path::{Path, PathBuf},
    std::process::Command as StdCommand,
};

#[cfg(all(feature = "agentfs", target_os = "macos"))]
fn configure_agentfs_env(command: &mut StdCommand, driver_path: &Path) -> tempfile::TempDir {
    use std::fs;

    let socket_dir = tempfile::Builder::new()
        .prefix("agentfs-provider-core-")
        .tempdir()
        .expect("failed to create temporary directory for AgentFS socket");
    let socket_path = socket_dir.path().join("agentfs.sock");
    let _ = fs::remove_file(&socket_path);
    command.env("AGENTFS_INTERPOSE_SOCKET", &socket_path);
    command.env("AGENTFS_INTERPOSE_EXE", driver_path);
    socket_dir
}

#[cfg(all(feature = "agentfs", target_os = "macos"))]
#[test]
fn provider_core_behavior_agentfs() {
    let shim_path = assert_interpose_shim_exists().expect("interpose shim not found");
    let driver = assert_driver_exists().expect("harness driver binary not found");

    let mut command = StdCommand::new(&driver);
    command
        .arg("provider-matrix")
        .arg("--provider")
        .arg("agentfs")
        .env("DYLD_INSERT_LIBRARIES", &shim_path);
    let _socket_dir = configure_agentfs_env(&mut command, driver.as_path());

    let output = command
        .output()
        .expect("failed to execute fs-snapshots harness provider-matrix agentfs scenario");

    assert!(
        output.status.success(),
        "provider-matrix agentfs scenario exited with status {:?}\nstdout:\n{}\nstderr:\n{}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).expect("stdout not valid utf-8");
    if env::var_os("FS_SNAPSHOTS_HARNESS_DEBUG").is_some() {
        let _ = writeln!(io::stdout(), "AgentFS provider matrix stdout:\n{}", stdout);
    }
    assert!(
        stdout.contains("AgentFS provider matrix completed successfully"),
        "expected agentfs matrix success message in stdout, got:\n{}",
        stdout
    );
    assert!(
        stdout.contains("AgentFS matrix branch workspace:"),
        "expected branch workspace output in stdout, got:\n{}",
        stdout
    );

    let readonly_line = stdout
        .lines()
        .find(|line| line.starts_with("AgentFS matrix readonly mount:"))
        .expect("readonly mount output missing from AgentFS matrix run");
    let readonly_path = extract_path_from_line(readonly_line);

    let cleanup_line = stdout
        .lines()
        .find(|line| {
            line.starts_with("AgentFS matrix readonly export cleaned:")
                || line.starts_with("AgentFS matrix readonly export removed:")
        })
        .expect("cleanup confirmation for readonly export missing from AgentFS matrix run");

    let _ = writeln!(
        io::stdout(),
        "AgentFS readonly export cleanup confirmation: {}",
        cleanup_line
    );

    if readonly_path.exists() {
        assert!(
            is_directory_empty(&readonly_path),
            "readonly export directory should be empty after cleanup: {}",
            readonly_path.display()
        );
    }
}

#[cfg(not(all(feature = "agentfs", target_os = "macos")))]
#[test]
fn provider_core_behavior_agentfs() {
    let _ = writeln!(
        io::stdout(),
        "Skipping AgentFS provider core behaviour test: agentfs feature disabled or unsupported platform"
    );
}

#[cfg(all(feature = "agentfs", target_os = "macos"))]
fn extract_path_from_line(line: &str) -> PathBuf {
    let path_str = line
        .split_once(':')
        .map(|x| x.1)
        .expect("expected path after colon in output line")
        .trim();
    PathBuf::from(path_str)
}

#[cfg(all(feature = "agentfs", target_os = "macos"))]
fn is_directory_empty(path: &Path) -> bool {
    match std::fs::read_dir(path) {
        Ok(mut entries) => entries.next().is_none(),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => true,
        Err(err) => panic!(
            "failed to inspect readonly export directory {}: {}",
            path.display(),
            err
        ),
    }
}
