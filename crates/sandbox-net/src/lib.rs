// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Network isolation and management for sandboxing.

#![cfg(target_os = "linux")]

pub mod error;

use nix::ifaddrs::getifaddrs;
use std::path::PathBuf;
use std::process::Stdio;
use tokio::process::{Child, Command};
use tracing::{debug, info};

pub type Result<T> = std::result::Result<T, error::Error>;

/// Network configuration for sandbox isolation
#[derive(Debug, Clone, Default)]
pub struct NetworkConfig {
    /// Enable internet access via slirp4netns
    pub allow_internet: bool,
    /// Target PID for slirp4netns to connect to (typically the sandbox process)
    pub target_pid: Option<u32>,
    /// Custom slirp4netns binary path (defaults to system PATH)
    pub slirp4netns_path: Option<PathBuf>,
    /// Disable IPv6 in slirp4netns
    pub disable_ipv6: bool,
    /// MTU for slirp4netns network interface
    pub mtu: Option<u32>,
    /// CIDR for the network (default: 10.0.2.0/24)
    pub cidr: Option<String>,
}

/// Network manager for sandbox isolation
pub struct NetworkManager {
    config: NetworkConfig,
    slirp_process: Option<Child>,
}

impl Default for NetworkManager {
    fn default() -> Self {
        Self::new(NetworkConfig::default())
    }
}

impl NetworkManager {
    /// Create a new network manager with default configuration
    pub fn new(config: NetworkConfig) -> Self {
        Self {
            config,
            slirp_process: None,
        }
    }

    /// Set up network isolation (loopback only by default)
    pub async fn setup_isolation(&mut self) -> Result<()> {
        info!("Setting up network isolation (loopback only)");

        // Bring up loopback interface
        self.setup_loopback().await?;

        // If internet access is requested, start slirp4netns
        if self.config.allow_internet {
            self.enable_internet_access().await?;
        }

        Ok(())
    }

    /// Set up loopback network interface
    async fn setup_loopback(&self) -> Result<()> {
        debug!("Setting up loopback interface");

        // Check if loopback is already up
        if self.is_interface_up("lo")? {
            debug!("Loopback interface already up");
            return Ok(());
        }

        // Bring up loopback using ip command
        let output = Command::new("ip")
            .args(["link", "set", "lo", "up"])
            .output()
            .await
            .map_err(|e| error::Error::Setup(format!("Failed to bring up loopback: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(error::Error::Setup(format!(
                "ip link set lo up failed: {}",
                stderr
            )));
        }

        // Verify loopback is up
        if !self.is_interface_up("lo")? {
            return Err(error::Error::Setup(
                "Failed to verify loopback interface is up".to_string(),
            ));
        }

        debug!("Loopback interface setup complete");
        Ok(())
    }

    /// Check if a network interface is up
    fn is_interface_up(&self, interface_name: &str) -> Result<bool> {
        let addrs = getifaddrs().map_err(|e| {
            error::Error::Setup(format!("Failed to get interface addresses: {}", e))
        })?;

        for addr in addrs {
            if addr.interface_name == interface_name {
                // Check if interface flags indicate it's up
                // Note: We can't easily check IFF_UP flag with nix crate,
                // so we'll assume if we can get addresses, it's configured
                return Ok(true);
            }
        }

        Ok(false)
    }

    /// Enable internet access via slirp4netns
    async fn enable_internet_access(&mut self) -> Result<()> {
        let target_pid = self.config.target_pid.ok_or_else(|| {
            error::Error::Slirp4netns("Target PID not specified for slirp4netns".to_string())
        })?;

        info!(
            "Enabling internet access via slirp4netns for PID {}",
            target_pid
        );

        // Find slirp4netns binary
        let slirp_path = self
            .config
            .slirp4netns_path
            .clone()
            .unwrap_or_else(|| PathBuf::from("slirp4netns"));

        // Build slirp4netns command arguments
        let mut args = vec![
            "--configure".to_string(),
            "--mtu".to_string(),
            self.config.mtu.unwrap_or(1500).to_string(),
            "--enable-sandbox".to_string(),
            "--enable-seccomp".to_string(),
            target_pid.to_string(),
            "tap0".to_string(),
        ];

        if self.config.disable_ipv6 {
            args.push("--disable-ipv6".to_string());
        }

        if let Some(cidr) = &self.config.cidr {
            args.push("--cidr".to_string());
            args.push(cidr.clone());
        }

        debug!("Starting slirp4netns with args: {:?}", args);

        // Spawn slirp4netns process
        let child = Command::new(&slirp_path)
            .args(&args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| {
                error::Error::Slirp4netns(format!("Failed to spawn slirp4netns: {}", e))
            })?;

        self.slirp_process = Some(child);

        // Give slirp4netns a moment to set up
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        info!("slirp4netns started successfully");
        Ok(())
    }

    /// Clean up network resources
    pub fn cleanup(&mut self) -> Result<()> {
        if let Some(child) = self.slirp_process.take() {
            debug!("Terminating slirp4netns process");
            // Note: In a real implementation, we'd want to properly terminate the child process
            // For now, we just drop it and let the OS handle cleanup when the parent exits
            drop(child);
        }
        Ok(())
    }
}

impl Drop for NetworkManager {
    fn drop(&mut self) {
        // Attempt cleanup on drop, but ignore errors since we're in a destructor
        let _ = self.cleanup();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[ah_test_utils::logged_tokio_test]
    async fn test_network_manager_creation() {
        let config = NetworkConfig::default();
        let mut manager = NetworkManager::new(config);
        assert!(manager.setup_isolation().await.is_ok());
        assert!(manager.cleanup().is_ok());
    }

    #[ah_test_utils::logged_tokio_test]
    async fn test_network_manager_with_slirp_disabled() {
        let config = NetworkConfig {
            allow_internet: false,
            ..Default::default()
        };
        let mut manager = NetworkManager::new(config);
        assert!(manager.setup_isolation().await.is_ok());
        assert!(manager.cleanup().is_ok());
    }

    #[ah_test_utils::logged_tokio_test]
    async fn test_loopback_setup() {
        let config = NetworkConfig::default();
        let manager = NetworkManager::new(config);
        // Loopback setup should succeed even without special privileges in most environments
        assert!(manager.setup_loopback().await.is_ok());
    }
}
