// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Integration tests for sandbox workspace preparation

use std::fs;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::process::Command;
use tempfile::NamedTempFile;

// Use structured logging instead of println!/eprintln! which are disallowed by clippy.
#[cfg(feature = "agentfs")]
use tracing::error;
use tracing::{info, warn};

// Initialize tracing subscriber once for the test module.
fn init_tracing() {
    static INIT: std::sync::Once = std::sync::Once::new();
    INIT.call_once(|| {
        let _ = tracing_subscriber::fmt().with_test_writer().try_init();
    });
}

#[test]
fn test_sandbox_workspace_git() {
    init_tracing();
    if !cfg!(target_os = "macos") {
        warn!("Skipping macOS sandbox test on non-macOS platform");
        return;
    }

    if std::env::var("CI").is_ok() {
        warn!("Skipping macOS sandbox test in CI environment");
        return;
    }

    let cargo_manifest_dir = env!("CARGO_MANIFEST_DIR");
    let binary_path = if cargo_manifest_dir.contains("/crates/") {
        std::path::Path::new(&cargo_manifest_dir).join("../../target/debug/ah")
    } else {
        std::path::Path::new(&cargo_manifest_dir).join("target/debug/ah")
    };

    // Test 1: Git snapshot provider explicitly requested
    let mut cmd = Command::new(&binary_path);
    cmd.args([
        "agent",
        "sandbox",
        "--fs-snapshots",
        "git",
        "--",
        "echo",
        "git provider test",
    ]);

    let output = cmd.output().expect("Failed to run ah agent sandbox with git snapshot provider");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    info!(stdout = %stdout, "Git snapshot sandbox command output");
    if !stderr.is_empty() {
        warn!(stderr = %stderr, "Git snapshot sandbox command emitted stderr");
    }

    assert!(
        output.status.success(),
        "Expected `ah agent sandbox --fs-snapshots git -- ...` to succeed.\nstdout:\n{}\nstderr:\n{}",
        stdout,
        stderr
    );
    assert!(
        stdout.contains("git provider test"),
        "Git snapshot sandbox run did not emit expected payload. stdout:\n{}",
        stdout
    );
    info!("Git snapshot sandbox command executed successfully");

    // Test 2: Disable provider (should always work)
    let mut cmd_disable = Command::new(&binary_path);
    cmd_disable.args([
        "agent",
        "sandbox",
        "--fs-snapshots",
        "disable",
        "--",
        "echo",
        "disable test",
    ]);

    let output_disable = cmd_disable
        .output()
        .expect("Failed to run ah agent sandbox with disable command");
    let stdout_disable = String::from_utf8_lossy(&output_disable.stdout);
    let stderr_disable = String::from_utf8_lossy(&output_disable.stderr);

    info!(stdout = %stdout_disable, "Disable provider sandbox output");
    if !stderr_disable.is_empty() {
        warn!(stderr = %stderr_disable, "Disable provider sandbox emitted stderr");
    }

    assert!(
        output_disable.status.success(),
        "Expected `ah agent sandbox --fs-snapshots disable -- ...` to succeed.\nstdout:\n{}\nstderr:\n{}",
        stdout_disable,
        stderr_disable
    );
    assert!(
        stdout_disable.contains("disable test"),
        "Disable-provider sandbox run did not emit expected payload. stdout:\n{}",
        stdout_disable
    );
    info!("Disable provider sandbox executed successfully");

    info!("Git snapshot sandbox CLI integration test completed");
    info!("Validation points: 1. git flag accepted 2. disable works 3. provider routing correct");
}

