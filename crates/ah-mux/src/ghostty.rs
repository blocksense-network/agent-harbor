//! Ghostty multiplexer implementation
//!
//! Ghostty is a fast, native macOS terminal emulator with evolving CLI support.
//! This implementation is based on the draft specification and may need updates
//! as Ghostty's automation capabilities mature.

use std::process::Command;

use ah_mux_core::{Multiplexer, WindowId, PaneId, WindowOptions, CommandOptions, SplitDirection, MuxError};

/// Ghostty multiplexer implementation
pub struct GhosttyMultiplexer;

impl GhosttyMultiplexer {
    /// Create a new Ghostty multiplexer instance
    pub fn new() -> Result<Self, MuxError> {
        Ok(Self)
    }

    /// Check if ghostty is available
    pub fn is_available() -> bool {
        Command::new("ghostty")
            .arg("--version")
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    }

    /// Get multiplexer identifier
    pub fn id() -> &'static str {
        "ghostty"
    }

    /// Run a ghostty command with the given arguments
    fn run_ghostty_command(&self, args: &[&str]) -> Result<String, MuxError> {
        let output = Command::new("ghostty")
            .args(args)
            .output()
            .map_err(|e| MuxError::Other(format!("Failed to execute ghostty: {}", e)))?;

        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
        } else {
            let error_msg = format!(
                "ghostty command failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            Err(MuxError::CommandFailed(error_msg))
        }
    }
}

impl Multiplexer for GhosttyMultiplexer {
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
            args.extend_from_slice(&["--working-directory".to_string(), cwd.to_string_lossy().to_string()]);
        }

        // Add command if specified in profile
        if let Some(profile) = opts.profile {
            args.extend_from_slice(&["--command".to_string(), profile.to_string()]);
        }

        // Convert to &str for the command
        let args_str: Vec<&str> = args.iter().map(|s| s.as_str()).collect();

        // Generate window ID
        let window_id = if let Some(title) = opts.title {
            format!("ghostty:{}", title)
        } else {
            format!("ghostty:{}", std::process::id())
        };

        self.run_ghostty_command(&args_str)?;

        Ok(window_id)
    }

    fn split_pane(&self, _window: &WindowId, _target: Option<&PaneId>, _dir: SplitDirection, _percent: Option<u8>, _opts: &CommandOptions, _initial_cmd: Option<&str>) -> Result<PaneId, MuxError> {
        // Ghostty's CLI is evolving - split functionality may be added in future versions
        // For now, this is not available
        Err(MuxError::NotAvailable("Ghostty split-pane functionality is not yet available in CLI"))
    }

    fn run_command(&self, _pane: &PaneId, _cmd: &str, _opts: &CommandOptions) -> Result<(), MuxError> {
        // Ghostty's CLI is still evolving
        // Commands are typically passed during window creation
        Err(MuxError::NotAvailable("Ghostty programmatic command execution is not yet available"))
    }

    fn send_text(&self, _pane: &PaneId, _text: &str) -> Result<(), MuxError> {
        // Ghostty may support this via AppleScript/JXA in the future
        Err(MuxError::NotAvailable("Ghostty does not support sending text to panes yet"))
    }

    fn focus_window(&self, _window: &WindowId) -> Result<(), MuxError> {
        // Could potentially use AppleScript/JXA for window focusing
        Err(MuxError::NotAvailable("Ghostty programmatic window focusing is not available"))
    }

    fn focus_pane(&self, _pane: &PaneId) -> Result<(), MuxError> {
        // Split functionality not available yet
        Err(MuxError::NotAvailable("Ghostty pane focusing is not available without splits"))
    }

    fn list_windows(&self, _title_substr: Option<&str>) -> Result<Vec<WindowId>, MuxError> {
        // CLI doesn't support listing yet
        Err(MuxError::NotAvailable("Ghostty does not support listing windows"))
    }

    fn list_panes(&self, _window: &WindowId) -> Result<Vec<PaneId>, MuxError> {
        // No pane support yet
        Err(MuxError::NotAvailable("Ghostty does not support listing panes"))
    }
}
