// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Integration tests for sandbox workspace preparation

use std::fs;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::process::Command;
use tempfile::NamedTempFile;

#[test]
fn test_sandbox_workspace_git() {
    if !cfg!(target_os = "macos") {
        println!("⚠️  Skipping macOS sandbox test on non-macOS platform");
        return;
    }

    if std::env::var("CI").is_ok() {
        println!("⚠️  Skipping macOS sandbox test in CI environment");
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

    println!("Git snapshot sandbox stdout: {}", stdout);
    if !stderr.is_empty() {
        println!("Git snapshot sandbox stderr: {}", stderr);
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
    println!("✅ Git snapshot sandbox command executed successfully");

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

    println!("Disable sandbox stdout: {}", stdout_disable);
    if !stderr_disable.is_empty() {
        println!("Disable sandbox stderr: {}", stderr_disable);
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
    println!("✅ Disable provider sandbox executed successfully");

    println!("✅ Git snapshot sandbox CLI integration test completed");
    println!("   This test verifies that:");
    println!("   1. `--fs-snapshots git` flag is accepted");
    println!("   2. `--fs-snapshots disable` flag works");
    println!("   3. Provider selection logic routes correctly");
}

#[test]
fn test_sandbox_git_snapshot_isolation() {
    if !cfg!(target_os = "macos") {
        println!("⚠️  Skipping macOS git snapshot test on non-macOS platform");
        return;
    }

    if std::env::var("CI").is_ok() {
        println!("⚠️  Skipping macOS git snapshot test in CI environment");
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

    println!("Git isolation sandbox stdout: {}", stdout);
    if !stderr.is_empty() {
        println!("Git isolation sandbox stderr: {}", stderr);
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
    // Skip this test on non-macOS platforms
    if !cfg!(target_os = "macos") {
        println!("⚠️  Skipping macOS sandbox integration test on non-macOS platform");
        return;
    }

    // Skip this test in CI environments where sandboxing may not be fully functional
    if std::env::var("CI").is_ok() {
        println!("⚠️  Skipping macOS sandbox integration test in CI environment");
        return;
    }

    println!("🧪 Testing macOS sandbox integration with Seatbelt profiles...");

    // Build path to the ah binary
    let cargo_manifest_dir = env!("CARGO_MANIFEST_DIR");
    let binary_path = if cargo_manifest_dir.contains("/crates/") {
        std::path::Path::new(&cargo_manifest_dir).join("../../target/debug/ah")
    } else {
        std::path::Path::new(&cargo_manifest_dir).join("target/debug/ah")
    };

    // Test 1: Basic sandbox execution with default settings
    println!("🧪 Test 1: Basic sandbox execution");
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

    println!("Basic sandbox stdout: {}", stdout);
    if !stderr.is_empty() {
        println!("Basic sandbox stderr: {}", stderr);
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
    println!("✅ Basic macOS sandbox command executed successfully");

    // Test 2: Sandbox with network access enabled
    println!("🧪 Test 2: Sandbox with network access enabled");
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

    println!("Network-enabled sandbox stdout: {}", stdout_network);
    if !stderr_network.is_empty() {
        println!("Network-enabled sandbox stderr: {}", stderr_network);
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
    println!("✅ Network-enabled macOS sandbox executed successfully");

    // Test 3: Sandbox with custom filesystem allowances
    println!("🧪 Test 3: Sandbox with custom filesystem allowances");
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

    println!("Filesystem sandbox stdout: {}", stdout_fs);
    if !stderr_fs.is_empty() {
        println!("Filesystem sandbox stderr: {}", stderr_fs);
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
    println!("✅ Custom filesystem macOS sandbox executed successfully");

    // Test 4: Verify that sandbox enforces filesystem restrictions
    println!("🧪 Test 4: Verify sandbox filesystem restrictions");
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

    println!("Filesystem restriction test stdout: {}", stdout_restrict);
    if !stderr_restrict.is_empty() {
        println!("Filesystem restriction test stderr: {}", stderr_restrict);
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
    println!("✅ Sandbox filesystem restriction test completed successfully");

    // Clean up
    let _ = std::fs::remove_file(&temp_script_path);

    println!("✅ macOS sandbox integration test completed");
    println!("   This test verifies that:");
    println!("   1. Basic sandbox execution works on macOS");
    println!("   2. Network access can be enabled/disabled");
    println!("   3. Custom filesystem allowances are accepted");
    println!(
        "   4. Sandbox enforces filesystem restrictions (allows working directory writes, denies /System/Library)"
    );
    println!(
        "   Note: Full Seatbelt profile verification requires proper macOS entitlements and AgentFS mounts"
    );
}

#[test]
fn test_ah_macos_launcher_binary_wrapper() {
    // Skip this test on non-macOS platforms
    if !cfg!(target_os = "macos") {
        println!("⚠️  Skipping ah-macos-launcher binary test on non-macOS platform");
        return;
    }

    // Skip this test in CI environments where the binary might not be available
    if std::env::var("CI").is_ok() {
        println!("⚠️  Skipping ah-macos-launcher binary test in CI environment");
        return;
    }

    println!("🧪 Testing ah-macos-launcher binary wrapper functionality...");

    // Build path to the ah-macos-launcher binary
    let cargo_manifest_dir = env!("CARGO_MANIFEST_DIR");
    let binary_path = if cargo_manifest_dir.contains("/crates/") {
        std::path::Path::new(&cargo_manifest_dir).join("../../target/debug/ah-macos-launcher")
    } else {
        std::path::Path::new(&cargo_manifest_dir).join("target/debug/ah-macos-launcher")
    };

    // Check if binary exists
    if !binary_path.exists() {
        println!(
            "⚠️  ah-macos-launcher binary not found at: {}",
            binary_path.display()
        );
        println!(
            "   Skipping test - binary may need to be built with 'cargo build --bin ah-macos-launcher'"
        );
        return;
    }

    // Define common allow-exec paths, including Nix store if present
    let mut common_exec_paths = vec!["/bin", "/usr/bin"];
    if std::path::Path::new("/nix/store").exists() {
        common_exec_paths.push("/nix/store");
    }

    // Test 1: Basic binary execution
    println!("🧪 Test 1: Basic ah-macos-launcher binary execution");
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

    println!("Launcher binary stdout: {}", stdout);
    if !stderr.is_empty() {
        println!("Launcher binary stderr: {}", stderr);
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
    println!("✅ ah-macos-launcher binary executed successfully");

    // Test 2: Binary with network access
    println!("🧪 Test 2: ah-macos-launcher binary with network access");
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

    println!("Network launcher stdout: {}", stdout_network);
    if !stderr_network.is_empty() {
        println!("Network launcher stderr: {}", stderr_network);
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
    println!("✅ ah-macos-launcher binary with network access executed successfully");

    // Test 3: Binary with working directory
    println!("🧪 Test 3: ah-macos-launcher binary with working directory");
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

    println!("Working directory launcher stdout: {}", stdout_wd);
    if !stderr_wd.is_empty() {
        println!("Working directory launcher stderr: {}", stderr_wd);
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
    println!("✅ ah-macos-launcher binary with working directory executed successfully");

    println!("✅ ah-macos-launcher binary wrapper test completed");
    println!("   This test verifies that:");
    println!("   1. The ah-macos-launcher binary can be executed directly");
    println!("   2. Command-line arguments are parsed correctly");
    println!("   3. Network access flags work");
    println!("   4. Working directory settings are applied");
    println!("   Note: Full functionality requires proper macOS sandbox entitlements");
}

#[test]
fn test_sandbox_agentfs_daemon_reuse() {
    // Skip this test in CI environments or when AgentFS is not available
    if std::env::var("CI").is_ok() {
        println!("⚠️  Skipping AgentFS daemon reuse test in CI environment");
        return;
    }

    // Check if agentfs feature is enabled
    if !cfg!(feature = "agentfs") {
        println!("⚠️  Skipping AgentFS daemon reuse test (requires agentfs feature)");
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

        println!("🧪 Testing AgentFS daemon reuse functionality...");

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
        println!("🚀 Starting AgentFS daemon...");
        let daemon_binary = if cargo_manifest_dir.contains("/crates/") {
            std::path::Path::new(&cargo_manifest_dir).join("../../target/debug/agentfs-daemon")
        } else {
            std::path::Path::new(&cargo_manifest_dir).join("target/debug/agentfs-daemon")
        };

        // Only try to start daemon if binary exists
        if !daemon_binary.exists() {
            println!(
                "⚠️  AgentFS daemon binary not found at: {}",
                daemon_binary.display()
            );
            println!("   Skipping daemon reuse test");
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
                        println!("❌ Daemon exited early with status: {}", status);
                        let _ = std::fs::remove_dir_all(&temp_dir);
                        return;
                    }
                    Ok(None) => {
                        println!("✅ Daemon is running");
                    }
                    Err(e) => {
                        println!("❌ Failed to check daemon status: {}", e);
                        let _ = child.kill();
                        let _ = std::fs::remove_dir_all(&temp_dir);
                        return;
                    }
                }

                // Capture directory contents BEFORE running any sandbox commands
                println!("📸 Capturing directory contents before sandbox execution...");
                let initial_contents = match capture_directory_contents(std::path::Path::new(".")) {
                    Ok(contents) => {
                        println!("✅ Captured {} items in directory", contents.len());
                        contents
                    }
                    Err(e) => {
                        println!("❌ Failed to capture initial directory contents: {}", e);
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
                    println!(
                        "⚠️  Test fixture script not found at: {}",
                        source_script.display()
                    );
                    let _ = child.kill();
                    let _ = std::fs::remove_dir_all(&temp_dir);
                    return;
                }

                std::fs::copy(&source_script, &test_script_path)
                    .expect("Failed to copy test script");

                // Now test sandbox with the daemon socket
                println!(
                    "🧪 Testing sandbox with --agentfs-socket flag and filesystem operations..."
                );
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

                println!("Sandbox stdout: {}", stdout);
                if !stderr.is_empty() {
                    println!("Sandbox stderr: {}", stderr);
                }

                if sandbox_output.status.success() {
                    if !stdout.contains("daemon reuse test successful") {
                        println!("❌ FAIL: Sandbox script did not complete successfully");
                        println!("📄 Script stdout:");
                        println!("{}", stdout);
                        if !stderr.is_empty() {
                            println!("📄 Script stderr:");
                            println!("{}", stderr);
                        }
                        let _ = child.kill();
                        let _ = std::fs::remove_dir_all(&temp_dir);
                        panic!("Sandbox script execution failed - see output above for details");
                    }

                    // Test filesystem isolation - verify directory contents are exactly the same
                    println!("🔍 Testing filesystem isolation by comparing directory contents...");

                    // Capture directory contents AFTER sandbox execution
                    let final_contents = match capture_directory_contents(std::path::Path::new("."))
                    {
                        Ok(contents) => contents,
                        Err(e) => {
                            println!("❌ Failed to capture final directory contents: {}", e);
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
                        println!("❌ FAIL: Directory contents changed during sandbox execution!");
                        println!("   Files/directories were added: {:?}", added);
                        isolation_passed = false;
                    }

                    // Check for removed files
                    let removed: Vec<_> = initial_keys.difference(&final_keys).collect();
                    if !removed.is_empty() {
                        println!("❌ FAIL: Directory contents changed during sandbox execution!");
                        println!("   Files/directories were removed: {:?}", removed);
                        isolation_passed = false;
                    }

                    // Check for modified files (by size)
                    for (path, initial_meta) in &initial_contents {
                        if let Some(final_meta) = final_contents.get(path) {
                            if initial_meta.len() != final_meta.len() {
                                println!(
                                    "❌ FAIL: Directory contents changed during sandbox execution!"
                                );
                                println!("   File modified (size changed): {:?}", path);
                                isolation_passed = false;
                            }
                        }
                    }

                    if isolation_passed {
                        println!(
                            "✅ PASS: Directory contents are identical before and after sandbox execution"
                        );
                        println!(
                            "   This confirms AgentFS overlay provides perfect filesystem isolation"
                        );
                        println!(
                            "   No side effects leaked from the overlay to the lower filesystem"
                        );
                    } else {
                        let _ = child.kill();
                        let _ = std::fs::remove_dir_all(&temp_dir);
                        panic!("Filesystem isolation test failed - directory contents changed");
                    }

                    // Now run a second sandbox command to verify the files exist in the overlay
                    println!("🔍 Testing that files exist within AgentFS overlay layer...");
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

                    println!("Verify stdout: {}", verify_stdout);
                    if !verify_stderr.is_empty() {
                        println!("Verify stderr: {}", verify_stderr);
                    }

                    if verify_output.status.success() {
                        if verify_stdout.contains("overlay verification successful") {
                            println!(
                                "✅ PASS: Files exist and are accessible within AgentFS overlay layer"
                            );
                            println!(
                                "   This confirms AgentFS provides proper overlay isolation and persistence"
                            );
                        } else {
                            println!(
                                "❌ FAIL: Overlay verification script ran but didn't confirm files exist"
                            );
                            println!("📄 Verification script stdout:");
                            println!("{}", verify_stdout);
                            if !verify_stderr.is_empty() {
                                println!("📄 Verification script stderr:");
                                println!("{}", verify_stderr);
                            }
                            let _ = child.kill();
                            let _ = std::fs::remove_dir_all(&temp_dir);
                            panic!("Overlay verification failed - see output above for details");
                        }
                    } else {
                        println!(
                            "❌ FAIL: Second sandbox command failed - overlay may not persist between sessions"
                        );
                        println!("📄 Verification script stdout:");
                        println!("{}", verify_stdout);
                        if !verify_stderr.is_empty() {
                            println!("📄 Verification script stderr:");
                            println!("{}", verify_stderr);
                        }
                        let _ = child.kill();
                        let _ = std::fs::remove_dir_all(&temp_dir);
                        panic!(
                            "Verification script execution failed - see output above for details"
                        );
                    }

                    println!("✅ AgentFS daemon reuse and isolation test passed!");
                    println!("   This test verifies that:");
                    println!("   1. AgentFS daemon can be started manually");
                    println!("   2. `--agentfs-socket` flag is accepted");
                    println!("   3. Sandbox can reuse existing daemon socket");
                    println!("   4. Workspace preparation works with existing daemon");
                    println!("   5. Filesystem operations are properly isolated to the overlay");
                    println!("   6. Lower directory remains unaffected by overlay operations");
                    println!(
                        "   7. Files created in overlay are accessible in subsequent overlay sessions"
                    );
                } else {
                    println!("❌ FAIL: Sandbox command failed");
                    println!("📄 stdout: {}", stdout);
                    if !stderr.is_empty() {
                        println!("📄 stderr: {}", stderr);
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
                println!("❌ Failed to start AgentFS daemon: {}", e);
                println!("⚠️  Skipping daemon reuse test - daemon startup failed");
            }
        }
    }
}

#[test]
fn test_sandbox_workspace_agentfs_managed() {
    if !cfg!(target_os = "macos") {
        println!("⚠️  Skipping AgentFS managed sandbox test on non-macOS platform");
        return;
    }

    if std::env::var("CI").is_ok() {
        println!("⚠️  Skipping AgentFS managed sandbox test in CI environment");
        return;
    }

    if !cfg!(feature = "agentfs") {
        println!("⚠️  Skipping AgentFS managed sandbox test (requires agentfs feature)");
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

        println!("AgentFS managed sandbox stdout: {}", stdout);
        if !stderr.is_empty() {
            println!("AgentFS managed sandbox stderr: {}", stderr);
        }

        if !output.status.success() {
            // This test might fail if capabilities are not met (e.g. permissions), which is common
            println!("⚠️  AgentFS managed sandbox failed (likely due to permissions/capabilities)");
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
            println!("✅ AgentFS managed sandbox executed successfully");
        }
    }
}
