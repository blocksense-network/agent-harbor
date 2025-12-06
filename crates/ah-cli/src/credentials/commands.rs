// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! CLI surface for credentials management commands (milestone M4).
#![allow(clippy::disallowed_methods)] // CLI is allowed to print to stdout/stderr

use crate::config::{self, ConfigResult};
use ah_agents::{
    AcquisitionOptions, CredentialAcquirer, claude::ClaudeAgent, codex::CodexAgent,
    cursor::CursorAgent,
};
use ah_credentials::{
    AccountRegistry, AccountStatus, AgentType, CredentialsConfig,
    storage::{
        cleanup_temp_dirs, credential_file_path, read_account_credentials,
        write_account_credentials,
    },
    validation::validate_account_name,
};
use anyhow::Result;
use chrono::Utc;
use clap::{Args, Subcommand, ValueEnum};
use crossterm::style::Stylize;
use serde_json::json;
use std::collections::BTreeMap;
use std::io::Write;

#[derive(Debug, Clone, Args)]
pub struct CredentialsArgs {
    /// Emit JSON instead of human output
    #[arg(long)]
    pub json: bool,

    /// Render compact human output
    #[arg(long)]
    pub compact: bool,

    #[command(subcommand)]
    pub command: CredentialCommands,
}

#[derive(Debug, Clone, Subcommand)]
pub enum CredentialCommands {
    /// Acquire and store credentials for an agent
    Add {
        /// Agent to acquire credentials for
        #[arg(value_enum)]
        agent: AgentKind,
        /// Account name/label (auto-generated when omitted)
        name: Option<String>,
        /// Store credentials encrypted at rest
        #[arg(long)]
        encrypted: bool,
        /// Passphrase to use for encryption (falls back to prompt)
        #[arg(long)]
        passphrase: Option<String>,
    },
    /// List stored credential accounts
    List,
    /// Remove an account and its credentials
    Remove {
        /// Account name or alias
        account: String,
    },
    /// Verify an account by reading its stored credentials
    Verify {
        /// Account name or alias
        account: String,
        /// Passphrase for encrypted accounts
        #[arg(long)]
        passphrase: Option<String>,
    },
    /// Re-authenticate an account by re-running acquisition
    Reauth {
        /// Account name or alias
        account: String,
        /// Passphrase for encrypted accounts
        #[arg(long)]
        passphrase: Option<String>,
    },
    /// Encrypt an existing plaintext account
    Encrypt {
        /// Account name or alias
        account: String,
        /// Passphrase to encrypt with
        #[arg(long)]
        passphrase: Option<String>,
    },
    /// Decrypt an encrypted account into plaintext
    Decrypt {
        /// Account name or alias
        account: String,
        /// Passphrase to decrypt with
        #[arg(long)]
        passphrase: Option<String>,
    },
    /// Show encryption status for accounts
    EncryptStatus {
        /// Optional account filter
        account: Option<String>,
    },
}

#[derive(Debug, Clone, ValueEnum)]
pub enum AgentKind {
    Codex,
    Claude,
    Cursor,
}

impl CredentialsArgs {
    /// Execute the credential command set.
    pub async fn run(self, config: &ConfigResult) -> Result<()> {
        let mut credentials_config = CredentialsConfig::from_resolved_config(
            &config.resolved_json,
            config::base_config_dir(&config.paths),
        )?;

        // Ensure AH_HOME override in tests still works for storage resolution
        if credentials_config.base_config_dir.is_none() {
            credentials_config.base_config_dir = Some(config::base_config_dir(&config.paths));
        }

        match self.command {
            CredentialCommands::Add {
                agent,
                name,
                encrypted,
                passphrase,
            } => {
                let pass = maybe_prompt_passphrase(passphrase, encrypted, "Enter passphrase: ")?;
                add_account(agent, name, encrypted, pass.as_deref(), &credentials_config).await
            }
            CredentialCommands::List => {
                list_accounts(&credentials_config, self.json, self.compact).await
            }
            CredentialCommands::Remove { account } => {
                remove_account(&credentials_config, &account, self.json).await
            }
            CredentialCommands::Verify {
                account,
                passphrase,
            } => {
                verify_account(
                    &credentials_config,
                    &account,
                    passphrase.as_deref(),
                    self.json,
                )
                .await
            }
            CredentialCommands::Reauth {
                account,
                passphrase,
            } => {
                reauth_account(
                    &credentials_config,
                    &account,
                    passphrase.as_deref(),
                    self.json,
                )
                .await
            }
            CredentialCommands::Encrypt {
                account,
                passphrase,
            } => {
                let pass = maybe_prompt_passphrase(passphrase, true, "Encryption passphrase: ")?;
                encrypt_account(&credentials_config, &account, pass.as_deref(), self.json).await
            }
            CredentialCommands::Decrypt {
                account,
                passphrase,
            } => {
                let pass = maybe_prompt_passphrase(passphrase, true, "Decryption passphrase: ")?;
                decrypt_account(&credentials_config, &account, pass.as_deref(), self.json).await
            }
            CredentialCommands::EncryptStatus { account } => {
                encrypt_status(
                    &credentials_config,
                    account.as_deref(),
                    self.json,
                    self.compact,
                )
                .await
            }
        }
    }
}

