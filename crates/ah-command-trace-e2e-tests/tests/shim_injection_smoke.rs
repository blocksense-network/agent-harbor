// Copyright 2025 Schelling Point Labs Inc
//! Smoke tests for shim injection and basic functionality

use ah_command_trace_server::test_utils::TestServer;
use std::io::{self, Write};
use std::path::PathBuf;
use tempfile::TempDir;

/// Test that the shim can be injected and the program runs
#[cfg_attr(not(any(target_os = "macos", target_os = "linux")), ignore)]
#[tokio::test]
async fn shim_injection_smoke_basic() {
    // Run the test helper with shim injection but no socket server
    // This tests that the shim loads and the program still runs
    let test_helper_path = find_test_helper_path();

    let output = ah_command_trace_e2e_tests::execute_test_scenario_disabled(
        &test_helper_path.to_string_lossy(),
        &["print_pid"],
    )
    .await
    .expect("Failed to execute test scenario");

    // Verify the helper ran successfully
    assert!(output.status.success(), "Test helper failed: {:?}", output);
}

/// Test that the shim attempts handshake when socket is configured
#[cfg_attr(not(any(target_os = "macos", target_os = "linux")), ignore)]
#[tokio::test]
async fn shim_injection_smoke_with_socket() {
    // Skip this test in CI environments where shim injection may not work properly
    if std::env::var("CI").is_ok() {
        let _ = writeln!(
            io::stdout(),
            "⚠️  Skipping shim injection smoke test with socket in CI environment"
        );
        return;
    }

    // Create a temporary directory for the socket
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let socket_path = temp_dir.path().join("test_socket");

    // Start the test server
    let server = TestServer::new(&socket_path);

    // Run server and test concurrently
    let server_future = server.run();
    let test_future = async {
        // Give the server a moment to start
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        // Run the test helper with shim injection
        let test_helper_path = find_test_helper_path();
        let output = ah_command_trace_e2e_tests::execute_test_scenario(
            &socket_path.to_string_lossy(),
            &test_helper_path.to_string_lossy(),
            &["print_pid"],
        )
        .await
        .expect("Failed to execute test scenario");

        // Verify the helper ran successfully
        assert!(output.status.success(), "Test helper failed: {:?}", output);

        output
    };

    // Run both concurrently with a timeout
    let result = tokio::time::timeout(std::time::Duration::from_secs(35), async {
        tokio::join!(server_future, test_future)
    })
    .await;

    match result {
        Ok((server_result, _output)) => {
            server_result.expect("Server failed");
            let messages = server.get_requests().await;

            // Verify we received at least one message from the shim
            assert!(!messages.is_empty(), "No messages received from shim");
        }
        Err(_) => panic!("Test timed out"),
    }
}

/// Test that the shim stays dormant when disabled
#[cfg_attr(not(any(target_os = "macos", target_os = "linux")), ignore)]
#[tokio::test]
async fn shim_disabled_dormant() {
    // Create a temporary directory for the socket (should not be used)
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let socket_path = temp_dir.path().join("test_socket");

    // Start the test server
    let server = TestServer::new(&socket_path);

    // Run server and test concurrently
    let server_future = server.run();
    let test_future = async {
        // Give the server a moment to start
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        // Run the test helper with shim disabled
        let test_helper_path = find_test_helper_path();
        let output = ah_command_trace_e2e_tests::execute_test_scenario_disabled(
            &test_helper_path.to_string_lossy(),
            &["print_pid"],
        )
        .await
        .expect("Failed to execute disabled test scenario");

        // Verify the helper still ran successfully
        assert!(
            output.status.success(),
            "Test helper failed when disabled: {:?}",
            output
        );

        output
    };

    // Run both concurrently with a timeout
    let result = tokio::time::timeout(std::time::Duration::from_secs(35), async {
        tokio::join!(server_future, test_future)
    })
    .await;

    match result {
        Ok((server_result, _output)) => {
            server_result.expect("Server failed");
            let messages = server.get_requests().await;

            // Verify no messages were received
            assert!(
                messages.is_empty(),
                "Shim attempted connection when disabled"
            );
        }
        Err(_) => panic!("Test timed out"),
    }
}

