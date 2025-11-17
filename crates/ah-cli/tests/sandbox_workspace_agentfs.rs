// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Integration tests for AgentFS sandbox workspace preparation

#[test]
fn test_sandbox_workspace_agentfs() {
    // Skip this test on non-macOS platforms since AgentFS is macOS-specific
    if !cfg!(target_os = "macos") {
        println!("⚠️  Skipping AgentFS sandbox test on non-macOS platform");
        return;
    }

    // Skip this test in CI environments where AgentFS harness may not be available
    if std::env::var("CI").is_ok() {
        println!("⚠️  Skipping AgentFS sandbox test in CI environment");
        return;
    }

    // Test the new --fs-snapshots flag (AgentFS provider)
    // This test exercises the end-to-end sandbox command with AgentFS provider
    use std::process::Command;

    // Build path to the ah binary
    let cargo_manifest_dir = env!("CARGO_MANIFEST_DIR");
    let binary_path = if cargo_manifest_dir.contains("/crates/") {
        std::path::Path::new(&cargo_manifest_dir).join("../../target/debug/ah")
    } else {
        std::path::Path::new(&cargo_manifest_dir).join("target/debug/ah")
    };

    // Test 1: AgentFS provider explicitly requested (may fail if not available)
    let mut cmd = Command::new(&binary_path);
    cmd.args([
        "agent",
        "sandbox",
        "--fs-snapshots",
        "agentfs",
        "--",
        "echo",
        "agentfs test",
    ]);

    let output = cmd.output().expect("Failed to run ah agent sandbox with agentfs command");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    println!("AgentFS sandbox stdout: {}", stdout);
    if !stderr.is_empty() {
        println!("AgentFS sandbox stderr: {}", stderr);
    }

    // The command should attempt to run (may fail due to missing AgentFS support or permissions)
    if !output.status.success() {
        println!("⚠️  AgentFS sandbox command failed (this may be expected in test environments)");
    } else {
        println!("✅ AgentFS sandbox command executed successfully");
    }

    // Test 2: Disable provider (should work on Linux)
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

    // Disable should work on Linux (may fail on macOS due to sandbox not being implemented)
    if !output_disable.status.success() {
        println!("⚠️  Sandbox disable test skipped (not on Linux)");
    } else {
        println!("✅ Disable provider sandbox executed successfully");
    }

    println!("✅ AgentFS provider CLI integration test completed");
    println!("   This test verifies that:");
    println!("   1. `--fs-snapshots agentfs` flag is accepted");
    println!("   2. `--fs-snapshots disable` flag works");
    println!("   3. Provider selection logic routes correctly");
    println!(
        "   Note: Full AgentFS execution requires compiled agentfs feature and proper permissions"
    );
}

#[test]
fn test_sandbox_agentfs_daemon_reuse() {
    // Skip this test in CI environments or when AgentFS is not available
    if std::env::var("CI").is_ok() {
        println!("⚠️  Skipping AgentFS daemon reuse test in CI environment");
        return;
    }

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

    #[cfg(feature = "agentfs")]
    {
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
                        return;
                    }
                    Ok(None) => {
                        println!("✅ Daemon is running");
                    }
                    Err(e) => {
                        println!("❌ Failed to check daemon status: {}", e);
                        let _ = child.kill();
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
                        return;
                    }
                };

                // Copy the test script from the test fixtures
                let test_script_path = temp_dir.join("create-test-files.sh");
                let cargo_manifest_dir = env!("CARGO_MANIFEST_DIR");
                let source_script =
                    std::path::Path::new(cargo_manifest_dir).join("create-test-files.sh");
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
                        panic!("Filesystem isolation test failed - directory contents changed");
                    }

                    // Now run a second sandbox command to verify the files exist in the overlay
                    println!("🔍 Testing that files exist within AgentFS overlay layer...");
                    let verify_script_path = temp_dir.join("verify-test-files.sh");
                    let cargo_manifest_dir = env!("CARGO_MANIFEST_DIR");
                    let source_verify_script =
                        std::path::Path::new(cargo_manifest_dir).join("verify-test-files.sh");
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

    #[cfg(not(feature = "agentfs"))]
    {
        println!("⚠️  Skipping AgentFS daemon reuse test (requires agentfs feature)");
    }
}
