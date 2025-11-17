// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Git snapshot provider facade for Agent Harbor.
//!
//! This crate provides a facade that re-exports the Git provider from
//! the main ah-fs-snapshots crate. It exists for backwards compatibility
//! and to provide a convenient import path for users who only need Git.

pub use ah_fs_snapshots::GitProvider;
