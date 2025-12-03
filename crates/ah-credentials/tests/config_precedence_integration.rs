// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Integration tests for config precedence and AH_HOME override

use ah_credentials::config::CredentialsConfig;
use config_core::{load_all, paths::Paths};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

#[tokio::test]
async fn test_credentials_config_extraction_from_merged_config() {
    let log_path = ah_credentials::test_utils::setup_test_logging(
        "test_credentials_config_extraction_from_merged_config",
    );

    if let Err(e) = std::fs::write(
        &log_path,
        "Testing credentials config extraction from merged config\n",
    ) {
        panic!("Failed to write to test log {}: {}", log_path.display(), e);
    }
    let temp_dir = TempDir::new().unwrap();

    // Create a user config with credentials settings
    let user_config_path = temp_dir.path().join("user_config.toml");
    fs::write(
        &user_config_path,
        r#"
        [credentials]
        storage-path = "/custom/credentials/path"
        default-accounts = { codex = "work", claude = "personal" }

        [credentials.auto-verify]
        on-start = true
        interval = 3600
        "#,
    )
    .unwrap();

    // Create paths for config loading
    let paths = Paths {
        system: temp_dir.path().join("system.toml"),
        user: user_config_path,
        repo: None,
        repo_user: None,
        cli_config: None,
    };

    // Load and merge configuration
    let resolved = load_all(&paths, None).unwrap();

    // Extract credentials config from merged JSON using from_resolved_config
    // This demonstrates the real integration path where base_config_dir is set
    let credentials_config =
        CredentialsConfig::from_resolved_config(&resolved.json, temp_dir.path().to_path_buf())
            .unwrap();

    // Verify the config was extracted correctly
    assert_eq!(
        credentials_config.storage_path,
        Some("/custom/credentials/path".into())
    );
    assert_eq!(
        credentials_config.default_accounts.get("codex"),
        Some(&"work".to_string())
    );
    assert_eq!(
        credentials_config.default_accounts.get("claude"),
        Some(&"personal".to_string())
    );
    assert!(credentials_config.auto_verify.on_start);
    assert_eq!(credentials_config.auto_verify.interval, Some(3600));

    if let Err(e) = std::fs::write(&log_path, "Config extraction test completed successfully\n") {
        panic!("Failed to write to test log {}: {}", log_path.display(), e);
    }

    assert!(
        log_path.exists(),
        "Test log file should exist at: {}",
        log_path.display()
    );
}

#[tokio::test]
async fn test_config_precedence_user_over_system() {
    let log_path =
        ah_credentials::test_utils::setup_test_logging("test_config_precedence_user_over_system");

    if let Err(e) = std::fs::write(&log_path, "Testing config precedence: user over system\n") {
        panic!("Failed to write to test log {}: {}", log_path.display(), e);
    }

    let temp_dir = TempDir::new().unwrap();

    // Create system config
    let system_config_path = temp_dir.path().join("system_config.toml");
    fs::write(
        &system_config_path,
        r#"
        [credentials]
        storage-path = "/system/path"
        default-accounts = { codex = "system-default" }
        "#,
    )
    .unwrap();

    // Create user config that overrides
    let user_config_path = temp_dir.path().join("user_config.toml");
    fs::write(
        &user_config_path,
        r#"
        [credentials]
        storage-path = "/user/path"
        default-accounts = { codex = "user-default", claude = "user-claude" }
        "#,
    )
    .unwrap();

    let paths = Paths {
        system: system_config_path,
        user: user_config_path,
        repo: None,
        repo_user: None,
        cli_config: None,
    };

    let resolved = load_all(&paths, None).unwrap();
    let credentials_config: CredentialsConfig =
        config_core::extract::get_at(&resolved.json, "credentials").unwrap();

    // User config should override system config
    assert_eq!(credentials_config.storage_path, Some("/user/path".into()));
    assert_eq!(
        credentials_config.default_accounts.get("codex"),
        Some(&"user-default".to_string())
    );
    assert_eq!(
        credentials_config.default_accounts.get("claude"),
        Some(&"user-claude".to_string())
    );

    if let Err(e) = std::fs::write(&log_path, "User over system precedence test completed\n") {
        panic!("Failed to write to test log {}: {}", log_path.display(), e);
    }

    assert!(
        log_path.exists(),
        "Test log file should exist at: {}",
        log_path.display()
    );
}

