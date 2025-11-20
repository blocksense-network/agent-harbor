// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Device management and access control for sandboxing.
//!
//! This crate provides device allowlisting and prohibition capabilities for containers
//! and virtual machines running within sandboxes.

#![cfg(target_os = "linux")]

pub mod error;

use nix::mount::{MsFlags, mount};
use std::collections::HashSet;
use std::path::Path;
use tracing::{debug, info, warn};

pub type Result<T> = std::result::Result<T, error::Error>;

/// Device access policy
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeviceAccess {
    /// Allow access to this device
    Allow,
    /// Deny access to this device
    Deny,
    /// Allow with specific permissions (future extension)
    AllowWithPerms(u32),
}

/// Configuration for device access control
#[derive(Debug, Clone)]
pub struct DeviceConfig {
    /// Whether containers are allowed (enables /dev/fuse, cgroup delegation)
    pub allow_containers: bool,
    /// Whether KVM access is allowed (enables /dev/kvm pass-through)
    pub allow_kvm: bool,
    /// Additional allowed device paths
    pub allowed_devices: Vec<String>,
    /// Prohibited device paths (takes precedence over allowed_devices)
    pub prohibited_devices: Vec<String>,
    /// Storage directories to pre-allow for containers
    pub container_storage_dirs: Vec<String>,
}

impl Default for DeviceConfig {
    fn default() -> Self {
        Self {
            allow_containers: false,
            allow_kvm: false,
            allowed_devices: Vec::new(),
            prohibited_devices: vec![
                // Default prohibited devices for security
                "/var/run/docker.sock".to_string(), // Host Docker socket
                "/run/docker.sock".to_string(),
            ],
            container_storage_dirs: vec![
                "/tmp".to_string(),
                "/var/tmp".to_string(),
                "/home".to_string(), // Allow but with restrictions via other means
            ],
        }
    }
}

impl DeviceConfig {
    /// Create configuration for container workloads
    pub fn for_containers() -> Self {
        Self {
            allow_containers: true,
            allow_kvm: false,
            allowed_devices: vec![
                "/dev/fuse".to_string(), // Required for overlay filesystems
                "/dev/null".to_string(),
                "/dev/zero".to_string(),
                "/dev/random".to_string(),
                "/dev/urandom".to_string(),
                "/dev/tty".to_string(),
                "/dev/ptmx".to_string(),
                "/dev/pts".to_string(),
            ],
            prohibited_devices: vec![
                "/var/run/docker.sock".to_string(),
                "/run/docker.sock".to_string(),
                "/dev/kvm".to_string(), // Not needed for containers
                "/dev/mem".to_string(),
                "/dev/kmem".to_string(),
            ],
            container_storage_dirs: vec![
                "/tmp".to_string(),
                "/var/tmp".to_string(),
                "/home".to_string(),
            ],
        }
    }

    /// Create configuration for VM workloads
    pub fn for_vms() -> Self {
        Self {
            allow_containers: false,
            allow_kvm: true,
            allowed_devices: vec![
                "/dev/kvm".to_string(), // KVM device for hardware acceleration
                "/dev/null".to_string(),
                "/dev/zero".to_string(),
                "/dev/random".to_string(),
                "/dev/urandom".to_string(),
                "/dev/tty".to_string(),
                "/dev/ptmx".to_string(),
                "/dev/pts".to_string(),
            ],
            prohibited_devices: vec![
                "/var/run/docker.sock".to_string(),
                "/run/docker.sock".to_string(),
                "/dev/fuse".to_string(), // Not needed for VMs
                "/dev/mem".to_string(),
                "/dev/kmem".to_string(),
            ],
            container_storage_dirs: Vec::new(), // VMs don't need container storage
        }
    }

    /// Create configuration for both containers and VMs
    pub fn for_containers_and_vms() -> Self {
        let mut config = Self::for_containers();
        config.allow_kvm = true;
        config.allowed_devices.push("/dev/kvm".to_string());
        config
    }

    /// Check if a device path is allowed
    pub fn is_device_allowed(&self, device_path: &str) -> bool {
        // Prohibited devices take precedence
        if self
            .prohibited_devices
            .iter()
            .any(|p| device_path.starts_with(p) || device_path == p)
        {
            return false;
        }

        // Check container-specific allowances
        if self.allow_containers && device_path == "/dev/fuse" {
            return true;
        }

        // Check KVM-specific allowances
        if self.allow_kvm && device_path == "/dev/kvm" {
            return true;
        }

        // Check explicit allowlist
        self.allowed_devices
            .iter()
            .any(|d| device_path.starts_with(d) || device_path == d)
    }

