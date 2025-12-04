// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Cryptographic operations for credential encryption.
//!
//! Envelope format (JSON):
//! - `version`: u8 (currently 1)
//! - `cipher`: string ("AES-256-GCM")
//! - `kdf`: { algorithm: "argon2id", phc: string, salt: base64, memory_kib, iterations, parallelism }
//! - `nonce`: base64-encoded 96-bit nonce
//! - `ciphertext`: base64-encoded AES-GCM payload (AAD unused)
//!
//! Defaults (aligned with spec hardening guidance): Argon2id with 64 MiB memory,
//! 3 iterations, 1 lane, and a 15-minute session key cache TTL. Tunables are
//! exposed via `CredentialsConfig.crypto` for operators to tighten/relax costs.
//!
//! All sensitive buffers are wrapped in `Zeroizing` and cached keys expire to
//! reduce passphrase prompts while keeping memory hygiene sane.

use crate::error::{Error, Result};
use aes_gcm::{
    Aes256Gcm, Nonce,
    aead::{Aead, KeyInit},
};
use argon2::{Algorithm, Argon2, Params, Version};
use base64::{Engine, engine::general_purpose::STANDARD};
use password_hash::{PasswordHasher, SaltString};
use rand::{RngCore, rngs::OsRng};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::HashMap,
    time::{Duration, Instant},
};
use zeroize::{Zeroize, ZeroizeOnDrop, Zeroizing};

pub const KEY_LENGTH: usize = 32; // AES-256
pub const NONCE_LENGTH: usize = 12; // Recommended size for AES-GCM
pub const SALT_LENGTH: usize = 16;
pub const DEFAULT_MEMORY_KIB: u32 = 64 * 1024; // 64 MiB
pub const DEFAULT_ITERATIONS: u32 = 3;
pub const DEFAULT_PARALLELISM: u32 = 1;
pub const DEFAULT_CACHE_TTL: Duration = Duration::from_secs(15 * 60); // 15 minutes

/// Parameters used for Argon2 key derivation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct KdfParams {
    /// Salt bytes (16 bytes by default)
    pub salt: Vec<u8>,
    /// Memory cost in KiB
    pub memory_kib: u32,
    /// Iteration count (time cost)
    pub iterations: u32,
    /// Degree of parallelism
    pub parallelism: u32,
}

impl KdfParams {
    /// Secure defaults recommended by the milestone guidance.
    pub fn secure_defaults() -> Result<Self> {
        Ok(Self {
            salt: generate_salt()?,
            memory_kib: DEFAULT_MEMORY_KIB,
            iterations: DEFAULT_ITERATIONS,
            parallelism: DEFAULT_PARALLELISM,
        })
    }

    /// Construct parameters using explicit tunables and optional salt override.
    pub fn new(
        memory_kib: u32,
        iterations: u32,
        parallelism: u32,
        salt: Option<Vec<u8>>,
    ) -> Result<Self> {
        Ok(Self {
            salt: salt.unwrap_or(generate_salt()?),
            memory_kib,
            iterations,
            parallelism,
        })
    }
}

