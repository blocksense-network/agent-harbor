// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Configuration types for AgentFS Core

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Case sensitivity modes
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub enum CaseSensitivity {
    Sensitive,
    InsensitivePreserving,
}

/// Memory policy for storage backends
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MemoryPolicy {
    pub max_bytes_in_memory: Option<u64>,
    pub spill_directory: Option<PathBuf>,
}

impl Default for MemoryPolicy {
    fn default() -> Self {
        Self {
            max_bytes_in_memory: Some(1024 * 1024 * 1024), // 1GB
            spill_directory: None,
        }
    }
}

/// System limits
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FsLimits {
    pub max_open_handles: u32,
    pub max_branches: u32,
    pub max_snapshots: u32,
}

impl Default for FsLimits {
    fn default() -> Self {
        Self {
            max_open_handles: 10000,
            max_branches: 1000,
            max_snapshots: 10000,
        }
    }
}

/// Cache policy settings
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CachePolicy {
    pub attr_ttl_ms: u32,
    pub entry_ttl_ms: u32,
    pub negative_ttl_ms: u32,
    pub enable_readdir_plus: bool,
    pub auto_cache: bool,
    pub writeback_cache: bool,
}

impl Default for CachePolicy {
    fn default() -> Self {
        Self {
            attr_ttl_ms: 1000,
            entry_ttl_ms: 1000,
            negative_ttl_ms: 1000,
            enable_readdir_plus: true,
            auto_cache: true,
            writeback_cache: false,
        }
    }
}

/// Main filesystem configuration
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FsConfig {
    pub case_sensitivity: CaseSensitivity,
    pub memory: MemoryPolicy,
    pub limits: FsLimits,
    pub cache: CachePolicy,
    pub enable_xattrs: bool,
    pub enable_ads: bool,
    pub track_events: bool,
    pub security: SecurityPolicy,
    pub backstore: BackstoreMode,
    pub overlay: OverlayConfig,
    pub interpose: InterposeConfig,
}

impl Default for FsConfig {
    fn default() -> Self {
        Self {
            case_sensitivity: CaseSensitivity::Sensitive,
            memory: MemoryPolicy::default(),
            limits: FsLimits::default(),
            cache: CachePolicy::default(),
            enable_xattrs: true,
            enable_ads: false,
            track_events: false,
            security: SecurityPolicy::default(),
            backstore: BackstoreMode::default(),
            overlay: OverlayConfig::default(),
            interpose: InterposeConfig::default(),
        }
    }
}

/// Security/permissions/ownership policy
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct SecurityPolicy {
    /// If true, enforce POSIX permission checks (future)
    pub enforce_posix_permissions: bool,
    /// Default uid assigned to newly created nodes
    pub default_uid: u32,
    /// Default gid assigned to newly created nodes
    pub default_gid: u32,
    /// Enable Windows ACL compatibility bridge (future)
    pub enable_windows_acl_compat: bool,
    /// If true, emulate Unix root DAC override (root bypasses discretionary checks)
    pub root_bypass_permissions: bool,
}

/// Backstore configuration
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum BackstoreMode {
    /// Store upper layer data in memory (default)
    InMemory,
    /// Store upper layer data in a host filesystem directory
    HostFs {
        /// Root directory for upper layer storage
        root: PathBuf,
        /// Whether to prefer native filesystem snapshots
        prefer_native_snapshots: bool,
    },
    /// Create and use a RAM disk with APFS filesystem
    RamDisk {
        /// Size of RAM disk in megabytes
        size_mb: u32,
    },
}

impl Default for BackstoreMode {
    fn default() -> Self {
        Self::InMemory
    }
}

/// Overlay configuration
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OverlayConfig {
    /// Enable overlay mode (default: false)
    pub enabled: bool,
    /// Root path of the lower filesystem (required when enabled)
    pub lower_root: Option<PathBuf>,
    /// Copy-up policy for metadata-only changes
    pub copyup_mode: CopyUpMode,
    /// Optional subdirectory within the lower filesystem exposed to clients (defaults to root)
    pub visible_subdir: Option<PathBuf>,
}

impl Default for OverlayConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            lower_root: None,
            copyup_mode: CopyUpMode::Lazy,
            visible_subdir: None,
        }
    }
}

/// Copy-up mode for overlay operations
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum CopyUpMode {
    /// Copy up on first data write only
    Lazy,
    /// Copy up immediately on any metadata change
    Eager,
}

impl Default for CopyUpMode {
    fn default() -> Self {
        Self::Lazy
    }
}

/// Interpose configuration for FD-forwarding mode
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InterposeConfig {
    /// Enable interpose mode (default: false)
    pub enabled: bool,
    /// Maximum file size for bounded copy (bytes)
    pub max_copy_bytes: u64,
    /// Require reflink support for forwarding (default: false)
    pub require_reflink: bool,
    /// Allow experimental Windows reparse fast-path (default: false)
    pub allow_windows_reparse: bool,
}

impl Default for InterposeConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            max_copy_bytes: 64 * 1024 * 1024, // 64MB
            require_reflink: false,
            allow_windows_reparse: false,
        }
    }
}
