// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Sandbox-related configuration types for Linux local sandboxing.
//!
//! This module defines the configuration schema for the sandbox subsystem as specified
//! in `specs/Public/Sandboxing/Local-Sandboxing-on-Linux.md` Sections 13 & 26.
//!
//! # Configuration Keys
//!
//! All sandbox configuration keys are nested under the `sandbox` section in config files:
//!
//! ```toml
//! [sandbox]
//! mode = "dynamic"
//! debug = true
//! allow-network = false
//! ```
//!
//! Available keys:
//!
//! - `sandbox.mode` - Sandbox mode: `dynamic` or `static`
//! - `sandbox.debug` - Enable debugging/ptrace inside sandbox
//! - `sandbox.allow-network` - Enable internet egress via slirp4netns
//! - `sandbox.containers` - Allow rootless containers inside sandbox
//! - `sandbox.vm` - Allow VMs inside sandbox
//! - `sandbox.allow-kvm` - Expose /dev/kvm for VM acceleration
//! - `sandbox.rw-paths` - List of read-write path carve-outs
//! - `sandbox.overlay-paths` - List of overlay mount paths
//! - `sandbox.blacklist-paths` - List of blocked/hidden paths
//! - `sandbox.tmpfs-size` - Size limit for isolated `/tmp` tmpfs mount
//! - `sandbox.limits.*` - Resource limits (pids-max, memory-max, etc.)
//!
//! # Environment Variables
//!
//! Sandbox settings can be overridden via environment variables with the `AH_SANDBOX_` prefix:
//!
//! - `AH_SANDBOX_MODE` → `sandbox.mode`
//! - `AH_SANDBOX_DEBUG` → `sandbox.debug`
//! - `AH_SANDBOX_ALLOW_NETWORK` → `sandbox.allow-network`
//! - `AH_SANDBOX_CONTAINERS` → `sandbox.containers`
//! - `AH_SANDBOX_VM` → `sandbox.vm`
//! - `AH_SANDBOX_ALLOW_KVM` → `sandbox.allow-kvm`
//! - `AH_SANDBOX_TMPFS_SIZE` → `sandbox.tmpfs-size`
//! - `AH_SANDBOX_LIMITS_PIDS_MAX` → `sandbox.limits.pids-max`
//! - `AH_SANDBOX_LIMITS_MEMORY_MAX` → `sandbox.limits.memory-max`
//! - `AH_SANDBOX_LIMITS_MEMORY_HIGH` → `sandbox.limits.memory-high`
//! - `AH_SANDBOX_LIMITS_CPU_MAX` → `sandbox.limits.cpu-max`
//!
//! # Precedence
//!
//! Configuration values are resolved with the following precedence (highest first):
//!
//! 1. CLI flags (e.g., `--allow-network`)
//! 2. Environment variables (e.g., `AH_SANDBOX_ALLOW_NETWORK`)
//! 3. CLI config file (via `--config`)
//! 4. Repo-user config (`<repo>/.agents/config.local.toml`)
//! 5. Repo config (`<repo>/.agents/config.toml`)
//! 6. User config (`~/.config/agent-harbor/config.toml`)
//! 7. System config (`/etc/agent-harbor/config.toml`)

use serde::{Deserialize, Serialize};

/// Sandbox operating mode.
///
/// Determines how filesystem access control is handled within the sandbox.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum SandboxMode {
    /// Dynamic mode: First access to a non-allowed path blocks; the supervisor
    /// prompts the human to approve/deny. Approvals can be persisted to policy stores.
    /// This is the default mode for interactive sessions.
    Dynamic,

    /// Static mode: Read-only view with configurable blacklist of sensitive directories
    /// and configurable set of writable overlays. No interactive gating; intended for
    /// trusted or non-interactive sessions.
    Static,
}

impl Default for SandboxMode {
    fn default() -> Self {
        // Default to dynamic mode (interactive read allow-list) per spec Section 13
        Self::Dynamic
    }
}

