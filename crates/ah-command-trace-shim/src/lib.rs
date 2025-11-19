#![cfg_attr(not(any(target_os = "macos", target_os = "linux")), allow(dead_code))]
// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

/// Command Trace Interposition Shim
///
/// This crate provides a cross-platform interposition shim that captures command
/// execution and output streams. It uses:
/// - macOS: DYLD_INSERT_LIBRARIES for dynamic library interposition
/// - Linux: LD_PRELOAD for shared library preloading
///
/// The shim maintains an internal FD table to track file descriptors across
/// dup/fork operations and captures stdout/stderr writes from child processes.
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

/// Shared POSIX functionality
pub mod posix;

/// Core types and logic shared across platforms
pub mod core {
    use once_cell::sync::OnceCell;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicBool, Ordering};
    use tracing::{debug, error, info, warn};

    // One-time tracing subscriber initialization guard
    static TRACING_INIT: AtomicBool = AtomicBool::new(false);

    /// Environment variable names for configuration
    pub const ENV_ENABLED: &str = "AH_CMDTRACE_ENABLED";
    pub const ENV_SOCKET: &str = "AH_CMDTRACE_SOCKET";
    pub const ENV_LOG: &str = "AH_CMDTRACE_LOG";

    /// Global state for the shim
    pub static SHIM_STATE: OnceCell<Mutex<ShimState>> = OnceCell::new();

    /// Current state of the shim
    #[derive(Debug, Clone, Default)]
    pub enum ShimState {
        /// Shim is disabled or not initialized
        #[default]
        Disabled,
        /// Shim is initialized and ready
        Ready {
            socket_path: String,
            log_enabled: bool,
        },
        /// Shim encountered an error during initialization
        Error(String),
    }

    /// Initialize the shim state from environment variables
    pub fn initialize_shim_state() -> ShimState {
        // Initialize tracing subscriber once (stderr writer, env filter optional)
        if !TRACING_INIT.load(Ordering::Relaxed)
            && tracing_subscriber::fmt().with_writer(std::io::stderr).try_init().is_ok()
        {
            TRACING_INIT.store(true, Ordering::Relaxed);
        }
        debug!("Initializing shim state");
        debug!(
            enabled = ?std::env::var(ENV_ENABLED),
            socket = ?std::env::var(ENV_SOCKET),
            log = ?std::env::var(ENV_LOG),
            "Environment variables"
        );

        let enabled = std::env::var(ENV_ENABLED)
            .map(|v| v == "1" || v.to_lowercase() == "true")
            .unwrap_or(true); // Default to enabled

        debug!(enabled, "Shim enabled setting evaluated");

        if !enabled {
            warn!("Shim disabled via environment");
            return ShimState::Disabled;
        }

        let socket_path = match std::env::var(ENV_SOCKET) {
            Ok(path) if !path.is_empty() => {
                info!(socket_path = %path, "Socket path configured");
                path
            }
            _ => {
                error!("Socket path not set or empty");
                return ShimState::Error("AH_CMDTRACE_SOCKET not set or empty".into());
            }
        };

        let log_enabled = std::env::var(ENV_LOG)
            .map(|v| v != "0" && v.to_lowercase() != "false")
            .unwrap_or(true); // Default to logging enabled

        info!(log_enabled, "Shim logging configuration resolved");
        info!("Shim Ready state established");

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
            info!(message, "shim log message");
        }
    }
}
