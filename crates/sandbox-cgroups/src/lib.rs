// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Cgroup v2 management for resource limits and metrics.

#![cfg(target_os = "linux")]

pub mod error;

use std::fs;
use std::path::{Path, PathBuf};
use tracing::{debug, info, warn};
use uuid::Uuid;

pub type Result<T> = std::result::Result<T, error::Error>;

/// Configuration for PID limits
#[derive(Debug, Clone, PartialEq)]
pub struct PidLimits {
    /// Maximum number of processes/threads (pids.max)
    pub max: Option<u64>,
}

impl Default for PidLimits {
    fn default() -> Self {
        Self {
            // Default to reasonable limit to prevent fork bombs
            max: Some(1024),
        }
    }
}

/// Configuration for memory limits
#[derive(Debug, Clone, PartialEq)]
pub struct MemoryLimits {
    /// Soft memory limit in bytes (memory.high) - triggers reclaim but doesn't kill
    pub high: Option<u64>,
    /// Hard memory limit in bytes (memory.max) - triggers OOM kill
    pub max: Option<u64>,
}

impl Default for MemoryLimits {
    fn default() -> Self {
        Self {
            // Default to 1GB high limit, 2GB max limit
            high: Some(1024 * 1024 * 1024),    // 1GB
            max: Some(2 * 1024 * 1024 * 1024), // 2GB
        }
    }
}

/// Configuration for CPU limits
#[derive(Debug, Clone, PartialEq)]
pub struct CpuLimits {
    /// CPU quota as percentage (cpu.max) - e.g., "50000 100000" for 50% of one CPU
    pub max: Option<String>,
}

impl Default for CpuLimits {
    fn default() -> Self {
        Self {
            // Default to 80% of one CPU core
            max: Some("80000 100000".to_string()),
        }
    }
}

/// Complete cgroup configuration
#[derive(Debug, Clone, PartialEq)]
pub struct CgroupConfig {
    /// Base path for cgroup filesystem (usually /sys/fs/cgroup)
    pub cgroup_root: PathBuf,
    /// PID limits
    pub pids: PidLimits,
    /// Memory limits
    pub memory: MemoryLimits,
    /// CPU limits
    pub cpu: CpuLimits,
}

impl Default for CgroupConfig {
    fn default() -> Self {
        Self {
            cgroup_root: PathBuf::from("/sys/fs/cgroup"),
            pids: PidLimits::default(),
            memory: MemoryLimits::default(),
            cpu: CpuLimits::default(),
        }
    }
}

/// Resource usage metrics collected from cgroups
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CgroupMetrics {
    /// Current number of processes
    pub current_pids: Option<u64>,
    /// Current memory usage in bytes
    pub memory_current: Option<u64>,
    /// Memory usage events (e.g., OOM kills)
    pub memory_events: Option<std::collections::HashMap<String, u64>>,
    /// CPU usage in nanoseconds
    pub cpu_usage: Option<u64>,
    /// CPU user time in nanoseconds
    pub cpu_user: Option<u64>,
    /// CPU system time in nanoseconds
    pub cpu_system: Option<u64>,
}

/// Cgroup manager for resource control
pub struct CgroupManager {
    config: CgroupConfig,
    session_id: String,
    cgroup_path: Option<PathBuf>,
}

impl Default for CgroupManager {
    fn default() -> Self {
        Self::new(CgroupConfig::default())
    }
}

impl CgroupManager {
    /// Create a new cgroup manager with the given configuration
    pub fn new(config: CgroupConfig) -> Self {
        let session_id = format!("sbx-{}", Uuid::new_v4().simple());
        Self {
            config,
            session_id,
            cgroup_path: None,
        }
    }

    /// Get the cgroup path for this session
    pub fn cgroup_path(&self) -> Option<&Path> {
        self.cgroup_path.as_deref()
    }

    /// Set up cgroup subtree and limits
    pub fn setup_limits(&mut self) -> Result<()> {
        // Check if cgroup v2 is available
        if !self.is_cgroup_v2_available()? {
            warn!("cgroup v2 not available, skipping resource limits");
            return Ok(());
        }

        // Create session-specific cgroup directory
        let cgroup_path = self.config.cgroup_root.join(&self.session_id);
        self.create_cgroup_subtree(&cgroup_path)?;

        self.cgroup_path = Some(cgroup_path.clone());
        info!("Created cgroup subtree at {:?}", cgroup_path);

        // Set resource limits
        self.set_pid_limits()?;
        self.set_memory_limits()?;
        self.set_cpu_limits()?;

        debug!("Applied resource limits to cgroup {:?}", cgroup_path);
        Ok(())
    }