/// Resource limits for sandbox cgroups v2 integration.
///
/// These limits protect the host from runaway processes within the sandbox.
/// All limits are applied via the cgroup v2 interface.
///
/// See `specs/Public/Sandboxing/Local-Sandboxing-on-Linux.md` Section 11 for details.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub struct SandboxLimits {
    /// Maximum number of processes (PIDs) allowed in the sandbox.
    ///
    /// Prevents fork-bomb attacks. Corresponds to cgroup v2 `pids.max`.
    ///
    /// Default: 1024
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pids_max: Option<u32>,

    /// Maximum memory limit for the sandbox.
    ///
    /// When this limit is exceeded, the OOM killer is invoked.
    /// Corresponds to cgroup v2 `memory.max`.
    ///
    /// Accepts size suffixes: k, m, g (case-insensitive).
    ///
    /// Default: "2G"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory_max: Option<String>,

    /// Memory high watermark for the sandbox.
    ///
    /// When exceeded, memory reclaim is triggered but processes are not killed.
    /// Corresponds to cgroup v2 `memory.high`.
    ///
    /// Accepts size suffixes: k, m, g (case-insensitive).
    ///
    /// Default: "1G"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory_high: Option<String>,

    /// CPU quota and period for the sandbox.
    ///
    /// Format: "quota period" where quota is the maximum CPU time in microseconds
    /// allowed during each period (also in microseconds).
    /// Corresponds to cgroup v2 `cpu.max`.
    ///
    /// Example: "80000 100000" means 80ms of CPU time per 100ms period (80% of one core).
    ///
    /// Default: "80000 100000"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cpu_max: Option<String>,

    /// I/O throttle settings for the sandbox.
    ///
    /// Format depends on the specific throttle type. Corresponds to cgroup v2 `io.max`.
    ///
    /// Example: "8:0 rbps=1048576 wbps=1048576" limits device 8:0 to 1MB/s read/write.
    ///
    /// Default: None (no I/O throttling)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub io_max: Option<String>,
}

impl Default for SandboxLimits {
    fn default() -> Self {
        Self {
            pids_max: Some(1024),
            memory_max: Some("2G".to_string()),
            memory_high: Some("1G".to_string()),
            cpu_max: Some("80000 100000".to_string()),
            io_max: None,
        }
    }
}

/// Complete sandbox configuration.
///
/// This struct defines all configurable options for the Linux local sandbox.
/// It integrates with the layered configuration system and supports precedence
/// from system config through CLI flags.
///
/// See `specs/Public/Sandboxing/Local-Sandboxing-on-Linux.md` Sections 13 & 26.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, Default)]
#[serde(rename_all = "kebab-case")]
pub struct SandboxConfig {
    /// Sandbox operating mode.
    ///
    /// - `dynamic`: Interactive read allow-list (default)
    /// - `static`: Read-only with blacklists, no interactive gating
    ///
    /// CLI: `--mode dynamic|static`
    /// Env: `AH_SANDBOX_MODE`
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<SandboxMode>,

    /// Enable debugging/ptrace operations inside the sandbox.
    ///
    /// When enabled, allows debuggers like gdb to attach to processes within
    /// the sandbox. Debugging is scoped to sandbox processes only.
    ///
    /// CLI: `--debug` / `--no-debug`
    /// Env: `AH_SANDBOX_DEBUG`
    ///
    /// Default: true
    #[serde(skip_serializing_if = "Option::is_none")]
    pub debug: Option<bool>,

    /// Enable internet egress via slirp4netns.
    ///
    /// When disabled (default), the sandbox has loopback-only networking.
    /// When enabled, outbound internet access is provided via slirp4netns
    /// user-mode networking.
    ///
    /// CLI: `--allow-network`
    /// Env: `AH_SANDBOX_ALLOW_NETWORK`
    ///
    /// Default: false
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allow_network: Option<bool>,

    /// Allow rootless containers inside the sandbox.
    ///
    /// When enabled, provides `/dev/fuse` access, delegated cgroup subtree,
    /// and pre-allowed storage directories for rootless container runtimes
    /// like Podman.
    ///
    /// Note: The host Docker socket is never exposed, even with this enabled.
    ///
    /// CLI: `--containers`
    /// Env: `AH_SANDBOX_CONTAINERS`
    ///
    /// Default: false
    #[serde(skip_serializing_if = "Option::is_none")]
    pub containers: Option<bool>,

    /// Allow VMs inside the sandbox.
    ///
    /// When enabled, allows running VMs with QEMU user-mode networking.
    /// For hardware acceleration, also enable `allow-kvm`.
    ///
    /// CLI: `--vm`
    /// Env: `AH_SANDBOX_VM`
    ///
    /// Default: false
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vm: Option<bool>,

