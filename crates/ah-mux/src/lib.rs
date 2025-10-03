//! Terminal multiplexer implementations
//!
//! This crate provides concrete implementations of the Multiplexer trait
//! for various terminal multiplexers (tmux, kitty, etc.).

#[cfg(feature = "kitty")]
pub mod kitty;
pub mod tmux;

use ah_mux_core::*;

/// Get the default multiplexer for the current system
pub fn default_multiplexer() -> Result<Box<dyn Multiplexer + Send + Sync>, MuxError> {
    // Priority order: tmux > wezterm > kitty > zellij > screen
    #[cfg(feature = "tmux")]
    if let Ok(tmux) = tmux::TmuxMultiplexer::new() {
        if tmux.is_available() {
            return Ok(Box::new(tmux));
        }
    }

    #[cfg(feature = "kitty")]
    if let Ok(kitty) = kitty::KittyMultiplexer::new() {
        if kitty.is_available() {
            return Ok(Box::new(kitty));
        }
    }

    // Add other multiplexers here as they are implemented

    Err(MuxError::NotAvailable("No supported multiplexer found"))
}

/// Get a multiplexer by name
pub fn multiplexer_by_name(name: &str) -> Result<Box<dyn Multiplexer + Send + Sync>, MuxError> {
    match name {
        #[cfg(feature = "tmux")]
        "tmux" => {
            let tmux = tmux::TmuxMultiplexer::new().map_err(|e| {
                MuxError::Other(format!("Failed to create tmux multiplexer: {}", e))
            })?;
            Ok(Box::new(tmux))
        }
        #[cfg(feature = "kitty")]
        "kitty" => {
            let kitty = kitty::KittyMultiplexer::new().map_err(|e| {
                MuxError::Other(format!("Failed to create kitty multiplexer: {}", e))
            })?;
            Ok(Box::new(kitty))
        }
        // Add other multiplexers here
        _ => Err(MuxError::Other(format!(
            "Unsupported multiplexer: {}",
            name
        ))),
    }
}
