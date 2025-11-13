// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Common types and utilities for agent implementations

use serde::{Deserialize, Serialize};

/// Common status information for all AI agents
///
/// This struct provides a standardized representation of an agent's health status,
/// including availability, version, authentication state, and any errors encountered.
/// It is used by all agent implementations to ensure consistent status reporting.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentStatus {
    /// Whether the CLI is installed and available
    pub available: bool,
    /// Version information if available
    pub version: Option<String>,
    /// Whether the user is authenticated
    pub authenticated: bool,
    /// Authentication method used (e.g., "API_KEY", "OAuth", "Token", etc.)
    pub auth_method: Option<String>,
    /// Source of authentication (config file path, environment variable name, etc.)
    pub auth_source: Option<String>,
    /// Any error that occurred during status check
    pub error: Option<String>,
}

impl AgentStatus {
    /// Create a new `AgentStatus` with default values indicating unavailability
    pub fn new() -> Self {
        Self {
            available: false,
            version: None,
            authenticated: false,
            auth_method: None,
            auth_source: None,
            error: None,
        }
    }

    /// Create a new `AgentStatus` indicating an error occurred
    pub fn error(error: impl Into<String>) -> Self {
        Self {
            available: false,
            version: None,
            authenticated: false,
            auth_method: None,
            auth_source: None,
            error: Some(error.into()),
        }
    }

    /// Set the available status
    pub fn with_available(mut self, available: bool) -> Self {
        self.available = available;
        self
    }

    /// Set the version information
    pub fn with_version(mut self, version: impl Into<String>) -> Self {
        self.version = Some(version.into());
        self
    }

    /// Set the authentication status
    pub fn with_authenticated(mut self, authenticated: bool) -> Self {
        self.authenticated = authenticated;
        self
    }

    /// Set the authentication method
    pub fn with_auth_method(mut self, auth_method: impl Into<String>) -> Self {
        self.auth_method = Some(auth_method.into());
        self
    }

    /// Set the authentication source
    pub fn with_auth_source(mut self, auth_source: impl Into<String>) -> Self {
        self.auth_source = Some(auth_source.into());
        self
    }

    /// Set an error message
    pub fn with_error(mut self, error: impl Into<String>) -> Self {
        self.error = Some(error.into());
        self
    }
}

impl Default for AgentStatus {
    fn default() -> Self {
        Self::new()
    }
}