    /// Expose /dev/kvm for VM hardware acceleration.
    ///
    /// Only effective when `vm` is also enabled. Increases kernel attack
    /// surface but provides significant performance improvements for VMs.
    ///
    /// CLI: `--allow-kvm`
    /// Env: `AH_SANDBOX_ALLOW_KVM`
    ///
    /// Default: false
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allow_kvm: Option<bool>,

    /// List of read-write path carve-outs.
    ///
    /// These paths are bind-mounted read-write into the sandbox, bypassing
    /// the default read-only sealing. Use for project directories and caches.
    ///
    /// CLI: `--rw <PATH>` (repeatable)
    /// Env: `AH_SANDBOX_RW_PATHS` (colon-separated)
    ///
    /// Default: [working directory, standard cache directories]
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rw_paths: Vec<String>,

    /// List of overlay mount paths.
    ///
    /// These paths are mounted as overlayfs, allowing apparent writes that
    /// are persisted to a per-session upperdir. Useful for paths that need
    /// to appear writable but shouldn't modify the host.
    ///
    /// CLI: `--overlay <PATH>` (repeatable)
    /// Env: `AH_SANDBOX_OVERLAY_PATHS` (colon-separated)
    ///
    /// Default: []
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub overlay_paths: Vec<String>,

    /// List of blocked/hidden paths (for static mode).
    ///
    /// In static mode, these paths are inaccessible from within the sandbox.
    /// Typically used for sensitive directories like `.ssh`, `.gnupg`, etc.
    ///
    /// CLI: `--blacklist <PATH>` (repeatable)
    /// Env: `AH_SANDBOX_BLACKLIST_PATHS` (colon-separated)
    ///
    /// Default: [~/.ssh, ~/.gnupg, cloud credential directories]
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub blacklist_paths: Vec<String>,

    /// Size limit for isolated `/tmp` tmpfs mount.
    ///
    /// The sandbox mounts a private tmpfs at `/tmp` with this size limit.
    /// Accepts size suffixes: k, m, g (case-insensitive).
    /// Set to "0" to disable `/tmp` isolation.
    ///
    /// CLI: `--tmpfs-size <SIZE>`
    /// Env: `AH_SANDBOX_TMPFS_SIZE`
    ///
    /// Default: "256m"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tmpfs_size: Option<String>,

    /// Resource limits for the sandbox.
    ///
    /// Controls PIDs, memory, CPU, and I/O limits via cgroups v2.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limits: Option<SandboxLimits>,
}

impl SandboxConfig {
    /// Create a new SandboxConfig with all default values.
    pub fn new() -> Self {
        Self::default()
    }

    /// Get the effective sandbox mode, using the default if not configured.
    pub fn effective_mode(&self) -> SandboxMode {
        self.mode.unwrap_or_default()
    }

    /// Get the effective debug setting, using the default (true) if not configured.
    pub fn effective_debug(&self) -> bool {
        self.debug.unwrap_or(true)
    }

    /// Get the effective allow-network setting, using the default (false) if not configured.
    pub fn effective_allow_network(&self) -> bool {
        self.allow_network.unwrap_or(false)
    }

    /// Get the effective containers setting, using the default (false) if not configured.
    pub fn effective_containers(&self) -> bool {
        self.containers.unwrap_or(false)
    }

    /// Get the effective VM setting, using the default (false) if not configured.
    pub fn effective_vm(&self) -> bool {
        self.vm.unwrap_or(false)
    }

    /// Get the effective allow-kvm setting, using the default (false) if not configured.
    pub fn effective_allow_kvm(&self) -> bool {
        self.allow_kvm.unwrap_or(false)
    }

    /// Get the effective tmpfs size, using the default ("256m") if not configured.
    pub fn effective_tmpfs_size(&self) -> &str {
        self.tmpfs_size.as_deref().unwrap_or("256m")
    }

    /// Get the effective resource limits, using defaults where not configured.
    pub fn effective_limits(&self) -> SandboxLimits {
        self.limits.clone().unwrap_or_default()
    }