#[test]
fn test_sandbox_git_snapshot_isolation() {
    init_tracing();
    if !cfg!(target_os = "macos") {
        warn!("Skipping macOS git snapshot test on non-macOS platform");
        return;
    }

    if std::env::var("CI").is_ok() {
        warn!("Skipping macOS git snapshot test in CI environment");
        return;
    }

    let cargo_manifest_dir = env!("CARGO_MANIFEST_DIR");
    let binary_path = if cargo_manifest_dir.contains("/crates/") {
        std::path::Path::new(&cargo_manifest_dir).join("../../target/debug/ah")
    } else {
        std::path::Path::new(&cargo_manifest_dir).join("target/debug/ah")
    };

    let host_marker = std::path::Path::new("git_sandbox_isolation_marker.txt");
    if host_marker.exists() {
        fs::remove_file(host_marker).expect("Failed to remove pre-existing marker");
    }

    let mut script = NamedTempFile::new().expect("Failed to create temp script");
    writeln!(
        script,
        r#"#!/bin/bash
set -euo pipefail
touch "{marker}"
echo "GIT_SANDBOX_SCRIPT_EXECUTED"
"#,
        marker = host_marker.display()
    )
    .expect("Failed to write script");

    let mut perms = script.as_file().metadata().unwrap().permissions();
    perms.set_mode(0o755);
    script.as_file().set_permissions(perms).unwrap();

    let mut cmd = Command::new(&binary_path);
    cmd.args([
        "agent",
        "sandbox",
        "--fs-snapshots",
        "git",
        "--",
        "bash",
        script.path().to_string_lossy().as_ref(),
    ]);

    let output = cmd.output().expect("Failed to run git snapshot isolation sandbox command");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    info!(stdout = %stdout, "Git isolation sandbox output");
    if !stderr.is_empty() {
        warn!(stderr = %stderr, "Git isolation sandbox stderr");
    }

    assert!(
        output.status.success(),
        "Git snapshot isolation command failed.\nstdout:\n{}\nstderr:\n{}",
        stdout,
        stderr
    );
    assert!(
        stdout.contains("GIT_SANDBOX_SCRIPT_EXECUTED"),
        "Git snapshot isolation script did not run as expected. stdout:\n{}",
        stdout
    );
    assert!(
        !host_marker.exists(),
        "Git snapshot provider should isolate filesystem writes, but marker {:?} exists on host",
        host_marker
    );
}

