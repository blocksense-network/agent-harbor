// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Credential acquisition helpers for agent logins.
//!
//! This module keeps the acquisition pipeline close to the agent adapters while
//! remaining library-first. The helpers:
//! - create a unique, isolated HOME under the configured temp root
//! - launch the agent in interactive mode so the user can complete login
//! - extract the produced credential payloads as JSON key-value maps
//! - surface basic expiry metadata for downstream verification
//! - clean up the temporary HOME to avoid credential leakage

use crate::{AgentError, AgentResult};
use chrono::{DateTime, Duration, Utc};
use serde_json::Value;
use std::path::{Path, PathBuf};
use tempfile::Builder;
use tokio::process::Child;
use tokio::time::timeout;
use tracing::{debug, warn};

/// Options that control acquisition behavior.
#[derive(Debug, Clone)]
pub struct AcquisitionOptions {
    /// Root directory where temporary HOMEs will be created.
    pub temp_root: PathBuf,
    /// Remove the temporary HOME after extraction succeeds.
    pub cleanup: bool,
    /// Maximum time to wait for the agent login flow to finish.
    pub timeout: std::time::Duration,
}

impl AcquisitionOptions {
    /// Build options using the provided temp root.
    pub fn with_temp_root(temp_root: PathBuf) -> Self {
        Self {
            temp_root,
            ..Default::default()
        }
    }
}

impl Default for AcquisitionOptions {
    fn default() -> Self {
        Self {
            temp_root: std::env::temp_dir().join("agent-harbor").join("credentials-temp"),
            cleanup: true,
            timeout: std::time::Duration::from_secs(300),
        }
    }
}

/// Result of a credential acquisition run.
#[derive(Debug, Clone)]
pub struct AcquisitionResult {
    /// Extracted credential payload.
    pub credentials: Value,
    /// Optional expiry timestamp extracted from the payload.
    pub expires_at: Option<DateTime<Utc>>,
    /// Path to the temporary HOME that was used (may be removed when cleanup=true).
    pub temp_home: PathBuf,
}

impl AcquisitionResult {
    /// Whether the credentials are already expired according to the extracted metadata.
    pub fn is_expired(&self) -> bool {
        matches!(self.expires_at, Some(ts) if ts <= Utc::now())
    }
}

/// Trait implemented by agent-specific acquisition helpers.
#[async_trait::async_trait]
pub trait CredentialAcquirer: Send + Sync {
    /// Human-readable agent kind, e.g. "codex" or "claude".
    fn agent_kind(&self) -> &'static str;

    /// Launch the agent in interactive login mode using the provided HOME.
    async fn launch_for_login(&self, home_dir: &Path) -> AgentResult<Child>;

    /// Extract credentials produced by the agent under the provided HOME.
    async fn extract_credentials(&self, home_dir: &Path) -> AgentResult<Value>;

    /// Detect expiry timestamp from the credential payload, if present.
    fn detect_expiry(&self, credentials: &Value) -> Option<DateTime<Utc>> {
        extract_expiry(credentials)
    }
}

/// Run the acquisition pipeline for the given agent helper.
pub async fn run_acquisition<A: CredentialAcquirer>(
    acquirer: &A,
    options: AcquisitionOptions,
) -> AgentResult<AcquisitionResult> {
    // Ensure temp root exists with restrictive permissions.
    tokio::fs::create_dir_all(&options.temp_root).await.map_err(|e| {
        AgentError::ConfigCreationFailed(format!(
            "Failed to create temp root {}: {}",
            options.temp_root.display(),
            e
        ))
    })?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let metadata = tokio::fs::metadata(&options.temp_root).await?;
        let mut perms = metadata.permissions();
        perms.set_mode(0o700);
        tokio::fs::set_permissions(&options.temp_root, perms).await?;
    }

    // Allocate a unique HOME under the temp root.
    let tempdir = Builder::new()
        .prefix("ah-acq-")
        .tempdir_in(&options.temp_root)
        .map_err(|e| AgentError::ConfigCreationFailed(e.to_string()))?;

    let home_dir = if options.cleanup {
        tempdir.path().to_path_buf()
    } else {
        // Persist the directory for debugging when cleanup is disabled.
        #[allow(deprecated)]
        {
            tempdir.into_path()
        }
    };

    let mut child = acquirer.launch_for_login(&home_dir).await?;

    // Wait for completion with timeout handling.
    let status = match timeout(options.timeout, child.wait()).await {
        Ok(result) => result.map_err(AgentError::ProcessSpawnFailed)?,
        Err(_) => {
            let _ = child.kill().await;
            return Err(AgentError::ConfigurationError(format!(
                "Timed out waiting for {} login flow after {:?}",
                acquirer.agent_kind(),
                options.timeout
            )));
        }
    };

    if !status.success() {
        return Err(AgentError::Other(anyhow::anyhow!(
            "{} login exited with status {}",
            acquirer.agent_kind(),
            status
        )));
    }

    // Extract credentials and expiry metadata.
    let credentials = acquirer.extract_credentials(&home_dir).await?;
    let expires_at = acquirer.detect_expiry(&credentials);

    // Best-effort cleanup of the temporary HOME to avoid leaking tokens.
    if options.cleanup {
        if let Err(e) = tokio::fs::remove_dir_all(&home_dir).await {
            warn!("Failed to clean up temp HOME {}: {}", home_dir.display(), e);
        } else {
            debug!("Cleaned up temp HOME {}", home_dir.display());
        }
    }

    Ok(AcquisitionResult {
        credentials,
        expires_at,
        temp_home: home_dir,
    })
}

/// Extract an expiry timestamp from common credential shapes.
fn extract_expiry(credentials: &Value) -> Option<DateTime<Utc>> {
    // ISO-8601 string fields commonly used in specs.
    let string_fields = ["expires_at", "expiry", "expires"];
    for key in string_fields {
        if let Some(s) = credentials.get(key).and_then(|v| v.as_str()) {
            if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
                return Some(dt.with_timezone(&Utc));
            }
        }
    }

    // Relative expiry in seconds.
    if let Some(seconds) = credentials.get("expires_in").and_then(|v| v.as_i64()) {
        return Some(Utc::now() + Duration::seconds(seconds));
    }

    None
}