    /// Merge another config into this one, with the other config taking precedence.
    ///
    /// Only non-None values from `other` are applied. This is used for config
    /// layer merging where higher-precedence layers override lower ones.
    pub fn merge(&mut self, other: &SandboxConfig) {
        if other.mode.is_some() {
            self.mode = other.mode;
        }
        if other.debug.is_some() {
            self.debug = other.debug;
        }
        if other.allow_network.is_some() {
            self.allow_network = other.allow_network;
        }
        if other.containers.is_some() {
            self.containers = other.containers;
        }
        if other.vm.is_some() {
            self.vm = other.vm;
        }
        if other.allow_kvm.is_some() {
            self.allow_kvm = other.allow_kvm;
        }
        if !other.rw_paths.is_empty() {
            self.rw_paths = other.rw_paths.clone();
        }
        if !other.overlay_paths.is_empty() {
            self.overlay_paths = other.overlay_paths.clone();
        }
        if !other.blacklist_paths.is_empty() {
            self.blacklist_paths = other.blacklist_paths.clone();
        }
        if other.tmpfs_size.is_some() {
            self.tmpfs_size = other.tmpfs_size.clone();
        }
        if let Some(ref other_limits) = other.limits {
            let mut limits = self.limits.clone().unwrap_or_default();
            if other_limits.pids_max.is_some() {
                limits.pids_max = other_limits.pids_max;
            }
            if other_limits.memory_max.is_some() {
                limits.memory_max = other_limits.memory_max.clone();
            }
            if other_limits.memory_high.is_some() {
                limits.memory_high = other_limits.memory_high.clone();
            }
            if other_limits.cpu_max.is_some() {
                limits.cpu_max = other_limits.cpu_max.clone();
            }
            if other_limits.io_max.is_some() {
                limits.io_max = other_limits.io_max.clone();
            }
            self.limits = Some(limits);
        }
    }

    /// Validate the configuration, returning an error if any values are invalid.
    pub fn validate(&self) -> Result<(), SandboxConfigError> {
        // Validate tmpfs_size format
        if let Some(ref size) = self.tmpfs_size {
            validate_size_string(size).map_err(|e| SandboxConfigError::InvalidTmpfsSize {
                value: size.clone(),
                reason: e,
            })?;
        }

        // Validate limits
        if let Some(ref limits) = self.limits {
            if let Some(ref memory_max) = limits.memory_max {
                validate_size_string(memory_max).map_err(|e| {
                    SandboxConfigError::InvalidMemoryMax {
                        value: memory_max.clone(),
                        reason: e,
                    }
                })?;
            }
            if let Some(ref memory_high) = limits.memory_high {
                validate_size_string(memory_high).map_err(|e| {
                    SandboxConfigError::InvalidMemoryHigh {
                        value: memory_high.clone(),
                        reason: e,
                    }
                })?;
            }
            if let Some(ref cpu_max) = limits.cpu_max {
                validate_cpu_max(cpu_max).map_err(|e| SandboxConfigError::InvalidCpuMax {
                    value: cpu_max.clone(),
                    reason: e,
                })?;
            }
        }

        // Validate that allow-kvm is only set if vm is also set
        if self.allow_kvm == Some(true) && self.vm != Some(true) {
            return Err(SandboxConfigError::AllowKvmRequiresVm);
        }

        Ok(())
    }
}

/// Errors that can occur during sandbox configuration validation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SandboxConfigError {
    /// Invalid tmpfs-size value
    InvalidTmpfsSize { value: String, reason: String },
    /// Invalid memory-max value
    InvalidMemoryMax { value: String, reason: String },
    /// Invalid memory-high value
    InvalidMemoryHigh { value: String, reason: String },
    /// Invalid cpu-max value
    InvalidCpuMax { value: String, reason: String },
    /// allow-kvm requires vm to be enabled
    AllowKvmRequiresVm,
}

impl std::fmt::Display for SandboxConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidTmpfsSize { value, reason } => {
                write!(f, "Invalid tmpfs-size '{}': {}", value, reason)
            }
            Self::InvalidMemoryMax { value, reason } => {
                write!(f, "Invalid memory-max '{}': {}", value, reason)
            }
            Self::InvalidMemoryHigh { value, reason } => {
                write!(f, "Invalid memory-high '{}': {}", value, reason)
            }
            Self::InvalidCpuMax { value, reason } => {
                write!(f, "Invalid cpu-max '{}': {}", value, reason)
            }
            Self::AllowKvmRequiresVm => {
                write!(f, "allow-kvm requires vm to be enabled")
            }
        }
    }
}

impl std::error::Error for SandboxConfigError {}