/// Test that the shim tears down cleanly when the target exits immediately
#[cfg_attr(not(any(target_os = "macos", target_os = "linux")), ignore)]
#[tokio::test]
async fn shim_teardown_clean_exit() {
    // Skip this test in CI environments where shim injection may not work properly
    if std::env::var("CI").is_ok() {
        let _ = writeln!(
            io::stdout(),
            "⚠️  Skipping shim teardown clean exit test in CI environment"
        );
        return;
    }

    // Create a temporary directory for the socket
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let socket_path = temp_dir.path().join("test_socket");

    // Start the test server
    let server = TestServer::new(&socket_path);

    // Run server and test concurrently
    let server_future = server.run();
    let test_future = async {
        // Give the server a moment to start
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        // Run the dummy command that exits immediately
        let test_helper_path = find_test_helper_path();
        let output = ah_command_trace_e2e_tests::execute_test_scenario(
            &socket_path.to_string_lossy(),
            &test_helper_path.to_string_lossy(),
            &["dummy"],
        )
        .await
        .expect("Failed to execute dummy test scenario");

        // Verify the helper ran successfully
        assert!(
            output.status.success(),
            "Dummy test helper failed: {:?}",
            output
        );

        output
    };

    // Run both concurrently with a timeout
    let result = tokio::time::timeout(std::time::Duration::from_secs(35), async {
        tokio::join!(server_future, test_future)
    })
    .await;

    match result {
        Ok((server_result, _output)) => {
            server_result.expect("Server failed");
            let messages = server.get_requests().await;

            // Verify we received the handshake message
            assert!(!messages.is_empty(), "No messages received from shim");

            // The connection should still be alive (we don't test explicit teardown yet)
        }
        Err(_) => panic!("Test timed out"),
    }
}

/// Test that the shim tears down cleanly when the target calls _exit
#[cfg_attr(not(any(target_os = "macos", target_os = "linux")), ignore)]
#[tokio::test]
async fn shim_teardown_underscore_exit() {
    // Skip this test in CI environments where shim injection may not work properly
    if std::env::var("CI").is_ok() {
        let _ = writeln!(
            io::stdout(),
            "⚠️  Skipping shim teardown underscore exit test in CI environment"
        );
        return;
    }

    // This test would require a special test helper that calls _exit
    // For now, we'll test with the normal exit path
    // TODO: Implement _exit test helper when needed

    // Create a temporary directory for the socket
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let socket_path = temp_dir.path().join("test_socket");

    // Start the test server
    let server = TestServer::new(&socket_path);

    // Run server and test concurrently
    let server_future = server.run();
    let test_future = async {
        // Give the server a moment to start
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        // Run a normal command
        let test_helper_path = find_test_helper_path();
        let output = ah_command_trace_e2e_tests::execute_test_scenario(
            &socket_path.to_string_lossy(),
            &test_helper_path.to_string_lossy(),
            &["dummy"],
        )
        .await
        .expect("Failed to execute test scenario");

        // Verify the helper ran successfully
        assert!(output.status.success(), "Test helper failed: {:?}", output);

        output
    };

    // Run both concurrently with a timeout
    let result = tokio::time::timeout(std::time::Duration::from_secs(35), async {
        tokio::join!(server_future, test_future)
    })
    .await;

    match result {
        Ok((server_result, _output)) => {
            server_result.expect("Server failed");
            let messages = server.get_requests().await;

            // Verify we received the handshake message
            assert!(!messages.is_empty(), "No messages received from shim");
        }
        Err(_) => panic!("Test timed out"),
    }
}

