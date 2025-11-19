// Copyright 2025 Schelling Point Labs Inc
//! Output capture tests for shim

use ah_command_trace_proto::Request;
use ah_command_trace_server::test_utils::TestServer;
use std::path::PathBuf;
use tempfile::TempDir;

fn find_mixed_output_path() -> PathBuf {
    let profile = std::env::var("PROFILE").unwrap_or_else(|_| "debug".into());
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("target")
        .join(&profile);

    let helper_path = root.join("mixed_output");

    if !helper_path.exists() {
        let deps_path = root.join("deps").join("mixed_output");
        if deps_path.exists() {
            return deps_path;
        }
        panic!("mixed_output binary not found. Run `cargo test -p ah-command-trace-e2e-tests`");
    }

    helper_path
}

#[cfg_attr(not(any(target_os = "macos", target_os = "linux")), ignore)]
#[tokio::test]
async fn shim_captures_mixed_output() {
    if std::env::var("CI").is_ok() {
        println!("⚠️  Skipping shim output capture test in CI environment");
        return;
    }

    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let socket_path = temp_dir.path().join("test_socket");
    let server = TestServer::new(&socket_path);
    let server_future = server.run();

    let test_future = async {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        let helper_path = find_mixed_output_path();
        let output = ah_command_trace_e2e_tests::execute_test_scenario(
            &socket_path.to_string_lossy(),
            &helper_path.to_string_lossy(),
            &[],
        )
        .await
        .expect("Failed to execute mixed_output");

        assert!(output.status.success(), "mixed_output failed: {:?}", output);
        output
    };

    let result = tokio::time::timeout(std::time::Duration::from_secs(35), async {
        tokio::join!(server_future, test_future)
    })
    .await;

    match result {
        Ok((server_result, output)) => {
            server_result.expect("Server failed");
            let messages = server.get_requests().await;

            // Filter chunks
            let chunks: Vec<_> = messages
                .iter()
                .filter_map(|m| {
                    if let Request::CommandChunk(chunk) = m {
                        Some(chunk.clone())
                    } else {
                        None
                    }
                })
                .collect();

            if chunks.is_empty() {
                eprintln!("No output chunks received.");
                eprintln!("Stdout: {}", String::from_utf8_lossy(&output.stdout));
                eprintln!("Stderr: {}", String::from_utf8_lossy(&output.stderr));
                panic!("No output chunks received");
            }

            // Reconstruct streams
            let mut stdout = Vec::new();
            let mut stderr = Vec::new();

            for chunk in chunks {
                if chunk.stream_type == 0 {
                    // Stdout
                    stdout.extend_from_slice(&chunk.data);
                } else if chunk.stream_type == 1 {
                    // Stderr
                    stderr.extend_from_slice(&chunk.data);
                }
            }

            let stdout_str = String::from_utf8_lossy(&stdout);
            let stderr_str = String::from_utf8_lossy(&stderr);

            // Check content
            // 1. Stdout: "1"
            assert!(stdout_str.contains("1"), "Stdout missing '1'");
            // 2. Stderr: "2"
            assert!(stderr_str.contains("2"), "Stderr missing '2'");

            // 3. 4K chunks
            // We might not get exactly 4096 bytes in one chunk, but the total data should be there.
            // Wait, contains might be slow for large strings if not careful, but 4K is small.
            // Check for presence of 'A's
            let a_count = stdout.iter().filter(|&&b| b == b'A').count();
            assert!(a_count >= 4096, "Stdout missing 4K 'A's");

            let b_count = stderr.iter().filter(|&&b| b == b'B').count();
            assert!(b_count >= 4096, "Stderr missing 4K 'B's");

            // 4. Writev
            assert!(
                stdout_str.contains("writev stdout"),
                "Stdout missing writev output"
            );

            // 5. FD 7 dup
            assert!(
                stdout_str.contains("Writing to FD 7"),
                "Stdout missing FD 7 output"
            );

            // 6. ANSI and binary
            assert!(
                stdout_str.contains("\x1b[31mRed Text"),
                "Stdout missing ANSI code"
            );
            // Binary check
            let binary_pattern = b"\x00\x01\x02\x03 Binary Data \xff\xfe";
            assert!(
                stdout.windows(binary_pattern.len()).any(|w| w == binary_pattern),
                "Stdout missing binary data"
            );
        }
        Err(_) => panic!("Test timed out"),
    }
}
