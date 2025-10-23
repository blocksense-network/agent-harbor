// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! AgentFS FSKit Host - macOS filesystem adapter using Apple's FSKit framework
//!
//! This crate provides a FSKit Unary File System extension that bridges
//! AgentFS Core operations to macOS via the FSKit framework.

#[cfg(target_os = "macos")]
mod fskit;
#[cfg(target_os = "macos")]
mod xpc_control;

#[cfg(not(target_os = "macos"))]
mod stub;

#[cfg(target_os = "macos")]
pub use fskit::*;
#[cfg(target_os = "macos")]
pub use xpc_control::*;

#[cfg(not(target_os = "macos"))]
pub use stub::*;

use agentfs_core::{FsConfig, FsCore};
use std::sync::Arc;

/// Configuration for the FSKit adapter
#[derive(Clone, Debug)]
pub struct FsKitConfig {
    /// Underlying AgentFS configuration
    pub fs_config: FsConfig,
    /// Mount point path
    pub mount_point: String,
    /// XPC service name for control operations
    pub xpc_service_name: Option<String>,
}

/// FSKit adapter instance
pub struct FsKitAdapter {
    core: Arc<FsCore>,
    config: FsKitConfig,
}

impl FsKitAdapter {
    /// Create a new FSKit adapter
    pub fn new(config: FsKitConfig) -> Result<Self, Box<dyn std::error::Error>> {
        let core = Arc::new(FsCore::new(config.fs_config.clone())?);

        Ok(Self { core, config })
    }

    /// Get reference to the underlying AgentFS core
    pub fn core(&self) -> &Arc<FsCore> {
        &self.core
    }

    /// Get adapter configuration
    pub fn config(&self) -> &FsKitConfig {
        &self.config
    }
}
