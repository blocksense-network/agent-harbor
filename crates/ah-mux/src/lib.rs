// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Terminal multiplexer implementations
//!
//! This crate provides concrete implementations of the Multiplexer trait
//! for various terminal multiplexers (tmux, kitty, zellij, screen, wezterm, etc.).

use tracing::{debug, error, info, instrument, warn};

pub mod detection;

#[cfg(feature = "emacs")]
pub mod emacs;
#[cfg(feature = "ghostty")]
pub mod ghostty;
pub mod iterm2;
#[cfg(feature = "kitty")]
pub mod kitty;
#[cfg(feature = "neovim")]
pub mod neovim;
#[cfg(feature = "screen")]
pub mod screen;
#[cfg(all(feature = "tilix", target_os = "linux"))]
pub mod tilix;
pub mod tmux;
#[cfg(feature = "vim")]
pub mod vim;
#[cfg(feature = "wezterm")]
pub mod wezterm;
#[cfg(feature = "windows-terminal")]
pub mod windows_terminal;
#[cfg(feature = "zellij")]
pub mod zellij;

#[cfg(feature = "emacs")]
pub use emacs::EmacsMultiplexer;
#[cfg(feature = "ghostty")]
pub use ghostty::GhosttyMultiplexer;
pub use iterm2::ITerm2Multiplexer;
#[cfg(feature = "kitty")]
pub use kitty::KittyMultiplexer;
#[cfg(feature = "neovim")]
pub use neovim::NeovimMultiplexer;
#[cfg(feature = "screen")]
pub use screen::ScreenMultiplexer;
#[cfg(all(feature = "tilix", target_os = "linux"))]
pub use tilix::TilixMultiplexer;
pub use tmux::TmuxMultiplexer;
#[cfg(feature = "vim")]
pub use vim::VimMultiplexer;
#[cfg(feature = "wezterm")]
pub use wezterm::WezTermMultiplexer;
#[cfg(feature = "windows-terminal")]
pub use windows_terminal::WindowsTerminalMultiplexer;
#[cfg(feature = "zellij")]
pub use zellij::ZellijMultiplexer;

use ah_mux_core::*;

// Re-export detection functions
pub use detection::*;

/// Get the default multiplexer for the current system
#[instrument(fields(component = "ah_mux", operation = "default_multiplexer"))]
pub fn default_multiplexer() -> Result<Box<dyn Multiplexer + Send + Sync>, MuxError> {
    info!("Starting multiplexer detection with priority order");

    // Priority order: tmux > wezterm > kitty > zellij > screen > tilix > windows-terminal > ghostty > neovim > vim > emacs
    #[cfg(feature = "tmux")]
    if let Ok(tmux) = tmux::TmuxMultiplexer::new() {
        if tmux.is_available() {
            info!(multiplexer = "tmux", "Found available multiplexer");
            return Ok(Box::new(tmux));
        } else {
            debug!(multiplexer = "tmux", "Multiplexer not available");
        }
    }

    #[cfg(target_os = "macos")]
    if let Ok(iterm2) = iterm2::ITerm2Multiplexer::new() {
        if iterm2.is_available() {
            info!(multiplexer = "iterm2", "Found available multiplexer");
            return Ok(Box::new(iterm2));
        } else {
            debug!(multiplexer = "iterm2", "Multiplexer not available");
        }
    }

    #[cfg(feature = "wezterm")]
    if let Ok(wezterm) = wezterm::WezTermMultiplexer::new() {
        if wezterm.is_available() {
            info!(multiplexer = "wezterm", "Found available multiplexer");
            return Ok(Box::new(wezterm));
        } else {
            debug!(multiplexer = "wezterm", "Multiplexer not available");
        }
    }

    #[cfg(feature = "kitty")]
    if let Ok(kitty) = kitty::KittyMultiplexer::new() {
        if kitty.is_available() {
            info!(multiplexer = "kitty", "Found available multiplexer");
            return Ok(Box::new(kitty));
        } else {
            debug!(multiplexer = "kitty", "Multiplexer not available");
        }
    }

    #[cfg(feature = "zellij")]
    if let Ok(zellij) = zellij::ZellijMultiplexer::new() {
        if zellij.is_available() {
            info!(multiplexer = "zellij", "Found available multiplexer");
            return Ok(Box::new(zellij));
        } else {
            debug!(multiplexer = "zellij", "Multiplexer not available");
        }
    }

    #[cfg(feature = "screen")]
    if let Ok(screen) = screen::ScreenMultiplexer::new() {
        if screen.is_available() {
            info!(multiplexer = "screen", "Found available multiplexer");
            return Ok(Box::new(screen));
        } else {
            debug!(multiplexer = "screen", "Multiplexer not available");
        }
    }

    #[cfg(all(feature = "tilix", target_os = "linux"))]
    if let Ok(tilix) = tilix::TilixMultiplexer::new() {
        if tilix.is_available() {
            info!(multiplexer = "tilix", "Found available multiplexer");
            return Ok(Box::new(tilix));
        } else {
            debug!(multiplexer = "tilix", "Multiplexer not available");
        }
    }

    #[cfg(feature = "windows-terminal")]
    if let Ok(wt) = windows_terminal::WindowsTerminalMultiplexer::new() {
        if wt.is_available() {
            info!(
                multiplexer = "windows-terminal",
                "Found available multiplexer"
            );
            return Ok(Box::new(wt));
        } else {
            debug!(
                multiplexer = "windows-terminal",
                "Multiplexer not available"
            );
        }
    }

    #[cfg(feature = "ghostty")]
    if let Ok(ghostty) = ghostty::GhosttyMultiplexer::new() {
        if ghostty.is_available() {
            info!(multiplexer = "ghostty", "Found available multiplexer");
            return Ok(Box::new(ghostty));
        } else {
            debug!(multiplexer = "ghostty", "Multiplexer not available");
        }
    }

    #[cfg(feature = "neovim")]
    if let Ok(neovim) = neovim::NeovimMultiplexer::new() {
        if neovim.is_available() {
            info!(multiplexer = "neovim", "Found available multiplexer");
            return Ok(Box::new(neovim));
        } else {
            debug!(multiplexer = "neovim", "Multiplexer not available");
        }
    }

    #[cfg(feature = "vim")]
    if let Ok(vim) = vim::VimMultiplexer::new() {
        if vim.is_available() {
            info!(multiplexer = "vim", "Found available multiplexer");
            return Ok(Box::new(vim));
        } else {
            debug!(multiplexer = "vim", "Multiplexer not available");
        }
    }

    #[cfg(feature = "emacs")]
    if let Ok(emacs) = emacs::EmacsMultiplexer::new() {
        if emacs.is_available() {
            info!(multiplexer = "emacs", "Found available multiplexer");
            return Ok(Box::new(emacs));
        } else {
            debug!(multiplexer = "emacs", "Multiplexer not available");
        }
    }

    warn!("No supported multiplexer found, checked all available options");
    Err(MuxError::NotAvailable("No supported multiplexer found"))
}

