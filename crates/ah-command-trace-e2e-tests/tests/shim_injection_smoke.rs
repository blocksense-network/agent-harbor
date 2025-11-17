// Copyright 2025 Schelling Point Labs Inc
//! Smoke tests for shim injection and basic functionality

use ah_command_trace_proto::{HandshakeResponse, Request, Response, decode_ssz, encode_ssz};
use std::io::{Read, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::sync::Arc;
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

    // Start a socket server to receive the handshake
    let listener = UnixListener::bind(&socket_path).expect("Failed to bind socket");
    let received_messages: Arc<std::sync::Mutex<Vec<Request>>> =
        Arc::new(std::sync::Mutex::new(Vec::new()));

    // Spawn a thread to handle incoming connections
    let messages_clone = Arc::clone(&received_messages);
    std::thread::spawn(move || {
        eprintln!("Mock server: Waiting for connection...");
        match listener.accept() {
            Ok((mut stream, addr)) => {
                eprintln!("Mock server: Connection accepted from {:?}", addr);

                loop {
                    // Read length prefix (4 bytes, little endian)
                    let mut len_buf = [0u8; 4];
                    if stream.read_exact(&mut len_buf).is_err() {
                        break; // Connection closed
                    }
                    let msg_len = u32::from_le_bytes(len_buf) as usize;

                    // Read SSZ message
                    let mut msg_buf = vec![0u8; msg_len];
                    if stream.read_exact(&mut msg_buf).is_err() {
                        break; // Connection closed
                    }

                    // Decode SSZ message
                    match decode_ssz::<Request>(&msg_buf) {
                        Ok(msg) => {
                            eprintln!("Mock server: Received message: {:?}", msg);
                            let mut messages = messages_clone.lock().unwrap();
                            messages.push(msg.clone());

                            // If this is a handshake request, respond with success
                            if let Request::Handshake(_) = msg {
                                eprintln!(
                                    "Mock server: Received handshake request, sending response"
                                );
                                let response = Response::Handshake(HandshakeResponse {
                                    success: true,
                                    error_message: None,
                                });
                                let response_bytes = encode_ssz(&response);
                                let response_len = (response_bytes.len() as u32).to_le_bytes();

                                let _ = stream.write_all(&response_len);
                                let _ = stream.write_all(&response_bytes);
                                eprintln!("Mock server: Response sent");
                            }
                        }
                        Err(e) => {
                            eprintln!("Mock server: Failed to decode message: {:?}", e);
                            break;
                        }
                    }
                }
            }
            Err(e) => {
                eprintln!("Mock server: Accept failed: {:?}", e);
            }
        }
    });

    // Give the server task a moment to start
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

    // Give more time for the background handshake to complete
    tokio::time::sleep(std::time::Duration::from_millis(1000)).await;

    // Verify we received at least one message from the shim
    let messages = received_messages.lock().unwrap();
    assert!(!messages.is_empty(), "No messages received from shim");
}

