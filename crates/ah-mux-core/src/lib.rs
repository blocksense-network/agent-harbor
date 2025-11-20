// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Low-level, AH-agnostic multiplexer trait and shared types
//!
//! This crate defines the common interface that all terminal multiplexer
//! implementations must provide, without any agent-harbor specific logic.

/// Opaque identifiers; implementations may encode native IDs/titles internally.
pub type WindowId = String;
pub type PaneId = String;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SplitDirection {
    Horizontal,
    Vertical,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SplitMode {
    /// No splitting - create new window
    None,
    /// Split horizontally (side by side)
    Horizontal,
    /// Split vertically (top/bottom)
    Vertical,
    /// Auto - let the multiplexer decide the best split direction
    Auto,
}

#[derive(Clone, Debug, Default)]
pub struct WindowOptions<'a> {
    pub title: Option<&'a str>,
    pub cwd: Option<&'a std::path::Path>,
    pub profile: Option<&'a str>, // implementation-defined (e.g., iTerm2 profile)
    pub init_command: Option<&'a str>, // optional command to run when window is created
    pub focus: bool,
}

#[derive(Clone, Debug, Default)]
pub struct CommandOptions<'a> {
    pub cwd: Option<&'a std::path::Path>,
    pub env: Option<&'a [(&'a str, &'a str)]>,
}

#[derive(thiserror::Error, Debug)]
pub enum MuxError {
    #[error("multiplexer not available: {0}")]
    NotAvailable(&'static str),
    #[error("not found")]
    NotFound,
    #[error("command failed: {0}")]
    CommandFailed(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("configuration error: {0}")]
    ConfigurationError(String),
    #[error("other: {0}")]
    Other(String),
}

/// Core multiplexer trait that all implementations must provide
pub trait Multiplexer {
    /// Implementation identifier (e.g., "tmux", "wezterm", "kitty", "iterm2", "vim", "emacs").
    fn id(&self) -> &'static str;

    /// Check whether the implementation is available and usable on this system.
    fn is_available(&self) -> bool;

    /// Open a new top-level window/tab. Returns its WindowId.
    fn open_window(&self, opts: &WindowOptions) -> Result<WindowId, MuxError>;

    /// Split the given target pane (or initial window) and return the new PaneId.
    /// If window is None, splits the active/current window.
    fn split_pane(
        &self,
        window: Option<&WindowId>,
        target: Option<&PaneId>,
        dir: SplitDirection,
        percent: Option<u8>,
        opts: &CommandOptions,
        initial_cmd: Option<&str>,
    ) -> Result<PaneId, MuxError>;

    /// Run a command in an existing pane.
    fn run_command(&self, pane: &PaneId, cmd: &str, opts: &CommandOptions) -> Result<(), MuxError>;

    /// Send literal text to a pane (including newlines). Optional to implement; may return NotAvailable.
    fn send_text(&self, pane: &PaneId, text: &str) -> Result<(), MuxError> {
        let _ = (pane, text);
        Err(MuxError::NotAvailable(self.id()))
    }

    /// Focus a window or pane.
    fn focus_window(&self, window: &WindowId) -> Result<(), MuxError>;
    fn focus_pane(&self, pane: &PaneId) -> Result<(), MuxError>;

    /// Discover windows (best effort). Implementations may filter by title substring.
    fn list_windows(&self, title_substr: Option<&str>) -> Result<Vec<WindowId>, MuxError>;

    /// Discover panes within a window (best effort).
    fn list_panes(&self, window: &WindowId) -> Result<Vec<PaneId>, MuxError>;

    /// Get the current pane ID that this process is running in (best effort).
    /// Returns None if not in a multiplexer or if detection fails.
    fn current_pane(&self) -> Result<Option<PaneId>, MuxError> {
        let _ = self;
        Ok(None)
    }

    /// Get the current window ID that this process is running in (best effort).
    /// Returns None if not in a multiplexer or if detection fails.
    fn current_window(&self) -> Result<Option<WindowId>, MuxError> {
        let _ = self;
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_split_direction_debug() {
        assert_eq!(format!("{:?}", SplitDirection::Horizontal), "Horizontal");
        assert_eq!(format!("{:?}", SplitDirection::Vertical), "Vertical");
    }

    #[test]
    fn test_mux_error_display() {
        let err = MuxError::NotAvailable("tmux");
        assert_eq!(err.to_string(), "multiplexer not available: tmux");

        let err = MuxError::NotFound;
        assert_eq!(err.to_string(), "not found");

        let err = MuxError::CommandFailed("command failed".to_string());
        assert_eq!(err.to_string(), "command failed: command failed");
    }
}
