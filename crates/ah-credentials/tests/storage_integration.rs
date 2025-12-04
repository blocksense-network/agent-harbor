// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Integration tests for storage functionality

use ah_credentials::{
    config::CredentialsConfig,
    crypto::KdfParams,
    storage::{
        ensure_credentials_dirs, load_registry, read_account_credentials, save_registry,
        validate_permissions, write_account_credentials, write_credential_file,
    },
    test_utils,
    types::{Account, AccountRegistry, AgentType},
};
use std::collections::HashMap;
use tempfile::TempDir;

#[tokio::test]
async fn test_ensure_credentials_dirs() {
    let log_path = test_utils::setup_test_logging("test_ensure_credentials_dirs");

    // Write to log file for this test
    if let Err(e) = std::fs::write(&log_path, "Starting directory creation test\n") {
        panic!("Failed to write to test log {}: {}", log_path.display(), e);
    }

    // Assert log file exists (per testing guidelines)
    assert!(
        log_path.exists(),
        "Test log file should exist at: {}",
        log_path.display()
    );

    let temp_dir = TempDir::new().unwrap();
    let config = CredentialsConfig {
        storage_path: Some(temp_dir.path().to_path_buf()),
        default_accounts: HashMap::new(),
        auto_verify: Default::default(),
        crypto: Default::default(),
        base_config_dir: None,
        ah_home_override: None,
    };

    // Ensure directories are created
    ensure_credentials_dirs(&config).await.unwrap();

    // Write completion to log
    if let Err(e) = std::fs::write(&log_path, "Directory creation completed successfully\n") {
        panic!(
            "Failed to write completion to test log {}: {}",
            log_path.display(),
            e
        );
    }

    // Check that directories exist
    assert!(config.storage_dir().unwrap().exists());
    assert!(config.keys_dir().unwrap().exists());
    assert!(config.temp_dir().unwrap().exists());

    // Verify log file still exists after test
    assert!(
        log_path.exists(),
        "Test log file should still exist after test at: {}",
        log_path.display()
    );
}

#[tokio::test]
async fn test_schema_validation_rejects_malformed_data() {
    let log_path = test_utils::setup_test_logging("test_schema_validation_rejects_malformed_data");

    if let Err(e) = std::fs::write(&log_path, "Testing schema validation rejection\n") {
        panic!("Failed to write to test log {}: {}", log_path.display(), e);
    }

    let temp_dir = TempDir::new().unwrap();

    // Manually create an invalid accounts.toml file with malformed structure
    let accounts_file = temp_dir.path().join("accounts.toml");
    std::fs::write(
        &accounts_file,
        r#"
        # Invalid TOML with malformed structure - missing required fields
        [[accounts]]
        name = "test"
        # Missing required 'agent' field
        unknown_extra_field = "should fail schema validation"

        [[accounts]]
        name = ""
        agent = "codex"
        # Empty name should fail validation
        "#,
    )
    .unwrap();

    let config = CredentialsConfig {
        storage_path: Some(temp_dir.path().to_path_buf()),
        default_accounts: HashMap::new(),
        auto_verify: Default::default(),
        crypto: Default::default(),
        base_config_dir: None,
        ah_home_override: None,
    };

    // Loading should fail due to schema validation
    let result = load_registry(&config).await;
    assert!(
        result.is_err(),
        "Schema validation should have rejected malformed data"
    );

    // Log the successful rejection
    if let Err(e) = std::fs::write(
        &log_path,
        "Schema validation correctly rejected malformed data\n",
    ) {
        panic!(
            "Failed to write completion to test log {}: {}",
            log_path.display(),
            e
        );
    }

    assert!(
        log_path.exists(),
        "Test log file should exist at: {}",
        log_path.display()
    );
}

#[tokio::test]
async fn test_registry_save_load() {
    let temp_dir = TempDir::new().unwrap();
    let config = CredentialsConfig {
        storage_path: Some(temp_dir.path().to_path_buf()),
        default_accounts: HashMap::new(),
        auto_verify: Default::default(),
        crypto: Default::default(),
        base_config_dir: None,
        ah_home_override: None,
    };

    // Create a registry with some accounts
    let mut registry = AccountRegistry::new();
    let mut account1 = Account::new("test-codex".to_string(), AgentType::Codex);
    account1.add_alias("work".to_string());
    account1.encrypted = true;

    let account2 = Account::new("test-claude".to_string(), AgentType::Claude);

    registry.add_account(account1);
    registry.add_account(account2);

    // Save the registry
    save_registry(&config, &registry).await.unwrap();

    // Load it back
    let loaded_registry = load_registry(&config).await.unwrap();

    // Verify contents
    assert_eq!(loaded_registry.len(), 2);

    let loaded_account1 = loaded_registry.find_account("test-codex").unwrap();
    assert_eq!(loaded_account1.name, "test-codex");
    assert_eq!(loaded_account1.agent, AgentType::Codex);
    assert_eq!(loaded_account1.aliases, vec!["work"]);
    assert!(loaded_account1.encrypted);

    let loaded_account2 = loaded_registry.find_account("test-claude").unwrap();
    assert_eq!(loaded_account2.name, "test-claude");
    assert_eq!(loaded_account2.agent, AgentType::Claude);
    assert!(!loaded_account2.encrypted);
}