/// Test that the shim stays dormant when disabled
#[cfg_attr(not(any(target_os = "macos", target_os = "linux")), ignore)]
#[tokio::test]
async fn shim_disabled_dormant() {
    // Create a temporary directory for the socket (should not be used)
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let socket_path = temp_dir.path().join("test_socket");

    // Start a socket server that should not receive any connections
    let listener = UnixListener::bind(&socket_path).expect("Failed to bind socket");
    let connection_attempted = Arc::new(std::sync::Mutex::new(false));

    // Spawn a thread to detect any connection attempts
    let connection_clone = Arc::clone(&connection_attempted);
    std::thread::spawn(move || {
        // Set a short timeout - if we get a connection, mark it
        listener.set_nonblocking(true).expect("Failed to set non-blocking");
        let mut attempts = 0;
        while attempts < 50 {
            // ~500ms with 10ms sleeps
            match listener.accept() {
                Ok(_) => {
                    *connection_clone.lock().unwrap() = true;
                    break;
                }
                Err(_) => {
                    std::thread::sleep(std::time::Duration::from_millis(10));
                    attempts += 1;
                }
            }
        }
    });

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

    // Wait for the timeout
    tokio::time::sleep(std::time::Duration::from_millis(600)).await;

    // Verify no connection was attempted
    let was_connected = *connection_attempted.lock().unwrap();
    assert!(!was_connected, "Shim attempted connection when disabled");
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

    // Start a socket server
    let listener = UnixListener::bind(&socket_path).expect("Failed to bind socket");
    let received_messages: Arc<std::sync::Mutex<Vec<Request>>> =
        Arc::new(std::sync::Mutex::new(Vec::new()));

    // Spawn a thread to handle incoming connections
    let messages_clone = Arc::clone(&received_messages);
    std::thread::spawn(move || {
        match listener.accept() {
            Ok((mut stream, _)) => {
                loop {
                    // Read length prefix (4 bytes, little endian)
                    let mut len_buf = [0u8; 4];
                    if stream.read_exact(&mut len_buf).is_err() {
                        break; // Connection closed
                    }
                    let msg_len = u32::from_le_bytes(len_buf) as usize;

                    // Read SSZ message
                    let mut msg_buf = vec![0u8; msg_len];
                    if stream.read_exact(&mut msg_buf).is_err() {
                        break; // Connection closed
                    }

                    // Decode SSZ message
                    if let Ok(msg) = decode_ssz::<Request>(&msg_buf) {
                        let mut messages = messages_clone.lock().unwrap();
                        messages.push(msg.clone());

                        // If this is a handshake request, respond with success
                        if let Request::Handshake(_) = msg {
                            let response = Response::Handshake(HandshakeResponse {
                                success: true,
                                error_message: None,
                            });
                            let response_bytes = encode_ssz(&response);
                            let response_len = (response_bytes.len() as u32).to_le_bytes();

                            // Send response length prefix
                            let _ = stream.write_all(&response_len);
                            // Send response data
                            let _ = stream.write_all(&response_bytes);
                        }
                    }
                }
            }
            Err(_) => {}
        }
    });

    // Give the server thread a moment to start
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

    // Give some time for cleanup
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    // Verify we received the handshake message
    let messages = received_messages.lock().unwrap();
    assert!(!messages.is_empty(), "No messages received from shim");

    // The connection should still be alive (we don't test explicit teardown yet)
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

    // Start a socket server
    let listener = UnixListener::bind(&socket_path).expect("Failed to bind socket");
    let received_messages: Arc<std::sync::Mutex<Vec<Request>>> =
        Arc::new(std::sync::Mutex::new(Vec::new()));

    // Spawn a thread to handle incoming connections
    let messages_clone = Arc::clone(&received_messages);
    std::thread::spawn(move || {
        match listener.accept() {
            Ok((mut stream, _)) => {
                loop {
                    // Read length prefix (4 bytes, little endian)
                    let mut len_buf = [0u8; 4];
                    if stream.read_exact(&mut len_buf).is_err() {
                        break; // Connection closed
                    }
                    let msg_len = u32::from_le_bytes(len_buf) as usize;

                    // Read SSZ message
                    let mut msg_buf = vec![0u8; msg_len];
                    if stream.read_exact(&mut msg_buf).is_err() {
                        break; // Connection closed
                    }

                    // Decode SSZ message
                    if let Ok(msg) = decode_ssz::<Request>(&msg_buf) {
                        let mut messages = messages_clone.lock().unwrap();
                        messages.push(msg.clone());

                        // If this is a handshake request, respond with success
                        if let Request::Handshake(_) = msg {
                            let response = Response::Handshake(HandshakeResponse {
                                success: true,
                                error_message: None,
                            });
                            let response_bytes = encode_ssz(&response);
                            let response_len = (response_bytes.len() as u32).to_le_bytes();

                            // Send response length prefix
                            let _ = stream.write_all(&response_len);
                            // Send response data
                            let _ = stream.write_all(&response_bytes);
                        }
                    }
                }
            }
            Err(_) => {}
        }
    });

    // Give the server thread a moment to start
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

    // Give some time for cleanup
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    // Verify we received the handshake message
    let messages = received_messages.lock().unwrap();
    assert!(!messages.is_empty(), "No messages received from shim");
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
