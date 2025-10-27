// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Repository Enumeration - Abstract Repository Discovery Interface
//!
//! This module defines the `RepositoriesEnumerator` trait that abstracts repository
//! discovery functionality across different modes (local, remote).
//!
//! ## Architecture Overview
//!
//! The RepositoriesEnumerator trait provides a clean abstraction for discovering
//! available repositories, allowing the ViewModel to be decoupled from the specifics
//! of repository discovery.
//!
//! Different implementations handle different discovery modes:
//! - Local: Discover repositories from local filesystem and VCS detection
//! - Remote: Discover repositories via REST API calls to remote server
//!
//! ## Design Principles
//!
//! ### Location and Dependencies
//!
//! The `RepositoriesEnumerator` trait is located in `ah-core` because:
//!
//! 1. **ah-core contains repository enumeration logic**: Whether discovering
//!    repositories locally through VCS detection or remotely via REST APIs,
//!    ah-core provides the full enumeration environment.
//!
//! 2. **ah-core implements RepositoriesEnumerator for all modes**: It contains
//!    implementations that use local VCS detection (`LocalRepositoriesEnumerator`)
//!    and REST API clients (`RemoteRepositoriesEnumerator`).
//!
//! ## Usage in MVVM Architecture
//!
//! The ViewModel holds a reference to a RepositoriesEnumerator and calls
//! `list_repositories()` to populate the repository selection UI. The
//! RepositoriesEnumerator handles the actual discovery details and returns
//! a result that the ViewModel can translate into domain messages.

use ah_domain_types::Repository;
use async_trait::async_trait;

/// Abstract trait for repository discovery functionality
///
/// This trait defines the interface that all repository enumerators must implement.
/// Different implementations handle different discovery modes:
/// - Local: Discover repositories from local filesystem and VCS detection
/// - Remote: Discover repositories via REST API calls to remote server
#[async_trait]
pub trait RepositoriesEnumerator: Send + Sync {
    /// List available repositories
    ///
    /// Returns repositories that can be used for task creation.
    /// Local implementations discover repositories from filesystem and VCS detection.
    /// Remote implementations query REST APIs for available repositories.
    async fn list_repositories(&self) -> Vec<Repository>;

    /// Get a human-readable description of this repository enumerator
    fn description(&self) -> &str;
}