#[tokio::test]
async fn test_empty_registry_save_load() {
    let temp_dir = TempDir::new().unwrap();
    let config = CredentialsConfig {
        storage_path: Some(temp_dir.path().to_path_buf()),
        default_accounts: HashMap::new(),
        auto_verify: Default::default(),
        crypto: Default::default(),
        base_config_dir: None,
        ah_home_override: None,
    };

    // Save empty registry
    let registry = AccountRegistry::new();
    save_registry(&config, &registry).await.unwrap();

    // Load it back
    let loaded_registry = load_registry(&config).await.unwrap();
    assert!(loaded_registry.is_empty());
}

#[tokio::test]
async fn test_registry_load_nonexistent_file() {
    let temp_dir = TempDir::new().unwrap();
    let config = CredentialsConfig {
        storage_path: Some(temp_dir.path().to_path_buf()),
        default_accounts: HashMap::new(),
        auto_verify: Default::default(),
        crypto: Default::default(),
        base_config_dir: None,
        ah_home_override: None,
    };

    // Load from non-existent file should return empty registry
    let loaded_registry = load_registry(&config).await.unwrap();
    assert!(loaded_registry.is_empty());
}

#[tokio::test]
async fn test_plain_and_encrypted_credentials_coexist() {
    let log_path = test_utils::setup_test_logging("test_plain_and_encrypted_credentials_coexist");

    let temp_dir = TempDir::new().unwrap();
    let config = CredentialsConfig {
        storage_path: Some(temp_dir.path().to_path_buf()),
        default_accounts: HashMap::new(),
        auto_verify: Default::default(),
        crypto: Default::default(),
        base_config_dir: None,
        ah_home_override: None,
    };

    let mut plain = Account::new("plain".into(), AgentType::Codex);
    plain.encrypted = false;
    let mut enc = Account::new("enc".into(), AgentType::Claude);
    enc.encrypted = true;

    let plain_data = serde_json::json!({"token": "plain-token"});
    let enc_data = serde_json::json!({"token": "secret-token"});

    // Write both accounts
    write_account_credentials(&config, &plain, &plain_data, None, None)
        .await
        .unwrap();
    write_account_credentials(
        &config,
        &enc,
        &enc_data,
        Some("pass"),
        Some(KdfParams::secure_defaults().unwrap()),
    )
    .await
    .unwrap();

    // Ensure encrypted file does not contain plaintext token
    let enc_path = config.keys_dir().unwrap().join("enc.enc");
    let enc_contents = std::fs::read(&enc_path).unwrap();
    assert!(!String::from_utf8_lossy(&enc_contents).contains("secret-token"));

    // Plain file should include plaintext
    let plain_path = config.keys_dir().unwrap().join("plain.json");
    let plain_contents = std::fs::read(&plain_path).unwrap();
    assert!(String::from_utf8_lossy(&plain_contents).contains("plain-token"));

    // Read back both
    let loaded_plain = read_account_credentials(&config, &plain, None).await.unwrap();
    let loaded_enc = read_account_credentials(&config, &enc, Some("pass")).await.unwrap();

    assert_eq!(plain_data, loaded_plain);
    assert_eq!(enc_data, loaded_enc);

    assert!(log_path.exists());
}

#[cfg(unix)]
#[tokio::test]
async fn test_directory_permissions() {
    use std::os::unix::fs::PermissionsExt;

    let temp_dir = TempDir::new().unwrap();
    let config = CredentialsConfig {
        storage_path: Some(temp_dir.path().to_path_buf()),
        default_accounts: HashMap::new(),
        auto_verify: Default::default(),
        crypto: Default::default(),
        base_config_dir: None,
        ah_home_override: None,
    };

    // Create directories
    ensure_credentials_dirs(&config).await.unwrap();

    // Check permissions
    let storage_dir = config.storage_dir().unwrap();
    let keys_dir = config.keys_dir().unwrap();
    let temp_dir_path = config.temp_dir().unwrap();

    let storage_perms = std::fs::metadata(&storage_dir).unwrap().permissions();
    let keys_perms = std::fs::metadata(&keys_dir).unwrap().permissions();
    let temp_perms = std::fs::metadata(&temp_dir_path).unwrap().permissions();

    // All should have 0700 permissions (owner read/write/execute only)
    assert_eq!(storage_perms.mode() & 0o777, 0o700);
    assert_eq!(keys_perms.mode() & 0o777, 0o700);
    assert_eq!(temp_perms.mode() & 0o777, 0o700);
}

