// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Stub implementation for non-macOS platforms

use super::FsKitAdapter;

impl FsKitAdapter {
    /// Mount the filesystem (stub implementation)
    pub fn mount(&self) -> Result<(), Box<dyn std::error::Error>> {
        Err("FSKit is only available on macOS".into())
    }

    /// Unmount the filesystem (stub implementation)
    pub fn unmount(&self) -> Result<(), Box<dyn std::error::Error>> {
        Err("FSKit is only available on macOS".into())
    }

    /// Start XPC control service (stub implementation)
    pub fn start_xpc_service(&self) -> Result<(), Box<dyn std::error::Error>> {
        Err("FSKit is only available on macOS".into())
    }
}
