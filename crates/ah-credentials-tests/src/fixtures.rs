// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Test fixtures for credentials management

use ah_credentials::{
    config::CredentialsConfig,
    types::{Account, AccountRegistry, AgentType},
};
use std::path::PathBuf;
use tempfile::TempDir;

/// Test fixture that provides a temporary credentials directory
pub struct TestCredentialsFixture {
    pub temp_dir: TempDir,
    pub config: CredentialsConfig,
    pub registry: ah_credentials::registry::AccountRegistry,
}

impl TestCredentialsFixture {
    /// Create a new test fixture with temporary directories
    pub async fn new() -> Self {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let storage_path = temp_dir.path().to_path_buf();

        let config = CredentialsConfig {
            storage_path: Some(storage_path),
            default_accounts: Default::default(),
            auto_verify: Default::default(),
            crypto: Default::default(),
            base_config_dir: None,
            ah_home_override: None,
        };

        let registry = ah_credentials::registry::AccountRegistry::new(config.clone());

        Self {
            temp_dir,
            config,
            registry,
        }
    }

    /// Create a sample account for testing
    pub fn create_sample_account(name: &str, agent: AgentType) -> Account {
        let mut account = Account::new(name.to_string(), agent);
        account.add_alias(format!("{}-alias", name));
        account
    }

    /// Add a sample account to the registry
    pub async fn add_sample_account(&self, name: &str, agent: AgentType) -> Account {
        let account = Self::create_sample_account(name, agent);
        self.registry.add_account(account.clone()).await.unwrap();
        account
    }

    /// Create a populated registry with sample accounts
    pub async fn create_populated_registry(&self) -> AccountRegistry {
        let mut registry = AccountRegistry::new();

        // Add sample accounts for each agent type
        let codex_account = Self::create_sample_account("codex-work", AgentType::Codex);
        let claude_account = Self::create_sample_account("claude-personal", AgentType::Claude);
        let cursor_account = Self::create_sample_account("cursor-team", AgentType::Cursor);

        registry.add_account(codex_account);
        registry.add_account(claude_account);
        registry.add_account(cursor_account);

        registry
    }

    /// Get the path to the accounts.toml file
    pub fn accounts_file(&self) -> PathBuf {
        self.config.accounts_file().unwrap()
    }

    /// Get the path to the keys directory
    pub fn keys_dir(&self) -> PathBuf {
        self.config.keys_dir().unwrap()
    }

    /// Get the path to the temp directory
    pub fn temp_dir_path(&self) -> PathBuf {
        self.config.temp_dir().unwrap()
    }
}

/// Sample credential data for testing
pub struct SampleCredentials {
    pub codex: serde_json::Value,
    pub claude: serde_json::Value,
    pub cursor: serde_json::Value,
}

impl Default for SampleCredentials {
    fn default() -> Self {
        Self {
            codex: serde_json::json!({
                "api_key": "sk-test-codex-key-12345",
                "endpoint": "https://api.github.com/copilot_internal/v2",
                "user": "test-user"
            }),
            claude: serde_json::json!({
                "api_key": "sk-ant-test-claude-key-67890",
                "organization_id": "org-test-123",
                "project_id": "proj-test-456"
            }),
            cursor: serde_json::json!({
                "access_token": "ghu_test_cursor_token_abcdef",
                "refresh_token": "ghr_test_refresh_token_ghijkl",
                "token_type": "bearer",
                "expires_at": "2025-12-31T23:59:59Z"
            }),
        }
    }
}
