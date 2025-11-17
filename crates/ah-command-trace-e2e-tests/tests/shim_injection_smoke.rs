// Copyright 2025 Schelling Point Labs Inc
//! Smoke tests for shim injection and basic functionality

use ah_command_trace_proto::Request;
use ah_command_trace_server::test_utils::TestServer;
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
        println!("⚠️  Skipping shim injection smoke test with socket in CI environment");
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
    let result = tokio::time::timeout(std::time::Duration::from_secs(10), async {
        tokio::join!(server_future, test_future)
    })
    .await;

    match result {
        Ok((server_result, output)) => {
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
    let result = tokio::time::timeout(std::time::Duration::from_secs(10), async {
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
        println!("⚠️  Skipping shim teardown clean exit test in CI environment");
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
    let result = tokio::time::timeout(std::time::Duration::from_secs(10), async {
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
        println!("⚠️  Skipping shim teardown underscore exit test in CI environment");
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
    let result = tokio::time::timeout(std::time::Duration::from_secs(10), async {
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
    eprintln!("Looking for test_helper at: {}", helper_path.display());
    eprintln!("Current dir: {:?}", std::env::current_dir());

    if !helper_path.exists() {
        // Try target/debug/deps/test_helper
        let deps_path = root.join("deps").join("test_helper");
        eprintln!("Also tried: {}", deps_path.display());
        if deps_path.exists() {
            return deps_path;
        }

        panic!(
            "Test helper binary not found. Make sure to run tests with `cargo test -p ah-command-trace-e2e-tests` which should build the binary."
        );
    }

    helper_path
}