/// Get a multiplexer by name
#[instrument(fields(component = "ah_mux", operation = "multiplexer_by_name", multiplexer_name = %name))]
pub fn multiplexer_by_name(name: &str) -> Result<Box<dyn Multiplexer + Send + Sync>, MuxError> {
    info!("Creating multiplexer by name");

    match name {
        #[cfg(feature = "tmux")]
        "tmux" => {
            let tmux = tmux::TmuxMultiplexer::new().map_err(|e| {
                error!(error = %e, "Failed to create tmux multiplexer");
                MuxError::Other(format!("Failed to create tmux multiplexer: {}", e))
            })?;
            debug!("Successfully created tmux multiplexer");
            Ok(Box::new(tmux))
        }
        #[cfg(target_os = "macos")]
        "iterm2" => {
            let iterm2 = iterm2::ITerm2Multiplexer::new().map_err(|e| {
                error!(error = %e, "Failed to create iTerm2 multiplexer");
                MuxError::Other(format!("Failed to create iTerm2 multiplexer: {}", e))
            })?;
            debug!("Successfully created iTerm2 multiplexer");
            Ok(Box::new(iterm2))
        }
        #[cfg(feature = "kitty")]
        "kitty" => {
            let kitty = kitty::KittyMultiplexer::new().map_err(|e| {
                error!(error = %e, "Failed to create kitty multiplexer");
                MuxError::Other(format!("Failed to create kitty multiplexer: {}", e))
            })?;
            debug!("Successfully created kitty multiplexer");
            Ok(Box::new(kitty))
        }
        #[cfg(feature = "wezterm")]
        "wezterm" => {
            let wezterm = wezterm::WezTermMultiplexer::new().map_err(|e| {
                MuxError::Other(format!("Failed to create wezterm multiplexer: {}", e))
            })?;
            Ok(Box::new(wezterm))
        }
        #[cfg(feature = "zellij")]
        "zellij" => {
            let zellij = zellij::ZellijMultiplexer::new().map_err(|e| {
                MuxError::Other(format!("Failed to create zellij multiplexer: {}", e))
            })?;
            Ok(Box::new(zellij))
        }
        #[cfg(feature = "screen")]
        "screen" => {
            let screen = screen::ScreenMultiplexer::new().map_err(|e| {
                MuxError::Other(format!("Failed to create screen multiplexer: {}", e))
            })?;
            Ok(Box::new(screen))
        }
        #[cfg(all(feature = "tilix", target_os = "linux"))]
        "tilix" => {
            let tilix = tilix::TilixMultiplexer::new()?;
            Ok(Box::new(tilix))
        }
        #[cfg(feature = "windows-terminal")]
        "windows-terminal" => {
            let wt = windows_terminal::WindowsTerminalMultiplexer::new()?;
            Ok(Box::new(wt))
        }
        #[cfg(feature = "ghostty")]
        "ghostty" => {
            let ghostty = ghostty::GhosttyMultiplexer::new()?;
            Ok(Box::new(ghostty))
        }
        #[cfg(feature = "neovim")]
        "neovim" => {
            let neovim = neovim::NeovimMultiplexer::new()?;
            Ok(Box::new(neovim))
        }
        #[cfg(feature = "vim")]
        "vim" => {
            let vim = vim::VimMultiplexer::new()?;
            Ok(Box::new(vim))
        }
        #[cfg(feature = "emacs")]
        "emacs" => {
            let emacs = emacs::EmacsMultiplexer::new()?;
            Ok(Box::new(emacs))
        }
        _ => {
            error!(multiplexer_name = %name, "Unsupported multiplexer requested");
            Err(MuxError::Other(format!(
                "Unsupported multiplexer: {}",
                name
            )))
        }
    }
}

