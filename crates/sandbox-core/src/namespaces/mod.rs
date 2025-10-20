//! Linux namespace management for sandbox isolation.

use nix::sched::{CloneFlags, unshare};
use nix::unistd::{Gid, Uid, getuid, setgroups, setresgid, setresuid};
use tracing::{debug, info, warn};

use crate::Result;
use crate::error::Error;

/// Configuration for namespace isolation
///
/// Note: Enabling user_ns allows unprivileged users to create other namespaces
/// within the same unshare() call, since they become "root" within the new user namespace.
/// This is crucial for sandboxing to work without requiring CAP_SYS_ADMIN in the parent namespace.
#[derive(Debug, Clone, Default)]
pub struct NamespaceConfig {
    /// Enable user namespace isolation
    ///
    /// When enabled, allows creation of other namespaces by unprivileged users.
    /// The process becomes root within the user namespace and can create mount, PID, etc. namespaces.
    pub user_ns: bool,
    /// Enable mount namespace isolation
    pub mount_ns: bool,
    /// Enable PID namespace isolation
    pub pid_ns: bool,
    /// Enable UTS namespace isolation
    pub uts_ns: bool,
    /// Enable IPC namespace isolation
    pub ipc_ns: bool,
    /// Enable time namespace isolation (optional, newer kernel feature)
    pub time_ns: bool,
    /// UID mapping for user namespace
    pub uid_map: Option<String>,
    /// GID mapping for user namespace
    pub gid_map: Option<String>,
}

/// Manager for Linux namespaces
pub struct NamespaceManager {
    config: NamespaceConfig,
}

impl NamespaceManager {
    /// Create a new namespace manager with the given configuration
    pub fn new(config: NamespaceConfig) -> Self {
        Self { config }
    }

    /// Enter all configured namespaces in a single unshare() call
    ///
    /// This creates ALL namespaces (user, mount, PID, UTS, IPC) in one atomic operation.
    /// User namespaces (CLONE_NEWUSER) enable unprivileged creation since Linux 3.8.
    /// When CLONE_NEWUSER is included, the process becomes "root" within the user namespace,
    /// allowing creation of other namespaces without CAP_SYS_ADMIN in the parent namespace.
    ///
    /// After namespace creation, UID/GID mappings are set up if user namespaces are enabled.
    pub fn enter_namespaces(&self) -> Result<()> {
        info!("Entering namespaces: {:?}", self.config);

        let mut flags = CloneFlags::empty();

        if self.config.user_ns {
            flags |= CloneFlags::CLONE_NEWUSER;
        }
        if self.config.mount_ns {
            flags |= CloneFlags::CLONE_NEWNS;
        }
        if self.config.pid_ns {
            flags |= CloneFlags::CLONE_NEWPID;
        }
        if self.config.uts_ns {
            flags |= CloneFlags::CLONE_NEWUTS;
        }
        if self.config.ipc_ns {
            flags |= CloneFlags::CLONE_NEWIPC;
        }
        if self.config.time_ns {
            // TIME namespace requires kernel 5.6+
            // Note: CLONE_NEWTIME may not be available in older nix versions
            warn!("TIME namespace requested but not supported in this nix version");
        }

        if !flags.is_empty() {
            unshare(flags).map_err(|e| {
                warn!("Failed to unshare namespaces: {}", e);
                Error::Namespace(format!("Failed to unshare namespaces: {}", e))
            })?;
        }

        // Set up user namespace mappings if enabled
        if self.config.user_ns {
            self.setup_user_mappings()?;
        }

        debug!("Successfully entered namespaces");
        Ok(())
    }