fn maybe_prompt_passphrase(
    provided: Option<String>,
    needed: bool,
    prompt: &str,
) -> Result<Option<String>> {
    if !needed {
        return Ok(None);
    }

    if let Some(pass) = provided {
        return Ok(Some(pass));
    }

    // Fallback to simple stdin prompt (echoed); avoids extra deps.
    eprint!("{}", prompt);
    std::io::stderr().flush()?;
    let mut buf = String::new();
    std::io::stdin().read_line(&mut buf)?;
    Ok(Some(buf.trim_end().to_string()))
}

async fn add_account(
    agent: AgentKind,
    name: Option<String>,
    encrypted: bool,
    passphrase: Option<&str>,
    config: &CredentialsConfig,
) -> Result<()> {
    let registry = AccountRegistry::new(config.clone());
    registry.load().await?;

    let account_name = name.unwrap_or_else(|| auto_name(&agent));
    validate_account_name(&account_name)
        .map_err(|e| anyhow::anyhow!("Invalid account name '{}': {}", account_name, e))?;

    if !registry.is_identifier_available(&account_name).await {
        anyhow::bail!("Account or alias '{}' already exists", account_name);
    }

    let acquisition = match agent {
        AgentKind::Codex => {
            acquire(
                &CodexAgent::new(),
                &account_name,
                encrypted,
                passphrase,
                config,
            )
            .await?
        }
        AgentKind::Claude => {
            acquire(
                &ClaudeAgent::new(),
                &account_name,
                encrypted,
                passphrase,
                config,
            )
            .await?
        }
        AgentKind::Cursor => {
            acquire(
                &CursorAgent::new(),
                &account_name,
                encrypted,
                passphrase,
                config,
            )
            .await?
        }
    };

    print_add_output(&acquisition.account, &acquisition.credential_file, false);
    Ok(())
}

async fn acquire<A: CredentialAcquirer>(
    acquirer: &A,
    account_name: &str,
    encrypted: bool,
    passphrase: Option<&str>,
    config: &CredentialsConfig,
) -> Result<crate::credentials::StoredAcquisition> {
    let service = crate::credentials::AcquisitionService::new(config.clone());
    let opts = AcquisitionOptions::with_temp_root(config.temp_dir()?);
    let stored = service
        .acquire_and_store(acquirer, account_name, encrypted, passphrase, Some(opts))
        .await?;

    // Cleanup stale temp dirs opportunistically
    let _ = cleanup_temp_dirs(config, 24 * 3600).await;
    Ok(stored)
}

async fn list_accounts(config: &CredentialsConfig, json: bool, compact: bool) -> Result<()> {
    let registry = load_registry(config).await?;
    let accounts = registry.all_accounts().await;

    if json {
        let payload = json!({ "accounts": accounts });
        println!("{}", serde_json::to_string_pretty(&payload)?);
        return Ok(());
    }

    if accounts.is_empty() {
        println!("No stored credentials.");
        return Ok(());
    }

    let mut grouped: BTreeMap<String, Vec<_>> = BTreeMap::new();
    for acct in &accounts {
        let key = format!("{:?}", acct.agent);
        grouped.entry(key).or_default().push(acct);
    }

    for (agent, list) in grouped {
        println!("{} Accounts:", agent);
        for acct in list {
            if compact {
                println!("- {} [{}]", acct.name, status_label(&acct.status, true));
            } else {
                println!(
                    "  {} {} (aliases: {})",
                    acct.name,
                    status_label(&acct.status, false),
                    if acct.aliases.is_empty() {
                        "-".to_string()
                    } else {
                        acct.aliases.join(", ")
                    }
                );
            }
        }
        println!();
    }

    Ok(())
}

