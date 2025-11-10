#![cfg_attr(not(target_os = "macos"), allow(dead_code))]
// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

#[cfg(target_os = "macos")]
pub mod macos;

#[cfg(target_os = "macos")]
pub use macos::*;

#[cfg(not(target_os = "macos"))]
mod non_macos;

#[cfg(not(target_os = "macos"))]
pub use non_macos::*;
