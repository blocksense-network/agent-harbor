// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! VCS repository abstraction crate for Agent Harbor.
//!
//! This crate provides a unified interface for working with different VCS types
//! (Git, Mercurial, Bazaar, Fossil) with consistent APIs for common operations.

pub mod error;
pub mod repo;
pub mod test_helpers;
pub mod vcs_types;

pub use error::{VcsError, VcsResult};
pub use repo::{FileStream, VcsRepo};
pub use vcs_types::VcsType;
