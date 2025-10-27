// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Branch Enumeration - Abstract Branch Discovery Interface
//!
//! This module defines the `BranchesEnumerator` trait that abstracts branch
//! discovery functionality across different modes (local, remote).

use ah_domain_types::Branch;
use async_trait::async_trait;

/// Abstract trait for branch discovery functionality
///
/// This trait defines the interface that all branch enumerators must implement.
/// Different implementations handle different discovery modes:
/// - Local: Discover branches from local VCS repositories
/// - Remote: Discover branches via REST API calls to remote server
#[async_trait]
pub trait BranchesEnumerator: Send + Sync {
    /// List branches for a specific repository
    ///
    /// Returns available branches for the given repository.
    /// Local implementations query the local VCS repository.
    /// Remote implementations query REST APIs for available branches.
    async fn list_branches(&self, repository_id: &str) -> Vec<Branch>;

    /// Get a human-readable description of this branch enumerator
    fn description(&self) -> &str;
}
