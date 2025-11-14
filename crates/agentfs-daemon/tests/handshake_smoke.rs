// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Smoke test for the AgentFS daemon handshake and control-plane operations.

#[cfg(target_os = "macos")]
mod macos {
    use agentfs_client::{AgentFsClient, ClientConfig};
    use agentfs_interpose_e2e_tests::find_daemon_path;
    use std::fs;
    use std::path::Path;
    use std::process::{Command, Stdio};
    use std::thread;
    use std::time::{Duration, Instant};

    #[test]
    fn handshake_smoke() {
        let daemon_path = find_daemon_path();
        let socket_dir = tempfile::Builder::new()
            .prefix("agentfs-handshake-")
            .tempdir()
            .expect("failed to create temporary directory for daemon socket");
        let socket_path = socket_dir.path().join("agentfs.sock");

        let repo_dir = tempfile::Builder::new()
            .prefix("agentfs-handshake-repo-")
            .tempdir()
            .expect("failed to create temporary repository");
        fs::write(repo_dir.path().join("README.md"), "handshake smoke test\n")
            .expect("failed to write repository marker");

        let child = Command::new(&daemon_path)
            .arg(socket_path.to_string_lossy().to_string())
            .arg("--lower-dir")
            .arg(repo_dir.path())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("failed to spawn agentfs-daemon process");

        wait_for_socket(&socket_path, Duration::from_secs(5))
            .expect("agentfs-daemon did not create socket in time");

        let client_config =
            ClientConfig::builder("handshake-smoke-test", env!("CARGO_PKG_VERSION"))
                .feature("control-plane")
                .read_timeout(Duration::from_secs(5))
                .write_timeout(Duration::from_secs(5))
                .build()
                .expect("failed to build AgentFS client configuration");

        let handshake_start = Instant::now();
        let mut client =
            AgentFsClient::connect(&socket_path, &client_config).expect("failed to connect");
        let handshake_elapsed = handshake_start.elapsed();
        assert!(
            handshake_elapsed <= Duration::from_secs(2),
            "handshake took longer than expected: {:?}",
            handshake_elapsed
        );

        let ack = client.handshake_ack();
        assert!(
            !ack.is_empty(),
            "daemon handshake acknowledgement payload should not be empty"
        );
        println!(
            "handshake_smoke: received handshake ack ({} bytes)",
            ack.len()
        );

        let snapshot = client
            .snapshot_create(Some("handshake-smoke".to_string()))
            .expect("snapshot_create failed");
        assert!(
            !snapshot.id.is_empty(),
            "daemon returned empty snapshot identifier"
        );
        println!("handshake_smoke: snapshot created {}", snapshot.id);

        let branch = client
            .branch_create(&snapshot.id, Some("handshake-branch".to_string()))
            .expect("branch_create failed");
        println!("handshake_smoke: branch created {}", branch.id);

        client
            .branch_bind(&branch.id, Some(std::process::id()))
            .expect("branch_bind failed");
        println!("handshake_smoke: branch bound to current process");

        let snapshots = client.snapshot_list().expect("snapshot_list failed");
        assert!(
            snapshots.iter().any(|record| record.id == snapshot.id),
            "expected handshake snapshot to appear in snapshot_list"
        );
        println!("handshake_smoke: snapshot_list contains created snapshot");

        terminate_child(child);
    }

    fn wait_for_socket(path: &Path, timeout: Duration) -> std::io::Result<()> {
        let start = Instant::now();
        while !path.exists() {
            if start.elapsed() > timeout {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::TimedOut,
                    format!("socket {} not created within {:?}", path.display(), timeout),
                ));
            }
            thread::sleep(Duration::from_millis(50));
        }
        Ok(())
    }

    fn terminate_child(mut child: std::process::Child) {
        let _ = child.kill();
        match child.wait_with_output() {
            Ok(output) => {
                if !output.stdout.is_empty() {
                    println!(
                        "agentfs-daemon stdout:\n{}",
                        String::from_utf8_lossy(&output.stdout)
                    );
                }
                if !output.stderr.is_empty() {
                    eprintln!(
                        "agentfs-daemon stderr:\n{}",
                        String::from_utf8_lossy(&output.stderr)
                    );
                }
            }
            Err(err) => {
                eprintln!("failed to gather agentfs-daemon output: {err}");
            }
        }
    }
}

#[cfg(not(target_os = "macos"))]
#[test]
fn handshake_smoke() {
    println!("Skipping AgentFS handshake smoke test: unsupported platform");
}