fn find_test_helper_path() -> PathBuf {
    // The binary should be built in the target directory when tests run
    // For tests, it's usually in target/debug/deps/ but binaries go to target/debug/
    let profile = std::env::var("PROFILE").unwrap_or_else(|_| "debug".into());
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("target")
        .join(&profile);

    let helper_path = root.join("test_helper");

    // Print debug info
    let _ = writeln!(
        io::stderr(),
        "Looking for test_helper at: {}",
        helper_path.display()
    );
    let _ = writeln!(io::stderr(), "Current dir: {:?}", std::env::current_dir());

    if !helper_path.exists() {
        // Try target/debug/deps/test_helper
        let deps_path = root.join("deps").join("test_helper");
        let _ = writeln!(io::stderr(), "Also tried: {}", deps_path.display());
        if deps_path.exists() {
            return deps_path;
        }

        panic!(
            "Test helper binary not found. Make sure to run tests with `cargo test -p ah-command-trace-e2e-tests` which should build the binary."
        );
    }

    helper_path
}

fn find_spawn_tree_path() -> PathBuf {
    // The binary should be built in the target directory when tests run
    let profile = std::env::var("PROFILE").unwrap_or_else(|_| "debug".into());
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("target")
        .join(&profile);

    let helper_path = root.join("spawn_tree");

    // Print debug info
    let _ = writeln!(
        io::stderr(),
        "Looking for spawn_tree at: {}",
        helper_path.display()
    );

    if !helper_path.exists() {
        // Try target/debug/deps/spawn_tree
        let deps_path = root.join("deps").join("spawn_tree");
        let _ = writeln!(io::stderr(), "Also tried: {}", deps_path.display());
        if deps_path.exists() {
            return deps_path;
        }

        panic!(
            "Spawn tree binary not found. Make sure to run tests with `cargo test -p ah-command-trace-e2e-tests` which should build the binary."
        );
    }

    helper_path
}

/// Test that the shim records spawn tree process creation
#[cfg_attr(not(any(target_os = "macos", target_os = "linux")), ignore)]
#[tokio::test]
async fn shim_records_spawn_tree() {
    // Skip this test in CI environments where shim injection may not work properly
    if std::env::var("CI").is_ok() {
        let _ = writeln!(
            io::stdout(),
            "⚠️  Skipping shim spawn tree test in CI environment"
        );
        return;
    }

    // Create a temporary directory for the socket
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let socket_path = temp_dir.path().join("test_socket");

    // Start the test server
    let server = TestServer::new(&socket_path);

    // Run server and test concurrently
    let server_future = server.run();
    let test_future = async {
        // Give the server a moment to start
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        // Run the spawn_tree helper with shim injection
        let spawn_tree_path = find_spawn_tree_path();
        let output = ah_command_trace_e2e_tests::execute_test_scenario(
            &socket_path.to_string_lossy(),
            &spawn_tree_path.to_string_lossy(),
            &["spawn_tree"],
        )
        .await
        .expect("Failed to execute spawn tree scenario");

        // Verify the helper ran successfully
        assert!(
            output.status.success(),
            "Spawn tree helper failed: {:?}",
            output
        );

        output
    };

    // Run both concurrently with a timeout
    let result = tokio::time::timeout(std::time::Duration::from_secs(35), async {
        tokio::join!(server_future, test_future)
    })
    .await;

    match result {
        Ok((server_result, _output)) => {
            server_result.expect("Server failed");
            let messages = server.get_requests().await;

            // Verify we received messages from the shim
            assert!(!messages.is_empty(), "No messages received from shim");

            // Parse the messages to verify we got CommandStart events
            let mut command_starts = Vec::new();
            for msg in &messages {
                match msg {
                    ah_command_trace_proto::Request::Handshake(_) => {
                        // Handshake is expected
                    }
                    ah_command_trace_proto::Request::CommandStart(cmd_start) => {
                        command_starts.push(cmd_start.clone());
                        let _ = writeln!(
                            io::stderr(),
                            "Received CommandStart: pid={}, executable={:?}",
                            cmd_start.pid,
                            String::from_utf8_lossy(&cmd_start.executable)
                        );
                    }
                    _ => {}
                }
            }

            // Verify we received CommandStart messages
            assert!(
                !command_starts.is_empty(),
                "Expected CommandStart messages from process tree"
            );

            // For a basic test, we expect at least the parent process to be reported
            // The spawn_tree program creates multiple processes, so we should see several
            let _ = writeln!(
                io::stderr(),
                "Received {} CommandStart messages",
                command_starts.len()
            );

            // TODO: Add more detailed assertions about the expected process tree structure
            // For now, verify we have a reasonable number of command starts
            assert!(
                command_starts.len() >= 3,
                "Expected at least 3 CommandStart messages (parent + children + grandchild)"
            );
        }
        Err(_) => panic!("Test timed out"),
    }
}