    /// Set up UID/GID mappings for user namespace
    ///
    /// After creating a user namespace, we need to establish UID/GID mappings
    /// by writing to /proc/self/uid_map and /proc/self/gid_map. This tells the kernel
    /// how to map UIDs/GIDs between the parent and child namespaces.
    ///
    /// This operation requires CAP_SETUID/CAP_SETGID capabilities or root privileges
    /// in the parent namespace. However, if the user namespace was created by an
    /// unprivileged user, they can only map their own UID/GID (no privilege escalation).
    fn setup_user_mappings(&self) -> Result<()> {
        // For user namespaces, we need to write to /proc/self/uid_map and /proc/self/gid_map
        // This must be done after unshare but before executing the child

        if let Some(uid_map) = &self.config.uid_map {
            self.write_mapping("/proc/self/uid_map", uid_map)?;
        } else {
            // Default mapping: current UID maps to root in namespace (equivalent to --map-root-user)
            let uid = getuid().as_raw();
            let default_uid_map = format!("{} 0 1", uid);
            self.write_mapping("/proc/self/uid_map", &default_uid_map)?;
        }

        if let Some(gid_map) = &self.config.gid_map {
            self.write_mapping("/proc/self/gid_map", gid_map)?;
        } else {
            // Default mapping: current GID maps to root in namespace (equivalent to --map-root-user)
            let gid = nix::unistd::getgid().as_raw();
            let default_gid_map = format!("{} 0 1", gid);
            self.write_mapping("/proc/self/gid_map", &default_gid_map)?;
        }

        // Set setgroups to "deny" for user namespaces (required since Linux 3.19)
        self.write_mapping("/proc/self/setgroups", "deny")?;

        // Switch to root in the namespace
        setresuid(Uid::from_raw(0), Uid::from_raw(0), Uid::from_raw(0)).map_err(|e| {
            warn!("Failed to set uid: {}", e);
            Error::Namespace(format!("Failed to set uid: {}", e))
        })?;

        setresgid(Gid::from_raw(0), Gid::from_raw(0), Gid::from_raw(0)).map_err(|e| {
            warn!("Failed to set gid: {}", e);
            Error::Namespace(format!("Failed to set gid: {}", e))
        })?;

        debug!("User namespace mappings configured");
        Ok(())
    }

    /// Write a mapping to a proc file
    fn write_mapping(&self, path: &str, content: &str) -> Result<()> {
        use std::fs::OpenOptions;
        use std::io::Write;

        let mut file = OpenOptions::new().write(true).open(path).map_err(|e| {
            warn!("Failed to open {}: {}", path, e);
            Error::Namespace(format!("Failed to open {}: {}", path, e))
        })?;

        file.write_all(content.as_bytes()).map_err(|e| {
            warn!("Failed to write to {}: {}", path, e);
            Error::Namespace(format!("Failed to write to {}: {}", path, e))
        })?;

        debug!("Wrote mapping to {}: {}", path, content);
        Ok(())
    }

    /// Check if the current process is running in the expected namespaces
    pub fn verify_namespaces(&self) -> Result<()> {
        // Read namespace IDs from /proc/self/ns/*
        // This is a basic verification - in a real implementation we'd check
        // that we're in different namespaces from the parent

        if self.config.pid_ns {
            let pid_ns = std::fs::read_link("/proc/self/ns/pid")
                .map_err(|e| Error::Namespace(format!("Failed to read PID namespace: {}", e)))?;
            debug!("PID namespace: {:?}", pid_ns);
        }

        if self.config.user_ns {
            let user_ns = std::fs::read_link("/proc/self/ns/user")
                .map_err(|e| Error::Namespace(format!("Failed to read user namespace: {}", e)))?;
            debug!("User namespace: {:?}", user_ns);
        }

        if self.config.mount_ns {
            let mount_ns = std::fs::read_link("/proc/self/ns/mnt")
                .map_err(|e| Error::Namespace(format!("Failed to read mount namespace: {}", e)))?;
            debug!("Mount namespace: {:?}", mount_ns);
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_namespace_manager_creation() {
        let config = NamespaceConfig {
            user_ns: true,
            mount_ns: true,
            pid_ns: true,
            ..Default::default()
        };
        let manager = NamespaceManager::new(config);
        // Just verify creation - actual namespace operations require root/sudo
        assert!(manager.config.user_ns);
        assert!(manager.config.mount_ns);
        assert!(manager.config.pid_ns);
    }
}
