// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Sandbox isolation abstractions for Agent Harbor.
//!
//! This crate provides abstractions for different sandboxing technologies
//! (Linux namespaces, Docker, etc.) to enable secure agent execution.

use async_trait::async_trait;

pub mod error;

/// Result type for sandbox operations.
pub type Result<T> = std::result::Result<T, Error>;

/// Error type for sandbox operations.
pub use error::Error;

/// Configuration for sandbox execution.
#[derive(Debug, Clone)]
pub struct SandboxConfig {
    /// Root filesystem for the sandbox.
    pub rootfs: std::path::PathBuf,
    /// Working directory inside the sandbox.
    pub working_dir: Option<std::path::PathBuf>,
    /// Environment variables to set.
    pub env: Vec<(String, String)>,
    /// Command and arguments to execute.
    pub command: Vec<String>,
}

/// Result of sandbox execution.
#[derive(Debug)]
pub struct SandboxResult {
    /// Exit code of the process.
    pub exit_code: i32,
    /// Whether the execution was successful.
    pub success: bool,
}

/// Core trait for sandbox providers.
#[async_trait]
pub trait SandboxProvider: Send + Sync {
    /// Execute a command in the sandbox.
    async fn execute(&self, config: &SandboxConfig) -> Result<SandboxResult>;

    /// Check if this provider is available on the current system.
    fn is_available() -> bool
    where
        Self: Sized;
}

/// Get the best available sandbox provider for the current platform.
pub fn default_provider() -> Result<Box<dyn SandboxProvider>> {
    // For now, return an error as sandbox providers need to be integrated
    // TODO: Implement proper provider detection when subcrates are integrated
    Err(Error::NoAvailableProvider)
}