async fn remove_account(config: &CredentialsConfig, identifier: &str, json: bool) -> Result<()> {
    let registry = AccountRegistry::new(config.clone());
    registry.load().await?;

    let target = registry
        .find_account(identifier)
        .await
        .ok_or_else(|| anyhow::anyhow!("Account '{}' not found", identifier))?;

    registry
        .remove_account(&target.name)
        .await?
        .ok_or_else(|| anyhow::anyhow!("Account '{}' not found", identifier))?;
    registry.save().await?;

    // Remove credential files (both enc/plain if present)
    for encrypted in [true, false] {
        let path = credential_file_path(config, &target.name, encrypted)?;
        if path.exists() {
            let _ = tokio::fs::remove_file(&path).await;
        }
    }

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({ "removed": target.name }))?
        );
    } else {
        println!("Removed credentials for account '{}'", target.name);
    }
    Ok(())
}

async fn verify_account(
    config: &CredentialsConfig,
    identifier: &str,
    passphrase: Option<&str>,
    json: bool,
) -> Result<()> {
    let registry = AccountRegistry::new(config.clone());
    registry.load().await?;

    let mut account = registry
        .find_account(identifier)
        .await
        .ok_or_else(|| anyhow::anyhow!("Account '{}' not found", identifier))?;

    let data = read_account_credentials(config, &account, passphrase).await?;
    // A simple verification: ensure we can parse JSON and bump last_used/status.
    if data.is_object() {
        account.status = AccountStatus::Active;
        account.last_used = Utc::now();
        registry.remove_account(&account.name).await?;
        registry.add_account(account.clone()).await?;
        registry.save().await?;

        if json {
            println!(
                "{}",
                serde_json::to_string_pretty(&json!({
                    "account": account.name,
                    "status": "active"
                }))?
            );
        } else {
            println!("{} verified successfully", account.name.as_str().green());
        }
        Ok(())
    } else {
        anyhow::bail!(
            "Credential payload for '{}' is not valid JSON object",
            account.name
        );
    }
}

async fn reauth_account(
    config: &CredentialsConfig,
    identifier: &str,
    passphrase: Option<&str>,
    json: bool,
) -> Result<()> {
    let registry = AccountRegistry::new(config.clone());
    registry.load().await?;

    let existing = registry
        .find_account(identifier)
        .await
        .ok_or_else(|| anyhow::anyhow!("Account '{}' not found", identifier))?;

    let acq = match existing.agent {
        AgentType::Codex => {
            acquire(
                &CodexAgent::new(),
                &existing.name,
                existing.encrypted,
                passphrase,
                config,
            )
            .await?
        }
        AgentType::Claude => {
            acquire(
                &ClaudeAgent::new(),
                &existing.name,
                existing.encrypted,
                passphrase,
                config,
            )
            .await?
        }
        AgentType::Cursor => {
            acquire(
                &CursorAgent::new(),
                &existing.name,
                existing.encrypted,
                passphrase,
                config,
            )
            .await?
        }
    };

    let mut updated = existing.clone();
    updated.status = if acq.acquisition.is_expired() {
        AccountStatus::Expired
    } else {
        AccountStatus::Active
    };
    updated.last_used = Utc::now();

    registry.remove_account(&existing.name).await?;
    registry.add_account(updated.clone()).await?;
    registry.save().await?;

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "account": updated.name,
                "status": format!("{:?}", updated.status)
            }))?
        );
    } else {
        println!("Re-authenticated {}", updated.name.as_str().green());
    }
    Ok(())
}

async fn encrypt_account(
    config: &CredentialsConfig,
    identifier: &str,
    passphrase: Option<&str>,
    json: bool,
) -> Result<()> {
    let registry = AccountRegistry::new(config.clone());
    registry.load().await?;

    let mut account = registry
        .find_account(identifier)
        .await
        .ok_or_else(|| anyhow::anyhow!("Account '{}' not found", identifier))?;

    if account.encrypted {
        anyhow::bail!("Account '{}' is already encrypted", account.name);
    }

    let data = read_account_credentials(config, &account, None).await?;
    account.encrypted = true;
    let kdf = Some(config.kdf_params()?);
    write_account_credentials(config, &account, &data, passphrase, kdf).await?;

    // Remove plaintext
    let plain = credential_file_path(config, &account.name, false)?;
    if plain.exists() {
        let _ = tokio::fs::remove_file(&plain).await;
    }

    registry.remove_account(&account.name).await?;
    registry.add_account(account.clone()).await?;
    registry.save().await?;

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "account": account.name,
                "encrypted": true
            }))?
        );
    } else {
        println!("Encrypted account {}", account.name.as_str().green());
    }
    Ok(())
}