#[tokio::test]
async fn test_config_precedence_repo_over_user() {
    let log_path =
        ah_credentials::test_utils::setup_test_logging("test_config_precedence_repo_over_user");

    if let Err(e) = std::fs::write(&log_path, "Testing config precedence: repo over user\n") {
        panic!("Failed to write to test log {}: {}", log_path.display(), e);
    }

    let temp_dir = TempDir::new().unwrap();
    let repo_root = temp_dir.path().join("repo");
    fs::create_dir(&repo_root).unwrap();

    // Create user config
    let user_config_path = temp_dir.path().join("user_config.toml");
    fs::write(
        &user_config_path,
        r#"
        [credentials]
        storage-path = "/user/path"
        "#,
    )
    .unwrap();

    // Create repo config that should override user
    let repo_config_path = repo_root.join(".agents").join("config.toml");
    fs::create_dir_all(repo_config_path.parent().unwrap()).unwrap();
    fs::write(
        &repo_config_path,
        r#"
        [credentials]
        storage-path = "/repo/path"
        "#,
    )
    .unwrap();

    let paths = Paths {
        system: temp_dir.path().join("system.toml"),
        user: user_config_path,
        repo: Some(repo_config_path),
        repo_user: None,
        cli_config: None,
    };

    let resolved = load_all(&paths, None).unwrap();
    let credentials_config: CredentialsConfig =
        config_core::extract::get_at(&resolved.json, "credentials").unwrap();

    // Repo config should override user config
    assert_eq!(credentials_config.storage_path, Some("/repo/path".into()));

    if let Err(e) = std::fs::write(&log_path, "Repo over user precedence test completed\n") {
        panic!("Failed to write to test log {}: {}", log_path.display(), e);
    }

    assert!(
        log_path.exists(),
        "Test log file should exist at: {}",
        log_path.display()
    );
}

#[tokio::test]
async fn test_ah_home_environment_override() {
    let log_path =
        ah_credentials::test_utils::setup_test_logging("test_ah_home_environment_override");

    if let Err(e) = std::fs::write(&log_path, "Testing AH_HOME environment override\n") {
        panic!("Failed to write to test log {}: {}", log_path.display(), e);
    }

    let temp_dir = TempDir::new().unwrap();
    let ah_home = temp_dir.path().join("ah-home");

    let config = CredentialsConfig {
        storage_path: None, // No custom storage path set
        default_accounts: HashMap::new(),
        auto_verify: Default::default(),
        base_config_dir: None,
        ah_home_override: Some(ah_home.clone()),
    };

    // The storage_dir method should use ah_home_override
    let storage_dir = config.storage_dir().unwrap();
    assert_eq!(storage_dir, ah_home.join("credentials"));

    if let Err(e) = std::fs::write(&log_path, "AH_HOME environment override test completed\n") {
        panic!("Failed to write to test log {}: {}", log_path.display(), e);
    }

    assert!(
        log_path.exists(),
        "Test log file should exist at: {}",
        log_path.display()
    );
}

#[tokio::test]
async fn test_ah_home_custom_storage_path_precedence() {
    let log_path = ah_credentials::test_utils::setup_test_logging(
        "test_ah_home_custom_storage_path_precedence",
    );

    if let Err(e) = std::fs::write(
        &log_path,
        "Testing custom storage path precedence over AH_HOME\n",
    ) {
        panic!("Failed to write to test log {}: {}", log_path.display(), e);
    }

    let temp_dir = TempDir::new().unwrap();
    let ah_home = temp_dir.path().join("ah-home");

    let config = CredentialsConfig {
        storage_path: Some("/explicit/custom/path".into()), // Explicit storage path
        default_accounts: HashMap::new(),
        auto_verify: Default::default(),
        base_config_dir: None,
        ah_home_override: Some(ah_home.clone()),
    };

    // Explicit storage_path should take precedence over AH_HOME override
    let storage_dir = config.storage_dir().unwrap();
    assert_eq!(storage_dir, PathBuf::from("/explicit/custom/path"));

    if let Err(e) = std::fs::write(&log_path, "Custom storage path precedence test completed\n") {
        panic!("Failed to write to test log {}: {}", log_path.display(), e);
    }

    assert!(
        log_path.exists(),
        "Test log file should exist at: {}",
        log_path.display()
    );
}

#[tokio::test]
async fn test_credentials_config_serialization() {
    let log_path =
        ah_credentials::test_utils::setup_test_logging("test_credentials_config_serialization");

    if let Err(e) = std::fs::write(&log_path, "Testing credentials config serialization\n") {
        panic!("Failed to write to test log {}: {}", log_path.display(), e);
    }

    let mut default_accounts = HashMap::new();
    default_accounts.insert("codex".to_string(), "work-account".to_string());
    default_accounts.insert("claude".to_string(), "personal-account".to_string());

    let config = CredentialsConfig {
        storage_path: Some("/test/path".into()),
        default_accounts,
        auto_verify: ah_credentials::config::AutoVerifyConfig {
            on_start: true,
            interval: Some(7200),
        },
        base_config_dir: None,
        ah_home_override: None,
    };

    // Serialize to JSON
    let json = serde_json::to_value(&config).unwrap();

    // Verify structure
    assert_eq!(json["storage-path"], "/test/path");
    assert_eq!(json["default-accounts"]["codex"], "work-account");
    assert_eq!(json["default-accounts"]["claude"], "personal-account");
    assert_eq!(json["auto-verify"]["on-start"], true);
    assert_eq!(json["auto-verify"]["interval"], 7200);

    // Deserialize back
    let deserialized: CredentialsConfig = serde_json::from_value(json).unwrap();
    assert_eq!(deserialized.storage_path, Some("/test/path".into()));
    assert!(deserialized.auto_verify.on_start);
    assert_eq!(deserialized.auto_verify.interval, Some(7200));

    if let Err(e) = std::fs::write(&log_path, "Config serialization test completed\n") {
        panic!("Failed to write to test log {}: {}", log_path.display(), e);
    }

    assert!(
        log_path.exists(),
        "Test log file should exist at: {}",
        log_path.display()
    );
}

