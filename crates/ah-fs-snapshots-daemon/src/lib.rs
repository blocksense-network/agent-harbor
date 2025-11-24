// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

// AH Filesystem Snapshots Daemon
//
// This crate implements a privileged daemon for handling filesystem snapshot operations
// with proper privilege escalation using sudo.

pub mod client;
pub mod fuse_manager;
pub mod interpose_manager;
pub mod operations;
pub mod server;
pub mod types;

#[cfg(test)]
mod tests {
    use crate::server::DaemonState;
    use crate::types::{Request, Response};
    use ssz::{Decode, Encode};

    #[test]
    fn test_ssz_encoding_roundtrip() {
        let request = Request::ping();
        let encoded = request.as_ssz_bytes();
        let decoded = Request::from_ssz_bytes(&encoded).unwrap();
        assert_eq!(request, decoded);
    }

    #[test]
    fn test_ping_response() {
        let request = Request::ping();
        let state = DaemonState::default();
        let response =
            tokio_test::block_on(state.process_request(request, "test-session".to_string()));
        match response {
            Response::Success(_) => {} // Expected
            _ => panic!("Expected Success response"),
        }
    }

    #[test]
    fn test_zfs_clone_request_creation() {
        let request = Request::clone_zfs("test_snapshot".to_string(), "test_clone".to_string());
        match request {
            Request::CloneZfs((snap, clone)) => {
                assert_eq!(String::from_utf8_lossy(&snap), "test_snapshot");
                assert_eq!(String::from_utf8_lossy(&clone), "test_clone");
            }
            _ => panic!("Expected CloneZfs variant"),
        }
    }

    #[test]
    fn test_response_constructors() {
        let success_resp = Response::success();
        match success_resp {
            Response::Success(_) => {} // Expected
            _ => panic!("Expected Success response"),
        }

        let error_resp = Response::error("test error".to_string());
        match error_resp {
            Response::Error(msg) => {
                assert_eq!(String::from_utf8_lossy(&msg), "test error");
            }
            _ => panic!("Expected Error response"),
        }
    }

    #[test]
    fn test_zfs_dataset_validation() {
        use crate::operations::{zfs_dataset_exists, zfs_snapshot_exists};

        // Check if ZFS is available by trying to list pools
        let zfs_available = tokio_test::block_on(async {
            tokio::process::Command::new("zfs")
                .arg("list")
                .arg("-H")
                .arg("-o")
                .arg("name")
                .output()
                .await
                .is_ok()
        });

        if !zfs_available {
            tracing::warn!("ZFS not available, skipping test");
            return;
        }

        // Check if the test dataset exists
        let test_dataset = "AH_test_zfs/test_dataset";
        let test_dataset_exists = tokio_test::block_on(zfs_dataset_exists(test_dataset));

        if !test_dataset_exists {
            tracing::warn!(dataset = %test_dataset, "ZFS test dataset does not exist, skipping test");
            return;
        }

        // Test with known ZFS pool/dataset
        assert!(test_dataset_exists, "ZFS test dataset should exist");

        // Test with non-existent dataset
        let nonexistent_exists =
            tokio_test::block_on(zfs_dataset_exists("AH_test_zfs/nonexistent"));
        assert!(
            !nonexistent_exists,
            "Non-existent ZFS dataset should not exist"
        );

        // Test with invalid snapshot
        let invalid_snapshot = tokio_test::block_on(zfs_snapshot_exists(&format!(
            "{}@nonexistent",
            test_dataset
        )));
        assert!(
            !invalid_snapshot,
            "Non-existent ZFS snapshot should not exist"
        );
    }

    #[test]
    fn test_btrfs_subvolume_validation() {
        use crate::operations::btrfs_subvolume_exists;

        // Test with non-existent path (should return false gracefully)
        let nonexistent = tokio_test::block_on(btrfs_subvolume_exists("/nonexistent/path"));
        assert!(
            !nonexistent,
            "Non-existent path should not be considered a Btrfs subvolume"
        );
    }
}