async fn decrypt_account(
    config: &CredentialsConfig,
    identifier: &str,
    passphrase: Option<&str>,
    json: bool,
) -> Result<()> {
    let registry = AccountRegistry::new(config.clone());
    registry.load().await?;

    let mut account = registry
        .find_account(identifier)
        .await
        .ok_or_else(|| anyhow::anyhow!("Account '{}' not found", identifier))?;

    if !account.encrypted {
        anyhow::bail!("Account '{}' is not encrypted", account.name);
    }

    let data = read_account_credentials(config, &account, passphrase).await?;
    account.encrypted = false;
    write_account_credentials(config, &account, &data, None, None).await?;

    // Remove encrypted payload
    let enc = credential_file_path(config, &account.name, true)?;
    if enc.exists() {
        let _ = tokio::fs::remove_file(&enc).await;
    }

    registry.remove_account(&account.name).await?;
    registry.add_account(account.clone()).await?;
    registry.save().await?;

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "account": account.name,
                "encrypted": false
            }))?
        );
    } else {
        println!("Decrypted account {}", account.name.as_str().green());
    }
    Ok(())
}

async fn encrypt_status(
    config: &CredentialsConfig,
    filter: Option<&str>,
    json: bool,
    compact: bool,
) -> Result<()> {
    let registry = load_registry(config).await?;
    let mut accounts = registry.all_accounts().await;
    if let Some(f) = filter {
        accounts.retain(|a| a.name == f || a.aliases.iter().any(|al| al == f));
    }

    if accounts.is_empty() {
        anyhow::bail!("No matching accounts found");
    }

    if json {
        let payload: serde_json::Value = json!({
            "accounts": accounts.iter().map(|a| json!({
                "name": a.name,
                "encrypted": a.encrypted
            })).collect::<Vec<_>>()
        });
        println!("{}", serde_json::to_string_pretty(&payload)?);
    } else {
        for acct in accounts {
            let state = if acct.encrypted {
                "encrypted"
            } else {
                "plaintext"
            };
            if compact {
                println!("{} ({})", acct.name, state);
            } else {
                println!(
                    "{}: {}",
                    acct.name,
                    if acct.encrypted {
                        state.green()
                    } else {
                        state.yellow()
                    }
                );
            }
        }
    }

    Ok(())
}

fn auto_name(agent: &AgentKind) -> String {
    let ts = Utc::now().format("%Y%m%d%H%M%S");
    format!("{}-{}", format!("{:?}", agent).to_lowercase(), ts)
}

async fn load_registry(config: &CredentialsConfig) -> Result<AccountRegistry> {
    let registry = AccountRegistry::new(config.clone());
    registry.load().await.map(|_| registry).map_err(|e| anyhow::anyhow!(e))
}

fn status_label(status: &AccountStatus, compact: bool) -> String {
    match status {
        AccountStatus::Active => {
            if compact {
                "active".to_string()
            } else {
                "active".green().to_string()
            }
        }
        AccountStatus::Expired => {
            if compact {
                "expired".to_string()
            } else {
                "expired".yellow().to_string()
            }
        }
        AccountStatus::Inactive => {
            if compact {
                "inactive".to_string()
            } else {
                "inactive".dark_grey().to_string()
            }
        }
        AccountStatus::Error => {
            if compact {
                "error".to_string()
            } else {
                "error".red().to_string()
            }
        }
    }
}

fn print_add_output(account: &ah_credentials::Account, path: &std::path::Path, json: bool) {
    if json {
        let payload = json!({
            "account": account.name,
            "agent": format!("{:?}", account.agent).to_lowercase(),
            "encrypted": account.encrypted,
            "path": path.display().to_string()
        });
        println!("{}", serde_json::to_string_pretty(&payload).unwrap());
    } else {
        println!(
            "Stored credentials for {} at {}",
            account.name.as_str().green(),
            path.display()
        );
    }
}
