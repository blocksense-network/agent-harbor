// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only
//
// Verifies that the production shim auto-propagates itself into indirect
// children so CommandStart/CommandChunk events keep flowing beyond the first
// fork/exec hop.

use ah_command_trace_e2e_tests::{find_shim_path, platform::inject_shim_and_run};
use ah_command_trace_proto::{CommandChunk, Request};
use ah_command_trace_server::test_utils::TestServer;
use tempfile::TempDir;
use tokio::task::JoinHandle;
use tokio::time::{Duration, sleep};

fn chunk_contains(requests: &[Request], needle: &str) -> bool {
    requests.iter().any(|r| {
        if let Request::CommandChunk(CommandChunk { data, .. }) = r {
            String::from_utf8_lossy(data).contains(needle)
        } else {
            false
        }
    })
}

#[tokio::test]
async fn indirect_children_keep_the_shim_loaded() {
    // Bring up the real SSZ server
    let tmp = TempDir::new().expect("tmpdir");
    let socket_path = tmp.path().join("trace.sock");
    let server = TestServer::new(&socket_path);
    let server_handle = server.clone();
    let server_task: JoinHandle<anyhow::Result<()>> =
        tokio::spawn(async move { server.run().await.map_err(|e| anyhow::anyhow!(e)) });

    // Give the server a moment to bind before the shim tries to connect.
    sleep(Duration::from_millis(75)).await;

    let shim = find_shim_path();

    // python3 (shim-loaded) -> python3 (shim-propagated) -> stdout marker
    let script = r#"
import subprocess, sys
subprocess.run(["python3", "-c", "print('AUTO_PROP_GRANDCHILD')"], check=True)
sys.stderr.flush()
"#;

    let output = inject_shim_and_run(
        &shim,
        socket_path.to_string_lossy().as_ref(),
        "python3",
        &["-c", script],
    )
    .await
    .expect("python launch");

    assert!(
        output.status.success(),
        "python3 should succeed: stdout={:?}, stderr={:?}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    // Let the shim flush any pending messages.
    sleep(Duration::from_millis(150)).await;

    let requests = server_handle.get_requests().await;
    server_task.abort();

    assert!(
        chunk_contains(&requests, "AUTO_PROP_GRANDCHILD"),
        "expected stdout chunk from grandchild; saw {:?}",
        requests
    );
}
