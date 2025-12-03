// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Integration tests for registry operations

use ah_credentials::{
    config::CredentialsConfig,
    registry::AccountRegistry,
    types::{Account, AgentType},
};
use std::collections::HashMap;

#[tokio::test]
async fn test_registry_full_lifecycle() {
    let log_path = ah_credentials::test_utils::setup_test_logging("test_registry_full_lifecycle");

    if let Err(e) = std::fs::write(&log_path, "Testing registry full lifecycle\n") {
        panic!("Failed to write to test log {}: {}", log_path.display(), e);
    }

    let temp_dir = tempfile::TempDir::new().unwrap();
    let config = CredentialsConfig {
        storage_path: Some(temp_dir.path().to_path_buf()),
        default_accounts: HashMap::new(),
        auto_verify: Default::default(),
        base_config_dir: None,
        ah_home_override: None,
    };

    let registry = AccountRegistry::new(config.clone());

    // Initially empty
    assert!(registry.is_empty().await);
    assert_eq!(registry.count().await, 0);

    // Add some accounts
    let account1 = Account::new("work-codex".to_string(), AgentType::Codex);
    let account2 = Account::new("personal-claude".to_string(), AgentType::Claude);
    let account3 = Account::new("team-cursor".to_string(), AgentType::Cursor);

    registry.add_account(account1.clone()).await.unwrap();
    registry.add_account(account2.clone()).await.unwrap();
    registry.add_account(account3.clone()).await.unwrap();

    assert_eq!(registry.count().await, 3);

    // Test finding accounts
    let found1 = registry.find_account("work-codex").await.unwrap();
    assert_eq!(found1.name, "work-codex");
    assert_eq!(found1.agent, AgentType::Codex);

    let found2 = registry.find_account("personal-claude").await.unwrap();
    assert_eq!(found2.name, "personal-claude");
    assert_eq!(found2.agent, AgentType::Claude);

    // Test accounts for agent
    let codex_accounts = registry.accounts_for_agent(&AgentType::Codex).await;
    assert_eq!(codex_accounts.len(), 1);
    assert_eq!(codex_accounts[0].name, "work-codex");

    let claude_accounts = registry.accounts_for_agent(&AgentType::Claude).await;
    assert_eq!(claude_accounts.len(), 1);
    assert_eq!(claude_accounts[0].name, "personal-claude");

    let cursor_accounts = registry.accounts_for_agent(&AgentType::Cursor).await;
    assert_eq!(cursor_accounts.len(), 1);
    assert_eq!(cursor_accounts[0].name, "team-cursor");

    // Test identifier availability
    assert!(registry.is_identifier_available("available-name").await);
    assert!(!registry.is_identifier_available("work-codex").await);
    assert!(!registry.is_identifier_available("personal-claude").await);

    // Test adding alias
    registry.add_alias("work-codex", "codex-work".to_string()).await.unwrap();
    let updated_account = registry.find_account("codex-work").await.unwrap();
    assert!(updated_account.has_alias("codex-work"));

    // Test marking account used
    let original_time = updated_account.last_used;
    registry.mark_account_used("work-codex").await.unwrap();
    let marked_account = registry.find_account("work-codex").await.unwrap();
    assert!(marked_account.last_used > original_time);

    // Test removing alias
    registry.remove_alias("work-codex", "codex-work").await.unwrap();
    let account_without_alias = registry.find_account("work-codex").await.unwrap();
    assert!(!account_without_alias.has_alias("codex-work"));

    // Test removing account
    let removed = registry.remove_account("personal-claude").await.unwrap();
    assert_eq!(removed.unwrap().name, "personal-claude");
    assert_eq!(registry.count().await, 2);
    assert!(registry.find_account("personal-claude").await.is_none());

    // Test save and reload
    registry.save().await.unwrap();

    // Create new registry instance and reload
    let new_registry = AccountRegistry::new(config);
    new_registry.load().await.unwrap();
    assert_eq!(new_registry.count().await, 2);

    let reloaded_codex = new_registry.find_account("work-codex").await.unwrap();
    assert_eq!(reloaded_codex.name, "work-codex");
    assert_eq!(reloaded_codex.agent, AgentType::Codex);

    if let Err(e) = std::fs::write(&log_path, "Registry full lifecycle test completed\n") {
        panic!("Failed to write to test log {}: {}", log_path.display(), e);
    }

    assert!(
        log_path.exists(),
        "Test log file should exist at: {}",
        log_path.display()
    );
}

