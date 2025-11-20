// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Tilix multiplexer implementation
//!
//! Tilix is a Linux tiling terminal emulator that supports tabs and splits
//! through command-line actions and session files.
//!
//! ## Capabilities
//!
//! - Tabs/workspaces: Yes (sessions and windows)
//! - Horizontal/vertical splits: Yes (via `session-add-right`, `session-add-down` actions)
//! - Addressability: Limited - via session layout definitions; runtime targeting via D-Bus/window focus is limited
//! - Start commands per pane: Yes, via `--command` and `--working-directory` or session layouts
//! - Focus/activate pane: Limited - window manager focus only
//! - Send keys: Not supported natively
//! - Startup layout: Command-line actions or saved session layouts
//!
//! ## References
//!
//! - Tilix documentation: https://gnunn1.github.io/tilix-web/manual/

use std::process::Command;
use tracing::{debug, error, info, instrument, warn};

use ah_mux_core::{
    CommandOptions, Multiplexer, MuxError, PaneId, SplitDirection, WindowId, WindowOptions,
};

/// Tilix multiplexer implementation
#[derive(Debug)]
pub struct TilixMultiplexer;

impl TilixMultiplexer {
    /// Create a new Tilix multiplexer instance
    #[instrument]
    pub fn new() -> Result<Self, MuxError> {
        debug!("Creating new Tilix multiplexer");
        if !Self::is_available() {
            error!("Tilix is not available");
            return Err(MuxError::NotAvailable("Tilix"));
        }
        info!("Tilix multiplexer created successfully");
        Ok(Self)
    }

    /// Check if tilix is available
    #[instrument]
    pub fn is_available() -> bool {
        debug!("Checking Tilix availability");
        let available = Command::new("tilix")
            .arg("--version")
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false);
        debug!("Tilix availability: {}", available);
        available
    }

    /// Get multiplexer identifier
    pub fn id() -> &'static str {
        "tilix"
    }

    /// Wrap a command with PATH environment variable for execution in bash
    ///
    /// Tilix requires explicit PATH propagation when executing commands via --command flag.
    /// This helper ensures the command runs with the current PATH environment.
    ///
    /// # Arguments
    ///
    /// * `cmd` - The command to wrap
    ///
    /// # Returns
    ///
    /// A bash command string with PATH environment properly set
    #[instrument]
    fn wrap_command_with_path(cmd: &str) -> String {
        let path = std::env::var("PATH").unwrap_or_default();
        debug!("Wrapping command with PATH: {}", path);
        format!("env PATH={} bash -c '{}'", path, cmd)
    }

    /// Run a tilix command with the given arguments
    #[instrument(skip(args))]
    fn run_tilix_command(&self, args: &[&str]) -> Result<String, MuxError> {
        // Sanitize args for logging by replacing PATH values with $PATH
        let sanitized_args: Vec<String> = args
            .iter()
            .map(|&arg| {
                if arg.starts_with("env PATH=") {
                    // Replace the actual PATH value with $PATH placeholder
                    let after_equals = arg.find('=').map(|i| &arg[i + 1..]).unwrap_or("");
                    if let Some(bash_pos) = after_equals.find(" bash ") {
                        format!("env PATH=$PATH {}", &after_equals[bash_pos..])
                    } else {
                        "env PATH=$PATH bash -c '...'".to_string()
                    }
                } else {
                    arg.to_string()
                }
            })
            .collect();
        debug!("Running tilix command with args: {:?}", sanitized_args);

        let output = Command::new("tilix").args(args).output().map_err(|e| {
            error!("Failed to execute tilix: {}", e);
            MuxError::Other(format!("Failed to execute tilix: {}", e))
        })?;

        if output.status.success() {
            let result = String::from_utf8_lossy(&output.stdout).trim().to_string();
            debug!("tilix command completed successfully");
            Ok(result)
        } else {
            let error_msg = format!(
                "tilix command failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            error!("{}", error_msg);
            Err(MuxError::CommandFailed(error_msg))
        }
    }
}