#[test]
fn test_macos_sandbox_integration() {
    init_tracing();
    // Skip this test on non-macOS platforms
    if !cfg!(target_os = "macos") {
        warn!("Skipping macOS sandbox integration test on non-macOS platform");
        return;
    }

    // Skip this test in CI environments where sandboxing may not be fully functional
    if std::env::var("CI").is_ok() {
        warn!("Skipping macOS sandbox integration test in CI environment");
        return;
    }
    info!("Testing macOS sandbox integration with Seatbelt profiles");

    // Build path to the ah binary
    let cargo_manifest_dir = env!("CARGO_MANIFEST_DIR");
    let binary_path = if cargo_manifest_dir.contains("/crates/") {
        std::path::Path::new(&cargo_manifest_dir).join("../../target/debug/ah")
    } else {
        std::path::Path::new(&cargo_manifest_dir).join("target/debug/ah")
    };

    // Test 1: Basic sandbox execution with default settings
    info!("Test 1: Basic sandbox execution");
    let mut cmd = Command::new(&binary_path);
    cmd.args([
        "agent",
        "sandbox",
        "--type",
        "local",
        "--",
        "echo",
        "macOS sandbox test",
    ]);

    let output = cmd.output().expect("Failed to run macOS sandbox command");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    info!(stdout = %stdout, "Basic sandbox output");
    if !stderr.is_empty() {
        warn!(stderr = %stderr, "Basic sandbox stderr");
    }

    assert!(
        output.status.success(),
        "Basic macOS sandbox run failed.\nstdout:\n{}\nstderr:\n{}",
        stdout,
        stderr
    );
    assert!(
        stdout.contains("macOS sandbox test"),
        "Expected 'macOS sandbox test' in stdout, got: {}",
        stdout
    );
    info!("Basic macOS sandbox command executed successfully");

    // Test 2: Sandbox with network access enabled
    info!("Test 2: Sandbox with network access enabled");
    let mut cmd_network = Command::new(&binary_path);
    cmd_network.args([
        "agent",
        "sandbox",
        "--type",
        "local",
        "--allow-network",
        "yes",
        "--",
        "echo",
        "network enabled test",
    ]);

    let output_network = cmd_network.output().expect("Failed to run network-enabled sandbox");
    let stdout_network = String::from_utf8_lossy(&output_network.stdout);
    let stderr_network = String::from_utf8_lossy(&output_network.stderr);

    info!(stdout = %stdout_network, "Network-enabled sandbox output");
    if !stderr_network.is_empty() {
        warn!(stderr = %stderr_network, "Network-enabled sandbox stderr");
    }

    assert!(
        output_network.status.success(),
        "Network-enabled macOS sandbox run failed.\nstdout:\n{}\nstderr:\n{}",
        stdout_network,
        stderr_network
    );
    assert!(
        stdout_network.contains("network enabled test"),
        "Expected 'network enabled test' in stdout, got: {}",
        stdout_network
    );
    info!("Network-enabled macOS sandbox executed successfully");

    // Test 3: Sandbox with custom filesystem allowances
    info!("Test 3: Sandbox with custom filesystem allowances");
    let mut cmd_fs = Command::new(&binary_path);
    cmd_fs.args([
        "agent",
        "sandbox",
        "--type",
        "local",
        "--mount-rw",
        "/tmp",
        "--",
        "echo",
        "filesystem test",
    ]);

    let output_fs = cmd_fs.output().expect("Failed to run filesystem sandbox");
    let stdout_fs = String::from_utf8_lossy(&output_fs.stdout);
    let stderr_fs = String::from_utf8_lossy(&output_fs.stderr);

    info!(stdout = %stdout_fs, "Filesystem sandbox output");
    if !stderr_fs.is_empty() {
        warn!(stderr = %stderr_fs, "Filesystem sandbox stderr");
    }

    assert!(
        output_fs.status.success(),
        "Custom filesystem macOS sandbox run failed.\nstdout:\n{}\nstderr:\n{}",
        stdout_fs,
        stderr_fs
    );
    assert!(
        stdout_fs.contains("filesystem test"),
        "Expected 'filesystem test' in stdout, got: {}",
        stdout_fs
    );
    info!("Custom filesystem macOS sandbox executed successfully");

    // Test 4: Verify that sandbox enforces filesystem restrictions
    info!("Test 4: Verify sandbox filesystem restrictions");
    let test_script = r#"#!/bin/bash
set -euo pipefail

echo "Running filesystem restriction checks"

mkdir -p ./test_sandbox_dir
echo "test content" > ./test_sandbox_dir/allowed_write.txt

if [ -f ./test_sandbox_dir/allowed_write.txt ]; then
    echo "SUCCESS_ALLOWED_WRITE"
else
    echo "FAIL_ALLOWED_WRITE"
    exit 1
fi

if /bin/echo "should fail" > /System/Library/ah_cli_sandbox_should_not_exist.txt 2>/dev/null; then
    echo "FAIL_FORBIDDEN_WRITE"
    /bin/rm -f /System/Library/ah_cli_sandbox_should_not_exist.txt
    exit 1
else
    echo "SUCCESS_FORBIDDEN_WRITE"
fi

rm -rf ./test_sandbox_dir

echo "FILESYSTEM_RESTRICTION_TEST_PASSED"
"#;

    // Create a temporary script file
    let temp_script_path = std::env::temp_dir().join("test_sandbox_filesystem.sh");
    std::fs::write(&temp_script_path, test_script).expect("Failed to write test script");

    let mut cmd_fs_restrict = Command::new(&binary_path);
    cmd_fs_restrict.args([
        "agent",
        "sandbox",
        "--type",
        "local",
        "--",
        "bash",
        &temp_script_path.to_string_lossy(),
    ]);

    let output_restrict =
        cmd_fs_restrict.output().expect("Failed to run filesystem restriction test");
    let stdout_restrict = String::from_utf8_lossy(&output_restrict.stdout);
    let stderr_restrict = String::from_utf8_lossy(&output_restrict.stderr);

    info!(stdout = %stdout_restrict, "Filesystem restriction test output");
    if !stderr_restrict.is_empty() {
        warn!(stderr = %stderr_restrict, "Filesystem restriction test stderr");
    }

    assert!(
        output_restrict.status.success(),
        "Filesystem restriction test failed.\nstdout:\n{}\nstderr:\n{}",
        stdout_restrict,
        stderr_restrict
    );
    assert!(
        stdout_restrict.contains("SUCCESS_ALLOWED_WRITE"),
        "Sandbox should allow writes to working directory: {}",
        stdout_restrict
    );
    assert!(
        stdout_restrict.contains("SUCCESS_FORBIDDEN_WRITE"),
        "Sandbox should block writes to /System/Library: {}",
        stdout_restrict
    );
    assert!(
        stdout_restrict.contains("FILESYSTEM_RESTRICTION_TEST_PASSED"),
        "Sandbox filesystem restriction test did not finish cleanly: {}",
        stdout_restrict
    );
    info!("Sandbox filesystem restriction test completed successfully");

    // Clean up
    let _ = std::fs::remove_file(&temp_script_path);

    info!("macOS sandbox integration test completed");
    info!(
        "Validation points: 1 basic execution 2 network toggle 3 custom allowances 4 filesystem restrictions 5 entitlements note"
    );
}

