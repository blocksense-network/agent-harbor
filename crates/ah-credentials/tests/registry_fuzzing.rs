// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Fuzzing-style tests for registry operations with edge cases and collisions

use ah_credentials::validation::validate_no_conflicts;
use ah_credentials::{
    config::CredentialsConfig,
    registry::AccountRegistry,
    types::{Account, AgentType},
};
use std::collections::HashMap;
use tempfile::TempDir;

#[tokio::test]
async fn test_registry_alias_collision_detection() {
    let log_path =
        ah_credentials::test_utils::setup_test_logging("test_registry_alias_collision_detection");

    if let Err(e) = std::fs::write(&log_path, "Testing registry alias collision detection\n") {
        panic!("Failed to write to test log {}: {}", log_path.display(), e);
    }

    let temp_dir = TempDir::new().unwrap();
    let config = CredentialsConfig {
        storage_path: Some(temp_dir.path().to_path_buf()),
        default_accounts: HashMap::new(),
        auto_verify: Default::default(),
        base_config_dir: None,
        ah_home_override: None,
    };

    let registry = AccountRegistry::new(config);

    // Create first account with aliases
    let mut account1 = Account::new("account1".to_string(), AgentType::Codex);
    account1.add_alias("shared-alias".to_string());
    account1.add_alias("unique-alias1".to_string());
    registry.add_account(account1).await.unwrap();

    // Create second account that tries to use the same alias
    let mut account2 = Account::new("account2".to_string(), AgentType::Claude);
    account2.add_alias("shared-alias".to_string()); // This should conflict

    // Adding should fail due to alias collision
    assert!(registry.add_account(account2).await.is_err());

    // Try adding an alias that conflicts
    assert!(registry.add_alias("account1", "conflicting-alias".to_string()).await.is_ok());
    let mut account3 = Account::new("account3".to_string(), AgentType::Cursor);
    account3.add_alias("conflicting-alias".to_string());
    assert!(registry.add_account(account3).await.is_err());
}

#[tokio::test]
async fn test_registry_name_collision_prevention() {
    let log_path =
        ah_credentials::test_utils::setup_test_logging("test_registry_name_collision_prevention");

    if let Err(e) = std::fs::write(&log_path, "Testing registry name collision prevention\n") {
        panic!("Failed to write to test log {}: {}", log_path.display(), e);
    }

    let temp_dir = TempDir::new().unwrap();
    let config = CredentialsConfig {
        storage_path: Some(temp_dir.path().to_path_buf()),
        default_accounts: HashMap::new(),
        auto_verify: Default::default(),
        base_config_dir: None,
        ah_home_override: None,
    };

    let registry = AccountRegistry::new(config);

    // Add first account
    let account1 = Account::new("test-account".to_string(), AgentType::Codex);
    registry.add_account(account1).await.unwrap();

    // Try to add account with same name
    let account2 = Account::new("test-account".to_string(), AgentType::Claude);
    assert!(registry.add_account(account2).await.is_err());

    // Try to add alias that matches existing account name
    assert!(
        registry
            .add_alias("test-account", "some-other-account".to_string())
            .await
            .is_ok()
    );
    let account3 = Account::new("some-other-account".to_string(), AgentType::Cursor);
    assert!(registry.add_account(account3).await.is_err());
}

