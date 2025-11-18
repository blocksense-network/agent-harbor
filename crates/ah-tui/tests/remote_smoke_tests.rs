// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Smoke tests for the remote manual-testing harness.
//!
//! These tests execute the orchestration script in headless (`--smoke`) mode
//! to ensure the mock remote workflow stays healthy. They run as part of
//! `cargo nextest run --workspace` via the top-level `just test-rust` target.

use std::{
    env, fs,
    net::TcpListener,
    path::{Path, PathBuf},
    process::Command,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

use ah_core::{RemoteRepositoriesEnumerator, RepositoriesEnumerator, RestApiClient};
use ah_rest_api_contract::{BranchInfo, Repository};
use async_trait::async_trait;
use futures::stream;
use url::Url;

fn project_root() -> PathBuf {
    // crates/ah-tui -> crates -> repo root
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest_dir
        .parent()
        .and_then(Path::parent)
        .expect("workspace root directory")
        .to_path_buf()
}

fn pick_free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .expect("bind ephemeral port")
        .local_addr()
        .expect("socket address")
        .port()
}

fn extract_path(label: &str, output: &str) -> PathBuf {
    output
        .lines()
        .find_map(|line| line.split_once(label).map(|(_, value)| PathBuf::from(value.trim())))
        .unwrap_or_else(|| panic!("expected '{}' line in script output:\n{}", label, output))
}

#[test]
fn manual_remote_smoke_rest() {
    let root = project_root();
    let script = root.join("scripts/manual-test-remote.py");
    assert!(script.exists(), "expected script at {}", script.display());

    // Use a dedicated port to avoid collisions with developer machines.
    let port = pick_free_port();

    let output = Command::new("python3")
        .arg(&script)
        .arg("--mode")
        .arg("rest")
        .arg("--smoke")
        .arg("--no-build")
        .arg("--port")
        .arg(port.to_string())
        .arg("--timeout")
        .arg("30")
        .current_dir(&root)
        .output()
        .expect("failed to invoke manual-test-remote.py");
    assert!(
        output.status.success(),
        "manual-test-remote.py exited with {:?}\nstdout:\n{}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let run_dir = extract_path("Run directory:", &stdout);
    let log_dir = extract_path("Log directory:", &stdout);

    assert!(
        log_dir.ends_with(run_dir.file_name().unwrap()),
        "log directory should share run id"
    );

    let server_log = log_dir.join("rest-server.log");
    assert!(
        server_log.exists(),
        "REST server log not written at {}",
        server_log.display()
    );
    let log_size = fs::metadata(&server_log).expect("server log metadata").len();
    assert!(log_size > 0, "mock server log should not be empty");

    // Ensure the smoke validator wrote the run summary.
    let script_log = log_dir.join("script.log");
    assert!(
        script_log.exists(),
        "script log missing at {}",
        script_log.display()
    );
}

#[test]
fn manual_remote_smoke_mock() {
    let root = project_root();
    let script = root.join("scripts/manual-test-remote.py");
    assert!(script.exists(), "expected script at {}", script.display());

    let port = pick_free_port();

    let output = Command::new("python3")
        .arg(&script)
        .arg("--mode")
        .arg("mock")
        .arg("--smoke")
        .arg("--no-build")
        .arg("--port")
        .arg(port.to_string())
        .arg("--timeout")
        .arg("30")
        .current_dir(&root)
        .output()
        .expect("failed to invoke manual-test-remote.py");
    assert!(
        output.status.success(),
        "manual-test-remote.py exited with {:?}\nstdout:\n{}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let run_dir = extract_path("Run directory:", &stdout);
    let log_dir = extract_path("Log directory:", &stdout);

    assert!(
        log_dir.ends_with(run_dir.file_name().unwrap()),
        "log directory should share run id"
    );

    let server_log = log_dir.join("rest-server.log");
    assert!(
        server_log.exists(),
        "Mock REST server log not written at {}",
        server_log.display()
    );

    let script_log = log_dir.join("script.log");
    assert!(
        script_log.exists(),
        "script log missing at {}",
        script_log.display()
    );
    let log_contents =
        fs::read_to_string(&script_log).expect("unable to read manual-test script log");
    assert!(
        log_contents.contains("status=running"),
        "expected script log to mention running sessions"
    );
}

#[derive(Clone, Default)]
struct RecordingClient {
    list_repositories_calls: Arc<AtomicUsize>,
}

#[async_trait]
impl RestApiClient for RecordingClient {
    async fn create_task(
        &self,
        _request: &ah_rest_api_contract::CreateTaskRequest,
    ) -> Result<ah_rest_api_contract::CreateTaskResponse, Box<dyn std::error::Error + Send + Sync>>
    {
        panic!("create_task should not be called in repository enumerator test");
    }

    async fn stream_session_events(
        &self,
        _session_id: &str,
    ) -> Result<
        std::pin::Pin<
            Box<
                dyn futures::Stream<
                        Item = Result<
                            ah_rest_api_contract::SessionEvent,
                            Box<dyn std::error::Error + Send + Sync>,
                        >,
                    > + Send,
            >,
        >,
        Box<dyn std::error::Error + Send + Sync>,
    > {
        Ok(Box::pin(stream::empty()))
    }

    async fn list_sessions(
        &self,
        _filters: Option<&ah_rest_api_contract::FilterQuery>,
    ) -> Result<ah_rest_api_contract::SessionListResponse, Box<dyn std::error::Error + Send + Sync>>
    {
        panic!("list_sessions should not be called in repository enumerator test");
    }

    async fn list_repositories(
        &self,
        _tenant_id: Option<&str>,
        _project_id: Option<&str>,
    ) -> Result<Vec<Repository>, Box<dyn std::error::Error + Send + Sync>> {
        self.list_repositories_calls.fetch_add(1, Ordering::SeqCst);
        let repo = Repository {
            id: "r1".into(),
            display_name: "demo/repo1".into(),
            scm_provider: "github".into(),
            remote_url: Url::parse("https://github.com/demo/repo1")?,
            default_branch: "main".into(),
            last_used_at: None,
        };
        Ok(vec![repo])
    }

    async fn get_repository_branches(
        &self,
        _repository_id: &str,
    ) -> Result<Vec<BranchInfo>, Box<dyn std::error::Error + Send + Sync>> {
        Ok(vec![BranchInfo {
            name: "main".into(),
            is_default: true,
            last_commit: Some("abc123".into()),
        }])
    }

    async fn save_draft_task(
        &self,
        _draft_id: &str,
        _description: &str,
        _repository: &str,
        _branch: &str,
        _agents: &[ah_domain_types::AgentChoice],
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        panic!("save_draft_task should not be called in repository enumerator test");
    }

    async fn get_repository_files(
        &self,
        _repository_id: &str,
    ) -> Result<Vec<ah_rest_api_contract::RepositoryFile>, Box<dyn std::error::Error + Send + Sync>>
    {
        panic!("get_repository_files should not be called in repository enumerator test");
    }
}

#[tokio::test]
async fn remote_repositories_enumerator_invokes_rest_client() {
    let client = RecordingClient::default();
    let enumerator =
        RemoteRepositoriesEnumerator::new(client.clone(), "http://127.0.0.1:3000".into());

    let repos = enumerator.list_repositories().await;

    assert_eq!(
        client.list_repositories_calls.load(Ordering::SeqCst),
        1,
        "expected list_repositories to be invoked exactly once"
    );
    assert_eq!(repos.len(), 1);
    assert_eq!(repos[0].name, "demo/repo1");
    assert_eq!(repos[0].default_branch, "main");
}