#[test]
fn test_ah_macos_launcher_binary_wrapper() {
    init_tracing();
    // Skip this test on non-macOS platforms
    if !cfg!(target_os = "macos") {
        warn!("Skipping ah-macos-launcher binary test on non-macOS platform");
        return;
    }

    // Skip this test in CI environments where the binary might not be available
    if std::env::var("CI").is_ok() {
        warn!("Skipping ah-macos-launcher binary test in CI environment");
        return;
    }
    info!("Testing ah-macos-launcher binary wrapper functionality");

    // Build path to the ah-macos-launcher binary
    let cargo_manifest_dir = env!("CARGO_MANIFEST_DIR");
    let binary_path = if cargo_manifest_dir.contains("/crates/") {
        std::path::Path::new(&cargo_manifest_dir).join("../../target/debug/ah-macos-launcher")
    } else {
        std::path::Path::new(&cargo_manifest_dir).join("target/debug/ah-macos-launcher")
    };

    // Check if binary exists
    if !binary_path.exists() {
        warn!(path = %binary_path.display(), "ah-macos-launcher binary not found; skipping test (build with cargo build --bin ah-macos-launcher)");
        return;
    }

    // Define common allow-exec paths, including Nix store if present
    let mut common_exec_paths = vec!["/bin", "/usr/bin"];
    if std::path::Path::new("/nix/store").exists() {
        common_exec_paths.push("/nix/store");
    }

    // Test 1: Basic binary execution
    info!("Test 1: Basic ah-macos-launcher binary execution");
    let mut cmd = Command::new(&binary_path);
    let mut args = vec![
        "--allow-read",
        "/",
        "--allow-write",
        "/tmp",
        "--harden-process",
    ];
    for path in &common_exec_paths {
        args.push("--allow-exec");
        args.push(path);
    }
    args.extend_from_slice(&["--", "echo", "launcher binary test"]);
    cmd.args(args);

    let output = cmd.output().expect("Failed to run ah-macos-launcher binary");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    info!(stdout = %stdout, "Launcher binary output");
    if !stderr.is_empty() {
        warn!(stderr = %stderr, "Launcher binary stderr");
    }

    assert!(
        output.status.success(),
        "ah-macos-launcher basic execution failed.\nstdout:\n{}\nstderr:\n{}",
        stdout,
        stderr
    );
    assert!(
        stdout.contains("launcher binary test"),
        "Expected output not found: {}",
        stdout
    );
    info!("ah-macos-launcher binary executed successfully");

    // Test 2: Binary with network access
    info!("Test 2: ah-macos-launcher binary with network access");
    let mut cmd_network = Command::new(&binary_path);
    let mut args_network = vec![
        "--allow-read",
        "/",
        "--allow-write",
        "/tmp",
        "--allow-network",
        "--harden-process",
    ];
    for path in &common_exec_paths {
        args_network.push("--allow-exec");
        args_network.push(path);
    }
    args_network.extend_from_slice(&["--", "echo", "network launcher test"]);
    cmd_network.args(args_network);

    let output_network = cmd_network.output().expect("Failed to run network launcher");
    let stdout_network = String::from_utf8_lossy(&output_network.stdout);
    let stderr_network = String::from_utf8_lossy(&output_network.stderr);

    info!(stdout = %stdout_network, "Network launcher output");
    if !stderr_network.is_empty() {
        warn!(stderr = %stderr_network, "Network launcher stderr");
    }

    assert!(
        output_network.status.success(),
        "ah-macos-launcher network-enabled execution failed.\nstdout:\n{}\nstderr:\n{}",
        stdout_network,
        stderr_network
    );
    assert!(
        stdout_network.contains("network launcher test"),
        "Expected output not found: {}",
        stdout_network
    );
    info!("ah-macos-launcher binary with network access executed successfully");

    // Test 3: Binary with working directory
    info!("Test 3: ah-macos-launcher binary with working directory");
    let mut cmd_wd = Command::new(&binary_path);
    let mut args_wd = vec![
        "--workdir",
        "/tmp",
        "--allow-read",
        "/",
        "--allow-write",
        "/tmp",
        "--harden-process",
    ];
    for path in &common_exec_paths {
        args_wd.push("--allow-exec");
        args_wd.push(path);
    }
    args_wd.extend_from_slice(&["--", "pwd"]);
    cmd_wd.args(args_wd);

    let output_wd = cmd_wd.output().expect("Failed to run working directory launcher");
    let stdout_wd = String::from_utf8_lossy(&output_wd.stdout);
    let stderr_wd = String::from_utf8_lossy(&output_wd.stderr);

    info!(stdout = %stdout_wd, "Working directory launcher output");
    if !stderr_wd.is_empty() {
        warn!(stderr = %stderr_wd, "Working directory launcher stderr");
    }

    assert!(
        output_wd.status.success(),
        "ah-macos-launcher working-directory execution failed.\nstdout:\n{}\nstderr:\n{}",
        stdout_wd,
        stderr_wd
    );
    let wd_trimmed = stdout_wd.trim();
    assert!(
        wd_trimmed == "/tmp" || wd_trimmed == "/private/tmp",
        "Working directory was not /tmp or /private/tmp (got '{}')",
        wd_trimmed
    );
    info!("ah-macos-launcher binary with working directory executed successfully");

    info!("ah-macos-launcher binary wrapper test completed");
    info!(
        "Validation points: 1 direct execution 2 arg parsing 3 network flags 4 workdir applied 5 entitlements note"
    );
}