impl Multiplexer for TilixMultiplexer {
    fn id(&self) -> &'static str {
        Self::id()
    }

    #[instrument]
    fn is_available(&self) -> bool {
        Self::is_available()
    }

    #[instrument(skip(opts))]
    fn open_window(&self, opts: &WindowOptions) -> Result<WindowId, MuxError> {
        debug!(
            "Opening new Tilix window with options: title={:?}, cwd={:?}, init_command={:?}",
            opts.title, opts.cwd, opts.init_command
        );

        let mut args = vec![];

        // Add title if specified
        if let Some(title) = opts.title {
            debug!("Setting window title: {}", title);
            args.extend_from_slice(&["--title".to_string(), title.to_string()]);
        }

        // Add working directory if specified
        if let Some(cwd) = opts.cwd {
            let cwd_str = cwd.to_string_lossy().to_string();
            debug!("Setting working directory: {}", cwd_str);
            args.extend_from_slice(&["--working-directory".to_string(), cwd_str]);
        }

        // Add command if specified in init_command
        if let Some(init_cmd) = opts.init_command {
            debug!("Setting initial command: {}", init_cmd);
            let custom_command = Self::wrap_command_with_path(init_cmd);
            args.extend_from_slice(&["--command".to_string(), custom_command]);
        }

        args.push("--action".to_string());
        args.push("app-new-session".to_string());

        // Convert to &str for the command
        let args_str: Vec<&str> = args.iter().map(|s| s.as_str()).collect();

        // Run the command - tilix doesn't return window IDs directly
        // We'll use a generated ID based on title or timestamp
        let window_id = if let Some(title) = opts.title {
            let id = format!("tilix:{}", title);
            debug!("Generated window ID from title: {}", id);
            id
        } else {
            let id = format!("tilix:{}", std::process::id());
            debug!("Generated window ID from process ID: {}", id);
            id
        };

        self.run_tilix_command(&args_str)?;

        info!("Successfully opened Tilix window: {}", window_id);
        Ok(window_id)
    }

    #[instrument(skip(opts, initial_cmd))]
    fn split_pane(
        &self,
        _window: Option<&WindowId>,
        _target: Option<&PaneId>,
        dir: SplitDirection,
        _percent: Option<u8>,
        opts: &CommandOptions,
        initial_cmd: Option<&str>,
    ) -> Result<PaneId, MuxError> {
        debug!(
            "Splitting pane in direction: {:?}, with initial_cmd: {:?}, cwd: {:?}",
            dir, initial_cmd, opts.cwd
        );

        // Tilix uses actions to split panes within the current session
        // session-add-right: splits the active terminal horizontally (side by side)
        // session-add-down: splits the active terminal vertically (top/bottom)
        let action = match dir {
            SplitDirection::Vertical => "session-add-down",
            SplitDirection::Horizontal => "session-add-right",
        };

        debug!("Using Tilix action: {}", action);

        let mut args = vec!["--action".to_string(), action.to_string()];

        // Add working directory if specified
        if let Some(cwd) = opts.cwd {
            let cwd_str = cwd.to_string_lossy().to_string();
            debug!("Setting working directory for split: {}", cwd_str);
            args.extend_from_slice(&["--working-directory".to_string(), cwd_str]);
        }

        // Add command if specified
        if let Some(cmd) = initial_cmd {
            debug!("Setting initial command for split: {}", cmd);
            let custom_command = Self::wrap_command_with_path(cmd);
            args.extend_from_slice(&["--command".to_string(), custom_command]);
        }

        // Convert to &str for the command
        let args_str: Vec<&str> = args.iter().map(|s| s.as_str()).collect();

        self.run_tilix_command(&args_str)?;

        // Tilix doesn't return pane IDs, so we'll generate one based on the direction and timestamp
        let pane_id = format!(
            "tilix:pane:{}:{}",
            match dir {
                SplitDirection::Horizontal => "h",
                SplitDirection::Vertical => "v",
            },
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis()
        );

        info!("Successfully split pane ({}): {}", action, pane_id);
        Ok(pane_id)
    }

    #[instrument(skip(opts))]
    fn run_command(&self, pane: &PaneId, cmd: &str, opts: &CommandOptions) -> Result<(), MuxError> {
        debug!("Attempting to run command in pane {}: {}", pane, cmd);
        debug!("Command options: cwd={:?}, env={:?}", opts.cwd, opts.env);

        warn!(
            "Run command not supported for Tilix (pane: {}, cmd: {}): \
             Tilix does not support running commands in existing panes. \
             Commands must be specified when creating the pane.",
            pane, cmd
        );

        // Tilix doesn't have direct send-text or command execution capability for existing panes.
        // Commands must be specified at pane creation time via --command parameter.
        // See: https://gnunn1.github.io/tilix-web/manual/
        Err(MuxError::NotAvailable(
            "Tilix does not support running commands in existing panes - use initial_cmd in split_pane instead",
        ))
    }

    #[instrument]
    fn send_text(&self, pane: &PaneId, text: &str) -> Result<(), MuxError> {
        debug!(
            "Attempting to send text to pane {}: {} bytes",
            pane,
            text.len()
        );

        warn!(
            "Send text not supported for Tilix (pane: {}): \
             Tilix does not have native send-keys capability. \
             Interactive input requires external tools like xdotool or using non-interactive program flags.",
            pane
        );

        // Tilix doesn't have a direct send-text capability like tmux's send-keys.
        // This would require external tools like xdotool, ydotool, or similar input automation.
        // As noted in the spec: "Not supported natively; rely on the program's non-interactive flags."
        // See: https://gnunn1.github.io/tilix-web/manual/
        Err(MuxError::NotAvailable(
            "Tilix does not support sending text to panes - use non-interactive program flags",
        ))
    }

    #[instrument]
    fn focus_window(&self, window: &WindowId) -> Result<(), MuxError> {
        debug!("Attempting to focus window: {}", window);

        warn!(
            "Focus window not fully supported for Tilix (window: {}): \
             Tilix does not provide robust CLI for window focusing. \
             Window focus is typically handled by the window manager. \
             Consider using wmctrl or similar tools with window title matching.",
            window
        );

        // Tilix doesn't have direct window focusing via CLI.
        // Window focus is typically handled by the window manager (X11/Wayland).
        // External tools like wmctrl (X11) or window manager-specific commands would be needed.
        // As noted in the spec: "Use window titles and the window manager; Tilix itself does not
        // provide a robust CLI for 'focus by title'."
        // See: https://gnunn1.github.io/tilix-web/manual/
        Err(MuxError::NotAvailable(
            "Tilix does not support programmatic window focusing - use window manager tools like wmctrl",
        ))
    }

    #[instrument]
    fn focus_pane(&self, pane: &PaneId) -> Result<(), MuxError> {
        debug!("Attempting to focus pane: {}", pane);

        warn!(
            "Focus pane not supported for Tilix (pane: {}): \
             Tilix has move-focus actions but lacks addressable pane IDs. \
             Pane focus requires keyboard shortcuts or manual interaction.",
            pane
        );

        // Tilix has focus navigation actions (e.g., session-focus-up, session-focus-down),
        // but these are relative movements, not absolute pane targeting.
        // There's no way to specify "focus pane with ID X" via CLI.
        // See: https://gnunn1.github.io/tilix-web/manual/
        Err(MuxError::NotAvailable(
            "Tilix does not support programmatic pane focusing by ID - use keyboard navigation",
        ))
    }

    #[instrument]
    fn list_windows(&self, title_substr: Option<&str>) -> Result<Vec<WindowId>, MuxError> {
        debug!(
            "Attempting to list windows with title filter: {:?}",
            title_substr
        );

        warn!(
            "Window listing not supported for Tilix (filter: {:?}): \
             Tilix CLI does not provide window enumeration. \
             External tools like wmctrl or D-Bus queries may be needed.",
            title_substr
        );

        // Tilix doesn't provide a way to list windows programmatically via CLI.
        // This is a limitation of its command-line interface.
        // D-Bus interface may provide some capabilities, but it's not part of the standard CLI.
        // External tools like wmctrl can list X11 windows by WM_CLASS or title.
        // See: https://gnunn1.github.io/tilix-web/manual/
        Err(MuxError::NotAvailable(
            "Tilix does not support listing windows via CLI - consider D-Bus or wmctrl",
        ))
    }

    #[instrument]
    fn list_panes(&self, window: &WindowId) -> Result<Vec<PaneId>, MuxError> {
        debug!("Attempting to list panes for window: {}", window);

        warn!(
            "Pane listing not supported for Tilix (window: {}): \
             Tilix does not provide pane enumeration via CLI. \
             Session layout is managed internally without exposing pane IDs.",
            window
        );

        // Tilix doesn't provide pane enumeration via its CLI.
        // The internal session structure is not exposed programmatically.
        // D-Bus interface might provide limited capabilities but not standard pane enumeration.
        // See: https://gnunn1.github.io/tilix-web/manual/
        Err(MuxError::NotAvailable(
            "Tilix does not support listing panes via CLI",
        ))
    }
}
