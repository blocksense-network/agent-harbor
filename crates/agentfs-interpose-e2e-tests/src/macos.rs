// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! macOS-specific functionality for the AgentFS test helper.
//!
//! This module contains all macOS-specific test helper functions including
//! FSEvents testing, kqueue operations, and other platform-specific tests.

#[cfg(target_os = "macos")]
pub mod tests;

#[cfg(target_os = "macos")]
pub use tests::*;