    /// Add a process to the cgroup
    pub fn add_process(&self, pid: u32) -> Result<()> {
        if let Some(cgroup_path) = &self.cgroup_path {
            let cgroup_procs_path = cgroup_path.join("cgroup.procs");
            fs::write(&cgroup_procs_path, pid.to_string()).map_err(error::Error::Io)?;
            debug!("Added PID {} to cgroup {:?}", pid, cgroup_path);
        }
        Ok(())
    }

    /// Collect resource usage metrics
    pub fn collect_metrics(&self) -> Result<CgroupMetrics> {
        let mut metrics = CgroupMetrics {
            current_pids: None,
            memory_current: None,
            memory_events: None,
            cpu_usage: None,
            cpu_user: None,
            cpu_system: None,
        };

        if let Some(cgroup_path) = &self.cgroup_path {
            // Collect PID metrics
            if let Ok(content) = fs::read_to_string(cgroup_path.join("pids.current")) {
                if let Ok(pids) = content.trim().parse::<u64>() {
                    metrics.current_pids = Some(pids);
                }
            }

            // Collect memory metrics
            if let Ok(content) = fs::read_to_string(cgroup_path.join("memory.current")) {
                if let Ok(mem) = content.trim().parse::<u64>() {
                    metrics.memory_current = Some(mem);
                }
            }

            // Collect memory events
            if let Ok(content) = fs::read_to_string(cgroup_path.join("memory.events")) {
                let mut events = std::collections::HashMap::new();
                for line in content.lines() {
                    if let Some((key, value)) = line.split_once(' ') {
                        if let Ok(val) = value.parse::<u64>() {
                            events.insert(key.to_string(), val);
                        }
                    }
                }
                if !events.is_empty() {
                    metrics.memory_events = Some(events);
                }
            }

            // Collect CPU metrics
            if let Ok(content) = fs::read_to_string(cgroup_path.join("cpu.stat")) {
                for line in content.lines() {
                    if let Some((key, value)) = line.split_once(' ') {
                        if let Ok(val) = value.parse::<u64>() {
                            match key {
                                "usage_usec" => metrics.cpu_usage = Some(val * 1000), // Convert to nanoseconds
                                "user_usec" => metrics.cpu_user = Some(val * 1000),
                                "system_usec" => metrics.cpu_system = Some(val * 1000),
                                _ => {}
                            }
                        }
                    }
                }
            }
        }

        Ok(metrics)
    }

    /// Clean up the cgroup subtree
    pub fn cleanup(&self) -> Result<()> {
        if let Some(cgroup_path) = &self.cgroup_path {
            // Move all processes to parent cgroup first
            let parent_procs_path =
                cgroup_path.parent().unwrap_or(&self.config.cgroup_root).join("cgroup.procs");
            if let Ok(content) = fs::read_to_string(cgroup_path.join("cgroup.procs")) {
                if !content.trim().is_empty() {
                    // Try to move processes to parent cgroup
                    if let Err(e) = fs::write(&parent_procs_path, &content) {
                        warn!("Failed to move processes to parent cgroup: {}", e);
                    }
                }
            }

            // Remove the cgroup directory
            if let Err(e) = fs::remove_dir(cgroup_path) {
                warn!("Failed to remove cgroup directory {:?}: {}", cgroup_path, e);
            } else {
                info!("Cleaned up cgroup subtree {:?}", cgroup_path);
            }
        }
        Ok(())
    }

    /// Check if cgroup v2 is available
    fn is_cgroup_v2_available(&self) -> Result<bool> {
        let cgroup_root = &self.config.cgroup_root;

        // Check if the cgroup filesystem is mounted
        if !cgroup_root.exists() {
            return Ok(false);
        }

        // Check for cgroup v2 by looking for cgroup.controllers file
        let controllers_path = cgroup_root.join("cgroup.controllers");
        match fs::metadata(&controllers_path) {
            Ok(_) => Ok(true),
            Err(_) => Ok(false),
        }
    }

    /// Create the cgroup subtree
    fn create_cgroup_subtree(&self, cgroup_path: &Path) -> Result<()> {
        fs::create_dir_all(cgroup_path).map_err(|e| {
            error::Error::Setup(format!("Failed to create cgroup directory: {}", e))
        })?;

        // Enable controllers in the subtree
        let subtree_control_path = cgroup_path.join("cgroup.subtree_control");
        let controllers = "+pids +memory +cpu";
        fs::write(&subtree_control_path, controllers)
            .map_err(|e| error::Error::Setup(format!("Failed to enable controllers: {}", e)))?;

        Ok(())
    }

