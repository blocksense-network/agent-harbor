// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Tilix multiplexer implementation
//!
//! Tilix is a Linux tiling terminal emulator that supports tabs and splits
//! through command-line actions and session files.

use std::process::Command;

use ah_mux_core::{
    CommandOptions, Multiplexer, MuxError, PaneId, SplitDirection, WindowId, WindowOptions,
};

/// Tilix multiplexer implementation
pub struct TilixMultiplexer;

impl TilixMultiplexer {
    /// Create a new Tilix multiplexer instance
    pub fn new() -> Result<Self, MuxError> {
        Ok(Self)
    }

    /// Check if tilix is available
    pub fn is_available() -> bool {
        Command::new("tilix")
            .arg("--version")
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    }

    /// Get multiplexer identifier
    pub fn id() -> &'static str {
        "tilix"
    }

    /// Run a tilix command with the given arguments
    fn run_tilix_command(&self, args: &[&str]) -> Result<String, MuxError> {
        let output = Command::new("tilix")
            .args(args)
            .output()
            .map_err(|e| MuxError::Other(format!("Failed to execute tilix: {}", e)))?;

        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
        } else {
            let error_msg = format!(
                "tilix command failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            Err(MuxError::CommandFailed(error_msg))
        }
    }
}

impl Multiplexer for TilixMultiplexer {
    fn id(&self) -> &'static str {
        Self::id()
    }

    fn is_available(&self) -> bool {
        Self::is_available()
    }

    fn open_window(&self, opts: &WindowOptions) -> Result<WindowId, MuxError> {
        let mut args = vec!["--new-window".to_string()];

        // Add title if specified
        if let Some(title) = opts.title {
            args.extend_from_slice(&["--title".to_string(), title.to_string()]);
        }

        // Add working directory if specified
        if let Some(cwd) = opts.cwd {
            args.extend_from_slice(&[
                "--working-directory".to_string(),
                cwd.to_string_lossy().to_string(),
            ]);
        }

        // Add command if specified in profile
        if let Some(profile) = opts.profile {
            args.extend_from_slice(&["--command".to_string(), profile.to_string()]);
        }

        // Convert to &str for the command
        let args_str: Vec<&str> = args.iter().map(|s| s.as_str()).collect();

        // Run the command - tilix doesn't return window IDs directly
        // We'll use a generated ID based on title or process ID
        let window_id = if let Some(title) = opts.title {
            format!("tilix:{}", title)
        } else {
            format!("tilix:{}", std::process::id())
        };

        self.run_tilix_command(&args_str)?;

        Ok(window_id)
    }

    fn split_pane(
        &self,
        _window: &WindowId,
        _target: Option<&PaneId>,
        dir: SplitDirection,
        _percent: Option<u8>,
        _opts: &CommandOptions,
        _initial_cmd: Option<&str>,
    ) -> Result<PaneId, MuxError> {
        // Tilix uses actions to split panes
        let action = match dir {
            SplitDirection::Vertical => "app-new-session-right",
            SplitDirection::Horizontal => "app-new-session-down",
        };

        self.run_tilix_command(&["--action", action])?;
        // Tilix doesn't return pane IDs, so we'll generate one
        Ok("tilix:pane:1".to_string())
    }

    fn run_command(
        &self,
        _pane: &PaneId,
        _cmd: &str,
        _opts: &CommandOptions,
    ) -> Result<(), MuxError> {
        // Tilix doesn't have direct send-text capability
        // This would require external tools or creating a new pane with the command
        Err(MuxError::NotAvailable(
            "Tilix does not support running commands in existing panes",
        ))
    }

    fn send_text(&self, _pane: &PaneId, _text: &str) -> Result<(), MuxError> {
        // Tilix doesn't have a direct send-text capability
        // This would require external tools like xdotool or similar
        Err(MuxError::NotAvailable(
            "Tilix does not support sending text to panes",
        ))
    }

    fn focus_window(&self, _window: &WindowId) -> Result<(), MuxError> {
        // Tilix doesn't have direct window focusing via CLI
        // Window focus is typically handled by the window manager
        // We could potentially use window title matching with wmctrl or similar
        Err(MuxError::NotAvailable(
            "Tilix does not support programmatic window focusing",
        ))
    }

    fn focus_pane(&self, _pane: &PaneId) -> Result<(), MuxError> {
        // Tilix uses move-focus actions, but we don't know which pane is which
        Err(MuxError::NotAvailable(
            "Tilix does not support programmatic pane focusing",
        ))
    }

    fn list_windows(&self, _title_substr: Option<&str>) -> Result<Vec<WindowId>, MuxError> {
        // Tilix doesn't provide a way to list windows programmatically
        // This is a limitation of its CLI interface
        Err(MuxError::NotAvailable(
            "Tilix does not support listing windows",
        ))
    }

    fn list_panes(&self, _window: &WindowId) -> Result<Vec<PaneId>, MuxError> {
        // Tilix doesn't provide pane enumeration
        Err(MuxError::NotAvailable(
            "Tilix does not support listing panes",
        ))
    }
}