/// Get all available multiplexers for testing
pub fn available_multiplexers() -> Vec<(String, Box<dyn Multiplexer + Send + Sync>)> {
    let mut multiplexers = Vec::new();

    #[cfg(feature = "tmux")]
    if let Ok(tmux) = tmux::TmuxMultiplexer::new() {
        if tmux.is_available() {
            multiplexers.push((
                "tmux".to_string(),
                Box::new(tmux) as Box<dyn Multiplexer + Send + Sync>,
            ));
        }
    }

    #[cfg(target_os = "macos")]
    if let Ok(iterm2) = iterm2::ITerm2Multiplexer::new() {
        if iterm2.is_available() {
            multiplexers.push((
                "iterm2".to_string(),
                Box::new(iterm2) as Box<dyn Multiplexer + Send + Sync>,
            ));
        }
    }

    #[cfg(feature = "kitty")]
    if let Ok(kitty) = kitty::KittyMultiplexer::new() {
        if kitty.is_available() {
            multiplexers.push((
                "kitty".to_string(),
                Box::new(kitty) as Box<dyn Multiplexer + Send + Sync>,
            ));
        }
    }

    #[cfg(feature = "wezterm")]
    if let Ok(wezterm) = wezterm::WezTermMultiplexer::new() {
        if wezterm.is_available() {
            multiplexers.push((
                "wezterm".to_string(),
                Box::new(wezterm) as Box<dyn Multiplexer + Send + Sync>,
            ));
        }
    }

    #[cfg(feature = "zellij")]
    if let Ok(zellij) = zellij::ZellijMultiplexer::new() {
        if zellij.is_available() {
            multiplexers.push((
                "zellij".to_string(),
                Box::new(zellij) as Box<dyn Multiplexer + Send + Sync>,
            ));
        }
    }

    #[cfg(feature = "screen")]
    if let Ok(screen) = screen::ScreenMultiplexer::new() {
        if screen.is_available() {
            multiplexers.push((
                "screen".to_string(),
                Box::new(screen) as Box<dyn Multiplexer + Send + Sync>,
            ));
        }
    }

    #[cfg(all(feature = "tilix", target_os = "linux"))]
    if let Ok(tilix) = tilix::TilixMultiplexer::new() {
        if tilix.is_available() {
            multiplexers.push((
                "tilix".to_string(),
                Box::new(tilix) as Box<dyn Multiplexer + Send + Sync>,
            ));
        }
    }

    #[cfg(feature = "windows-terminal")]
    if let Ok(wt) = windows_terminal::WindowsTerminalMultiplexer::new() {
        if wt.is_available() {
            multiplexers.push((
                "windows-terminal".to_string(),
                Box::new(wt) as Box<dyn Multiplexer + Send + Sync>,
            ));
        }
    }

    #[cfg(feature = "ghostty")]
    if let Ok(ghostty) = ghostty::GhosttyMultiplexer::new() {
        if ghostty.is_available() {
            multiplexers.push((
                "ghostty".to_string(),
                Box::new(ghostty) as Box<dyn Multiplexer + Send + Sync>,
            ));
        }
    }

    #[cfg(feature = "neovim")]
    if let Ok(neovim) = neovim::NeovimMultiplexer::new() {
        if neovim.is_available() {
            multiplexers.push((
                "neovim".to_string(),
                Box::new(neovim) as Box<dyn Multiplexer + Send + Sync>,
            ));
        }
    }

    #[cfg(feature = "vim")]
    if let Ok(vim) = vim::VimMultiplexer::new() {
        if vim.is_available() {
            multiplexers.push((
                "vim".to_string(),
                Box::new(vim) as Box<dyn Multiplexer + Send + Sync>,
            ));
        }
    }

    #[cfg(feature = "emacs")]
    if let Ok(emacs) = emacs::EmacsMultiplexer::new() {
        if emacs.is_available() {
            multiplexers.push((
                "emacs".to_string(),
                Box::new(emacs) as Box<dyn Multiplexer + Send + Sync>,
            ));
        }
    }

    multiplexers
}
