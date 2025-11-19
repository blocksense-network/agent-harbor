// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

extern crate libc;

#[doc(hidden)]
pub use paste::paste as __stackable_paste;

#[cfg(target_env = "gnu")]
pub mod ld_preload;

#[cfg(any(target_os = "macos", target_os = "ios"))]
pub mod dyld_insert_libraries;

#[cfg(any(target_os = "macos", target_os = "ios"))]
pub use dyld_insert_libraries::{enable_hooks, hooks_allowed, with_reentrancy};
