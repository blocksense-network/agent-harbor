// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Windows Terminal multiplexer implementation
//!
//! Windows Terminal is Microsoft's modern terminal emulator that supports
//! tabs and panes through command-line arguments.

use std::process::Command;
use tracing::{debug, error, info, instrument, warn};

use ah_mux_core::{
    CommandOptions, Multiplexer, MuxError, PaneId, SplitDirection, WindowId, WindowOptions,
};

/// Windows Terminal multiplexer implementation
#[derive(Debug)]
pub struct WindowsTerminalMultiplexer;

impl WindowsTerminalMultiplexer {
    /// Create a new Windows Terminal multiplexer instance
    #[instrument]
    pub fn new() -> Result<Self, MuxError> {
        debug!("Creating new Windows Terminal multiplexer");
        if !Self::is_available() {
            error!("Windows Terminal is not available");
            return Err(MuxError::NotAvailable("Windows Terminal"));
        }
        info!("Windows Terminal multiplexer created successfully");
        Ok(Self)
    }

    /// Check if wt.exe is available
    #[instrument]
    pub fn is_available() -> bool {
        debug!("Checking Windows Terminal availability");
        let available = Command::new("wt")
            .arg("--version")
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false);
        debug!("Windows Terminal availability: {}", available);
        available
    }

    /// Get multiplexer identifier
    pub fn id() -> &'static str {
        "windows-terminal"
    }

    /// Run a wt command with the given arguments
    #[instrument(skip(args))]
    fn run_wt_command(&self, args: &[&str]) -> Result<String, MuxError> {
        debug!("Running wt command with args: {:?}", args);
        let output = Command::new("wt").args(args).output().map_err(|e| {
            error!("Failed to execute wt: {}", e);
            MuxError::Other(format!("Failed to execute wt: {}", e))
        })?;

        if output.status.success() {
            let result = String::from_utf8_lossy(&output.stdout).trim().to_string();
            debug!("wt command completed successfully");
            Ok(result)
        } else {
            let error_msg = format!(
                "wt command failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            error!("{}", error_msg);
            Err(MuxError::CommandFailed(error_msg))
        }
    }
}