    /// Set PID limits
    fn set_pid_limits(&self) -> Result<()> {
        if let Some(cgroup_path) = &self.cgroup_path {
            if let Some(max_pids) = self.config.pids.max {
                let pids_max_path = cgroup_path.join("pids.max");
                fs::write(&pids_max_path, max_pids.to_string())
                    .map_err(|e| error::Error::Limit(format!("Failed to set pids.max: {}", e)))?;
                debug!("Set pids.max to {}", max_pids);
            }
        }
        Ok(())
    }

    /// Set memory limits
    fn set_memory_limits(&self) -> Result<()> {
        if let Some(cgroup_path) = &self.cgroup_path {
            if let Some(high) = self.config.memory.high {
                let memory_high_path = cgroup_path.join("memory.high");
                fs::write(&memory_high_path, high.to_string()).map_err(|e| {
                    error::Error::Limit(format!("Failed to set memory.high: {}", e))
                })?;
                debug!("Set memory.high to {} bytes", high);
            }

            if let Some(max) = self.config.memory.max {
                let memory_max_path = cgroup_path.join("memory.max");
                fs::write(&memory_max_path, max.to_string())
                    .map_err(|e| error::Error::Limit(format!("Failed to set memory.max: {}", e)))?;
                debug!("Set memory.max to {} bytes", max);
            }
        }
        Ok(())
    }

    /// Set CPU limits
    fn set_cpu_limits(&self) -> Result<()> {
        if let Some(cgroup_path) = &self.cgroup_path {
            if let Some(ref max) = self.config.cpu.max {
                let cpu_max_path = cgroup_path.join("cpu.max");
                fs::write(&cpu_max_path, max)
                    .map_err(|e| error::Error::Limit(format!("Failed to set cpu.max: {}", e)))?;
                debug!("Set cpu.max to {}", max);
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[ah_test_utils::logged_test]
    fn test_cgroup_config_defaults() {
        let config = CgroupConfig::default();

        assert_eq!(config.cgroup_root, PathBuf::from("/sys/fs/cgroup"));
        assert_eq!(config.pids.max, Some(1024));
        assert_eq!(config.memory.high, Some(1024 * 1024 * 1024)); // 1GB
        assert_eq!(config.memory.max, Some(2 * 1024 * 1024 * 1024)); // 2GB
        assert_eq!(config.cpu.max, Some("80000 100000".to_string()));
    }

    #[ah_test_utils::logged_test]
    fn test_cgroup_manager_creation() {
        let config = CgroupConfig::default();
        let manager = CgroupManager::new(config);

        // Test that manager can be created and configured
        assert!(manager.cgroup_path().is_none());

        // Test metrics collection (should work even without cgroup setup)
        let metrics = manager.collect_metrics().unwrap();
        assert!(metrics.current_pids.is_none()); // No cgroup created yet
        assert!(metrics.memory_current.is_none());
        assert!(metrics.memory_events.is_none());
        assert!(metrics.cpu_usage.is_none());
        assert!(metrics.cpu_user.is_none());
        assert!(metrics.cpu_system.is_none());
    }

    #[ah_test_utils::logged_test]
    fn test_pid_limits_config() {
        let limits = PidLimits::default();
        assert_eq!(limits.max, Some(1024));

        let custom_limits = PidLimits { max: Some(512) };
        assert_eq!(custom_limits.max, Some(512));
    }

    #[ah_test_utils::logged_test]
    fn test_memory_limits_config() {
        let limits = MemoryLimits::default();
        assert_eq!(limits.high, Some(1024 * 1024 * 1024)); // 1GB
        assert_eq!(limits.max, Some(2 * 1024 * 1024 * 1024)); // 2GB

        let custom_limits = MemoryLimits {
            high: Some(512 * 1024 * 1024), // 512MB
            max: Some(1024 * 1024 * 1024), // 1GB
        };
        assert_eq!(custom_limits.high, Some(512 * 1024 * 1024));
        assert_eq!(custom_limits.max, Some(1024 * 1024 * 1024));
    }

    #[ah_test_utils::logged_test]
    fn test_cpu_limits_config() {
        let limits = CpuLimits::default();
        assert_eq!(limits.max, Some("80000 100000".to_string()));

        let custom_limits = CpuLimits {
            max: Some("50000 100000".to_string()), // 50% of one CPU
        };
        assert_eq!(custom_limits.max, Some("50000 100000".to_string()));
    }
}
