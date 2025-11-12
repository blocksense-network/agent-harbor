// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Terminal multiplexer implementations
//!
//! This crate provides concrete implementations of the Multiplexer trait
//! for various terminal multiplexers (tmux, kitty, zellij, screen, wezterm, etc.).

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
pub fn default_multiplexer() -> Result<Box<dyn Multiplexer + Send + Sync>, MuxError> {
    // Priority order: tmux > wezterm > kitty > zellij > screen > tilix > windows-terminal > ghostty > neovim > vim > emacs
    #[cfg(feature = "tmux")]
    if let Ok(tmux) = tmux::TmuxMultiplexer::new() {
        if tmux.is_available() {
            return Ok(Box::new(tmux));
        }
    }

    #[cfg(target_os = "macos")]
    if let Ok(iterm2) = iterm2::ITerm2Multiplexer::new() {
        if iterm2.is_available() {
            return Ok(Box::new(iterm2));
        }
    }

    #[cfg(feature = "wezterm")]
    if let Ok(wezterm) = wezterm::WezTermMultiplexer::new() {
        if wezterm.is_available() {
            return Ok(Box::new(wezterm));
        }
    }

    #[cfg(feature = "kitty")]
    if let Ok(kitty) = kitty::KittyMultiplexer::new() {
        if kitty.is_available() {
            return Ok(Box::new(kitty));
        }
    }

    #[cfg(feature = "zellij")]
    if let Ok(zellij) = zellij::ZellijMultiplexer::new() {
        if zellij.is_available() {
            return Ok(Box::new(zellij));
        }
    }

    #[cfg(feature = "screen")]
    if let Ok(screen) = screen::ScreenMultiplexer::new() {
        if screen.is_available() {
            return Ok(Box::new(screen));
        }
    }

    #[cfg(all(feature = "tilix", target_os = "linux"))]
    if let Ok(tilix) = tilix::TilixMultiplexer::new() {
        if tilix.is_available() {
            return Ok(Box::new(tilix));
        }
    }

    #[cfg(feature = "windows-terminal")]
    if let Ok(wt) = windows_terminal::WindowsTerminalMultiplexer::new() {
        if wt.is_available() {
            return Ok(Box::new(wt));
        }
    }

    #[cfg(feature = "ghostty")]
    if let Ok(ghostty) = ghostty::GhosttyMultiplexer::new() {
        if ghostty.is_available() {
            return Ok(Box::new(ghostty));
        }
    }

    #[cfg(feature = "neovim")]
    if let Ok(neovim) = neovim::NeovimMultiplexer::new() {
        if neovim.is_available() {
            return Ok(Box::new(neovim));
        }
    }

    #[cfg(feature = "vim")]
    if let Ok(vim) = vim::VimMultiplexer::new() {
        if vim.is_available() {
            return Ok(Box::new(vim));
        }
    }

    #[cfg(feature = "emacs")]
    if let Ok(emacs) = emacs::EmacsMultiplexer::new() {
        if emacs.is_available() {
            return Ok(Box::new(emacs));
        }
    }

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
        #[cfg(target_os = "macos")]
        "iterm2" => {
            let iterm2 = iterm2::ITerm2Multiplexer::new().map_err(|e| {
                MuxError::Other(format!("Failed to create iTerm2 multiplexer: {}", e))
            })?;
            Ok(Box::new(iterm2))
        }
        #[cfg(feature = "kitty")]
        "kitty" => {
            let kitty = kitty::KittyMultiplexer::new().map_err(|e| {
                MuxError::Other(format!("Failed to create kitty multiplexer: {}", e))
            })?;
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
        _ => Err(MuxError::Other(format!(
            "Unsupported multiplexer: {}",
            name
        ))),
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