    /// Get the list of devices that should be accessible
    pub fn get_allowed_devices(&self) -> Vec<String> {
        let mut devices = self.allowed_devices.clone();

        if self.allow_containers {
            devices.push("/dev/fuse".to_string());
        }

        if self.allow_kvm {
            devices.push("/dev/kvm".to_string());
        }

        // Remove duplicates and prohibited devices
        let mut unique_devices: HashSet<String> = devices.into_iter().collect();
        unique_devices.retain(|d| self.is_device_allowed(d));

        unique_devices.into_iter().collect()
    }
}

/// Device manager for setting up device access in sandboxes
pub struct DeviceManager {
    config: DeviceConfig,
}

impl Default for DeviceManager {
    fn default() -> Self {
        Self::new(DeviceConfig::default())
    }
}

impl DeviceManager {
    /// Create a new device manager with the given configuration
    pub fn new(config: DeviceConfig) -> Self {
        Self { config }
    }

    /// Set up device access for the sandbox
    pub async fn setup_devices(&self) -> Result<()> {
        info!("Setting up device access with config: {:?}", self.config);

        // Create /dev directory if it doesn't exist (in test environments)
        self.ensure_dev_directory().await?;

        // Set up basic devices that are always needed
        self.setup_basic_devices().await?;

        // Set up container-specific devices if allowed
        if self.config.allow_containers {
            self.setup_container_devices().await?;
        }

        // Set up VM-specific devices if allowed
        if self.config.allow_kvm {
            self.setup_kvm_devices().await?;
        }

        // Bind mount allowed storage directories for containers
        if self.config.allow_containers {
            self.setup_container_storage().await?;
        }

        debug!("Device setup complete");
        Ok(())
    }

    /// Clean up device mounts and configurations
    pub fn cleanup_devices(&self) -> Result<()> {
        debug!("Cleaning up device configurations");
        // For now, cleanup is handled by namespace teardown
        // In the future, we might need to explicitly unmount device files
        Ok(())
    }

    /// Check if a device access is permitted
    pub fn is_access_allowed(&self, device_path: &str) -> bool {
        self.config.is_device_allowed(device_path)
    }

    /// Get the current device configuration
    pub fn config(&self) -> &DeviceConfig {
        &self.config
    }

    /// Ensure /dev directory exists
    async fn ensure_dev_directory(&self) -> Result<()> {
        let dev_path = Path::new("/dev");
        if !dev_path.exists() {
            tokio::fs::create_dir_all(dev_path).await.map_err(|e| {
                error::Error::Io(std::io::Error::other(format!(
                    "Failed to create /dev directory: {}",
                    e
                )))
            })?;
        }
        Ok(())
    }

    /// Set up basic devices needed for all sandboxes
    async fn setup_basic_devices(&self) -> Result<()> {
        let basic_devices = vec![
            "/dev/null",
            "/dev/zero",
            "/dev/random",
            "/dev/urandom",
            "/dev/tty",
            "/dev/ptmx",
        ];

        for device in basic_devices {
            if Path::new(device).exists() {
                // In a real implementation, we might need to bind mount or create device nodes
                // For now, we just ensure the devices exist and are accessible
                debug!("Basic device {} is available", device);
            } else {
                warn!(
                    "Basic device {} not found, some functionality may be limited",
                    device
                );
            }
        }

        Ok(())
    }

    /// Set up container-specific devices
    async fn setup_container_devices(&self) -> Result<()> {
        info!("Setting up container-specific devices");

        // /dev/fuse is critical for overlay filesystems used by containers
        if Path::new("/dev/fuse").exists() {
            debug!("/dev/fuse is available for container overlay filesystems");
        } else {
            warn!("/dev/fuse not available, container overlay functionality may be limited");
        }

        // Set up /dev/pts for pseudo-terminals
        self.setup_devpts().await?;

        Ok(())
    }

    /// Set up KVM devices for virtual machines
    async fn setup_kvm_devices(&self) -> Result<()> {
        info!("Setting up KVM devices");

        if Path::new("/dev/kvm").exists() {
            debug!("/dev/kvm is available for hardware-accelerated virtualization");
        } else {
            warn!("/dev/kvm not available, VMs will use software emulation (slower)");
        }

        Ok(())
    }

