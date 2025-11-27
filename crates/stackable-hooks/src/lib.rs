// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

#![allow(clippy::macro_metavars_in_unsafe)]

extern crate libc;

#[doc(hidden)]
pub use paste::paste as __stackable_paste;

#[cfg(target_env = "gnu")]
pub mod ld_preload;

#[cfg(target_env = "gnu")]
pub use ld_preload::{disable_hooks, enable_hooks, hooks_allowed, with_reentrancy};

#[cfg(any(target_os = "macos", target_os = "ios"))]
pub mod dyld_insert_libraries;

#[cfg(any(target_os = "macos", target_os = "ios"))]
pub use dyld_insert_libraries::{disable_hooks, enable_hooks, hooks_allowed, with_reentrancy};

// Auto-propagation module for automatic shim injection into child processes
pub mod auto_propagation;
pub use auto_propagation::{disable_auto_propagation, enable_auto_propagation};

// Subprocess spawning hooks for auto-propagation
// Enabled by default via the `propagation-hooks` feature. An optional
// `propagation-hooks-env-control` feature gates runtime environment variable
// checks for deployments that need to disable propagation dynamically.
#[cfg(feature = "propagation-hooks")]
mod propagation_hooks;
