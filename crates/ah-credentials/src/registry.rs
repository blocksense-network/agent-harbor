// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! High-level account registry operations

use crate::{
    config::CredentialsConfig,
    error::{Error, Result},
    storage::{load_registry, save_registry},
    types::{Account, AccountRegistry as Registry, AgentType},
    validation::{validate_account, validate_account_name},
};
use std::sync::Arc;
use tokio::sync::RwLock;

/// Thread-safe account registry manager
#[derive(Clone)]
pub struct AccountRegistry {
    config: CredentialsConfig,
    registry: Arc<RwLock<Registry>>,
}

impl AccountRegistry {
    /// Create a new registry manager
    pub fn new(config: CredentialsConfig) -> Self {
        Self {
            config,
            registry: Arc::new(RwLock::new(Registry::new())),
        }
    }

    /// Load the registry from disk
    pub async fn load(&self) -> Result<()> {
        let loaded = load_registry(&self.config).await?;
        *self.registry.write().await = loaded;
        Ok(())
    }

    /// Save the registry to disk
    pub async fn save(&self) -> Result<()> {
        let registry = self.registry.read().await;
        save_registry(&self.config, &registry).await
    }

    /// Add a new account to the registry
    pub async fn add_account(&self, account: Account) -> Result<()> {
        let mut registry = self.registry.write().await;

        // Validate the account
        validate_account(&account, &registry)?;

        // Add to registry
        registry.add_account(account);

        Ok(())
    }

    /// Remove an account from the registry
    pub async fn remove_account(&self, identifier: &str) -> Result<Option<Account>> {
        let mut registry = self.registry.write().await;
        Ok(registry.remove_account(identifier))
    }

    /// Find an account by name or alias
    pub async fn find_account(&self, identifier: &str) -> Option<Account> {
        let registry = self.registry.read().await;
        registry.find_account(identifier).cloned()
    }

    /// Update an account's last used timestamp
    pub async fn mark_account_used(&self, identifier: &str) -> Result<()> {
        let mut registry = self.registry.write().await;

        if let Some(account) = registry.find_account_mut(identifier) {
            account.mark_used();
            Ok(())
        } else {
            Err(Error::AccountNotFound(identifier.to_string()))
        }
    }

    /// Add an alias to an account
    pub async fn add_alias(&self, account_name: &str, alias: String) -> Result<()> {
        validate_account_name(&alias)?;

        let mut registry = self.registry.write().await;

        // Check if alias is taken first (before borrowing mutably)
        if registry.is_identifier_taken(&alias) {
            return Err(Error::DuplicateAlias(alias));
        }

        if let Some(account) = registry.find_account_mut(account_name) {
            account.add_alias(alias);
            Ok(())
        } else {
            Err(Error::AccountNotFound(account_name.to_string()))
        }
    }

    /// Remove an alias from an account
    pub async fn remove_alias(&self, account_name: &str, alias: &str) -> Result<()> {
        let mut registry = self.registry.write().await;

        if let Some(account) = registry.find_account_mut(account_name) {
            account.remove_alias(alias);
            Ok(())
        } else {
            Err(Error::AccountNotFound(account_name.to_string()))
        }
    }

    /// Get all accounts for a specific agent type
    pub async fn accounts_for_agent(&self, agent: &AgentType) -> Vec<Account> {
        let registry = self.registry.read().await;
        registry.accounts_for_agent(agent).into_iter().cloned().collect()
    }

    /// Get all accounts
    pub async fn all_accounts(&self) -> Vec<Account> {
        let registry = self.registry.read().await;
        registry.accounts.clone()
    }

    /// Check if an identifier is available
    pub async fn is_identifier_available(&self, identifier: &str) -> bool {
        let registry = self.registry.read().await;
        !registry.is_identifier_taken(identifier)
    }

    /// Get the number of accounts
    pub async fn count(&self) -> usize {
        let registry = self.registry.read().await;
        registry.len()
    }

    /// Check if the registry is empty
    pub async fn is_empty(&self) -> bool {
        let registry = self.registry.read().await;
        registry.is_empty()
    }

    /// Get the default account for an agent type
    pub fn default_account_for_agent(&self, agent: &str) -> Option<&str> {
        self.config.default_account_for_agent(agent)
    }

    /// Reload the registry from disk
    pub async fn reload(&self) -> Result<()> {
        self.load().await
    }
}