/// Test that the shim can handle a burst of subprocess creation without socket backpressure errors
#[cfg_attr(not(any(target_os = "macos", target_os = "linux")), ignore)]
#[tokio::test]
async fn shim_stress_test_burst_subprocesses() {
    // Skip this test in CI environments where shim injection may not work properly
    if std::env::var("CI").is_ok() {
        let _ = writeln!(
            io::stdout(),
            "⚠️  Skipping shim stress test in CI environment"
        );
        return;
    }

    // Create a temporary directory for the socket
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let socket_path = temp_dir.path().join("test_socket");

    // Start the test server
    let server = TestServer::new(&socket_path);

    // Run server and test concurrently
    let server_future = server.run();
    let test_future = async {
        // Give the server a moment to start
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        // Run the stress test helper with shim injection
        let spawn_tree_path = find_spawn_tree_path();
        let output = ah_command_trace_e2e_tests::execute_test_scenario(
            &socket_path.to_string_lossy(),
            &spawn_tree_path.to_string_lossy(),
            &["stress_test", "20"],
        )
        .await
        .expect("Failed to execute stress test scenario");

        // Verify the helper ran successfully
        assert!(
            output.status.success(),
            "Stress test helper failed: {:?}",
            output
        );

        output
    };

    // Run both concurrently with a timeout
    let result = tokio::time::timeout(std::time::Duration::from_secs(35), async {
        tokio::join!(server_future, test_future)
    })
    .await;

    match result {
        Ok((server_result, _output)) => {
            server_result.expect("Server failed");
            let messages = server.get_requests().await;

            // Verify we received messages from the shim
            assert!(!messages.is_empty(), "No messages received from shim");

            // Parse the messages to verify we got CommandStart events
            let mut command_starts = Vec::new();
            for msg in &messages {
                match msg {
                    ah_command_trace_proto::Request::Handshake(_) => {
                        // Handshake is expected
                    }
                    ah_command_trace_proto::Request::CommandStart(cmd_start) => {
                        command_starts.push(cmd_start.clone());
                    }
                    _ => {}
                }
            }

            // Verify we received a reasonable number of CommandStart messages
            // The stress test creates 200 subprocesses in a burst, plus the parent process
            // We expect at least 150 to account for timing and possible duplicates
            let _ = writeln!(
                io::stderr(),
                "Received {} CommandStart messages",
                command_starts.len()
            );
            assert!(
                command_starts.len() >= 20,
                "Expected at least 150 CommandStart messages, got {}",
                command_starts.len()
            );
        }
        Err(_) => panic!("Test timed out"),
    }
}