    /// Set up container storage directories
    async fn setup_container_storage(&self) -> Result<()> {
        info!("Setting up container storage directories");

        for dir in &self.config.container_storage_dirs {
            let path = Path::new(dir);
            if path.exists() {
                debug!("Container storage directory {} is available", dir);
            } else {
                // Create directory if it doesn't exist
                if let Err(e) = tokio::fs::create_dir_all(path).await {
                    warn!(
                        "Failed to create container storage directory {}: {}",
                        dir, e
                    );
                } else {
                    debug!("Created container storage directory {}", dir);
                }
            }
        }

        Ok(())
    }

    /// Set up /dev/pts for pseudo-terminals
    async fn setup_devpts(&self) -> Result<()> {
        let devpts_path = Path::new("/dev/pts");

        if !devpts_path.exists() {
            // Try to mount devpts if it doesn't exist
            match mount(
                Some("devpts"),
                "/dev/pts",
                Some("devpts"),
                MsFlags::empty(),
                Some("newinstance,ptmxmode=0666"),
            ) {
                Ok(()) => debug!("Mounted devpts at /dev/pts"),
                Err(e) => {
                    warn!("Failed to mount devpts: {}", e);
                    // Try to create the directory at least
                    if let Err(e) = tokio::fs::create_dir_all(devpts_path).await {
                        warn!("Failed to create /dev/pts directory: {}", e);
                    }
                }
            }
        } else {
            debug!("/dev/pts already exists");
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_device_config_defaults() {
        let config = DeviceConfig::default();

        assert!(!config.allow_containers);
        assert!(!config.allow_kvm);
        assert!(config.allowed_devices.is_empty());
        assert!(!config.prohibited_devices.is_empty()); // Should have Docker socket prohibitions
        assert!(!config.container_storage_dirs.is_empty());
    }

    #[test]
    fn test_container_config() {
        let config = DeviceConfig::for_containers();

        assert!(config.allow_containers);
        assert!(!config.allow_kvm);
        assert!(config.allowed_devices.contains(&"/dev/fuse".to_string()));
        assert!(config.allowed_devices.contains(&"/dev/null".to_string()));
        assert!(config.prohibited_devices.contains(&"/var/run/docker.sock".to_string()));
        assert!(!config.prohibited_devices.contains(&"/dev/fuse".to_string()));
    }

    #[test]
    fn test_vm_config() {
        let config = DeviceConfig::for_vms();

        assert!(!config.allow_containers);
        assert!(config.allow_kvm);
        assert!(config.allowed_devices.contains(&"/dev/kvm".to_string()));
        assert!(config.allowed_devices.contains(&"/dev/null".to_string()));
        assert!(config.prohibited_devices.contains(&"/dev/fuse".to_string()));
        assert!(!config.prohibited_devices.contains(&"/dev/kvm".to_string()));
    }

    #[test]
    fn test_combined_config() {
        let config = DeviceConfig::for_containers_and_vms();

        assert!(config.allow_containers);
        assert!(config.allow_kvm);
        assert!(config.allowed_devices.contains(&"/dev/fuse".to_string()));
        assert!(config.allowed_devices.contains(&"/dev/kvm".to_string()));
    }

    #[test]
    fn test_device_allowance() {
        let container_config = DeviceConfig::for_containers();
        let vm_config = DeviceConfig::for_vms();

        // Container config should allow /dev/fuse but not /dev/kvm
        assert!(container_config.is_device_allowed("/dev/fuse"));
        assert!(!container_config.is_device_allowed("/dev/kvm"));
        assert!(!container_config.is_device_allowed("/var/run/docker.sock"));

        // VM config should allow /dev/kvm but not /dev/fuse
        assert!(!vm_config.is_device_allowed("/dev/fuse"));
        assert!(vm_config.is_device_allowed("/dev/kvm"));
        assert!(!vm_config.is_device_allowed("/var/run/docker.sock"));
    }

    #[test]
    fn test_get_allowed_devices() {
        let config = DeviceConfig::for_containers();
        let allowed = config.get_allowed_devices();

        assert!(allowed.contains(&"/dev/fuse".to_string()));
        assert!(allowed.contains(&"/dev/null".to_string()));
        assert!(!allowed.contains(&"/dev/kvm".to_string()));
        assert!(!allowed.contains(&"/var/run/docker.sock".to_string()));
    }

    #[tokio::test]
    async fn test_device_manager_creation() {
        let config = DeviceConfig::default();
        let manager = DeviceManager::new(config);

        // Should be able to set up devices (may fail in test environment due to permissions)
        let result = manager.setup_devices().await;
        // We expect this to succeed or fail gracefully in test environments
        assert!(result.is_ok() || matches!(result, Err(error::Error::Io(_))));

        let cleanup_result = manager.cleanup_devices();
        assert!(cleanup_result.is_ok());
    }
}
