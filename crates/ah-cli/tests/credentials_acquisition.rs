// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

use ah_agents::{AcquisitionOptions, CredentialAcquirer};
use ah_cli::credentials::AcquisitionService;
use ah_credentials_tests::TestCredentialsFixture;
use async_trait::async_trait;
use chrono::Utc;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::process::Child;

static LOG_COUNTER: AtomicUsize = AtomicUsize::new(0);

fn log_path(test: &str) -> PathBuf {
    let n = LOG_COUNTER.fetch_add(1, Ordering::SeqCst);
    let path = std::env::temp_dir().join(format!("ah-cli-creds-{}-{}.log", test, n));
    std::fs::write(&path, format!("log for {}\n", test)).ok();
    path
}

struct FixtureAcquirer {
    agent: &'static str,
    payload: serde_json::Value,
}

#[async_trait]
impl CredentialAcquirer for FixtureAcquirer {
    fn agent_kind(&self) -> &'static str {
        self.agent
    }

    async fn launch_for_login(&self, home_dir: &Path) -> ah_agents::AgentResult<Child> {
        let path = credential_path(self.agent, home_dir);
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let bytes = serde_json::to_vec(&self.payload)
            .map_err(|e| ah_agents::AgentError::OutputParsingFailed(e.to_string()))?;
        tokio::fs::write(&path, bytes).await?;

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
async fn stores_acquired_credentials_and_account() {
    let _log = log_path("stores_acquired_credentials_and_account");
    let fixture = TestCredentialsFixture::new().await;
    let service = AcquisitionService::new(fixture.config.clone());

    let acquirer = FixtureAcquirer {
        agent: "codex",
        payload: serde_json::json!({"api_key": "sk-test-999"}),
    };

    let stored = service
        .acquire_and_store(
            &acquirer,
            "codex-work",
            false,
            None,
            Some(AcquisitionOptions::with_temp_root(fixture.temp_dir_path())),
        )
        .await
        .expect("acquisition should succeed");

    assert_eq!(stored.account.name, "codex-work");
    assert!(stored.credential_file.exists());
    let on_disk = tokio::fs::read_to_string(&stored.credential_file).await.unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&on_disk).unwrap();
    assert_eq!(parsed["api_key"], "sk-test-999");

    let registry_content = tokio::fs::read_to_string(fixture.accounts_file()).await.unwrap();
    assert!(
        registry_content.contains("codex-work"),
        "account should be written to registry"
    );
    assert_eq!(stored.account.status, ah_credentials::AccountStatus::Active);
}

#[tokio::test]
async fn marks_expired_accounts() {
    let _log = log_path("marks_expired_accounts");
    let fixture = TestCredentialsFixture::new().await;
    let service = AcquisitionService::new(fixture.config.clone());

    let expired = (Utc::now() - chrono::Duration::hours(1)).to_rfc3339();
    let acquirer = FixtureAcquirer {
        agent: "claude",
        payload: serde_json::json!({"expires_at": expired}),
    };

    let stored = service
        .acquire_and_store(
            &acquirer,
            "claude-personal",
            false,
            None,
            Some(AcquisitionOptions::with_temp_root(fixture.temp_dir_path())),
        )
        .await
        .expect("acquisition should succeed");

    assert_eq!(
        stored.account.status,
        ah_credentials::AccountStatus::Expired
    );
}
