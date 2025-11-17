#![cfg_attr(not(any(target_os = "macos", target_os = "linux")), allow(dead_code))]
// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Command Trace Interposition Shim
//!
//! This crate provides a cross-platform interposition shim that captures command
//! execution and output streams. It uses:
//! - macOS: DYLD_INSERT_LIBRARIES for dynamic library interposition
//! - Linux: LD_PRELOAD for shared library preloading
//!
//! The shim maintains an internal FD table to track file descriptors across
//! dup/fork operations and captures stdout/stderr writes from child processes.

#[cfg(target_os = "macos")]
pub mod platform;

#[cfg(target_os = "linux")]
pub mod platform;

#[cfg(any(target_os = "macos", target_os = "linux"))]
pub use platform::*;

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
mod unsupported;

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
pub use unsupported::*;

/// Core types and logic shared across platforms
pub mod core {
    use once_cell::sync::OnceCell;
    use std::sync::Mutex;

    /// Environment variable names for configuration
    pub const ENV_ENABLED: &str = "AH_CMDTRACE_ENABLED";
    pub const ENV_SOCKET: &str = "AH_CMDTRACE_SOCKET";
    pub const ENV_LOG: &str = "AH_CMDTRACE_LOG";

    /// Global state for the shim
    pub static SHIM_STATE: OnceCell<Mutex<ShimState>> = OnceCell::new();

    /// Current state of the shim
    #[derive(Debug, Clone)]
    pub enum ShimState {
        /// Shim is disabled or not initialized
        Disabled,
        /// Shim is initialized and ready
        Ready {
            socket_path: String,
            log_enabled: bool,
        },
        /// Shim encountered an error during initialization
        Error(String),
    }

    impl Default for ShimState {
        fn default() -> Self {
            ShimState::Disabled
        }
    }

    /// Initialize the shim state from environment variables
    pub fn initialize_shim_state() -> ShimState {
        eprintln!("[ah-command-trace-shim] Initializing shim state...");
        eprintln!("[ah-command-trace-shim] Environment variables:");
        eprintln!(
            "[ah-command-trace-shim]   {}={:?}",
            ENV_ENABLED,
            std::env::var(ENV_ENABLED)
        );
        eprintln!(
            "[ah-command-trace-shim]   {}={:?}",
            ENV_SOCKET,
            std::env::var(ENV_SOCKET)
        );
        eprintln!(
            "[ah-command-trace-shim]   {}={:?}",
            ENV_LOG,
            std::env::var(ENV_LOG)
        );

        let enabled = std::env::var(ENV_ENABLED)
            .map(|v| v == "1" || v.to_lowercase() == "true")
            .unwrap_or(true); // Default to enabled

        eprintln!("[ah-command-trace-shim] Enabled: {}", enabled);

        if !enabled {
            eprintln!("[ah-command-trace-shim] Shim disabled");
            return ShimState::Disabled;
        }

        let socket_path = match std::env::var(ENV_SOCKET) {
            Ok(path) if !path.is_empty() => {
                eprintln!("[ah-command-trace-shim] Socket path: {}", path);
                path
            }
            _ => {
                eprintln!("[ah-command-trace-shim] Socket path not set or empty");
                return ShimState::Error("AH_CMDTRACE_SOCKET not set or empty".into());
            }
        };

        let log_enabled = std::env::var(ENV_LOG)
            .map(|v| v != "0" && v.to_lowercase() != "false")
            .unwrap_or(true); // Default to logging enabled

        eprintln!("[ah-command-trace-shim] Log enabled: {}", log_enabled);
        eprintln!("[ah-command-trace-shim] Returning Ready state");

        ShimState::Ready {
            socket_path,
            log_enabled,
        }
    }

    /// Log a message if logging is enabled
    pub fn log_message(state: &ShimState, message: &str) {
        if let ShimState::Ready {
            log_enabled: true, ..
        } = state
        {
            eprintln!("[ah-command-trace-shim] {}", message);
        }
    }
}
