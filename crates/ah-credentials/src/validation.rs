// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Validation routines for account names, aliases, and agent types

use crate::{
    error::{Error, Result},
    types::{Account, AccountRegistry, AgentType},
};
use regex::Regex;
use std::collections::HashSet;

/// Validate an account name
/// Account names must:
/// - Be 1-64 characters long
/// - Contain only alphanumeric characters, hyphens, and underscores
/// - Start and end with alphanumeric characters
pub fn validate_account_name(name: &str) -> Result<()> {
    if name.is_empty() {
        return Err(Error::InvalidAccountName(
            "Account name cannot be empty".to_string(),
        ));
    }

    if name.len() > 64 {
        return Err(Error::InvalidAccountName(
            "Account name cannot be longer than 64 characters".to_string(),
        ));
    }

    // Lazy static regex for performance - allows single hyphens/underscores but not consecutive
    static NAME_REGEX: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    let regex =
        NAME_REGEX.get_or_init(|| Regex::new(r"^[a-zA-Z0-9]+(?:[-_][a-zA-Z0-9]+)*$").unwrap());

    if !regex.is_match(name) {
        return Err(Error::InvalidAccountName(
            "Account name must contain only alphanumeric characters, hyphens, and underscores, and must start and end with alphanumeric characters".to_string()
        ));
    }

    Ok(())
}

/// Validate an alias
/// Aliases have the same restrictions as account names
pub fn validate_alias(alias: &str) -> Result<()> {
    validate_account_name(alias)
}

/// Validate that an agent type is supported
pub fn validate_agent_type(_agent: &AgentType) -> Result<()> {
    // All enum variants are currently supported
    // This function can be extended when new agent types are added
    // or when certain agents become deprecated
    Ok(())
}

/// Validate an account before adding it to the registry
pub fn validate_account(account: &Account, registry: &AccountRegistry) -> Result<()> {
    // Validate account name
    validate_account_name(&account.name)?;

    // Validate agent type
    validate_agent_type(&account.agent)?;

    // Validate aliases
    for alias in &account.aliases {
        validate_alias(alias)?;
    }

    // Check for conflicts with existing accounts
    validate_no_conflicts(account, registry)?;

    Ok(())
}

/// Validate that an account doesn't conflict with existing accounts
pub fn validate_no_conflicts(account: &Account, registry: &AccountRegistry) -> Result<()> {
    // Check if account name conflicts
    if registry.find_account(&account.name).is_some() {
        return Err(Error::AccountExists(account.name.clone()));
    }

    // Check if any alias conflicts
    for alias in &account.aliases {
        if registry.is_identifier_taken(alias) {
            return Err(Error::DuplicateAlias(alias.clone()));
        }
    }

    Ok(())
}

/// Clean up stale metadata in the registry
/// This function identifies and removes accounts that may have become stale
/// based on various heuristics
pub fn cleanup_stale_metadata(registry: &mut AccountRegistry) -> Vec<String> {
    let mut removed_accounts = Vec::new();
    let mut to_remove = Vec::new();

    // Find accounts to remove
    for (index, account) in registry.accounts.iter().enumerate() {
        // Remove accounts that have been inactive for more than 90 days
        // and have error status
        if account.status == crate::types::AccountStatus::Error {
            let days_since_used = (chrono::Utc::now() - account.last_used).num_days();
            if days_since_used > 90 {
                to_remove.push(index);
                removed_accounts.push(account.name.clone());
            }
        }
    }

    // Remove in reverse order to maintain indices
    to_remove.sort_by(|a, b| b.cmp(a));
    for index in to_remove {
        registry.accounts.remove(index);
    }

    removed_accounts
}

/// Validate the entire registry for consistency
pub fn validate_registry(registry: &AccountRegistry) -> Result<()> {
    let mut seen_identifiers = HashSet::new();

    for account in &registry.accounts {
        // Validate individual account
        validate_account_name(&account.name)?;
        validate_agent_type(&account.agent)?;

        // Check for duplicate identifiers within the registry
        for identifier in account.all_identifiers() {
            if seen_identifiers.contains(identifier) {
                return Err(Error::DuplicateAlias((*identifier).to_string()));
            }
            seen_identifiers.insert(identifier);
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Account, AccountRegistry, AgentType};

    #[test]
    fn test_validate_account_name_valid() {
        assert!(validate_account_name("valid-name").is_ok());
        assert!(validate_account_name("valid_name").is_ok());
        assert!(validate_account_name("ValidName123").is_ok());
        assert!(validate_account_name("a").is_ok());
        assert!(validate_account_name("a1").is_ok());
    }

    #[test]
    fn test_validate_account_name_invalid() {
        assert!(validate_account_name("").is_err());
        assert!(validate_account_name("-invalid").is_err());
        assert!(validate_account_name("invalid-").is_err());
        assert!(validate_account_name("_invalid").is_err());
        assert!(validate_account_name("invalid_").is_err());
        assert!(validate_account_name("invalid name").is_err());
        assert!(validate_account_name("invalid@name").is_err());
    }

    #[test]
    fn test_validate_no_conflicts() {
        let mut registry = AccountRegistry::new();
        let account1 = Account::new("test1".to_string(), AgentType::Codex);
        registry.add_account(account1);

        let account2 = Account::new("test2".to_string(), AgentType::Claude);
        assert!(validate_no_conflicts(&account2, &registry).is_ok());

        // Try to add account with conflicting name
        let account3 = Account::new("test1".to_string(), AgentType::Cursor);
        assert!(validate_no_conflicts(&account3, &registry).is_err());
    }
}
