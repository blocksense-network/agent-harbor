// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! File layout and storage operations for credentials

use crate::{
    config::CredentialsConfig,
    error::{Error, Result},
    types::AccountRegistry,
    validation::cleanup_stale_metadata,
};
use std::fs;
use std::path::Path;
use tokio::fs as async_fs;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

/// Ensure the credentials directory structure exists with proper permissions
pub async fn ensure_credentials_dirs(config: &CredentialsConfig) -> Result<()> {
    let storage_dir = config.storage_dir()?;
    let keys_dir = config.keys_dir()?;
    let temp_dir = config.temp_dir()?;

    // Create directories with proper permissions (0700)
    create_dir_with_permissions(&storage_dir, 0o700).await?;
    create_dir_with_permissions(&keys_dir, 0o700).await?;
    create_dir_with_permissions(&temp_dir, 0o700).await?;

    Ok(())
}

/// Create a directory with specific permissions
async fn create_dir_with_permissions(path: &Path, mode: u32) -> Result<()> {
    if !path.exists() {
        async_fs::create_dir_all(path).await?;
    }

    // Set permissions on Unix-like systems
    #[cfg(unix)]
    {
        let metadata = async_fs::metadata(path).await?;
        let mut permissions = metadata.permissions();
        permissions.set_mode(mode);
        async_fs::set_permissions(path, permissions).await?;
    }

    // On Windows and other platforms, we can't set Unix-style permissions
    // The OS will use default permissions, and validation will catch issues
    #[cfg(not(unix))]
    {
        // For now, just ensure the directory was created successfully
        // Windows permission handling would need more complex ACL management
        if !path.exists() {
            return Err(Error::Io(std::io::Error::new(
                std::io::ErrorKind::Other,
                "Failed to create directory on non-Unix platform",
            )));
        }
    }

    Ok(())
}

/// Load the account registry from disk
pub async fn load_registry(config: &CredentialsConfig) -> Result<AccountRegistry> {
    let accounts_file = config.accounts_file()?;

    if !accounts_file.exists() {
        return Ok(AccountRegistry::new());
    }

    let content = async_fs::read_to_string(&accounts_file).await?;
    let mut registry: AccountRegistry = toml::from_str(&content)?;

    // Clean up stale metadata (e.g., long-lived error accounts) before semantic validation.
    // This allows us to drop obviously expired/error entries even if their timestamps are
    // inconsistent (which can otherwise cause validation to fail during load).
    let removed = cleanup_stale_metadata(&mut registry);

    // Validate against schema after cleanup so only relevant entries are checked.
    validate_registry_schema(&registry)?;
    if !removed.is_empty() {
        tracing::info!(
            "Removed {} stale account(s) from registry: {:?}",
            removed.len(),
            removed
        );
    }

    Ok(registry)
}

/// Validate registry against JSON schema (with caching for performance)
fn validate_registry_schema(registry: &AccountRegistry) -> Result<()> {
    use schemars::schema_for;
    use std::sync::OnceLock;

    // Cache the compiled schema to avoid repeated compilation
    static COMPILED_SCHEMA: OnceLock<std::result::Result<jsonschema::JSONSchema, String>> =
        OnceLock::new();

    let compiled_schema = COMPILED_SCHEMA.get_or_init(|| {
        // Generate schema for AccountRegistry
        let schema = schema_for!(AccountRegistry);
        let schema_json = serde_json::to_value(&schema)
            .map_err(|e| format!("Failed to serialize schema: {}", e))?;

        // Compile and cache the schema
        jsonschema::JSONSchema::compile(&schema_json)
            .map_err(|e| format!("Failed to compile schema: {}", e))
    });

    let compiled_schema = match compiled_schema {
        Ok(schema) => schema,
        Err(err) => {
            return Err(Error::Validation(format!(
                "Schema compilation failed: {}",
                err
            )));
        }
    };

    // Convert registry to JSON for validation
    let registry_json = serde_json::to_value(registry).map_err(|e| {
        Error::Validation(format!(
            "Failed to serialize registry for validation: {}",
            e
        ))
    })?;

    // Validate using the cached compiled schema
    let validation_result = compiled_schema.validate(&registry_json);
    if let Err(errors) = validation_result {
        let error_messages: Vec<String> = errors.map(|e| e.to_string()).collect();
        return Err(Error::Validation(format!(
            "Schema validation failed: {}",
            error_messages.join(", ")
        )));
    }

    // Additional semantic validation
    for account in &registry.accounts {
        if account.name.is_empty() {
            return Err(Error::Validation(
                "Account name cannot be empty".to_string(),
            ));
        }

        // Validate account name format (reuse existing validation)
        crate::validation::validate_account_name(&account.name)?;

        // Ensure created time is not in the future (basic sanity check)
        let now = chrono::Utc::now();
        if account.created > now + chrono::Duration::hours(1) {
            return Err(Error::Validation(format!(
                "Account '{}' created time is too far in the future",
                account.name
            )));
        }

        // Ensure last_used is not before created
        if account.last_used < account.created {
            return Err(Error::Validation(format!(
                "Account '{}' last_used time is before created time",
                account.name
            )));
        }
    }

    Ok(())
}

