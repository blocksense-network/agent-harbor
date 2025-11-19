// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Platform-specific test utilities for shim injection

#[cfg(target_os = "macos")]
pub mod macos;
#[cfg(target_os = "macos")]
pub use macos::*;

#[cfg(not(target_os = "macos"))]
pub mod linux;
#[cfg_attr(not(target_os = "macos"), allow(unused_imports))]
#[cfg(not(target_os = "macos"))]
pub use linux::*;