#[tokio::test]
async fn test_credentials_config_default_values() {
    let log_path =
        ah_credentials::test_utils::setup_test_logging("test_credentials_config_default_values");

    if let Err(e) = std::fs::write(&log_path, "Testing credentials config default values\n") {
        panic!("Failed to write to test log {}: {}", log_path.display(), e);
    }

    let config = CredentialsConfig::default();

    assert!(config.storage_path.is_none());
    assert!(config.default_accounts.is_empty());
    assert!(!config.auto_verify.on_start);
    assert!(config.auto_verify.interval.is_none());

    if let Err(e) = std::fs::write(&log_path, "Config default values test completed\n") {
        panic!("Failed to write to test log {}: {}", log_path.display(), e);
    }

    assert!(
        log_path.exists(),
        "Test log file should exist at: {}",
        log_path.display()
    );
}

#[tokio::test]
async fn test_full_config_precedence_system_user_repo_ah_home() {
    let log_path = ah_credentials::test_utils::setup_test_logging(
        "test_full_config_precedence_system_user_repo_ah_home",
    );

    if let Err(e) = std::fs::write(
        &log_path,
        "Testing full config precedence with system/user/repo layering + AH_HOME\n",
    ) {
        panic!("Failed to write to test log {}: {}", log_path.display(), e);
    }

    let temp_dir = TempDir::new().unwrap();

    // Create system config (lowest precedence for base_config_dir)
    let system_config_path = temp_dir.path().join("system.toml");
    fs::write(
        &system_config_path,
        r#"
        [credentials]
        default-accounts = { codex = "system-default" }
        "#,
    )
    .unwrap();

    // Create user config (overrides system)
    let user_config_path = temp_dir.path().join("user.toml");
    fs::write(
        &user_config_path,
        r#"
        [credentials]
        default-accounts = { codex = "user-default" }
        "#,
    )
    .unwrap();

    // Create repo config (overrides user)
    let repo_config_path = temp_dir.path().join("repo.toml");
    fs::write(
        &repo_config_path,
        r#"
        [credentials]
        default-accounts = { codex = "repo-default" }
        "#,
    )
    .unwrap();

    // Set AH_HOME environment variable (should override base_config_dir)
    let ah_home_dir = temp_dir.path().join("ah-home");
    std::env::set_var("AH_HOME", ah_home_dir.to_string_lossy().to_string());

    // Create paths for config loading - note base_config_dir will be set to temp_dir
    let paths = Paths {
        system: system_config_path,
        user: user_config_path,
        repo: Some(repo_config_path),
        repo_user: None,
        cli_config: None,
    };

    // Load and merge configuration
    let resolved = load_all(&paths, None).unwrap();

    // Extract credentials config using from_resolved_config (real integration path)
    let credentials_config =
        CredentialsConfig::from_resolved_config(&resolved.json, temp_dir.path().to_path_buf())
            .unwrap();

    if let Err(e) = std::fs::write(
        &log_path,
        format!(
            "Resolved config storage_path: {:?}, base_config_dir: {:?}\n",
            credentials_config.storage_path, credentials_config.base_config_dir
        ),
    ) {
        panic!("Failed to write to test log {}: {}", log_path.display(), e);
    }

    // Verify precedence: AH_HOME should win over base_config_dir (which contains repo/user/system configs)
    let storage_dir = credentials_config.storage_dir().unwrap();
    assert_eq!(
        storage_dir,
        ah_home_dir.join("credentials"),
        "AH_HOME should take precedence over base_config_dir"
    );

    // Now test that storage_path wins over AH_HOME
    let config_with_explicit_path = CredentialsConfig {
        storage_path: Some("/explicit/custom/path".into()),
        default_accounts: HashMap::new(),
        auto_verify: Default::default(),
        base_config_dir: credentials_config.base_config_dir,
        ah_home_override: None,
    };

    let storage_dir_explicit = config_with_explicit_path.storage_dir().unwrap();
    assert_eq!(
        storage_dir_explicit,
        PathBuf::from("/explicit/custom/path"),
        "Explicit storage_path should take precedence over AH_HOME"
    );

    if let Err(e) = std::fs::write(&log_path, "Config precedence test completed successfully\n") {
        panic!("Failed to write to test log {}: {}", log_path.display(), e);
    }

    // Clean up
    std::env::remove_var("AH_HOME");
    assert!(
        log_path.exists(),
        "Test log file should exist at: {}",
        log_path.display()
    );
}
