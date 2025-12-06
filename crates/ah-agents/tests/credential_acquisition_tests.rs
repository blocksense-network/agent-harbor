// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

use ah_agents::{AcquisitionOptions, CredentialAcquirer, run_acquisition};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use tempfile::TempDir;
use tokio::process::Child;
use tokio::time::Duration;

static LOG_COUNTER: AtomicUsize = AtomicUsize::new(0);

fn log_path(test: &str) -> PathBuf {
    let n = LOG_COUNTER.fetch_add(1, Ordering::SeqCst);
    let path = std::env::temp_dir().join(format!("ah-agents-acq-{}-{}.log", test, n));
    std::fs::write(&path, format!("log for {}\n", test)).ok();
    path
}

struct StubAcquirer {
    agent: &'static str,
    payload: serde_json::Value,
}

#[async_trait::async_trait]
impl CredentialAcquirer for StubAcquirer {
    fn agent_kind(&self) -> &'static str {
        self.agent
    }

    async fn launch_for_login(&self, home_dir: &Path) -> ah_agents::AgentResult<Child> {
        // Create the expected credential file eagerly to keep the stub simple.
        let path = credential_path(self.agent, home_dir);
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let bytes = serde_json::to_vec(&self.payload)
            .map_err(|e| ah_agents::AgentError::OutputParsingFailed(e.to_string()))?;
        tokio::fs::write(&path, bytes).await?;

        // Lightweight process that exits immediately.
        tokio::process::Command::new("sh")
            .arg("-c")
            .arg("exit 0")
            .spawn()
            .map_err(ah_agents::AgentError::ProcessSpawnFailed)
    }

    async fn extract_credentials(
        &self,
        home_dir: &Path,
    ) -> ah_agents::AgentResult<serde_json::Value> {
        let path = credential_path(self.agent, home_dir);
        let content = tokio::fs::read_to_string(&path).await?;
        serde_json::from_str(&content)
            .map_err(|e| ah_agents::AgentError::OutputParsingFailed(e.to_string()))
    }
}

fn credential_path(agent: &str, home: &Path) -> PathBuf {
    match agent {
        "claude" => home.join(".config/claude/auth.json"),
        "cursor" => home.join(".config/cursor/auth.json"),
        _ => home.join(".codex/auth.json"),
    }
}

#[tokio::test]
async fn acquires_and_cleans_temp_home() {
    let _log = log_path("acquires_and_cleans_temp_home");
    let temp_root = TempDir::new().unwrap();
    let acquirer = StubAcquirer {
        agent: "codex",
        payload: serde_json::json!({"api_key": "sk-test-123"}),
    };

    let result = run_acquisition(
        &acquirer,
        AcquisitionOptions {
            temp_root: temp_root.path().to_path_buf(),
            cleanup: true,
            timeout: Duration::from_secs(5),
        },
    )
    .await
    .expect("acquisition should succeed");

    assert_eq!(result.credentials["api_key"], "sk-test-123");
    assert!(
        !result.temp_home.exists(),
        "temp home should be removed after cleanup"
    );
}

#[tokio::test]
async fn detects_expiry_metadata() {
    let _log = log_path("detects_expiry_metadata");
    let temp_root = TempDir::new().unwrap();
    let expired = (chrono::Utc::now() - chrono::Duration::seconds(60)).to_rfc3339();
    let acquirer = StubAcquirer {
        agent: "claude",
        payload: serde_json::json!({"expires_at": expired}),
    };

    let result = run_acquisition(
        &acquirer,
        AcquisitionOptions {
            temp_root: temp_root.path().to_path_buf(),
            cleanup: true,
            timeout: Duration::from_secs(5),
        },
    )
    .await
    .expect("acquisition should succeed");

    assert!(
        result.is_expired(),
        "expiry should be detected from payload"
    );
}

#[tokio::test]
async fn concurrent_acquisitions_are_isolated() {
    let _log = log_path("concurrent_acquisitions_are_isolated");
    let temp_root = TempDir::new().unwrap();

    let a1 = StubAcquirer {
        agent: "codex",
        payload: serde_json::json!({"api_key": "first"}),
    };
    let a2 = StubAcquirer {
        agent: "cursor",
        payload: serde_json::json!({"access_token": "second"}),
    };

    let opts = AcquisitionOptions {
        temp_root: temp_root.path().to_path_buf(),
        cleanup: true,
        timeout: Duration::from_secs(5),
    };

    let (r1, r2) = tokio::try_join!(
        run_acquisition(&a1, opts.clone()),
        run_acquisition(&a2, opts)
    )
    .expect("both acquisitions should succeed");

    assert_eq!(r1.credentials["api_key"], "first");
    assert_eq!(r2.credentials["access_token"], "second");

    // Temp root should be empty after both cleanups.
    let mut entries = tokio::fs::read_dir(temp_root.path()).await.unwrap();
    assert!(entries.next_entry().await.unwrap().is_none());
}

#[tokio::test]
async fn surfaces_launch_failures() {
    let _log = log_path("surfaces_launch_failures");
    let temp_root = TempDir::new().unwrap();

    struct FailingAcquirer;
    #[async_trait::async_trait]
    impl CredentialAcquirer for FailingAcquirer {
        fn agent_kind(&self) -> &'static str {
            "codex"
        }

        async fn launch_for_login(&self, _home_dir: &Path) -> ah_agents::AgentResult<Child> {
            Err(ah_agents::AgentError::ProcessSpawnFailed(
                std::io::Error::new(std::io::ErrorKind::NotFound, "missing binary"),
            ))
        }

        async fn extract_credentials(
            &self,
            _home_dir: &Path,
        ) -> ah_agents::AgentResult<serde_json::Value> {
            Ok(serde_json::json!({}))
        }
    }

    let err = run_acquisition(
        &FailingAcquirer,
        AcquisitionOptions {
            temp_root: temp_root.path().to_path_buf(),
            cleanup: true,
            timeout: Duration::from_secs(1),
        },
    )
    .await
    .expect_err("launch failure should bubble up");

    match err {
        ah_agents::AgentError::ProcessSpawnFailed(_) => {}
        other => panic!("unexpected error: {:?}", other),
    }
}
