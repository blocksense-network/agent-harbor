// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Configuration integration for credentials management

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Credentials-related configuration section
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, Default)]
#[serde(rename_all = "kebab-case")]
pub struct CredentialsConfig {
    /// Custom storage path for credentials directory
    /// If not set, uses the standard config directory
    pub storage_path: Option<PathBuf>,

    /// Default accounts to use when no account is specified
    /// Maps agent type to account identifier
    #[serde(default)]
    pub default_accounts: std::collections::HashMap<String, String>,

    /// Auto-verification settings
    #[serde(default)]
    pub auto_verify: AutoVerifyConfig,

    /// Base configuration directory (resolved by config-core)
    /// This is set when extracting from resolved configuration
    #[serde(skip)]
    pub base_config_dir: Option<PathBuf>,

    /// Override for AH_HOME (used for testing to avoid environment conflicts)
    #[serde(skip)]
    pub ah_home_override: Option<PathBuf>,
}

/// Auto-verification configuration
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, Default)]
#[serde(rename_all = "kebab-case")]
pub struct AutoVerifyConfig {
    /// Whether to verify credentials on startup
    #[serde(default)]
    pub on_start: bool,

    /// Interval for background verification (in seconds)
    /// None means no background verification
    pub interval: Option<u64>,
}

impl CredentialsConfig {
    /// Set the base configuration directory (resolved by config-core)
    pub fn with_base_config_dir(mut self, base_dir: PathBuf) -> Self {
        self.base_config_dir = Some(base_dir);
        self
    }

    /// Extract credentials config from resolved config-core configuration
    pub fn from_resolved_config(
        resolved_json: &serde_json::Value,
        base_config_dir: PathBuf,
    ) -> crate::error::Result<Self> {
        use config_core::extract::get_at;

        // Try to extract the credentials section
        let mut config: CredentialsConfig =
            get_at(resolved_json, "credentials").unwrap_or_else(|_| CredentialsConfig::default());

        // Set the base config directory from config-core resolution
        config.base_config_dir = Some(base_config_dir);

        Ok(config)
    }

    /// Get the credentials storage directory path
    /// Precedence: storage_path > AH_HOME > base_config_dir > dirs::config_dir
    pub fn storage_dir(&self) -> Result<PathBuf, crate::Error> {
        // First priority: explicit storage path from config
        if let Some(custom_path) = &self.storage_path {
            return Ok(custom_path.clone());
        }

        // Second priority: AH_HOME environment variable (or test override)
        // Note: ah_home_override is for testing, AH_HOME is the real environment variable
        if let Some(ah_home_override) = &self.ah_home_override {
            return Ok(ah_home_override.join("credentials"));
        }
        if let Ok(ah_home) = std::env::var("AH_HOME") {
            return Ok(PathBuf::from(ah_home).join("credentials"));
        }

        // Third priority: base config directory from config-core resolution
        if let Some(base_dir) = &self.base_config_dir {
            return Ok(base_dir.join("credentials"));
        }

        // Fourth priority: fallback to standard config directory
        let base_dir = dirs::config_dir().ok_or_else(|| {
            crate::Error::Config("Could not determine config directory".to_string())
        })?;
        Ok(base_dir.join("agent-harbor").join("credentials"))
    }

    /// Get the accounts.toml file path
    pub fn accounts_file(&self) -> Result<PathBuf, crate::Error> {
        Ok(self.storage_dir()?.join("accounts.toml"))
    }

    /// Get the keys directory path
    pub fn keys_dir(&self) -> Result<PathBuf, crate::Error> {
        Ok(self.storage_dir()?.join("keys"))
    }

    /// Get the temp directory path
    pub fn temp_dir(&self) -> Result<PathBuf, crate::Error> {
        Ok(self.storage_dir()?.join("temp"))
    }

    /// Get the default account for a given agent type
    pub fn default_account_for_agent(&self, agent: &str) -> Option<&str> {
        self.default_accounts.get(agent).map(|s| s.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_credentials_config_storage_dir() {
        // Test with custom storage path
        let mut config = CredentialsConfig {
            storage_path: Some("/custom/path".into()),
            default_accounts: HashMap::new(),
            auto_verify: Default::default(),
            base_config_dir: None,
            ah_home_override: None,
        };

        assert_eq!(
            config.storage_dir().unwrap(),
            std::path::PathBuf::from("/custom/path")
        );

        // Test without custom storage path (should use default logic)
        config.storage_path = None;
        let storage_dir = config.storage_dir().unwrap();
        // Should contain "agent-harbor" and "credentials"
        assert!(storage_dir.to_string_lossy().contains("agent-harbor"));
        assert!(storage_dir.to_string_lossy().ends_with("credentials"));
    }

    #[test]
    fn test_credentials_config_default_accounts() {
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

        assert_eq!(
            config.default_account_for_agent("codex"),
            Some("work-account")
        );
        assert_eq!(
            config.default_account_for_agent("claude"),
            Some("personal-account")
        );
        assert_eq!(config.default_account_for_agent("cursor"), None);
    }

    #[test]
    fn test_auto_verify_config_defaults() {
        let config = AutoVerifyConfig::default();
        assert!(!config.on_start);
        assert!(config.interval.is_none());
    }
}