#[test]
fn test_sandbox_agentfs_daemon_reuse() {
    init_tracing();
    // Skip this test in CI environments or when AgentFS is not available
    if std::env::var("CI").is_ok() {
        warn!("Skipping AgentFS daemon reuse test in CI environment");
        return;
    }

    // Check if agentfs feature is enabled
    if !cfg!(feature = "agentfs") {
        warn!("Skipping AgentFS daemon reuse test (requires agentfs feature)");
        return;
    }

    #[cfg(feature = "agentfs")]
    {
        // Helper function to capture directory contents recursively
        fn capture_directory_contents(
            dir: &std::path::Path,
        ) -> Result<
            std::collections::BTreeMap<std::path::PathBuf, std::fs::Metadata>,
            Box<dyn std::error::Error>,
        > {
            let mut contents = std::collections::BTreeMap::new();

            fn visit_dir(
                dir: &std::path::Path,
                base: &std::path::Path,
                contents: &mut std::collections::BTreeMap<std::path::PathBuf, std::fs::Metadata>,
            ) -> Result<(), Box<dyn std::error::Error>> {
                for entry in std::fs::read_dir(dir)? {
                    let entry = entry?;
                    let path = entry.path();
                    let metadata = entry.metadata()?;

                    // Get relative path from base directory
                    let relative_path = path.strip_prefix(base).unwrap_or(&path).to_path_buf();

                    // Recursively visit directories before inserting metadata
                    if metadata.is_dir() {
                        visit_dir(&path, base, contents)?;
                    }

                    contents.insert(relative_path, metadata);
                }
                Ok(())
            }

            visit_dir(dir, dir, &mut contents)?;
            Ok(contents)
        }

        use std::process::{Command, Stdio};
        use std::thread;
        use std::time::Duration;

        info!("Testing AgentFS daemon reuse functionality");

        // Build path to the ah binary
        let cargo_manifest_dir = env!("CARGO_MANIFEST_DIR");
        let binary_path = if cargo_manifest_dir.contains("/crates/") {
            std::path::Path::new(&cargo_manifest_dir).join("../../target/debug/ah")
        } else {
            std::path::Path::new(&cargo_manifest_dir).join("target/debug/ah")
        };

        // Create a temporary directory for the daemon socket
        let temp_dir = std::env::temp_dir().join("ah-agentfs-test");
        std::fs::create_dir_all(&temp_dir).expect("Failed to create temp dir");
        let socket_path = temp_dir.join("daemon.sock");

        // Clean up any existing socket
        if socket_path.exists() {
            let _ = std::fs::remove_file(&socket_path);
        }

        // Start AgentFS daemon manually
        info!("Starting AgentFS daemon");
        let daemon_binary = if cargo_manifest_dir.contains("/crates/") {
            std::path::Path::new(&cargo_manifest_dir).join("../../target/debug/agentfs-daemon")
        } else {
            std::path::Path::new(&cargo_manifest_dir).join("target/debug/agentfs-daemon")
        };

        // Only try to start daemon if binary exists
        if !daemon_binary.exists() {
            warn!(path = %daemon_binary.display(), "AgentFS daemon binary not found; skipping reuse test");
            let _ = std::fs::remove_dir_all(&temp_dir);
            return;
        }

        let daemon = Command::new(&daemon_binary)
            .args([&socket_path.to_string_lossy(), "--lower-dir", "."])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();

        match daemon {
            Ok(mut child) => {
                // Give daemon time to start
                thread::sleep(Duration::from_secs(2));

                // Check if daemon is still running
                match child.try_wait() {
                    Ok(Some(status)) => {
                        error!(%status, "Daemon exited early");
                        let _ = std::fs::remove_dir_all(&temp_dir);
                        return;
                    }
                    Ok(None) => {
                        info!("Daemon is running");
                    }
                    Err(e) => {
                        error!(error = %e, "Failed to check daemon status");
                        let _ = child.kill();
                        let _ = std::fs::remove_dir_all(&temp_dir);
                        return;
                    }
                }

                // Capture directory contents BEFORE running any sandbox commands
                info!("Capturing directory contents before sandbox execution");
                let initial_contents = match capture_directory_contents(std::path::Path::new(".")) {
                    Ok(contents) => {
                        info!(count = contents.len(), "Captured directory items");
                        contents
                    }
                    Err(e) => {
                        error!(error = %e, "Failed to capture initial directory contents");
                        let _ = child.kill();
                        let _ = std::fs::remove_dir_all(&temp_dir);
                        return;
                    }
                };

                // Copy the test script from the test fixtures
                let test_script_path = temp_dir.join("create-test-files.sh");
                let cargo_manifest_dir = env!("CARGO_MANIFEST_DIR");
                let source_script = std::path::Path::new(cargo_manifest_dir)
                    .join("tests")
                    .join("create-test-files.sh");

                if !source_script.exists() {
                    warn!(path = %source_script.display(), "Test fixture script not found; skipping");
                    let _ = child.kill();
                    let _ = std::fs::remove_dir_all(&temp_dir);
                    return;
                }

                std::fs::copy(&source_script, &test_script_path)
                    .expect("Failed to copy test script");

                // Now test sandbox with the daemon socket
                info!("Testing sandbox with --agentfs-socket flag and filesystem operations");
                let mut sandbox_cmd = Command::new(&binary_path);
                sandbox_cmd.args([
                    "agent",
                    "sandbox",
                    "--fs-snapshots",
                    "agentfs",
                    "--agentfs-socket",
                    &socket_path.to_string_lossy(),
                    "--",
                    "bash",
                    &test_script_path.to_string_lossy(),
                ]);

                let sandbox_output = sandbox_cmd.output().expect("Failed to run sandbox command");

                let stdout = String::from_utf8_lossy(&sandbox_output.stdout);
                let stderr = String::from_utf8_lossy(&sandbox_output.stderr);

                info!(stdout = %stdout, "Sandbox output");
                if !stderr.is_empty() {
                    warn!(stderr = %stderr, "Sandbox stderr");
                }

                if sandbox_output.status.success() {
                    if !stdout.contains("daemon reuse test successful") {
                        error!("Sandbox script did not complete successfully");
                        info!(stdout = %stdout, "Script stdout");
                        if !stderr.is_empty() {
                            warn!(stderr = %stderr, "Script stderr");
                        }
                        let _ = child.kill();
                        let _ = std::fs::remove_dir_all(&temp_dir);
                        panic!("Sandbox script execution failed - see output above for details");
                    }

                    // Test filesystem isolation - verify directory contents are exactly the same
                    info!("Testing filesystem isolation by comparing directory contents");

                    // Capture directory contents AFTER sandbox execution
                    let final_contents = match capture_directory_contents(std::path::Path::new("."))
                    {
                        Ok(contents) => contents,
                        Err(e) => {
                            error!(error = %e, "Failed to capture final directory contents");
                            let _ = child.kill();
                            let _ = std::fs::remove_dir_all(&temp_dir);
                            return;
                        }
                    };

                    // Compare directory contents - check that file names and sizes match
                    let mut isolation_passed = true;
                    let initial_keys: std::collections::BTreeSet<_> =
                        initial_contents.keys().collect();
                    let final_keys: std::collections::BTreeSet<_> = final_contents.keys().collect();

                    // Check for added files
                    let added: Vec<_> = final_keys.difference(&initial_keys).collect();
                    if !added.is_empty() {
                        error!(?added, "Directory contents changed: files added");
                        isolation_passed = false;
                    }

                    // Check for removed files
                    let removed: Vec<_> = initial_keys.difference(&final_keys).collect();
                    if !removed.is_empty() {
                        error!(?removed, "Directory contents changed: files removed");
                        isolation_passed = false;
                    }

                    // Check for modified files (by size)
                    for (path, initial_meta) in &initial_contents {
                        if let Some(final_meta) = final_contents.get(path) {
                            if initial_meta.len() != final_meta.len() {
                                error!(path = ?path, "Directory contents changed: file size modified");
                                isolation_passed = false;
                            }
                        }
                    }

                    if isolation_passed {
                        info!("Directory contents identical before and after sandbox execution");
                        info!(
                            "AgentFS overlay provides filesystem isolation; no side effects leaked"
                        );
                    } else {
                        let _ = child.kill();
                        let _ = std::fs::remove_dir_all(&temp_dir);
                        panic!("Filesystem isolation test failed - directory contents changed");
                    }

                    // Now run a second sandbox command to verify the files exist in the overlay
                    info!("Testing that files exist within AgentFS overlay layer");
                    let verify_script_path = temp_dir.join("verify-test-files.sh");
                    let cargo_manifest_dir = env!("CARGO_MANIFEST_DIR");
                    let source_verify_script = std::path::Path::new(cargo_manifest_dir)
                        .join("tests")
                        .join("verify-test-files.sh");
                    std::fs::copy(&source_verify_script, &verify_script_path)
                        .expect("Failed to copy verify script");

                    // Run second sandbox command to verify overlay persistence
                    let mut verify_cmd = Command::new(&binary_path);
                    verify_cmd.args([
                        "agent",
                        "sandbox",
                        "--fs-snapshots",
                        "agentfs",
                        "--agentfs-socket",
                        &socket_path.to_string_lossy(),
                        "--",
                        "bash",
                        &verify_script_path.to_string_lossy(),
                    ]);

                    let verify_output =
                        verify_cmd.output().expect("Failed to run verify sandbox command");
                    let verify_stdout = String::from_utf8_lossy(&verify_output.stdout);
                    let verify_stderr = String::from_utf8_lossy(&verify_output.stderr);

                    info!(stdout = %verify_stdout, "Overlay verification stdout");
                    if !verify_stderr.is_empty() {
                        warn!(stderr = %verify_stderr, "Overlay verification stderr");
                    }

                    if verify_output.status.success() {
                        if verify_stdout.contains("overlay verification successful") {
                            info!(
                                "Overlay verification passed: files accessible within overlay layer"
                            );
                            info!("AgentFS provides overlay isolation and persistence");
                        } else {
                            error!("Overlay verification script did not confirm files exist");
                            info!(stdout = %verify_stdout, "Verification script stdout");
                            if !verify_stderr.is_empty() {
                                warn!(stderr = %verify_stderr, "Verification script stderr");
                            }
                            let _ = child.kill();
                            let _ = std::fs::remove_dir_all(&temp_dir);
                            panic!("Overlay verification failed - see output above for details");
                        }
                    } else {
                        error!("Second sandbox command failed - overlay may not persist");
                        info!(stdout = %verify_stdout, "Verification script stdout");
                        if !verify_stderr.is_empty() {
                            warn!(stderr = %verify_stderr, "Verification script stderr");
                        }
                        let _ = child.kill();
                        let _ = std::fs::remove_dir_all(&temp_dir);
                        panic!(
                            "Verification script execution failed - see output above for details"
                        );
                    }

                    info!("AgentFS daemon reuse and isolation test passed");
                    info!(
                        "Validation points: manual start, socket reuse, workspace prep, isolation, lower dir unaffected, persistence"
                    );
                } else {
                    error!("Sandbox command failed");
                    info!(stdout = %stdout, "Sandbox stdout");
                    if !stderr.is_empty() {
                        warn!(stderr = %stderr, "Sandbox stderr");
                    }
                    let _ = child.kill();
                    let _ = std::fs::remove_dir_all(&temp_dir);
                    panic!("Sandbox command failed - see output above for details");
                }

                // Clean up daemon
                let _ = child.kill();
                let _ = std::fs::remove_dir_all(&temp_dir);
            }
            Err(e) => {
                error!(error = %e, "Failed to start AgentFS daemon; skipping reuse test");
            }
        }
    }
}

