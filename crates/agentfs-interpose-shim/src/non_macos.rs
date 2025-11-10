// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Stub implementation for non-macOS targets. The interposition shim is macOS-specific.

// Expose a no-op init hook so dependent crates can compile on non-macOS targets.
#[allow(unused_variables)]
pub fn init() {}
