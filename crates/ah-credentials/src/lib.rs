// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Library-first crate for secure multi-account credential storage and management.
//!
//! This crate provides:
//! - Account registry with metadata, aliases, and status tracking
//! - Secure file layout under `{config-dir}/credentials/`
//! - Configuration integration with precedence rules
//! - Validation routines for account names and agent types
//! - Encryption support for sensitive credentials

pub mod config;
pub mod crypto;
pub mod error;
pub mod registry;
pub mod storage;
pub mod types;
pub mod validation;

/// Re-export key types for convenience
pub use config::CredentialsConfig;
pub use crypto::{
    KeyCache, decrypt_credential_data, derive_key_from_passphrase, encrypt_credential_data,
    generate_salt, rotate_account_key,
};
pub use error::{Error, Result};
pub use registry::AccountRegistry;
pub use types::{Account, AccountStatus, AgentType};

/// Test utilities for logging and fixtures
pub mod test_utils {
    use std::path::PathBuf;
    use std::sync::OnceLock;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static TEST_LOG_COUNTER: AtomicUsize = AtomicUsize::new(0);
    static TEST_LOG_DIR: OnceLock<PathBuf> = OnceLock::new();

    /// Get the path for a test log file
    pub fn test_log_path(test_name: &str) -> PathBuf {
        let counter = TEST_LOG_COUNTER.fetch_add(1, Ordering::SeqCst);
        let log_dir =
            TEST_LOG_DIR.get_or_init(|| std::env::temp_dir().join("ah-credentials-test-logs"));

        // Create log directory if it doesn't exist
        std::fs::create_dir_all(log_dir).unwrap();

        log_dir.join(format!("test-{}-{}.log", test_name, counter))
    }

    /// Setup test logging for a test and return the log path
    pub fn setup_test_logging(test_name: &str) -> PathBuf {
        let log_path = test_log_path(test_name);

        // Write initial log entry
        if let Err(e) = std::fs::write(&log_path, format!("Starting test: {}\n", test_name)) {
            tracing::warn!(
                "Failed to write to test log file {}: {}",
                log_path.display(),
                e
            );
        }

        log_path
    }
}

#[cfg(test)]
mod crypto_tests {
    use super::crypto::*;

    #[test]
    fn test_encryption_placeholders_return_errors() {
        // Test that all encryption functions return appropriate errors
        assert!(derive_key_from_passphrase("test", &[0u8; 32], 1000).is_err());
        assert!(encrypt_credential_data(&serde_json::json!({"test": "data"}), &[0u8; 32]).is_err());
        assert!(decrypt_credential_data(&[0u8; 32], &[0u8; 32]).is_err());
        assert!(generate_salt().is_err());
        assert!(rotate_account_key(&[0u8; 32], &[0u8; 32], &[0u8; 32]).is_err());
    }

    #[test]
    fn test_key_cache_basic_functionality() {
        let mut cache = KeyCache::new();

        // Should return None for non-existent keys
        assert!(cache.get_key("nonexistent").is_none());

        // Should store and retrieve keys (placeholder implementation)
        cache.store_key("test".to_string(), vec![1, 2, 3]);
        assert!(cache.get_key("test").is_none()); // Will be None until implemented

        cache.clear_expired_keys(); // Should not panic
    }
}
