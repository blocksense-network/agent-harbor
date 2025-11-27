// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Integration test: launch a faux agent process under the production shim and
//! ensure the SSZ server receives CommandStart/CommandChunk events for all
//! sub-processes (direct exec, shell pipeline, python subprocess).

use ah_command_trace_e2e_tests::{find_shim_path, platform::inject_shim_and_run};
use ah_command_trace_proto::{CommandStart, Request};
use ah_command_trace_server::test_utils::TestServer;
use tempfile::TempDir;
use tokio::task::JoinHandle;
use tokio::time::{Duration, sleep};

fn count_starts(requests: &[Request]) -> usize {
    requests.iter().filter(|r| matches!(r, Request::CommandStart(_))).count()
}

fn executables(requests: &[Request]) -> Vec<String> {
    requests
        .iter()
        .filter_map(|r| {
            if let Request::CommandStart(CommandStart { executable, .. }) = r {
                Some(String::from_utf8_lossy(executable).to_lowercase())
            } else {
                None
            }
        })
        .collect()
}

#[tokio::test]
async fn agent_like_process_captures_all_children() {
    // Start the production SSZ server
    let tmp = TempDir::new().expect("tmpdir");
    let socket_path = tmp.path().join("trace.sock");
    let server = TestServer::new(&socket_path);
    let server_handle = server.clone();
    let server_task: JoinHandle<anyhow::Result<()>> =
        tokio::spawn(async move { server.run().await.map_err(|e| anyhow::anyhow!(e)) });

    // Launch the faux agent under the shim
    let shim = find_shim_path();
    let profile = std::env::var("PROFILE").unwrap_or_else(|_| "debug".into());
    let agent_bin = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("target")
        .join(profile)
        .join("agent_faux");
    assert!(
        agent_bin.exists(),
        "agent_faux helper is missing at {:?}",
        agent_bin
    );

    // Allow time for the server to bind
    sleep(Duration::from_millis(100)).await;

    let output = inject_shim_and_run(
        &shim,
        socket_path.to_string_lossy().as_ref(),
        agent_bin.to_string_lossy().as_ref(),
        &[],
    )
    .await
    .expect("run agent_faux");

    assert!(output.status.success(), "agent_faux failed: {:?}", output);

    // Wait briefly for the server to flush requests
    sleep(Duration::from_millis(200)).await;

    let requests = server_handle.get_requests().await;

    // Stop the server task to avoid dangling background work
    server_task.abort();

    // Basic coverage: at least the agent itself + children should be reported.
    let start_count = count_starts(&requests);
    assert!(
        start_count >= 5,
        "expected at least 5 CommandStart events, got {}",
        start_count
    );

    let execs = executables(&requests);
    let must_match = ["agent_faux", "echo", "sh", "python"];
    for needle in must_match {
        assert!(
            execs.iter().any(|e| e.contains(needle)),
            "missing CommandStart executable containing {needle}, saw {:?}",
            execs
        );
    }
}
