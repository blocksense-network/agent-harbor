// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! macOS-specific functionality for the AgentFS daemon.
//!
//! This module contains all macOS-specific code including kqueue operations
//! and other platform-specific filesystem watching functionality.

#[cfg(target_os = "macos")]
pub mod kqueue {
    // TODO: Move macOS-specific kqueue functionality here from watch_service.rs
}

#[cfg(target_os = "macos")]
pub mod interposition {
    // TODO: Move macOS-specific interposition functionality here
}
