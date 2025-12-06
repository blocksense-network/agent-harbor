// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Integration-like tests for the credentials CLI surface (milestone M4).
//! Each test writes its stdout to a unique log file to honor AGENTS guidelines.

use ah_credentials::{
    config::CredentialsConfig,
    storage::save_registry,
    types::{Account, AccountRegistry, AgentType},
};
use assert_cmd::prelude::*;
use std::process::Command;
use tempfile::TempDir;

fn config_for(home: &TempDir) -> CredentialsConfig {
    CredentialsConfig {
        storage_path: Some(home.path().join("credentials")),
        default_accounts: Default::default(),
        auto_verify: Default::default(),
        crypto: Default::default(),
        base_config_dir: None,
        ah_home_override: None,
    }
}

#[test]
fn list_compact_outputs_accounts() -> Result<(), Box<dyn std::error::Error>> {
    let home = TempDir::new()?;
    let cfg = config_for(&home);
    let mut registry = AccountRegistry::new();

    registry.add_account(Account::new("codex-work".to_string(), AgentType::Codex));
    registry.add_account(Account::new("cursor-old".to_string(), AgentType::Cursor));

    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async { save_registry(&cfg, &registry).await.unwrap() });

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("ah"));
    cmd.env("AH_HOME", home.path()).args(["credentials", "--compact", "list"]);
    let output = cmd.assert().success().get_output().stdout.clone();

    let log_path = home.path().join("log-credentials-list.txt");
    std::fs::write(&log_path, &output)?;

    let rendered = String::from_utf8_lossy(&output);
    assert!(rendered.contains("codex-work"));
    assert!(rendered.contains("cursor-old"));
    Ok(())
}

#[test]
fn encrypt_then_decrypt_round_trip() -> Result<(), Box<dyn std::error::Error>> {
    let home = TempDir::new()?;
    let cfg = config_for(&home);
    let mut registry = AccountRegistry::new();

    registry.add_account(Account::new("codex-secure".to_string(), AgentType::Codex));

    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async { save_registry(&cfg, &registry).await.unwrap() });

    let keys = cfg.keys_dir()?;
    std::fs::create_dir_all(&keys)?;
    std::fs::write(
        keys.join("codex-secure.json"),
        r#"{ "api_key": "sk-test-123" }"#,
    )?;

    let mut encrypt = Command::new(assert_cmd::cargo::cargo_bin!("ah"));
    encrypt.env("AH_HOME", home.path()).args([
        "credentials",
        "--json",
        "encrypt",
        "codex-secure",
        "--passphrase",
        "secret-pass",
    ]);
    let enc_out = encrypt.assert().success().get_output().stdout.clone();
    std::fs::write(home.path().join("log-credentials-encrypt.txt"), &enc_out)?;

    assert!(keys.join("codex-secure.enc").exists());
    assert!(
        !keys.join("codex-secure.json").exists(),
        "plaintext should be removed after encryption"
    );

    let mut decrypt = Command::new(assert_cmd::cargo::cargo_bin!("ah"));
    decrypt.env("AH_HOME", home.path()).args([
        "credentials",
        "decrypt",
        "codex-secure",
        "--passphrase",
        "secret-pass",
    ]);
    let dec_out = decrypt.assert().success().get_output().stdout.clone();
    std::fs::write(home.path().join("log-credentials-decrypt.txt"), &dec_out)?;

    assert!(
        keys.join("codex-secure.json").exists(),
        "plaintext should be restored after decrypt"
    );

    Ok(())
}