#[tokio::test]
async fn test_registry_error_cases() {
    let log_path = ah_credentials::test_utils::setup_test_logging("test_registry_error_cases");

    if let Err(e) = std::fs::write(&log_path, "Testing registry error cases\n") {
        panic!("Failed to write to test log {}: {}", log_path.display(), e);
    }

    let temp_dir = tempfile::TempDir::new().unwrap();
    let config = CredentialsConfig {
        storage_path: Some(temp_dir.path().to_path_buf()),
        default_accounts: HashMap::new(),
        auto_verify: Default::default(),
        base_config_dir: None,
        ah_home_override: None,
    };

    let registry = AccountRegistry::new(config);

    // Try to find non-existent account
    assert!(registry.find_account("nonexistent").await.is_none());

    // Try to mark non-existent account as used
    assert!(registry.mark_account_used("nonexistent").await.is_err());

    // Try to add alias to non-existent account
    assert!(registry.add_alias("nonexistent", "alias".to_string()).await.is_err());

    // Try to remove non-existent account
    assert!(registry.remove_account("nonexistent").await.unwrap().is_none());

    // Try to remove alias from non-existent account
    assert!(registry.remove_alias("nonexistent", "alias").await.is_err());

    if let Err(e) = std::fs::write(&log_path, "Registry error cases test completed\n") {
        panic!("Failed to write to test log {}: {}", log_path.display(), e);
    }

    assert!(
        log_path.exists(),
        "Test log file should exist at: {}",
        log_path.display()
    );
}

#[tokio::test]
async fn test_registry_with_aliases() {
    let log_path = ah_credentials::test_utils::setup_test_logging("test_registry_with_aliases");

    if let Err(e) = std::fs::write(&log_path, "Testing registry with aliases\n") {
        panic!("Failed to write to test log {}: {}", log_path.display(), e);
    }

    let temp_dir = tempfile::TempDir::new().unwrap();
    let config = CredentialsConfig {
        storage_path: Some(temp_dir.path().to_path_buf()),
        default_accounts: HashMap::new(),
        auto_verify: Default::default(),
        base_config_dir: None,
        ah_home_override: None,
    };

    let registry = AccountRegistry::new(config);

    // Create account with aliases
    let mut account = Account::new("primary".to_string(), AgentType::Codex);
    account.add_alias("secondary".to_string());
    account.add_alias("tertiary".to_string());

    registry.add_account(account).await.unwrap();

    // Should be findable by name and all aliases
    assert!(registry.find_account("primary").await.is_some());
    assert!(registry.find_account("secondary").await.is_some());
    assert!(registry.find_account("tertiary").await.is_some());

    // All identifiers should be taken
    assert!(!registry.is_identifier_available("primary").await);
    assert!(!registry.is_identifier_available("secondary").await);
    assert!(!registry.is_identifier_available("tertiary").await);

    // Test that we can't add duplicate aliases
    assert!(registry.add_alias("primary", "secondary".to_string()).await.is_err());
    assert!(registry.add_alias("primary", "new-alias".to_string()).await.is_ok());

    // Verify new alias was added
    let updated = registry.find_account("new-alias").await.unwrap();
    assert!(updated.has_alias("new-alias"));

    if let Err(e) = std::fs::write(&log_path, "Registry with aliases test completed\n") {
        panic!("Failed to write to test log {}: {}", log_path.display(), e);
    }

    assert!(
        log_path.exists(),
        "Test log file should exist at: {}",
        log_path.display()
    );
}

#[tokio::test]
async fn test_default_account_for_agent() {
    let log_path = ah_credentials::test_utils::setup_test_logging("test_default_account_for_agent");

    if let Err(e) = std::fs::write(&log_path, "Testing default account for agent\n") {
        panic!("Failed to write to test log {}: {}", log_path.display(), e);
    }

    let mut default_accounts = HashMap::new();
    default_accounts.insert("codex".to_string(), "work-account".to_string());
    default_accounts.insert("claude".to_string(), "personal-account".to_string());

    let config = CredentialsConfig {
        storage_path: None,
        default_accounts,
        auto_verify: Default::default(),
        base_config_dir: None,
        ah_home_override: None,
    };

    let registry = AccountRegistry::new(config);

    assert_eq!(
        registry.default_account_for_agent("codex"),
        Some("work-account")
    );
    assert_eq!(
        registry.default_account_for_agent("claude"),
        Some("personal-account")
    );
    assert_eq!(registry.default_account_for_agent("cursor"), None);

    if let Err(e) = std::fs::write(&log_path, "Default account for agent test completed\n") {
        panic!("Failed to write to test log {}: {}", log_path.display(), e);
    }

    assert!(
        log_path.exists(),
        "Test log file should exist at: {}",
        log_path.display()
    );
}