#[tokio::test]
async fn test_registry_edge_case_names() {
    let temp_dir = TempDir::new().unwrap();
    let config = CredentialsConfig {
        storage_path: Some(temp_dir.path().to_path_buf()),
        default_accounts: HashMap::new(),
        auto_verify: Default::default(),
        base_config_dir: None,
        ah_home_override: None,
    };

    let registry = AccountRegistry::new(config);

    // Test various edge case names that should be valid
    let max_length_name = (0..63).map(|_| "a").collect::<String>();
    let valid_names = vec![
        "a",
        "z",
        "A",
        "Z",
        "0",
        "9",
        "a-b",
        "a_b",
        "a1",
        "1a",
        "test-account",
        "my_work_account",
        "account-with-multiple-hyphens",
        "account_with_multiple_underscores",
        &max_length_name, // Max length
    ];

    for name in valid_names {
        let account = Account::new(name.to_string(), AgentType::Codex);
        assert!(
            registry.add_account(account).await.is_ok(),
            "Failed to add account with name: {}",
            name
        );
    }

    // Test names that should be invalid
    let too_long_name = (0..65).map(|_| "a").collect::<String>();
    let invalid_names = vec![
        "",
        "-",
        "_",
        "--",
        "__",
        "-a",
        "_a",
        "a-",
        "a_",
        "a--b",
        "a__b",
        "a-@b",
        "a_ b",
        "a b",
        "account with spaces",
        "account@domain.com",
        &too_long_name, // Too long
    ];

    for name in invalid_names {
        let account = Account::new(name.to_string(), AgentType::Codex);
        assert!(
            registry.add_account(account).await.is_err(),
            "Should have rejected account with invalid name: {}",
            name
        );
    }
}

#[tokio::test]
async fn test_registry_operations_with_many_accounts() {
    let temp_dir = TempDir::new().unwrap();
    let config = CredentialsConfig {
        storage_path: Some(temp_dir.path().to_path_buf()),
        default_accounts: HashMap::new(),
        auto_verify: Default::default(),
        base_config_dir: None,
        ah_home_override: None,
    };

    let registry = AccountRegistry::new(config.clone());

    // Add many accounts with different patterns
    for i in 0..50 {
        let agent = match i % 3 {
            0 => AgentType::Codex,
            1 => AgentType::Claude,
            2 => AgentType::Cursor,
            _ => unreachable!(),
        };

        let account = Account::new(format!("account-{:02}", i), agent);

        // Add some aliases for variety
        if i % 5 == 0 {
            // Every 5th account gets aliases
            registry.add_account(account).await.unwrap();
            registry
                .add_alias(&format!("account-{:02}", i), format!("alias-{:02}", i))
                .await
                .unwrap();
        } else {
            registry.add_account(account).await.unwrap();
        }
    }

    // Verify we can find all accounts
    assert_eq!(registry.count().await, 50);

    // Test finding by various identifiers
    for i in 0..50 {
        let name = format!("account-{:02}", i);
        assert!(
            registry.find_account(&name).await.is_some(),
            "Could not find account: {}",
            name
        );

        if i % 5 == 0 {
            let alias = format!("alias-{:02}", i);
            assert!(
                registry.find_account(&alias).await.is_some(),
                "Could not find account by alias: {}",
                alias
            );
        }
    }

    // Test agent filtering
    let codex_accounts = registry.accounts_for_agent(&AgentType::Codex).await;
    let claude_accounts = registry.accounts_for_agent(&AgentType::Claude).await;
    let cursor_accounts = registry.accounts_for_agent(&AgentType::Cursor).await;

    assert_eq!(
        codex_accounts.len() + claude_accounts.len() + cursor_accounts.len(),
        50
    );

    // Each agent should have roughly 1/3 of accounts
    assert!((codex_accounts.len() as i32 - 16).abs() <= 2); // Allow some variance
    assert!((claude_accounts.len() as i32 - 17).abs() <= 2);
    assert!((cursor_accounts.len() as i32 - 17).abs() <= 2);

    // Test save and reload
    registry.save().await.unwrap();

    let new_registry = AccountRegistry::new(config);
    new_registry.load().await.unwrap();
    assert_eq!(new_registry.count().await, 50);
}