/// Validate a size string (e.g., "256m", "2G", "1024k").
fn validate_size_string(s: &str) -> Result<(), String> {
    if s == "0" {
        return Ok(());
    }

    if s.is_empty() {
        return Err("size cannot be empty".to_string());
    }

    let s_lower = s.to_lowercase();

    // Check for valid suffix
    let (num_part, suffix) = if s_lower.ends_with('g') {
        (&s[..s.len() - 1], Some('g'))
    } else if s_lower.ends_with('m') {
        (&s[..s.len() - 1], Some('m'))
    } else if s_lower.ends_with('k') {
        (&s[..s.len() - 1], Some('k'))
    } else {
        (s, None)
    };

    // Parse the numeric part
    let _: u64 = num_part
        .parse()
        .map_err(|_| format!("invalid number '{}' in size string", num_part))?;

    if suffix.is_none() && !s.chars().all(|c| c.is_ascii_digit()) {
        return Err("size must be a number with optional k/m/g suffix".to_string());
    }

    Ok(())
}

/// Validate a cpu.max string (e.g., "80000 100000").
fn validate_cpu_max(s: &str) -> Result<(), String> {
    let parts: Vec<&str> = s.split_whitespace().collect();
    if parts.len() != 2 {
        return Err("cpu-max must be in format 'quota period' (e.g., '80000 100000')".to_string());
    }

    let _quota: u64 = parts[0]
        .parse()
        .map_err(|_| format!("invalid quota '{}' in cpu-max", parts[0]))?;
    let _period: u64 = parts[1]
        .parse()
        .map_err(|_| format!("invalid period '{}' in cpu-max", parts[1]))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sandbox_mode_default() {
        assert_eq!(SandboxMode::default(), SandboxMode::Dynamic);
    }

    #[test]
    fn test_sandbox_limits_default() {
        let limits = SandboxLimits::default();
        assert_eq!(limits.pids_max, Some(1024));
        assert_eq!(limits.memory_max, Some("2G".to_string()));
        assert_eq!(limits.memory_high, Some("1G".to_string()));
        assert_eq!(limits.cpu_max, Some("80000 100000".to_string()));
        assert_eq!(limits.io_max, None);
    }

    #[test]
    fn test_sandbox_config_effective_values() {
        let config = SandboxConfig::default();
        assert_eq!(config.effective_mode(), SandboxMode::Dynamic);
        assert!(config.effective_debug());
        assert!(!config.effective_allow_network());
        assert!(!config.effective_containers());
        assert!(!config.effective_vm());
        assert!(!config.effective_allow_kvm());
        assert_eq!(config.effective_tmpfs_size(), "256m");
    }

    #[test]
    fn test_sandbox_config_merge() {
        let mut base = SandboxConfig {
            mode: Some(SandboxMode::Dynamic),
            debug: Some(true),
            allow_network: Some(false),
            ..Default::default()
        };

        let overlay = SandboxConfig {
            mode: Some(SandboxMode::Static),
            allow_network: Some(true),
            containers: Some(true),
            ..Default::default()
        };

        base.merge(&overlay);

        assert_eq!(base.mode, Some(SandboxMode::Static));
        assert_eq!(base.debug, Some(true)); // Unchanged
        assert_eq!(base.allow_network, Some(true)); // Overridden
        assert_eq!(base.containers, Some(true)); // Added
    }

    #[test]
    fn test_sandbox_config_merge_limits() {
        let mut base = SandboxConfig {
            limits: Some(SandboxLimits {
                pids_max: Some(512),
                memory_max: Some("1G".to_string()),
                ..Default::default()
            }),
            ..Default::default()
        };

        // Create overlay with explicit None values for fields we don't want to override
        let overlay = SandboxConfig {
            limits: Some(SandboxLimits {
                pids_max: Some(2048),
                cpu_max: Some("50000 100000".to_string()),
                memory_max: None,  // Don't override memory_max
                memory_high: None, // Don't override memory_high
                io_max: None,      // Don't override io_max
            }),
            ..Default::default()
        };

        base.merge(&overlay);

        let limits = base.limits.unwrap();
        assert_eq!(limits.pids_max, Some(2048)); // Overridden
        assert_eq!(limits.memory_max, Some("1G".to_string())); // Unchanged
        assert_eq!(limits.cpu_max, Some("50000 100000".to_string())); // Added
    }

    #[test]
    fn test_validate_size_string() {
        assert!(validate_size_string("256m").is_ok());
        assert!(validate_size_string("2G").is_ok());
        assert!(validate_size_string("1024k").is_ok());
        assert!(validate_size_string("1024").is_ok());
        assert!(validate_size_string("0").is_ok());

        assert!(validate_size_string("").is_err());
        assert!(validate_size_string("abc").is_err());
        assert!(validate_size_string("256x").is_err());
    }

    #[test]
    fn test_validate_cpu_max() {
        assert!(validate_cpu_max("80000 100000").is_ok());
        assert!(validate_cpu_max("50000 100000").is_ok());

        assert!(validate_cpu_max("80000").is_err());
        assert!(validate_cpu_max("abc 100000").is_err());
        assert!(validate_cpu_max("80000 abc").is_err());
    }

    #[test]
    fn test_sandbox_config_validate() {
        let valid = SandboxConfig {
            tmpfs_size: Some("256m".to_string()),
            limits: Some(SandboxLimits {
                memory_max: Some("2G".to_string()),
                cpu_max: Some("80000 100000".to_string()),
                ..Default::default()
            }),
            ..Default::default()
        };
        assert!(valid.validate().is_ok());

        let invalid_tmpfs = SandboxConfig {
            tmpfs_size: Some("invalid".to_string()),
            ..Default::default()
        };
        assert!(matches!(
            invalid_tmpfs.validate(),
            Err(SandboxConfigError::InvalidTmpfsSize { .. })
        ));

        let allow_kvm_without_vm = SandboxConfig {
            allow_kvm: Some(true),
            vm: Some(false),
            ..Default::default()
        };
        assert!(matches!(
            allow_kvm_without_vm.validate(),
            Err(SandboxConfigError::AllowKvmRequiresVm)
        ));
    }

    #[test]
    fn test_sandbox_mode_serialization() {
        let dynamic = SandboxMode::Dynamic;
        let json = serde_json::to_string(&dynamic).unwrap();
        assert_eq!(json, "\"dynamic\"");

        let static_mode = SandboxMode::Static;
        let json = serde_json::to_string(&static_mode).unwrap();
        assert_eq!(json, "\"static\"");

        let parsed: SandboxMode = serde_json::from_str("\"dynamic\"").unwrap();
        assert_eq!(parsed, SandboxMode::Dynamic);
    }

    #[test]
    fn test_sandbox_config_serialization() {
        let config = SandboxConfig {
            mode: Some(SandboxMode::Static),
            debug: Some(false),
            allow_network: Some(true),
            containers: Some(true),
            vm: Some(true),
            allow_kvm: Some(true),
            rw_paths: vec!["/home/user/project".to_string()],
            overlay_paths: vec!["/usr/local".to_string()],
            blacklist_paths: vec!["/home/user/.ssh".to_string()],
            tmpfs_size: Some("512m".to_string()),
            limits: Some(SandboxLimits {
                pids_max: Some(2048),
                memory_max: Some("4G".to_string()),
                memory_high: Some("2G".to_string()),
                cpu_max: Some("160000 100000".to_string()),
                io_max: Some("8:0 rbps=1048576".to_string()),
            }),
        };

        let json = serde_json::to_string_pretty(&config).unwrap();
        let parsed: SandboxConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(config, parsed);

        // Verify kebab-case serialization
        assert!(json.contains("\"allow-network\""));
        assert!(json.contains("\"allow-kvm\""));
        assert!(json.contains("\"rw-paths\""));
        assert!(json.contains("\"overlay-paths\""));
        assert!(json.contains("\"blacklist-paths\""));
        assert!(json.contains("\"tmpfs-size\""));
        assert!(json.contains("\"pids-max\""));
        assert!(json.contains("\"memory-max\""));
        assert!(json.contains("\"memory-high\""));
        assert!(json.contains("\"cpu-max\""));
        assert!(json.contains("\"io-max\""));
    }

    #[test]
    fn test_sandbox_config_toml_parsing() {
        let toml_str = r#"
            mode = "static"
            debug = false
            allow-network = true
            containers = false
            tmpfs-size = "512m"
            rw-paths = ["/home/user/project"]
            
            [limits]
            pids-max = 2048
            memory-max = "4G"
        "#;

        let config: SandboxConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.mode, Some(SandboxMode::Static));
        assert_eq!(config.debug, Some(false));
        assert_eq!(config.allow_network, Some(true));
        assert_eq!(config.containers, Some(false));
        assert_eq!(config.tmpfs_size, Some("512m".to_string()));
        assert_eq!(config.rw_paths, vec!["/home/user/project".to_string()]);

        let limits = config.limits.unwrap();
        assert_eq!(limits.pids_max, Some(2048));
        assert_eq!(limits.memory_max, Some("4G".to_string()));
    }
}
