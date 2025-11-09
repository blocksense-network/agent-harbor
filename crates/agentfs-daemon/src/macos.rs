// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! macOS-specific functionality for the AgentFS daemon.
//!
//! This module contains all macOS-specific code including kqueue operations
//! and interposition helpers that are split out into their respective submodules.

#[cfg(target_os = "macos")]
pub mod kqueue;

#[cfg(not(target_os = "macos"))]
pub mod kqueue {}

#[cfg(target_os = "macos")]
pub mod interposition;

#[cfg(not(target_os = "macos"))]
pub mod interposition {}
