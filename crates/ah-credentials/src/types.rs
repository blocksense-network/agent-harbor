// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Core types for the credentials management system

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// Supported agent types for credential storage
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum AgentType {
    Codex,
    Claude,
    Cursor,
    // Future agents can be added here
}

/// Status of an account
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum AccountStatus {
    Active,
    Inactive,
    Expired,
    Error,
}

/// Account metadata and configuration
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, PartialEq)]
pub struct Account {
    /// User-friendly name for the account
    pub name: String,

    /// Type of agent this account is for
    pub agent: AgentType,

    /// Alternative names/identifiers for this account
    #[serde(default)]
    pub aliases: Vec<String>,

    /// Whether this account's credentials are encrypted
    #[serde(default)]
    pub encrypted: bool,

    /// When this account was created (ISO 8601 string)
    #[schemars(with = "String")]
    pub created: DateTime<Utc>,

    /// When this account was last used (ISO 8601 string)
    #[schemars(with = "String")]
    pub last_used: DateTime<Utc>,

    /// Current status of the account
    pub status: AccountStatus,
}

impl Account {
    /// Create a new account with the given parameters
    pub fn new(name: String, agent: AgentType) -> Self {
        let now = Utc::now();
        Self {
            name,
            agent,
            aliases: Vec::new(),
            encrypted: false,
            created: now,
            last_used: now,
            status: AccountStatus::Active,
        }
    }

    /// Add an alias to this account
    pub fn add_alias(&mut self, alias: String) {
        if !self.aliases.contains(&alias) {
            self.aliases.push(alias);
        }
    }

    /// Remove an alias from this account
    pub fn remove_alias(&mut self, alias: &str) {
        self.aliases.retain(|a| a != alias);
    }

    /// Update the last used timestamp
    pub fn mark_used(&mut self) {
        self.last_used = Utc::now();
    }

    /// Check if this account has the given alias
    pub fn has_alias(&self, alias: &str) -> bool {
        self.aliases.iter().any(|a| a == alias)
    }

    /// Get all identifiers for this account (name + aliases)
    pub fn all_identifiers(&self) -> HashSet<&str> {
        let mut identifiers: HashSet<&str> = HashSet::new();
        identifiers.insert(&self.name);
        for alias in &self.aliases {
            identifiers.insert(alias);
        }
        identifiers
    }
}

/// Registry containing all accounts
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, Default)]
pub struct AccountRegistry {
    /// List of all accounts
    pub accounts: Vec<Account>,
}

impl AccountRegistry {
    /// Create a new empty registry
    pub fn new() -> Self {
        Self::default()
    }

    /// Add an account to the registry
    pub fn add_account(&mut self, account: Account) {
        self.accounts.push(account);
    }

    /// Remove an account by name
    pub fn remove_account(&mut self, name: &str) -> Option<Account> {
        let pos = self.accounts.iter().position(|a| a.name == name)?;
        Some(self.accounts.remove(pos))
    }

    /// Find an account by name or alias
    pub fn find_account(&self, identifier: &str) -> Option<&Account> {
        self.accounts
            .iter()
            .find(|account| account.name == identifier || account.has_alias(identifier))
    }

    /// Find an account by name or alias (mutable)
    pub fn find_account_mut(&mut self, identifier: &str) -> Option<&mut Account> {
        self.accounts
            .iter_mut()
            .find(|account| account.name == identifier || account.has_alias(identifier))
    }

    /// Get all accounts for a specific agent type
    pub fn accounts_for_agent(&self, agent: &AgentType) -> Vec<&Account> {
        self.accounts.iter().filter(|account| account.agent == *agent).collect()
    }

    /// Check if an identifier (name or alias) is already taken
    pub fn is_identifier_taken(&self, identifier: &str) -> bool {
        self.accounts
            .iter()
            .any(|account| account.all_identifiers().contains(identifier))
    }

    /// Get the number of accounts in the registry
    pub fn len(&self) -> usize {
        self.accounts.len()
    }

    /// Check if the registry is empty
    pub fn is_empty(&self) -> bool {
        self.accounts.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_account_creation() {
        let account = Account::new("test-account".to_string(), AgentType::Codex);
        assert_eq!(account.name, "test-account");
        assert_eq!(account.agent, AgentType::Codex);
        assert!(account.aliases.is_empty());
        assert!(!account.encrypted);
        assert_eq!(account.status, AccountStatus::Active);
    }

    #[test]
    fn test_account_aliases() {
        let mut account = Account::new("test-account".to_string(), AgentType::Codex);
        account.add_alias("alias1".to_string());
        account.add_alias("alias2".to_string());

        assert!(account.has_alias("alias1"));
        assert!(account.has_alias("alias2"));
        assert!(!account.has_alias("alias3"));

        let identifiers: Vec<&str> = account.all_identifiers().into_iter().collect();
        assert!(identifiers.contains(&"test-account"));
        assert!(identifiers.contains(&"alias1"));
        assert!(identifiers.contains(&"alias2"));
    }

    #[test]
    fn test_account_mark_used() {
        let mut account = Account::new("test-account".to_string(), AgentType::Codex);
        let original_time = account.last_used;

        std::thread::sleep(std::time::Duration::from_millis(1));
        account.mark_used();

        assert!(account.last_used > original_time);
    }

    #[test]
    fn test_registry_operations() {
        let mut registry = AccountRegistry::new();

        // Test empty registry
        assert!(registry.is_empty());
        assert_eq!(registry.len(), 0);

        // Add accounts
        let account1 = Account::new("codex-work".to_string(), AgentType::Codex);
        let account2 = Account::new("claude-personal".to_string(), AgentType::Claude);

        registry.add_account(account1.clone());
        registry.add_account(account2.clone());

        assert_eq!(registry.len(), 2);
        assert!(!registry.is_empty());

        // Test finding accounts
        assert_eq!(registry.find_account("codex-work"), Some(&account1));
        assert_eq!(registry.find_account("claude-personal"), Some(&account2));
        assert_eq!(registry.find_account("nonexistent"), None);

        // Test accounts for agent
        let codex_accounts = registry.accounts_for_agent(&AgentType::Codex);
        assert_eq!(codex_accounts.len(), 1);
        assert_eq!(codex_accounts[0].name, "codex-work");

        // Test identifier conflicts
        assert!(registry.is_identifier_taken("codex-work"));
        assert!(registry.is_identifier_taken("claude-personal"));
        assert!(!registry.is_identifier_taken("available-name"));

        // Test removal
        let removed = registry.remove_account("codex-work");
        assert_eq!(removed, Some(account1));
        assert_eq!(registry.len(), 1);
        assert_eq!(registry.find_account("codex-work"), None);
    }

    #[test]
    fn test_toml_round_trip() {
        let mut registry = AccountRegistry::new();
        let mut account = Account::new("test-account".to_string(), AgentType::Codex);
        account.add_alias("alias1".to_string());
        account.encrypted = true;
        account.status = AccountStatus::Active;

        registry.add_account(account);

        // Serialize to TOML
        let toml_string = toml::to_string_pretty(&registry).unwrap();

        // Deserialize back
        let deserialized: AccountRegistry = toml::from_str(&toml_string).unwrap();

        assert_eq!(deserialized.len(), 1);
        let deserialized_account = deserialized.find_account("test-account").unwrap();
        assert_eq!(deserialized_account.name, "test-account");
        assert_eq!(deserialized_account.agent, AgentType::Codex);
        assert_eq!(deserialized_account.aliases, vec!["alias1"]);
        assert!(deserialized_account.encrypted);
        assert_eq!(deserialized_account.status, AccountStatus::Active);
    }
}