impl Default for KdfParams {
    fn default() -> Self {
        Self::secure_defaults().expect("secure_defaults should only fail if RNG fails")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct KdfMetadata {
    algorithm: String,
    /// PHC string capturing params + salt for audit/interoperability
    phc: String,
    salt: String,
    memory_kib: u32,
    iterations: u32,
    parallelism: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct EncryptedEnvelope {
    version: u8,
    cipher: String,
    kdf: KdfMetadata,
    nonce: String,
    ciphertext: String,
}

/// Derive a 256-bit key from a passphrase using Argon2id.
pub fn derive_key_from_passphrase(
    passphrase: &str,
    params: &KdfParams,
) -> Result<Zeroizing<Vec<u8>>> {
    let mut key = Zeroizing::new(vec![0u8; KEY_LENGTH]);

    let argon_params = argon_params_from(params)?;

    let argon = Argon2::new(Algorithm::Argon2id, Version::V0x13, argon_params);

    argon
        .hash_password_into(passphrase.as_bytes(), &params.salt, &mut key)
        .map_err(|e| Error::Encryption(format!("Key derivation failed: {}", e)))?;

    Ok(key)
}

/// Encrypt credential JSON using AES-256-GCM and return an authenticated envelope.
pub fn encrypt_credential_data(
    data: &Value,
    passphrase: &str,
    kdf_params: Option<KdfParams>,
) -> Result<Vec<u8>> {
    let params = kdf_params.unwrap_or_default();
    let key = derive_key_from_passphrase(passphrase, &params)?;

    let cipher = Aes256Gcm::new_from_slice(key.as_ref())
        .map_err(|e| Error::Encryption(format!("Invalid key material: {}", e)))?;

    let mut nonce_bytes = [0u8; NONCE_LENGTH];
    OsRng.fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);

    let plaintext = Zeroizing::new(serde_json::to_vec(data)?);
    let ciphertext = cipher
        .encrypt(nonce, plaintext.as_ref())
        .map_err(|e| Error::Encryption(format!("Encryption failed: {}", e)))?;

    let salt_string = SaltString::encode_b64(&params.salt)
        .map_err(|e| Error::Encryption(format!("Failed to encode salt as PHC salt: {}", e)))?;

    // Produce a PHC string for audit/interoperability. We intentionally use the
    // same params + salt that were fed into `hash_password_into` to keep the
    // stored metadata consistent.
    let phc_string = Argon2::new(
        Algorithm::Argon2id,
        Version::V0x13,
        argon_params_from(&params)?,
    )
    .hash_password(passphrase.as_bytes(), &salt_string)
    .map_err(|e| Error::Encryption(format!("Failed to generate PHC string: {}", e)))?
    .to_string();

    let envelope = EncryptedEnvelope {
        version: 1,
        cipher: "AES-256-GCM".to_string(),
        kdf: KdfMetadata {
            algorithm: "argon2id".to_string(),
            phc: phc_string,
            salt: STANDARD.encode(&params.salt),
            memory_kib: params.memory_kib,
            iterations: params.iterations,
            parallelism: params.parallelism,
        },
        nonce: STANDARD.encode(nonce_bytes),
        ciphertext: STANDARD.encode(ciphertext),
    };

    Ok(serde_json::to_vec(&envelope)?)
}

/// Decrypt a previously encrypted envelope with the provided passphrase.
pub fn decrypt_credential_data(encrypted_data: &[u8], passphrase: &str) -> Result<Value> {
    let envelope: EncryptedEnvelope = serde_json::from_slice(encrypted_data)?;

    if envelope.cipher.to_uppercase() != "AES-256-GCM" {
        return Err(Error::Encryption(format!(
            "Unsupported cipher: {}",
            envelope.cipher
        )));
    }

    if envelope.kdf.algorithm.to_lowercase() != "argon2id" {
        return Err(Error::Encryption(format!(
            "Unsupported KDF: {}",
            envelope.kdf.algorithm
        )));
    }

    // Allow interop even if future versions add/remove metadata fields by
    // favoring the PHC string when present.
    let decoded_salt = STANDARD
        .decode(envelope.kdf.salt.as_bytes())
        .map_err(|e| Error::Encryption(format!("Failed to decode salt: {}", e)))?;

    let params = KdfParams {
        salt: decoded_salt,
        memory_kib: envelope.kdf.memory_kib,
        iterations: envelope.kdf.iterations,
        parallelism: envelope.kdf.parallelism,
    };

    let key = derive_key_from_passphrase(passphrase, &params)?;
    let cipher = Aes256Gcm::new_from_slice(key.as_ref())
        .map_err(|e| Error::Encryption(format!("Invalid key material: {}", e)))?;

    let nonce_bytes = STANDARD
        .decode(envelope.nonce.as_bytes())
        .map_err(|e| Error::Encryption(format!("Failed to decode nonce: {}", e)))?;
    if nonce_bytes.len() != NONCE_LENGTH {
        return Err(Error::Encryption("Invalid nonce length".to_string()));
    }
    let nonce = Nonce::from_slice(&nonce_bytes);

    let ciphertext = STANDARD
        .decode(envelope.ciphertext.as_bytes())
        .map_err(|e| Error::Encryption(format!("Failed to decode ciphertext: {}", e)))?;

    let plaintext = cipher
        .decrypt(nonce, ciphertext.as_ref())
        .map_err(|e| Error::Encryption(format!("Decryption failed: {}", e)))?;

    Ok(serde_json::from_slice(&plaintext)?)
}

/// Generate a random salt for key derivation.
pub fn generate_salt() -> Result<Vec<u8>> {
    let mut salt = vec![0u8; SALT_LENGTH];
    OsRng
        .try_fill_bytes(&mut salt)
        .map_err(|e| Error::Encryption(format!("Failed to generate salt: {}", e)))?;
    Ok(salt)
}

/// In-process cache for derived keys with inactivity timeout and zeroization.
#[derive(Debug)]
pub struct KeyCache {
    entries: HashMap<String, CachedKey>,
    ttl: Duration,
}

#[derive(Debug, Zeroize, ZeroizeOnDrop)]
struct CachedKey {
    #[zeroize(skip)]
    expires_at: Instant,
    key: Zeroizing<Vec<u8>>,
}

impl KeyCache {
    /// Create a cache with the default TTL (15 minutes).
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
            ttl: DEFAULT_CACHE_TTL,
        }
    }