impl Multiplexer for WindowsTerminalMultiplexer {
    fn id(&self) -> &'static str {
        Self::id()
    }

    #[instrument]
    fn is_available(&self) -> bool {
        Self::is_available()
    }

    #[instrument(skip(opts))]
    fn open_window(&self, opts: &WindowOptions) -> Result<WindowId, MuxError> {
        debug!("Opening new Windows Terminal window");
        let mut args = vec!["new-tab".to_string()];

        // Add title if specified
        if let Some(title) = opts.title {
            debug!("Setting window title: {}", title);
            args.extend_from_slice(&["--title".to_string(), title.to_string()]);
        }

        // Add working directory if specified
        if let Some(cwd) = opts.cwd {
            debug!("Setting working directory: {}", cwd.display());
            args.extend_from_slice(&[
                "--startingDirectory".to_string(),
                cwd.to_string_lossy().to_string(),
            ]);
        }

        // Add command if specified in profile
        if let Some(profile) = opts.profile {
            debug!("Setting profile command: {}", profile);
            args.extend_from_slice(&["--command".to_string(), profile.to_string()]);
        } else {
            // Default to PowerShell
            args.push("--command".to_string());
            args.push("powershell".to_string());
        }

        // Convert to &str for the command
        let args_str: Vec<&str> = args.iter().map(|s| s.as_str()).collect();

        // Windows Terminal doesn't return window IDs, so we'll generate one
        let window_id = if let Some(title) = opts.title {
            format!("wt:{}", title)
        } else {
            format!("wt:{}", std::process::id())
        };

        self.run_wt_command(&args_str)?;
        info!("Opened Windows Terminal window: {}", window_id);
        Ok(window_id)
    }

    #[instrument(skip(opts, initial_cmd))]
    fn split_pane(
        &self,
        _window: Option<&WindowId>,
        _target: Option<&PaneId>,
        dir: SplitDirection,
        percent: Option<u8>,
        opts: &CommandOptions,
        initial_cmd: Option<&str>,
    ) -> Result<PaneId, MuxError> {
        debug!("Splitting pane in direction: {:?}", dir);
        let mut args = vec!["split-pane".to_string()];

        // Add direction
        let dir_flag = match dir {
            SplitDirection::Vertical => "-H".to_string(),
            SplitDirection::Horizontal => "-V".to_string(),
            SplitDirection::Auto => "-V".to_string(), // Fall back to horizontal split for now
        };
        args.push(dir_flag);

        // Add size if specified
        if let Some(size) = percent {
            debug!("Setting pane size: {}%", size);
            args.extend_from_slice(&["-s".to_string(), format!("{}", size)]);
        }

        // Add working directory if specified
        if let Some(cwd) = &opts.cwd {
            debug!("Setting working directory: {}", cwd.display());
            args.extend_from_slice(&["-d".to_string(), cwd.to_string_lossy().to_string()]);
        }

        // Add command if specified
        if let Some(cmd) = initial_cmd {
            debug!("Running initial command in new pane: {}", cmd);
            args.push(cmd.to_string());
        } else {
            // Default to PowerShell
            args.push("powershell".to_string());
        }

        // Convert to &str for the command
        let args_str: Vec<&str> = args.iter().map(|s| s.as_str()).collect();

        self.run_wt_command(&args_str)?;

        // Windows Terminal doesn't return pane IDs, so we'll generate one
        let pane_id = "wt:pane:1".to_string();
        info!("Split pane created: {}", pane_id);
        Ok(pane_id)
    }

    #[instrument(skip(_opts))]
    fn run_command(
        &self,
        _pane: &PaneId,
        _cmd: &str,
        _opts: &CommandOptions,
    ) -> Result<(), MuxError> {
        warn!(
            "Run command not available for Windows Terminal: no support for running commands in existing panes"
        );
        // Windows Terminal doesn't have direct send-text capability
        // This would require external automation tools
        Err(MuxError::NotAvailable(
            "Windows Terminal does not support running commands in existing panes",
        ))
    }

    #[instrument]
    fn send_text(&self, _pane: &PaneId, _text: &str) -> Result<(), MuxError> {
        warn!("Send text not available for Windows Terminal: no direct send-text capability");
        // Windows Terminal doesn't have a direct send-text capability
        // This would require external automation tools
        Err(MuxError::NotAvailable(
            "Windows Terminal does not support sending text to panes",
        ))
    }

    #[instrument]
    fn focus_window(&self, _window: &WindowId) -> Result<(), MuxError> {
        warn!("Focus window not available for Windows Terminal: no programmatic window focusing");
        // Windows Terminal doesn't have direct window focusing via CLI
        // Window focus is handled by the OS/window manager
        Err(MuxError::NotAvailable(
            "Windows Terminal does not support programmatic window focusing",
        ))
    }

    #[instrument]
    fn focus_pane(&self, _pane: &PaneId) -> Result<(), MuxError> {
        warn!("Focus pane not available for Windows Terminal: no programmatic pane focusing");
        // Windows Terminal supports move-focus but we don't know pane relationships
        Err(MuxError::NotAvailable(
            "Windows Terminal does not support programmatic pane focusing",
        ))
    }

    #[instrument]
    fn list_windows(&self, _title_substr: Option<&str>) -> Result<Vec<WindowId>, MuxError> {
        warn!("Window listing not available for Windows Terminal: no CLI support");
        // Windows Terminal doesn't provide a way to list windows programmatically
        Err(MuxError::NotAvailable(
            "Windows Terminal does not support listing windows",
        ))
    }

    #[instrument]
    fn list_panes(&self, _window: &WindowId) -> Result<Vec<PaneId>, MuxError> {
        warn!("Pane listing not available for Windows Terminal: no pane enumeration support");
        // Windows Terminal doesn't provide pane enumeration
        Err(MuxError::NotAvailable(
            "Windows Terminal does not support listing panes",
        ))
    }
}
