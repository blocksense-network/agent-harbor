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
    KdfParams, KeyCache, decrypt_credential_data, derive_key_from_passphrase,
    encrypt_credential_data, generate_salt, rotate_account_key,
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
    use crate::test_utils;
    use std::time::Duration;
    use zeroize::Zeroizing;

    #[test]
    fn test_encrypt_decrypt_round_trip() {
        let log = test_utils::setup_test_logging("crypto_round_trip");
        std::fs::write(&log, "round trip start\n").unwrap();

        let payload = serde_json::json!({"token": "abc123", "user": "test"});
        let encrypted = encrypt_credential_data(&payload, "passphrase", None).unwrap();
        let decrypted = decrypt_credential_data(&encrypted, "passphrase").unwrap();

        assert_eq!(payload, decrypted);
        assert!(log.exists());
    }

    #[test]
    fn test_tamper_detection() {
        let log = test_utils::setup_test_logging("crypto_tamper_detection");
        let payload = serde_json::json!({"token": "abc123"});
        let mut encrypted = encrypt_credential_data(&payload, "passphrase", None).unwrap();

        // Flip a byte in the ciphertext section of the envelope
        let pos = encrypted.len() / 2;
        encrypted[pos] ^= 0b0000_0001;

        let result = decrypt_credential_data(&encrypted, "passphrase");
        assert!(result.is_err());
        assert!(log.exists());
    }

    #[test]
    fn test_key_cache_timeout_and_refresh() {
        let mut cache = KeyCache::with_ttl(Duration::from_millis(50));
        let key = Zeroizing::new(vec![9u8; 32]);
        cache.store_key("acct".into(), key.clone());

        // First fetch succeeds
        assert!(cache.get_key("acct").is_some());

        // After TTL expires, key should be evicted
        std::thread::sleep(Duration::from_millis(60));
        cache.clear_expired_keys();
        assert!(cache.get_key("acct").is_none());
    }
}
