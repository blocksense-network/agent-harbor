// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Integration tests for encryption & key management (M2)

use ah_credentials::{
    crypto::{
        KdfParams, cached_or_derive_key, decrypt_credential_data, encrypt_credential_data,
        rotate_account_key,
    },
    test_utils,
};
use password_hash::PasswordHash;
use serde_json::json;
use std::time::Duration;
use zeroize::Zeroizing;

#[test]
fn test_kdf_parameter_variation_changes_keys() {
    let log = test_utils::setup_test_logging("crypto_kdf_variation");

    let params_a = KdfParams::secure_defaults().unwrap();
    let mut params_b = params_a.clone();
    params_b.iterations += 1;

    let key_a = ah_credentials::derive_key_from_passphrase("pass", &params_a).unwrap();
    let key_b = ah_credentials::derive_key_from_passphrase("pass", &params_b).unwrap();

    assert_ne!(
        &*key_a, &*key_b,
        "different params must yield different keys"
    );
    assert!(log.exists());
}

#[test]
fn test_rotation_reencrypts_with_new_passphrase() {
    let log = test_utils::setup_test_logging("crypto_rotation_reencrypts");

    let payload = json!({"token": "rotate-me", "meta": {"v": 1}});
    let encrypted_v1 = encrypt_credential_data(&payload, "old-pass", None).unwrap();

    // Rotate with a fresh salt/params
    let rotated = rotate_account_key("old-pass", "new-pass", &encrypted_v1, None).unwrap();

    // New passphrase should decrypt
    let decrypted = decrypt_credential_data(&rotated, "new-pass").unwrap();
    assert_eq!(payload, decrypted);

    // Old passphrase should fail on the rotated blob
    assert!(decrypt_credential_data(&rotated, "old-pass").is_err());
    assert!(log.exists());
}

#[test]
fn test_phc_string_published_in_envelope() {
    let payload = json!({"token": "phc-check"});
    let encrypted = encrypt_credential_data(&payload, "super-secret", None).unwrap();

    let envelope: serde_json::Value = serde_json::from_slice(&encrypted).unwrap();
    let phc = envelope["kdf"]["phc"].as_str().unwrap();

    // Parse to ensure it is a valid PHC string with argon2id
    let parsed = PasswordHash::new(phc).unwrap();
    assert_eq!(parsed.algorithm.as_str(), "argon2id");
}

#[test]
fn test_key_cache_refreshes_on_access() {
    let log = test_utils::setup_test_logging("crypto_cache_refresh");
    let mut cache = ah_credentials::KeyCache::with_ttl(Duration::from_millis(80));

    cache.store_key("acct".into(), Zeroizing::new(vec![7u8; 32]));

    // Access before expiry should refresh TTL
    std::thread::sleep(Duration::from_millis(40));
    assert!(cache.get_key("acct").is_some());

    // Because TTL refreshed, this should still be present
    std::thread::sleep(Duration::from_millis(60));
    assert!(cache.get_key("acct").is_some());

    // After one more period it should expire
    std::thread::sleep(Duration::from_millis(90));
    cache.clear_expired_keys();
    assert!(cache.get_key("acct").is_none());

    assert!(log.exists());
}

#[test]
fn test_cli_prompt_not_repeated_with_cache() {
    let log = test_utils::setup_test_logging("crypto_prompt_not_repeated");
    let params = KdfParams::secure_defaults().unwrap();
    let mut cache = ah_credentials::KeyCache::with_ttl(Duration::from_secs(5));
    let mut prompts = 0;

    let mut provider = || {
        prompts += 1;
        Ok("super-secret".to_string())
    };

    let first = cached_or_derive_key("acct", &mut cache, &params, &mut provider).unwrap();
    let second = cached_or_derive_key("acct", &mut cache, &params, &mut provider).unwrap();

    assert_eq!(
        prompts, 1,
        "passphrase should be requested only once while cached"
    );
    assert_eq!(&*first, &*second);
    assert!(log.exists());
}

#[test]
fn test_config_driven_cache_ttl() {
    let mut cfg = ah_credentials::CredentialsConfig::default();
    cfg.crypto.cache_ttl_secs = 1; // 1 second TTL

    let mut cache = cfg.key_cache();
    assert_eq!(cache.ttl(), Duration::from_secs(1));

    cache.store_key("short".into(), Zeroizing::new(vec![1u8; 32]));
    std::thread::sleep(Duration::from_millis(1100));
    cache.clear_expired_keys();
    assert!(cache.get_key("short").is_none());
}

#[test]
fn test_tampered_nonce_is_rejected() {
    let log = test_utils::setup_test_logging("crypto_tampered_nonce");

    let payload = json!({"token": "tamper"});
    let mut encrypted = encrypt_credential_data(&payload, "secure-pass", None).unwrap();

    // Parse to JSON and flip one char in nonce field to simulate tampering
    let mut envelope: serde_json::Value = serde_json::from_slice(&encrypted).unwrap();
    let nonce = envelope["nonce"].as_str().unwrap();
    let mut tampered = nonce.as_bytes().to_vec();
    tampered[0] ^= 0b0000_0001;
    envelope["nonce"] = serde_json::Value::String(String::from_utf8(tampered).unwrap());
    encrypted = serde_json::to_vec(&envelope).unwrap();

    let result = decrypt_credential_data(&encrypted, "secure-pass");
    assert!(result.is_err());
    assert!(log.exists());
}