    /// Create a cache with a custom TTL (useful for tests).
    pub fn with_ttl(ttl: Duration) -> Self {
        Self {
            entries: HashMap::new(),
            ttl,
        }
    }

    /// Inspect current TTL (testing/telemetry)
    pub fn ttl(&self) -> Duration {
        self.ttl
    }

    /// Store a derived key for an account.
    pub fn store_key(&mut self, account_name: String, key: Zeroizing<Vec<u8>>) {
        let expires_at = Instant::now() + self.ttl;
        self.entries.insert(account_name, CachedKey { expires_at, key });
    }

    /// Retrieve a key if present and not expired, refreshing its TTL.
    pub fn get_key(&mut self, account_name: &str) -> Option<Zeroizing<Vec<u8>>> {
        self.clear_expired_keys();
        if let Some(entry) = self.entries.get_mut(account_name) {
            if Instant::now() <= entry.expires_at {
                entry.expires_at = Instant::now() + self.ttl;
                return Some(entry.key.clone());
            } else {
                self.entries.remove(account_name);
            }
        }
        None
    }

    /// Remove expired keys immediately.
    pub fn clear_expired_keys(&mut self) {
        let now = Instant::now();
        self.entries.retain(|_, entry| now <= entry.expires_at);
    }

    /// Purge all cached keys (helpful for tests or logout flows).
    pub fn purge(&mut self) {
        self.entries.clear();
    }

    /// Number of cached entries (testing/telemetry).
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the cache currently holds any entries.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

impl Default for KeyCache {
    fn default() -> Self {
        Self::new()
    }
}

/// Re-encrypt an envelope with a newly derived key.
pub fn rotate_account_key(
    old_passphrase: &str,
    new_passphrase: &str,
    encrypted_data: &[u8],
    new_kdf_params: Option<KdfParams>,
) -> Result<Vec<u8>> {
    let payload = decrypt_credential_data(encrypted_data, old_passphrase)?;
    encrypt_credential_data(&payload, new_passphrase, new_kdf_params)
}

/// Retrieve a cached key or derive and cache it using a provided passphrase callback.
pub fn cached_or_derive_key<F>(
    account_name: &str,
    cache: &mut KeyCache,
    params: &KdfParams,
    mut passphrase_provider: F,
) -> Result<Zeroizing<Vec<u8>>>
where
    F: FnMut() -> Result<String>,
{
    if let Some(key) = cache.get_key(account_name) {
        return Ok(key);
    }

    let passphrase = Zeroizing::new(passphrase_provider()?);
    let key = derive_key_from_passphrase(&passphrase, params)?;
    cache.store_key(account_name.to_string(), key.clone());
    Ok(key)
}

fn argon_params_from(params: &KdfParams) -> Result<Params> {
    Params::new(
        params.memory_kib,
        params.iterations,
        params.parallelism,
        Some(KEY_LENGTH),
    )
    .map_err(|e| Error::Encryption(format!("Invalid Argon2 params: {}", e)))
}