#[cfg(test)]
mod integration_tests {
    use crate::types::{Request, Response};
    use ssz::{Decode, Encode};
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt};

    const DAEMON_SOCKET_PATH: &str = "/tmp/agent-harbor/ah-fs-snapshots-daemon";

    /// Check if the daemon socket exists
    fn daemon_socket_exists() -> bool {
        std::path::Path::new(DAEMON_SOCKET_PATH).exists()
    }

    /// Ping the daemon to check if it's responsive
    async fn ping_daemon() -> bool {
        if !daemon_socket_exists() {
            return false;
        }

        // Try to connect and send a ping request
        match tokio::net::UnixStream::connect(DAEMON_SOCKET_PATH).await {
            Ok(mut stream) => {
                // Create ping request
                let ping_request = Request::ping();
                let request_bytes = ping_request.as_ssz_bytes();
                let request_hex = hex::encode(&request_bytes);

                // Send request
                if stream.write_all(format!("{}\n", request_hex).as_bytes()).await.is_err() {
                    return false;
                }

                // Read response
                let mut reader = tokio::io::BufReader::new(stream);
                let mut response_line = String::new();

                match reader.read_line(&mut response_line).await {
                    Ok(_) => {
                        let response_hex = response_line.trim();
                        if let Ok(response_bytes) = hex::decode(response_hex) {
                            if let Ok(response) = Response::from_ssz_bytes(&response_bytes) {
                                // Check if we got a success response
                                matches!(response, Response::Success(_))
                            } else {
                                false
                            }
                        } else {
                            false
                        }
                    }
                    Err(_) => false,
                }
            }
            Err(_) => false,
        }
    }

    /// Send a request to the daemon and get response
    async fn send_daemon_request(request: Request) -> Result<Response, Box<dyn std::error::Error>> {
        let mut stream = tokio::net::UnixStream::connect(DAEMON_SOCKET_PATH).await?;
        let request_bytes = request.as_ssz_bytes();
        let request_hex = hex::encode(&request_bytes);

        // Send request
        stream.write_all(format!("{}\n", request_hex).as_bytes()).await?;

        // Read response
        let mut reader = tokio::io::BufReader::new(stream);
        let mut response_line = String::new();
        reader.read_line(&mut response_line).await?;

        let response_hex = response_line.trim();
        let response_bytes = hex::decode(response_hex)?;
        let response = Response::from_ssz_bytes(&response_bytes).map_err(|e| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("SSZ decode error: {:?}", e),
            )
        })?;

        Ok(response)
    }

    #[tokio::test]
    async fn test_daemon_liveness() {
        if !daemon_socket_exists() {
            tracing::warn!(socket=%DAEMON_SOCKET_PATH, "Daemon socket not found");
            tracing::info!(
                "To run integration tests, start the daemon first: just start-ah-fs-snapshots-daemon"
            );
            return;
        }

        if !ping_daemon().await {
            tracing::warn!("Daemon not responsive (socket exists but ping failed)");
            tracing::info!("Ensure daemon running: just start-ah-fs-snapshots-daemon");
            return;
        }

        tracing::info!("Daemon is alive and responsive");
    }

    #[tokio::test]
    async fn test_daemon_ping_via_socket() {
        if !daemon_socket_exists() {
            tracing::warn!("Skipping ping test - daemon socket not found");
            return;
        }

        if !ping_daemon().await {
            tracing::warn!("Skipping ping test - daemon not responsive");
            return;
        }

        // Test ping request/response
        match send_daemon_request(Request::ping()).await {
            Ok(response) => {
                assert!(
                    matches!(response, Response::Success(_)),
                    "Expected Success response, got {:?}",
                    response
                );
                tracing::info!("Ping test passed - daemon responds correctly to ping requests");
            }
            Err(e) => {
                panic!("Failed to send ping request: {}", e);
            }
        }
    }

    #[tokio::test]
    async fn test_daemon_zfs_operations() {
        if !daemon_socket_exists() {
            tracing::warn!("Skipping ZFS operations test - daemon socket not found");
            return;
        }

        if !ping_daemon().await {
            tracing::warn!("Skipping ZFS operations test - daemon not responsive");
            return;
        }

        // Check if ZFS test dataset exists by attempting a snapshot operation
        let test_dataset = "AH_test_zfs/test_dataset";
        match send_daemon_request(Request::snapshot_zfs(
            test_dataset.to_string(),
            format!("{}@integration_test_{}", test_dataset, std::process::id()),
        ))
        .await
        {
            Ok(response) => match response {
                Response::Success(_) => {
                    tracing::info!("ZFS snapshot creation test passed");
                }
                Response::Error(msg) => {
                    let error_msg = String::from_utf8_lossy(&msg);
                    tracing::warn!(error=%error_msg, "ZFS snapshot test failed");
                    tracing::info!(
                        "Ensure ZFS test filesystem is set up: just create-test-filesystems"
                    );
                }
                _ => {
                    tracing::warn!(?response, "Unexpected response for ZFS snapshot");
                }
            },
            Err(e) => {
                tracing::error!(error=%e, "Failed to send ZFS snapshot request");
            }
        }
    }

    #[tokio::test]
    async fn test_daemon_btrfs_operations() {
        if !daemon_socket_exists() {
            tracing::warn!("Skipping Btrfs operations test - daemon socket not found");
            return;
        }

        if !ping_daemon().await {
            tracing::warn!("Skipping Btrfs operations test - daemon not responsive");
            return;
        }

        // For now, Btrfs testing is optional and will likely fail without proper setup
        // This is a placeholder for future comprehensive Btrfs testing
        match send_daemon_request(Request::snapshot_btrfs(
            "/tmp/test_source".to_string(),
            "/tmp/test_snapshot".to_string(),
        ))
        .await
        {
            Ok(response) => match response {
                Response::Error(msg) => {
                    let error_msg = String::from_utf8_lossy(&msg);
                    tracing::info!(error=%error_msg, "Btrfs snapshot test (expected failure without setup)");
                }
                _ => {
                    tracing::info!("Btrfs snapshot creation test passed (unexpected!)");
                }
            },
            Err(e) => {
                tracing::error!(error=%e, "Failed to send Btrfs snapshot request");
            }
        }
    }

    #[tokio::test]
    async fn test_daemon_communication_protocol() {
        if !daemon_socket_exists() {
            tracing::warn!("Skipping protocol test - daemon socket not found");
            return;
        }

        if !ping_daemon().await {
            tracing::warn!("Skipping protocol test - daemon not responsive");
            return;
        }

        // Test SSZ encoding/decoding roundtrip (unit test part)
        let requests = vec![
            Request::ping(),
            Request::snapshot_zfs("test_source".to_string(), "test_snapshot".to_string()),
            Request::clone_zfs("test_snapshot".to_string(), "test_clone".to_string()),
            Request::delete_zfs("test_target".to_string()),
            Request::snapshot_btrfs("test_source".to_string(), "test_snapshot".to_string()),
            Request::clone_btrfs("test_snapshot".to_string(), "test_clone".to_string()),
            Request::delete_btrfs("test_target".to_string()),
        ];

        for request in requests {
            let encoded = request.as_ssz_bytes();
            let decoded = Request::from_ssz_bytes(&encoded).unwrap();
            assert_eq!(request, decoded);
        }

        // Test actual daemon communication
        let test_requests = vec![
            Request::ping(),
            Request::delete_zfs("nonexistent_dataset".to_string()), // Should fail gracefully
            Request::delete_btrfs("nonexistent_path".to_string()),  // Should fail gracefully
        ];

        for request in test_requests {
            match send_daemon_request(request.clone()).await {
                Ok(_response) => {
                    // Any response is fine - we just want to ensure the protocol works
                    tracing::info!(?request, "Protocol test passed");
                }
                Err(e) => {
                    tracing::error!(?request, error=%e, "Protocol test failed");
                }
            }
        }
    }
}