/// Save the account registry to disk
pub async fn save_registry(config: &CredentialsConfig, registry: &AccountRegistry) -> Result<()> {
    // Ensure directories exist
    ensure_credentials_dirs(config).await?;

    let accounts_file = config.accounts_file()?;
    let content = toml::to_string_pretty(registry)?;

    // Write to temporary file first, then rename for atomicity
    let temp_file = accounts_file.with_extension("tmp");
    async_fs::write(&temp_file, &content).await?;

    // Set permissions on the temp file (0600)
    #[cfg(unix)]
    {
        let metadata = async_fs::metadata(&temp_file).await?;
        let mut permissions = metadata.permissions();
        permissions.set_mode(0o600);
        async_fs::set_permissions(&temp_file, permissions).await?;
    }

    // Atomic rename
    async_fs::rename(&temp_file, &accounts_file).await?;

    Ok(())
}

/// Get the credential file path for an account
pub fn credential_file_path(
    config: &CredentialsConfig,
    account_name: &str,
    encrypted: bool,
) -> Result<std::path::PathBuf> {
    let keys_dir = config.keys_dir()?;
    let extension = if encrypted { "enc" } else { "json" };
    Ok(keys_dir.join(format!("{}.{}", account_name, extension)))
}

/// Write credential payload to disk with strict permissions (0600)
pub async fn write_credential_file(
    config: &CredentialsConfig,
    account_name: &str,
    encrypted: bool,
    data: &[u8],
) -> Result<std::path::PathBuf> {
    // Ensure directory structure exists
    ensure_credentials_dirs(config).await?;

    let path = credential_file_path(config, account_name, encrypted)?;
    async_fs::write(&path, data).await?;

    // Restrict permissions to owner read/write
    #[cfg(unix)]
    {
        let metadata = async_fs::metadata(&path).await?;
        let mut permissions = metadata.permissions();
        permissions.set_mode(0o600);
        async_fs::set_permissions(&path, permissions).await?;
    }

    Ok(path)
}

/// Check if a credential file exists for an account
pub async fn credential_file_exists(
    config: &CredentialsConfig,
    account_name: &str,
    encrypted: bool,
) -> Result<bool> {
    let path = credential_file_path(config, account_name, encrypted)?;
    Ok(path.exists())
}

/// Clean up stale temporary directories
/// Removes temp directories older than the specified age in seconds
pub async fn cleanup_temp_dirs(config: &CredentialsConfig, max_age_seconds: u64) -> Result<()> {
    let temp_dir = config.temp_dir()?;

    if !temp_dir.exists() {
        return Ok(());
    }

    let mut entries = async_fs::read_dir(&temp_dir).await?;
    let now = std::time::SystemTime::now();

    while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();

        // Only process directories
        if !path.is_dir() {
            continue;
        }

        // Check if directory is older than max_age_seconds
        if let Ok(metadata) = entry.metadata().await {
            if let Ok(modified) = metadata.modified() {
                if let Ok(age) = now.duration_since(modified) {
                    if age.as_secs() > max_age_seconds {
                        // Remove the stale directory
                        if let Err(e) = async_fs::remove_dir_all(&path).await {
                            tracing::warn!(
                                "Failed to remove stale temp dir {}: {}",
                                path.display(),
                                e
                            );
                        } else {
                            tracing::debug!("Removed stale temp dir: {}", path.display());
                        }
                    }
                }
            }
        }
    }

    Ok(())
}

/// Validate that all required directories and files have correct permissions
pub async fn validate_permissions(config: &CredentialsConfig) -> Result<()> {
    let dirs = vec![
        config.storage_dir()?,
        config.keys_dir()?,
        config.temp_dir()?,
    ];

    for dir in dirs {
        if !dir.exists() {
            return Err(Error::DirectoryNotAccessible(dir));
        }

        #[cfg(unix)]
        {
            let metadata = fs::metadata(&dir)?;
            let permissions = metadata.permissions();
            let mode = permissions.mode();

            // Check that directory has owner-only access (0700)
            // Reject any group or other permissions
            if mode & 0o077 != 0 {
                return Err(Error::PermissionDenied(dir));
            }
        }

        #[cfg(not(unix))]
        {
            // For non-Unix platforms, we can't enforce Unix-style permissions
            // Log a warning but continue validation since we at least verified the directory exists
            tracing::warn!(
                "Permission validation not fully supported on non-Unix platforms for directory: {}",
                dir.display()
            );
            // Continue to next directory instead of failing
        }
    }

    // Validate credential files under keys/ (if any)
    let keys_dir = config.keys_dir()?;
    if keys_dir.exists() {
        let entries = fs::read_dir(&keys_dir)?;
        for entry in entries {
            let entry = entry?;
            if entry.file_type()?.is_file() {
                #[cfg(unix)]
                {
                    let metadata = entry.metadata()?;
                    let permissions = metadata.permissions();
                    let mode = permissions.mode();
                    if mode & 0o177 != 0 {
                        return Err(Error::PermissionDenied(entry.path()));
                    }
                }

                #[cfg(not(unix))]
                {
                    tracing::warn!(
                        "Permission validation not fully supported on non-Unix platforms for credential file: {}",
                        entry.path().display()
                    );
                }
            }
        }
    }

    // Also validate accounts.toml if it exists
    let accounts_file = config.accounts_file()?;
    if accounts_file.exists() {
        #[cfg(unix)]
        {
            let metadata = fs::metadata(&accounts_file)?;
            let permissions = metadata.permissions();
            let mode = permissions.mode();

            // Check that file has owner-only access (0600)
            if mode & 0o177 != 0 {
                return Err(Error::PermissionDenied(accounts_file));
            }
        }

        #[cfg(not(unix))]
        {
            tracing::warn!(
                "Permission validation not fully supported on non-Unix platforms for file: {}",
                accounts_file.display()
            );
            // Continue validation since we at least verified the file exists
        }
    }

    Ok(())
}
