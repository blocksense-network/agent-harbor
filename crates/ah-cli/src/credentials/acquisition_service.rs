// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

use ah_agents::{AcquisitionOptions, AcquisitionResult, CredentialAcquirer, run_acquisition};
use ah_credentials::{
    Account, AccountRegistry, AccountStatus, AgentType, CredentialsConfig,
    storage::write_account_credentials,
};
use anyhow::{Result, anyhow};
use chrono::Utc;

/// Helper that wires ah-agents acquisition to ah-credentials storage.
pub struct AcquisitionService {
    config: CredentialsConfig,
}

impl AcquisitionService {
    pub fn new(config: CredentialsConfig) -> Self {
        Self { config }
    }

    /// Run the agent-specific acquisition flow and persist the resulting credentials.
    pub async fn acquire_and_store<A: CredentialAcquirer>(
        &self,
        acquirer: &A,
        account_name: &str,
        encrypted: bool,
        passphrase: Option<&str>,
        options: Option<AcquisitionOptions>,
    ) -> Result<StoredAcquisition> {
        let temp_root = self.config.temp_dir()?;
        let opts = options.unwrap_or_else(|| AcquisitionOptions::with_temp_root(temp_root));

        let acquisition = run_acquisition(acquirer, opts).await?;

        let mut account = Account::new(
            account_name.to_string(),
            map_agent_type(acquirer.agent_kind())?,
        );
        account.encrypted = encrypted;
        account.status = if acquisition.is_expired() {
            AccountStatus::Expired
        } else {
            AccountStatus::Active
        };
        account.last_used = Utc::now();

        let registry = AccountRegistry::new(self.config.clone());
        registry.load().await?;
        registry.add_account(account.clone()).await?;
        registry.save().await?;

        let kdf = if encrypted {
            Some(self.config.kdf_params()?)
        } else {
            None
        };

        let credential_file = write_account_credentials(
            &self.config,
            &account,
            &acquisition.credentials,
            passphrase,
            kdf,
        )
        .await?;

        Ok(StoredAcquisition {
            account,
            credential_file,
            acquisition,
        })
    }
}

fn map_agent_type(kind: &str) -> Result<AgentType> {
    match kind {
        "codex" => Ok(AgentType::Codex),
        "claude" => Ok(AgentType::Claude),
        "cursor" => Ok(AgentType::Cursor),
        other => Err(anyhow!("Unsupported agent '{}'", other)),
    }
}

/// Summary of a persisted acquisition.
pub struct StoredAcquisition {
    pub account: Account,
    pub credential_file: std::path::PathBuf,
    pub acquisition: AcquisitionResult,
}