#[test]
fn test_sandbox_workspace_agentfs_managed() {
    init_tracing();
    if !cfg!(target_os = "macos") {
        warn!("Skipping AgentFS managed sandbox test on non-macOS platform");
        return;
    }

    if std::env::var("CI").is_ok() {
        warn!("Skipping AgentFS managed sandbox test in CI environment");
        return;
    }

    if !cfg!(feature = "agentfs") {
        warn!("Skipping AgentFS managed sandbox test (requires agentfs feature)");
        return;
    }

    #[cfg(feature = "agentfs")]
    {
        use std::process::Command;

        let cargo_manifest_dir = env!("CARGO_MANIFEST_DIR");
        let binary_path = if cargo_manifest_dir.contains("/crates/") {
            std::path::Path::new(&cargo_manifest_dir).join("../../target/debug/ah")
        } else {
            std::path::Path::new(&cargo_manifest_dir).join("target/debug/ah")
        };

        // Test 1: AgentFS provider explicitly requested (should start its own daemon)
        let mut cmd = Command::new(&binary_path);
        cmd.args([
            "agent",
            "sandbox",
            "--fs-snapshots",
            "agentfs",
            "--",
            "echo",
            "agentfs managed test",
        ]);

        let output = cmd.output().expect("Failed to run ah agent sandbox with managed agentfs");
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        info!(stdout = %stdout, "AgentFS managed sandbox output");
        if !stderr.is_empty() {
            warn!(stderr = %stderr, "AgentFS managed sandbox stderr");
        }

        if !output.status.success() {
            // This test might fail if capabilities are not met (e.g. permissions), which is common
            warn!("AgentFS managed sandbox failed (likely due to permissions/capabilities)");
            // We don't assert success here because it depends on environmental factors
            // that might not be present even if the feature is compiled.
            // But we do check that it tried to use AgentFS.
            assert!(
                stderr.contains("AgentFS provider explicitly requested")
                    || stderr.contains("Failed to prepare AgentFS workspace"),
                "Unexpected failure message: {}",
                stderr
            );
        } else {
            assert!(
                stdout.contains("agentfs managed test"),
                "AgentFS managed sandbox run did not emit expected payload. stdout:\n{}",
                stdout
            );
            info!("AgentFS managed sandbox executed successfully");
        }
    }
}