#[tokio::test]
async fn test_validate_permissions_success() {
    let temp_dir = TempDir::new().unwrap();
    let config = CredentialsConfig {
        storage_path: Some(temp_dir.path().to_path_buf()),
        default_accounts: HashMap::new(),
        auto_verify: Default::default(),
        crypto: Default::default(),
        base_config_dir: None,
        ah_home_override: None,
    };

    // Create directories
    ensure_credentials_dirs(&config).await.unwrap();

    // Validate permissions should succeed
    validate_permissions(&config).await.unwrap();
}

#[cfg(unix)]
#[tokio::test]
async fn test_file_permissions_accounts_toml() {
    use std::os::unix::fs::PermissionsExt;

    let temp_dir = TempDir::new().unwrap();
    let config = CredentialsConfig {
        storage_path: Some(temp_dir.path().to_path_buf()),
        default_accounts: HashMap::new(),
        auto_verify: Default::default(),
        crypto: Default::default(),
        base_config_dir: None,
        ah_home_override: None,
    };

    // Create directories first
    ensure_credentials_dirs(&config).await.unwrap();

    // Create a registry and save it
    let mut registry = AccountRegistry::new();
    let account = Account::new("test-account".to_string(), AgentType::Codex);
    registry.add_account(account);

    save_registry(&config, &registry).await.unwrap();

    // Check that accounts.toml has correct permissions (0600)
    let accounts_file = config.accounts_file().unwrap();
    assert!(accounts_file.exists());

    let file_perms = std::fs::metadata(&accounts_file).unwrap().permissions();
    assert_eq!(file_perms.mode() & 0o777, 0o600);
}

#[cfg(unix)]
#[tokio::test]
async fn test_credential_file_permissions_and_validation() {
    use std::os::unix::fs::PermissionsExt;

    let log_path =
        test_utils::setup_test_logging("test_credential_file_permissions_and_validation");
    if let Err(e) = std::fs::write(&log_path, "Testing credential file permissions\n") {
        panic!("Failed to write to test log {}: {}", log_path.display(), e);
    }

    let temp_dir = TempDir::new().unwrap();
    let config = CredentialsConfig {
        storage_path: Some(temp_dir.path().to_path_buf()),
        default_accounts: HashMap::new(),
        auto_verify: Default::default(),
        crypto: Default::default(),
        base_config_dir: None,
        ah_home_override: None,
    };

    // Create credential file (plaintext for test)
    let path = write_credential_file(&config, "perm-test", false, br#"{ "token": "abc" }"#)
        .await
        .unwrap();

    // Verify permissions 0600
    let perms = std::fs::metadata(&path).unwrap().permissions();
    assert_eq!(perms.mode() & 0o777, 0o600);

    // validate_permissions should pass with credential files present
    validate_permissions(&config).await.unwrap();

    if let Err(e) = std::fs::write(&log_path, "Credential file permission test completed\n") {
        panic!("Failed to write to test log {}: {}", log_path.display(), e);
    }
}

#[tokio::test]
async fn test_stale_metadata_cleanup_on_load() {
    use chrono::{Duration, Utc};

    let log_path = test_utils::setup_test_logging("test_stale_metadata_cleanup_on_load");
    if let Err(e) = std::fs::write(&log_path, "Testing stale metadata cleanup during load\n") {
        panic!("Failed to write to test log {}: {}", log_path.display(), e);
    }

    let temp_dir = TempDir::new().unwrap();
    let config = CredentialsConfig {
        storage_path: Some(temp_dir.path().to_path_buf()),
        default_accounts: HashMap::new(),
        auto_verify: Default::default(),
        crypto: Default::default(),
        base_config_dir: None,
        ah_home_override: None,
    };

    // Build registry with a stale error account
    let mut registry = AccountRegistry::new();
    let mut account = Account::new("stale-account".to_string(), AgentType::Codex);
    account.status = ah_credentials::types::AccountStatus::Error;
    account.last_used = Utc::now() - Duration::days(120);
    registry.add_account(account);

    save_registry(&config, &registry).await.unwrap();

    // Loading should prune the stale account
    let loaded = load_registry(&config).await.unwrap();
    assert!(
        loaded.is_empty(),
        "Stale account should have been removed on load"
    );

    if let Err(e) = std::fs::write(&log_path, "Stale metadata cleanup test completed\n") {
        panic!("Failed to write to test log {}: {}", log_path.display(), e);
    }
}