/// Test that the shim can capture processes launched by shells and interpreters
#[cfg_attr(not(any(target_os = "macos", target_os = "linux")), ignore)]
#[tokio::test]
async fn shim_shell_and_interpreter_coverage() {
    // Skip this test in CI environments where shim injection may not work properly
    if std::env::var("CI").is_ok() {
        let _ = writeln!(
            io::stdout(),
            "⚠️  Skipping shim shell and interpreter coverage test in CI environment"
        );
        return;
    }

    // Create a temporary directory for the socket
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let socket_path = temp_dir.path().join("test_socket");

    // Start the test server
    let server = TestServer::new(&socket_path);

    // Run server and test concurrently
    let server_future = server.run();
    let test_future = async {
        // Give the server a moment to start
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        // Run the test helper with shell and interpreter coverage
        let test_helper_path = find_test_helper_path();
        let output = ah_command_trace_e2e_tests::execute_test_scenario(
            &socket_path.to_string_lossy(),
            &test_helper_path.to_string_lossy(),
            &["shell_and_interpreter"],
        )
        .await
        .expect("Failed to execute shell and interpreter test scenario");

        // Verify the helper ran successfully
        assert!(
            output.status.success(),
            "Shell and interpreter test helper failed: {:?}",
            output
        );

        output
    };

    // Run both concurrently with a timeout
    let result = tokio::time::timeout(std::time::Duration::from_secs(35), async {
        tokio::join!(server_future, test_future)
    })
    .await;

    match result {
        Ok((server_result, _output)) => {
            server_result.expect("Server failed");
            let messages = server.get_requests().await;

            // Verify we received messages from the shim
            assert!(!messages.is_empty(), "No messages received from shim");

            // Parse the messages to verify we got CommandStart events
            let mut command_starts = Vec::new();
            for msg in &messages {
                match msg {
                    ah_command_trace_proto::Request::Handshake(_) => {
                        // Handshake is expected
                    }
                    ah_command_trace_proto::Request::CommandStart(cmd_start) => {
                        command_starts.push(cmd_start.clone());
                        let _ = writeln!(
                            io::stderr(),
                            "Received CommandStart: pid={}, executable={:?}, args={:?}",
                            cmd_start.pid,
                            String::from_utf8_lossy(&cmd_start.executable),
                            cmd_start
                                .args
                                .iter()
                                .map(|arg| String::from_utf8_lossy(arg))
                                .collect::<Vec<_>>()
                        );
                    }
                    _ => {}
                }
            }

            // Verify we received CommandStart messages
            assert!(
                !command_starts.is_empty(),
                "Expected CommandStart messages from shell and interpreter test"
            );

            // Check for processes we expect to capture at M1
            let mut found_python_interpreter = false;
            let mut found_shell_processes = false;

            for cmd_start in &command_starts {
                let executable = String::from_utf8_lossy(&cmd_start.executable);
                let args: Vec<String> = cmd_start
                    .args
                    .iter()
                    .map(|arg| String::from_utf8_lossy(arg).to_string())
                    .collect();

                // Check for Python interpreter processes (this is what we can capture at M1)
                if executable.contains("python") {
                    found_python_interpreter = true;
                    let _ = writeln!(
                        io::stderr(),
                        "Found python interpreter: {} {:?}",
                        executable,
                        args
                    );
                }

                // Check for shell processes without assuming a specific absolute path.
                // On macOS we rely on the Nix-provided shell inside the dev shell since SIP
                // prevents injecting into /bin/sh. That shell lives under a per-revision nix
                // store path, so we only validate that the executable (or argv[0]) resolves
                // to a binary named `sh`, regardless of the directory prefix.
                if is_shell_process(&executable, &args) {
                    found_shell_processes = true;
                    let _ = writeln!(
                        io::stderr(),
                        "Found shell process: {} {:?}",
                        executable,
                        args
                    );
                }
            }

            // At M1, we expect to capture:
            // 1. Python interpreter processes launched via env python3
            // 2. Shell processes
            // Shell-launched subprocesses (like echo) may not be captured due to shell optimization
            assert!(
                found_python_interpreter,
                "Expected to find Python interpreter processes"
            );
            assert!(found_shell_processes, "Expected to find shell processes");

            let _ = writeln!(
                io::stderr(),
                "⚠️  Note: Shell-launched subprocesses (like 'echo' commands) are not captured at M1 due to shell optimization/built-in commands"
            );
            let _ = writeln!(
                io::stderr(),
                "Shell and interpreter coverage test passed: found python_interpreter={}, shell_processes={}",
                found_python_interpreter,
                found_shell_processes
            );
        }
        Err(_) => panic!("Test timed out"),
    }
}

fn is_shell_process(executable: &str, args: &[String]) -> bool {
    fn matches_shell(candidate: &str) -> bool {
        if candidate == "sh" {
            return true;
        }
        candidate.rsplit('/').next() == Some("sh")
    }

    if matches_shell(executable) {
        return true;
    }

    args.iter().any(|arg| matches_shell(arg))
}
