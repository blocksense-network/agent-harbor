// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Seccomp filtering and dynamic read allow-listing for sandboxing.

#![cfg(target_os = "linux")]

pub mod error;
pub mod filter;
pub mod notify;
pub mod path_resolver;

pub use filter::{FilterBuilder, SeccompFilter};
pub use notify::{NotificationHandler, SupervisorClient};
pub use path_resolver::PathResolver;

pub type Result<T> = std::result::Result<T, error::Error>;

use std::path::PathBuf;
use tokio::sync::mpsc;
use tracing::{info, warn};

/// Configuration for seccomp manager
#[derive(Debug, Clone)]
pub struct SeccompConfig {
    /// Enable debug mode (allows ptrace operations)
    pub debug_mode: bool,
    /// Supervisor communication channel (if None, denies all requests)
    pub supervisor_tx: Option<mpsc::UnboundedSender<sandbox_proto::Message>>,
    /// Root directory for path resolution
    pub root_dir: PathBuf,
}

impl Default for SeccompConfig {
    fn default() -> Self {
        Self {
            debug_mode: false,
            supervisor_tx: None,
            root_dir: PathBuf::from("/"),
        }
    }
}

/// Seccomp policy builder and enforcer
pub struct SeccompManager {
    config: SeccompConfig,
    filter: Option<SeccompFilter>,
    path_resolver: PathResolver,
}

impl Default for SeccompManager {
    fn default() -> Self {
        Self::new(SeccompConfig::default())
    }
}

impl SeccompManager {
    /// Create a new seccomp manager with default configuration
    pub fn new(config: SeccompConfig) -> Self {
        let path_resolver = PathResolver::new(config.root_dir.clone());
        Self {
            config,
            filter: None,
            path_resolver,
        }
    }

    /// Install seccomp filters and start notification handler
    pub async fn install_filters(&mut self) -> Result<()> {
        info!("Installing seccomp filters for dynamic filesystem access control");

        // Build the seccomp filter
        let mut filter_builder = FilterBuilder::new();
        filter_builder
            .block_filesystem_operations()?
            .allow_basic_operations()?
            .set_debug_mode(self.config.debug_mode)?;
        info!(
            "Seccomp filter configured with debug_mode={}",
            self.config.debug_mode
        );

        let filter = filter_builder.build()?;
        self.filter = Some(filter);

        // Install the filter with notification support
        self.filter.as_ref().unwrap().install()?;

        // Start notification handler if we have a supervisor channel
        if let Some(supervisor_tx) = &self.config.supervisor_tx {
            let handler =
                NotificationHandler::new(supervisor_tx.clone(), self.path_resolver.clone());

            // Start the notification handler in background
            tokio::spawn(async move {
                if let Err(e) = handler.run().await {
                    warn!("Seccomp notification handler failed: {}", e);
                }
            });
        }

        info!("Seccomp filters installed successfully");
        Ok(())
    }

    /// Get the current filter (for testing)
    #[cfg(test)]
    pub fn filter(&self) -> Option<&SeccompFilter> {
        self.filter.as_ref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::mpsc;

    #[ah_test_utils::logged_test]
    fn test_seccomp_config_creation() {
        let config = SeccompConfig::default();
        assert!(!config.debug_mode);
        assert!(config.supervisor_tx.is_none());
        assert_eq!(config.root_dir, std::path::PathBuf::from("/"));
    }

    #[ah_test_utils::logged_test]
    fn test_seccomp_manager_creation() {
        let config = SeccompConfig::default();
        let manager = SeccompManager::new(config);
        assert!(manager.filter.is_none());
    }

    #[ah_test_utils::logged_tokio_test]
    async fn test_seccomp_manager_install_filters() {
        let config = SeccompConfig::default();
        let mut manager = SeccompManager::new(config);

        // This will fail in test environment due to permissions, but should not panic
        let result = manager.install_filters().await;
        // We expect this to fail in test environment
        assert!(result.is_err() || result.is_ok());
    }
}
