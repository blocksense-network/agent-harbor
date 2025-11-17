// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! ZFS snapshot provider facade for Agent Harbor.
//!
//! This crate provides a facade that re-exports the ZFS provider from
//! the main ah-fs-snapshots crate. It exists for backwards compatibility
//! and to provide a convenient import path for users who only need ZFS.

pub use ah_fs_snapshots::ZfsProvider;