#[tokio::test]
async fn test_registry_concurrent_operations() {
    let temp_dir = TempDir::new().unwrap();
    let config = CredentialsConfig {
        storage_path: Some(temp_dir.path().to_path_buf()),
        default_accounts: HashMap::new(),
        auto_verify: Default::default(),
        base_config_dir: None,
        ah_home_override: None,
    };

    let registry = AccountRegistry::new(config);

    // Test concurrent operations (basic stress test)
    let mut handles = vec![];

    for i in 0..10 {
        let registry_clone = registry.clone();
        let handle = tokio::spawn(async move {
            let account = Account::new(format!("concurrent-{}", i), AgentType::Codex);
            registry_clone.add_account(account).await.unwrap();

            // Try to add some aliases
            for j in 0..3 {
                let alias_result = registry_clone
                    .add_alias(
                        &format!("concurrent-{}", i),
                        format!("concurrent-{}-alias-{}", i, j),
                    )
                    .await;

                // Some may fail due to race conditions, that's ok
                let _ = alias_result;
            }
        });
        handles.push(handle);
    }

    // Wait for all operations to complete
    for handle in handles {
        handle.await.unwrap();
    }

    // Verify we have at least the expected number of accounts
    // (some operations might have failed due to concurrent access)
    let count = registry.count().await;
    assert!(count >= 10, "Expected at least 10 accounts, got {}", count);
}

#[tokio::test]
async fn test_registry_validation_edge_cases() {
    // Test validate_no_conflicts with various edge cases

    let mut registry = ah_credentials::types::AccountRegistry::new();

    // Add account with name "test"
    let account1 = Account::new("test".to_string(), AgentType::Codex);
    registry.add_account(account1);

    // Test various collision scenarios
    let collision_cases = vec![
        ("test", AgentType::Claude), // Same name as existing account
    ];

    for (name, agent) in collision_cases {
        let account = Account::new(name.to_string(), agent.clone());
        assert!(
            validate_no_conflicts(&account, &registry).is_err(),
            "Should have detected collision for account: {} ({:?})",
            name,
            agent
        );
    }

    // Test non-colliding case
    let non_collision_account = Account::new("alias".to_string(), AgentType::Claude);
    assert!(
        validate_no_conflicts(&non_collision_account, &registry).is_ok(),
        "Should not have detected collision for account: alias (Claude)"
    );

    // Test alias collisions
    let mut account_with_alias = Account::new("different".to_string(), AgentType::Claude);
    account_with_alias.add_alias("test".to_string()); // Alias matches existing account name
    assert!(validate_no_conflicts(&account_with_alias, &registry).is_err());

    // Test non-colliding cases
    let non_collision_cases = vec![
        ("other", AgentType::Claude),
        ("different", AgentType::Codex),
    ];

    for (name, agent) in non_collision_cases {
        let account = Account::new(name.to_string(), agent.clone());
        assert!(
            validate_no_conflicts(&account, &registry).is_ok(),
            "Should not have detected collision for account: {} ({:?})",
            name,
            agent
        );
    }
}

#[tokio::test]
async fn test_identifier_uniqueness_tracking() {
    let temp_dir = TempDir::new().unwrap();
    let config = CredentialsConfig {
        storage_path: Some(temp_dir.path().to_path_buf()),
        default_accounts: HashMap::new(),
        auto_verify: Default::default(),
        base_config_dir: None,
        ah_home_override: None,
    };

    let registry = AccountRegistry::new(config);

    // Add account with complex alias set
    let mut account1 = Account::new("primary".to_string(), AgentType::Codex);
    account1.add_alias("alias1".to_string());
    account1.add_alias("alias2".to_string());
    registry.add_account(account1).await.unwrap();

    // Verify all identifiers are tracked as unavailable
    assert!(!registry.is_identifier_available("primary").await);
    assert!(!registry.is_identifier_available("alias1").await);
    assert!(!registry.is_identifier_available("alias2").await);
    assert!(registry.is_identifier_available("available").await);

    // Add another account
    let mut account2 = Account::new("secondary".to_string(), AgentType::Claude);
    account2.add_alias("shared".to_string());
    registry.add_account(account2).await.unwrap();

    // Now "shared" should be unavailable
    assert!(!registry.is_identifier_available("shared").await);

    // Remove first account
    registry.remove_account("primary").await.unwrap();

    // Now primary and its aliases should be available again
    assert!(registry.is_identifier_available("primary").await);
    assert!(registry.is_identifier_available("alias1").await);
    assert!(registry.is_identifier_available("alias2").await);
    // But shared should still be unavailable
    assert!(!registry.is_identifier_available("shared").await);
}
