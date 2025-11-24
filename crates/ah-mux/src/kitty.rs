// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! kitty multiplexer implementation
//!
//! Implements the Multiplexer trait for kitty using its remote control interface.
//! Based on the kitty integration guide in specs/Public/Terminal-Multiplexers/kitty.md
//!
//! ## Configuration Required
//!
//! Remote control must be enabled in `~/.config/kitty/kitty.conf`:
//!
//! ```conf
//! # Enable remote control (required for all kitty @ commands)
//! allow_remote_control yes
//!
//! # Set up UNIX socket for external control (scripts, other terminals)
//! # Kitty creates this socket file automatically when it starts
//! listen_on unix:/tmp/kitty-ah.sock
//!
//! # Enable layout splitting for more flexible window management
//! enabled_layouts splits
//! ```
//!
//! ## Usage
//!
//! - **From within Kitty:** Commands work without `--to` flag (uses stdio/environment)
//! - **From external scripts/terminals:** Must use socket path via `with_socket_path()` or
//!   set `KITTY_LISTEN_ON` environment variable
//!
//! ## Limitations
//!
//! - **Split sizes:** Kitty does not support custom split size percentages. Split sizes are
//!   determined automatically by kitty's layout algorithm. The `percent` parameter in
//!   `split_pane()` is ignored.
//!
//! See: <https://sw.kovidgoyal.net/kitty/remote-control/>

use ah_mux_core::*;
use std::process::{Command, Stdio};
use tracing::{debug, error, info, instrument, warn};

/// kitty multiplexer implementation
///
/// Provides remote control of kitty terminal emulator via `kitty @` commands.
/// Supports both internal control (from within kitty) and external control
/// (from other terminals/scripts via UNIX socket).
pub struct KittyMultiplexer {
    /// Socket path for remote control.
    ///
    /// - None: Uses stdio (for commands within kitty)
    /// - Some(path): Uses socket connection (for external control)
    ///
    /// Defaults to KITTY_LISTEN_ON environment variable if set
    socket_path: Option<String>,
}

impl Default for KittyMultiplexer {
    fn default() -> Self {
        Self {
            socket_path: std::env::var("KITTY_LISTEN_ON").ok(),
        }
    }
}

impl KittyMultiplexer {
    pub fn new() -> Result<Self, MuxError> {
        // Run configuration checks to ensure kitty is properly set up
        // This validates that kitty is installed, remote control is enabled,
        // and layout splitting is configured before creating the instance
        let kitty = Self::default();
        kitty.check_configuration()?;

        Ok(kitty)
    }

    pub fn with_socket_path(socket_path: String) -> Self {
        Self {
            socket_path: Some(socket_path),
        }
    }

    /// Run a kitty @ command and return its output
    ///
    /// Executes kitty remote control commands via `kitty @` interface.
    /// Uses socket connection if socket_path is set, otherwise uses stdio (for commands within kitty).
    ///
    /// See: https://sw.kovidgoyal.net/kitty/remote-control/
    #[instrument(skip(self), fields(component = "ah_mux", operation = "run_kitty_command", socket_path = ?self.socket_path, args = ?args))]
    fn run_kitty_command(&self, args: &[&str]) -> Result<String, MuxError> {
        debug!("Executing kitty @ command");
        // Use timeout to prevent hanging when remote control is disabled
        let cmd_args = self.build_kitty_args(args);
        let str_args: Vec<&str> = cmd_args.iter().map(|s| s.as_str()).collect();

        // Execute the kitty @ command
        let output = Command::new("timeout")
            .args(["30", "kitty"]) // 30 second timeout (longer for actual operations)
            .args(&str_args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .map_err(|e| {
                if e.kind() == std::io::ErrorKind::NotFound {
                    error!("kitty command not found");
                    MuxError::NotAvailable("kitty")
                } else {
                    error!(error = %e, "Failed to execute kitty command");
                    MuxError::CommandFailed(format!("Failed to execute kitty command: {}", e))
                }
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);

            // Check for specific error messages indicating remote control issues
            if stderr.contains("Remote control is disabled") {
                error!("kitty remote control is disabled");
                return Err(MuxError::CommandFailed(
                    "Remote control is disabled. Add 'allow_remote_control yes' to ~/.config/kitty/kitty.conf".to_string(),
                ));
            }

            if stderr.contains("Could not connect") || stderr.contains("no socket") {
                error!(stderr = %stderr, "Could not connect to kitty socket");
                return Err(MuxError::CommandFailed(format!(
                    "Could not connect to kitty socket. Ensure kitty is running with 'listen_on unix:/tmp/kitty-ah.sock' in kitty.conf. Error: {}",
                    stderr
                )));
            }

            error!(stderr = %stderr, "kitty @ command failed");
            return Err(MuxError::CommandFailed(format!(
                "kitty @ command failed: {}",
                stderr
            )));
        }

        let result = String::from_utf8_lossy(&output.stdout).to_string();
        debug!(output_length = %result.len(), "kitty @ command executed successfully");
        Ok(result)
    }

    /// Build kitty command arguments (helper method)
    fn build_kitty_args(&self, args: &[&str]) -> Vec<String> {
        let mut cmd_args = vec!["@".to_string()];

        // Add socket path if specified
        if let Some(socket) = &self.socket_path {
            // Ensure socket path is properly formatted with protocol
            let socket_path = if socket.starts_with("unix:") {
                socket.clone()
            } else {
                format!("unix:{}", socket)
            };
            cmd_args.extend_from_slice(&["--to".to_string(), socket_path]);
        }

        // Add the actual command arguments
        cmd_args.extend(args.iter().map(|s| s.to_string()));
        cmd_args
    }

    /// Get the window ID from kitty's launch output
    /// kitty @ launch returns the window ID
    fn parse_window_id_from_output(&self, output: &str) -> Result<String, MuxError> {
        // kitty @ launch returns just the window ID
        let window_id = output.trim();
        if window_id.is_empty() {
            return Err(MuxError::CommandFailed(
                "kitty @ launch returned empty window ID".to_string(),
            ));
        }
        Ok(window_id.to_string())
    }

    /// Get the pane ID from kitty's launch output
    /// For kitty, panes are also identified by window IDs since each pane is a separate window
    fn parse_pane_id_from_output(&self, output: &str) -> Result<String, MuxError> {
        // In kitty's model, each pane is a separate window, so pane ID is the same as window ID
        self.parse_window_id_from_output(output)
    }

    /// Get the currently focused tab window ID
    pub fn get_focused_tab_window_id(&self) -> Result<String, MuxError> {
        let output = self.run_kitty_command(&["ls"])?;

        // Parse JSON to find the focused window
        let json: serde_json::Value = serde_json::from_str(&output).map_err(|e| {
            MuxError::CommandFailed(format!("Failed to parse kitty @ ls JSON: {}", e))
        })?;

        // The structure is: array of OS windows -> tabs -> windows
        // We are interested of the focused tab
        if let Some(os_windows) = json.as_array() {
            for os_window in os_windows {
                if let Some(tabs) = os_window.get("tabs").and_then(|t| t.as_array()) {
                    for tab in tabs {
                        if tab.get("is_focused").and_then(|f| f.as_bool()).unwrap_or(false) {
                            if let Some(id) = tab.get("id").and_then(|i| i.as_u64()) {
                                println!("Found focused window: {}", id);
                                return Ok(id.to_string());
                            }
                        }
                    }
                }
            }
        }

        Err(MuxError::CommandFailed(
            "Could not find focused window in kitty @ ls output".to_string(),
        ))
    }
    /// Get the currently focused window ID
    pub fn get_focused_pane_window_id(&self) -> Result<String, MuxError> {
        let output = self.run_kitty_command(&["ls"])?;

        // Parse JSON to find the focused window
        let json: serde_json::Value = serde_json::from_str(&output).map_err(|e| {
            MuxError::CommandFailed(format!("Failed to parse kitty @ ls JSON: {}", e))
        })?;

        // The structure is: array of OS windows -> tabs -> windows (panes)
        // We are interested of the focused pane in the focused tab
        if let Some(os_windows) = json.as_array() {
            for os_window in os_windows {
                if let Some(tabs) = os_window.get("tabs").and_then(|t| t.as_array()) {
                    for tab in tabs {
                        if tab.get("is_focused").and_then(|f| f.as_bool()).unwrap_or(false) {
                            if let Some(windows) = tab.get("windows").and_then(|w| w.as_array()) {
                                for window in windows {
                                    if window
                                        .get("is_focused")
                                        .and_then(|f| f.as_bool())
                                        .unwrap_or(false)
                                    {
                                        if let Some(id) = window.get("id").and_then(|i| i.as_u64())
                                        {
                                            return Ok(id.to_string());
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        Err(MuxError::CommandFailed(
            "Could not find focused window in kitty @ ls output".to_string(),
        ))
    }

    /// Get the currently focused window title
    pub fn get_window_title(&self, window_id: String) -> Result<String, MuxError> {
        let output = self.run_kitty_command(&["ls"])?;

        // Parse JSON to find the focused window
        let json: serde_json::Value = serde_json::from_str(&output).map_err(|e| {
            MuxError::CommandFailed(format!("Failed to parse kitty @ ls JSON: {}", e))
        })?;

        // The structure is: array of OS windows -> tabs -> windows
        if let Some(os_windows) = json.as_array() {
            for os_window in os_windows {
                if let Some(tabs) = os_window.get("tabs").and_then(|t| t.as_array()) {
                    for tab in tabs {
                        if let Some(windows) = tab.get("windows").and_then(|w| w.as_array()) {
                            for window in windows {
                                if window.get("id").and_then(|f| f.as_u64()).unwrap_or(0)
                                    == window_id.parse::<u64>().unwrap_or(0)
                                {
                                    if let Some(title) =
                                        window.get("title").and_then(|i| i.as_str())
                                    {
                                        return Ok(title.to_string());
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        Err(MuxError::CommandFailed(
            "Could not find focused window in kitty @ ls output".to_string(),
        ))
    }

    /// Get detailed window information
    ///
    /// Returns a list of (window_id, title) tuples by parsing kitty @ ls JSON output.
    /// See: https://sw.kovidgoyal.net/kitty/remote-control/#kitty-ls
    pub fn list_windows_detailed(&self) -> Result<Vec<(String, String)>, MuxError> {
        // Get list of windows as JSON
        let output = self.run_kitty_command(&["ls"])?;

        // Parse JSON to extract window IDs and titles
        let json: serde_json::Value = serde_json::from_str(&output).map_err(|e| {
            MuxError::CommandFailed(format!("Failed to parse kitty @ ls JSON: {}", e))
        })?;

        // Expected structure: array of OS windows, each with tabs, each with windows
        let mut windows = Vec::new();

        if let Some(os_windows) = json.as_array() {
            for os_window in os_windows {
                if let Some(tabs) = os_window.get("tabs").and_then(|t| t.as_array()) {
                    for tab in tabs {
                        if let Some(window_list) = tab.get("windows").and_then(|w| w.as_array()) {
                            for window in window_list {
                                if let (Some(id), Some(title)) = (
                                    window.get("id").and_then(|i| i.as_u64()),
                                    window.get("title").and_then(|t| t.as_str()),
                                ) {
                                    windows.push((id.to_string(), title.to_string()));
                                }
                            }
                        }
                    }
                }
            }
        }

        Ok(windows)
    }

    /// Check if a window with the given ID exists
    pub fn window_exists(&self, window_id: &str) -> Result<bool, MuxError> {
        let windows = self.list_windows_detailed()?;
        Ok(windows.iter().any(|(id, _)| id == window_id))
    }

    /// Check if kitty remote control and layout splitting are properly configured
    ///
    /// This provides detailed feedback about configuration issues.
    /// Use this to diagnose why kitty multiplexer functionality isn't working.
    ///
    /// Instead of failing fast, this method collects all configuration problems
    /// and reports them together for a better user experience.
    pub fn check_configuration(&self) -> Result<(), MuxError> {
        let mut config_errors = Vec::new();

        // Check if kitty is installed
        let kitty_exists = self.check_kitty_installed()?;

        if !kitty_exists {
            warn!("Kitty is not installed or not in PATH");
            config_errors.push("Kitty is not installed or not in PATH.".to_string());
            // If kitty is not installed, no point checking further configuration
            return Err(MuxError::ConfigurationError(config_errors.join("\n")));
        }
        debug!("Kitty is installed and in PATH");

        // Check remote control configuration
        debug!("Checking kitty default configuration for remote control");
        let check_remote_control_from_config =
            self.check_remote_control_from_config().unwrap_or_else(|e| {
                debug!("Error checking remote control from config: {}", e);
                false
            });

        if !check_remote_control_from_config {
            warn!("Kitty remote control is not enabled.");

            config_errors.push(
                    "Remote control is disabled. Add 'allow_remote_control yes' to ~/.config/kitty/kitty.conf"
                        .to_string()
                );
        }

        // Check socket configuration
        let check_socket_from_config =
            self.check_listen_on_socket_from_config().unwrap_or_else(|e| {
                debug!("Error checking socket from config: {}", e);
                false
            });

        if !check_socket_from_config {
            warn!("Kitty listen_on socket is not set.");
            config_errors.push(
                "Socket listening is not configured. Add 'listen_on unix:/tmp/kitty-ah.sock' to ~/.config/kitty/kitty.conf"
                    .to_string()
            );
        }

        // Check layout splitting configuration
        let check_enabled_layouts_from_config =
            self.check_enable_layout_split_from_config().unwrap_or_else(|e| {
                debug!("Error checking layout splitting from config: {}", e);
                false
            });

        if !check_enabled_layouts_from_config {
            warn!("Kitty layout splitting is not enabled.");
            config_errors.push(
                "Layout splitting is disabled. Add 'enabled_layouts splits' to ~/.config/kitty/kitty.conf"
                    .to_string()
            );
        }

        // If we have any configuration errors, return them all together
        if !config_errors.is_empty() {
            let mut error_parts = vec![
                "Kitty configuration issues found:".to_string(),
                "".to_string(), // Empty line
            ];

            // Add numbered errors
            for (i, err) in config_errors.iter().enumerate() {
                error_parts.push(format!("{}. {}", i + 1, err));
            }

            error_parts.push("".to_string()); // Empty line
            error_parts.push("Then restart kitty.".to_string());

            let error_message = error_parts.join("\n");
            return Err(MuxError::ConfigurationError(error_message));
        }

        debug!("All kitty configuration checks passed");
        Ok(())
    }

    pub fn check_kitty_installed(&self) -> Result<bool, MuxError> {
        let status = Command::new("kitty")
            .arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        Ok(status)
    }

    /// Read the default kitty configuration file
    ///
    /// Reads the kitty configuration from `~/.config/kitty/kitty.conf`.
    /// This is useful for checking if remote control is properly configured.
    ///
    /// Returns the configuration file contents as a string.
    pub fn read_default_config(&self) -> Result<String, MuxError> {
        // Get the home directory
        let home_dir = std::env::var("HOME").map_err(|_| {
            MuxError::CommandFailed("HOME environment variable not set".to_string())
        })?;

        // Construct the path to kitty.conf
        let config_path =
            std::path::Path::new(&home_dir).join(".config").join("kitty").join("kitty.conf");

        debug!("Reading kitty config from {}", config_path.display());
        // Read the configuration file
        std::fs::read_to_string(&config_path).map_err(|e| {
            MuxError::CommandFailed(format!(
                "Failed to read kitty config at {}: {}",
                config_path.display(),
                e
            ))
        })
    }

    /// Check if remote control is enabled in the kitty config
    ///
    /// Parses the config file and checks for an active (non-commented) line
    /// containing "allow_remote_control yes".
    pub fn check_remote_control_from_config(&self) -> Result<bool, MuxError> {
        let config = self.read_default_config()?;

        // Check each line for the setting, ignoring comments
        for line in config.lines() {
            let trimmed = line.trim();
            // Skip empty lines and comments
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            // Check if this line contains the setting we're looking for
            if trimmed.contains("allow_remote_control") && trimmed.contains("yes") {
                debug!("allow_remote_control is enabled in default config");
                return Ok(true);
            }
        }

        warn!("allow_remote_control is not enabled in default config");
        Ok(false)
    }

    /// Check if listen_on unix:/tmp/kitty-ah.sock is set in the kitty config
    ///
    /// Parses the config file and checks for an active (non-commented) line
    /// containing "listen_on unix:/tmp/kitty-ah.sock".
    pub fn check_listen_on_socket_from_config(&self) -> Result<bool, MuxError> {
        let config = self.read_default_config()?;

        // Check each line for the setting, ignoring comments
        for line in config.lines() {
            let trimmed = line.trim();

            // Skip empty lines and comments
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            // Check if this line contains the setting we're looking for
            if trimmed.contains("listen_on") && trimmed.contains("unix:/tmp/kitty-ah.sock") {
                debug!("listen_on unix:/tmp/kitty-ah.sock is set in default config");
                return Ok(true);
            }
        }

        warn!("listen_on unix:/tmp/kitty-ah.sock is not set in default config");
        Ok(false)
    }

    /// Check if layout split is enabled in the kitty config
    ///
    /// Parses the config file and checks for an active (non-commented) line
    /// containing "enabled_layouts splits".
    pub fn check_enable_layout_split_from_config(&self) -> Result<bool, MuxError> {
        let config = self.read_default_config()?;

        // Check each line for the setting, ignoring comments
        for line in config.lines() {
            let trimmed = line.trim();
            // Skip empty lines and comments
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            // Check if this line contains the setting we're looking for
            if trimmed.contains("enabled_layouts") && trimmed.contains("splits") {
                debug!("enabled_layouts is enabled in default config");
                return Ok(true);
            }
        }
        warn!("enabled_layouts is not enabled in default config");
        Ok(false)
    }
}

impl Multiplexer for KittyMultiplexer {
    fn id(&self) -> &'static str {
        "kitty"
    }

    #[instrument(skip(self), fields(component = "ah_mux", operation = "is_available"))]
    fn is_available(&self) -> bool {
        debug!("Checking kitty availability");

        // Check if kitty command exists
        let kitty_exists = self.check_kitty_installed();
        match kitty_exists {
            Ok(true) => {
                debug!("kitty command found");
            }
            Ok(false) => {
                debug!("kitty command not found");
                return false;
            }
            Err(e) => {
                warn!("Failed to check kitty installation: {}", e);
                return false;
            }
        }

        let check_config = self.check_configuration();
        if let Err(e) = check_config {
            warn!("Kitty configuration check failed: {}", e);
            return false;
        }

        true
    }

    #[instrument(skip(self), fields(component = "ah_mux", operation = "open_window", title = ?opts.title, focus = %opts.focus))]
    fn open_window(&self, opts: &WindowOptions) -> Result<WindowId, MuxError> {
        info!("Opening new kitty window");
        // Create a new window/tab with the specified options
        let mut args = vec![
            "launch".to_string(),
            "--type".to_string(),
            "tab".to_string(),
        ];

        // Add title if specified
        if let Some(title) = opts.title {
            args.extend_from_slice(&["--title".to_string(), title.to_string()]);
        }

        // Add working directory if specified
        if let Some(cwd) = opts.cwd {
            args.extend_from_slice(&["--cwd".to_string(), cwd.to_string_lossy().to_string()]);
        }

        // Convert to slice of &str for the command
        let args_str: Vec<&str> = args.iter().map(|s| s.as_str()).collect();

        // Run the command and capture the window ID
        let output = self.run_kitty_command(&args_str)?;
        let window_id = self.parse_window_id_from_output(&output)?;

        // Focus the window if requested
        if opts.focus {
            self.focus_window(&window_id)?;
        }

        Ok(window_id)
    }

    #[instrument(skip(self, opts), fields(component = "ah_mux", operation = "split_pane", direction = ?dir, percent = ?percent, has_initial_cmd = %initial_cmd.is_some()))]
    fn split_pane(
        &self,
        window: Option<&WindowId>,
        target: Option<&PaneId>,
        dir: SplitDirection,
        percent: Option<u8>,
        opts: &CommandOptions,
        initial_cmd: Option<&str>,
    ) -> Result<PaneId, MuxError> {
        info!("Splitting kitty pane");

        let mut args = vec![
            "launch".to_string(),
            "--type".to_string(),
            "window".to_string(),
        ];

        let location = match dir {
            SplitDirection::Horizontal => "hsplit".to_string(),
            SplitDirection::Vertical => "vsplit".to_string(),
        };
        debug!(direction = ?dir, location = %location, "Using explicit split direction");

        args.extend_from_slice(&["--location".to_string(), location]);

        // Note: kitty does not support --size option for splits
        // Split sizes are determined automatically by kitty's layout algorithm
        // The percent parameter is ignored for kitty
        if cfg!(debug_assertions) && percent.is_some() {
            warn!("kitty does not support custom split sizes, ignoring percent parameter");
            debug!("kitty does not support custom split sizes, ignoring percent parameter");
        }

        // Target the specific pane/window if specified, otherwise operate on current window
        if let Some(target_pane) = target {
            args.extend_from_slice(&["--match".to_string(), format!("id:{}", target_pane)]);
        } else if let Some(window_id) = window {
            args.extend_from_slice(&["--match".to_string(), format!("id:{}", window_id)]);
        }
        // If neither target nor window is specified, kitty will operate on the current window

        // Add working directory if specified
        if let Some(cwd) = opts.cwd {
            args.extend_from_slice(&["--cwd".to_string(), cwd.to_string_lossy().to_string()]);
        }

        // Add environment variables if specified
        // See: https://sw.kovidgoyal.net/kitty/launch/
        if let Some(env_vars) = opts.env {
            for (key, value) in env_vars {
                args.extend_from_slice(&["--env".to_string(), format!("{}={}", key, value)]);
            }
        }

        // Add initial command if specified (after -- separator as documented)
        // See: https://sw.kovidgoyal.net/kitty/launch/
        // Note: We wrap the command in bash -c to handle complex command lines with arguments
        if let Some(cmd) = initial_cmd {
            args.push("--".to_string());
            args.push("bash".to_string());
            args.push("-c".to_string());
            args.push(cmd.to_string());
        }

        // Convert to slice of &str for the command
        let args_str: Vec<&str> = args.iter().map(|s| s.as_str()).collect();

        // Run the command and capture the pane ID
        debug!(args = ?args_str, "Running kitty launch command for pane split");
        let output = self.run_kitty_command(&args_str)?;
        let pane_id = self.parse_pane_id_from_output(&output)?;

        info!(pane_id = %pane_id, "kitty pane split successfully");
        Ok(pane_id)
    }

    fn run_command(&self, pane: &PaneId, cmd: &str, opts: &CommandOptions) -> Result<(), MuxError> {
        debug!(?pane, ?cmd, "run_command invoked");

        // For other panes, send the command text normally
        // Build the full command with working directory if specified
        let full_cmd = if let Some(cwd) = opts.cwd {
            format!("cd {} && {}", cwd.display(), cmd)
        } else {
            cmd.to_string()
        };

        let match_arg = format!("id:{}", pane);
        let text_arg = format!("{}\r", full_cmd); // \r is carriage return (Enter key)

        std::thread::sleep(std::time::Duration::from_millis(100));

        self.run_kitty_command(&["send-text", "--match", &match_arg, &text_arg])?;
        Ok(())
    }

    fn send_text(&self, pane: &PaneId, text: &str) -> Result<(), MuxError> {
        // Send literal text to the pane
        let match_arg = format!("id:{}", pane);
        self.run_kitty_command(&["send-text", "--match", &match_arg, "--no-newline", text])?;
        Ok(())
    }

    fn focus_window(&self, window: &WindowId) -> Result<(), MuxError> {
        let match_arg = format!("id:{}", window);
        self.run_kitty_command(&["focus-window", "--match", &match_arg])?;
        Ok(())
    }

    fn focus_pane(&self, pane: &PaneId) -> Result<(), MuxError> {
        // In kitty, focusing a pane is the same as focusing its window
        self.focus_window(pane)
    }

    fn current_window(&self) -> Result<Option<WindowId>, MuxError> {
        // We are interested in the focused pane in the focused tab
        match self.get_focused_pane_window_id() {
            Ok(window_id) if !window_id.is_empty() => {
                debug!(?window_id, "current_window: got from get_focused_window_id");
                return Ok(Some(window_id));
            }
            Err(e) => {
                debug!(?e, "current_window: get_focused_window_id failed");
            }
            _ => {
                debug!("current_window: get_focused_window_id returned empty");
            }
        }

        // Fallback: Try the KITTY_WINDOW_ID environment variable
        // This is set by kitty for all processes running inside it
        if let Ok(window_id) = std::env::var("KITTY_WINDOW_ID") {
            if !window_id.is_empty() {
                debug!(?window_id, "current_window: got from KITTY_WINDOW_ID env");
                return Ok(Some(window_id));
            }
        }

        debug!("current_window: no window found");
        Ok(None)
    }

    fn current_pane(&self) -> Result<Option<PaneId>, MuxError> {
        // In kitty, each pane is effectively a window, so the focused window ID
        // is the same as the focused pane ID
        self.current_window()
    }

    fn list_windows(&self, title_substr: Option<&str>) -> Result<Vec<WindowId>, MuxError> {
        // Get detailed window info via list_windows_detailed which parses JSON from kitty @ ls
        let windows = self.list_windows_detailed()?;

        // Filter by title substring if specified
        let filtered: Vec<WindowId> = if let Some(substr) = title_substr {
            windows
                .into_iter()
                .filter(|(_, title)| title.contains(substr))
                .map(|(id, _)| id)
                .collect()
        } else {
            windows.into_iter().map(|(id, _)| id).collect()
        };

        Ok(filtered)
    }

    fn list_panes(&self, _window: &WindowId) -> Result<Vec<PaneId>, MuxError> {
        // In kitty, each "pane" is actually a separate window, but we can treat
        // all windows as panes for compatibility. For now, return all windows.
        // This is a simplification - in a real implementation, we might need to
        // track which windows belong to which "logical" panes.
        self.list_windows(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use std::sync::Mutex;
    use std::thread;
    use std::time::Duration;

    // Global test Kitty instance
    static TEST_KITTY: Mutex<Option<std::process::Child>> = Mutex::new(None);

    /// Helper struct to manage test HOME directory and automatic cleanup
    struct TestHomeGuard {
        _temp_dir: tempfile::TempDir,
        original_home: Option<String>,
    }

    impl Drop for TestHomeGuard {
        fn drop(&mut self) {
            // Restore original HOME when the guard is dropped
            if let Some(home) = &self.original_home {
                std::env::set_var("HOME", home);
            } else {
                std::env::remove_var("HOME");
            }
        }
    }

    /// Create a temporary HOME directory with a kitty config file
    ///
    /// # Arguments
    /// * `config_content` - The content to write to kitty.conf
    ///
    /// # Returns
    /// A guard that will restore the original HOME when dropped
    fn setup_test_home_with_config(config_content: &str) -> TestHomeGuard {
        use std::env;
        use std::fs;
        use tempfile::TempDir;

        // Create a temporary directory to act as HOME
        let temp_dir = TempDir::new().unwrap();
        let temp_home = temp_dir.path();

        // Create the .config/kitty directory structure
        let config_dir = temp_home.join(".config").join("kitty");
        fs::create_dir_all(&config_dir).unwrap();

        // Write the config file
        let config_path = config_dir.join("kitty.conf");
        fs::write(&config_path, config_content).unwrap();

        // Set the HOME environment variable to our temp directory
        let original_home = env::var("HOME").ok();
        env::set_var("HOME", temp_home);

        TestHomeGuard {
            _temp_dir: temp_dir,
            original_home,
        }
    }

    /// Create a temporary HOME directory without a config file
    ///
    /// # Returns
    /// A guard that will restore the original HOME when dropped
    fn setup_test_home_without_config() -> TestHomeGuard {
        use std::env;
        use tempfile::TempDir;

        // Create a temporary directory to act as HOME (without creating config file)
        let temp_dir = TempDir::new().unwrap();
        let temp_home = temp_dir.path();

        // Set the HOME environment variable to our temp directory
        let original_home = env::var("HOME").ok();
        env::set_var("HOME", temp_home);

        TestHomeGuard {
            _temp_dir: temp_dir,
            original_home,
        }
    }

    /// Start a test Kitty instance with remote control enabled
    fn start_test_kitty() -> Result<(), Box<dyn std::error::Error>> {
        let mut kitty_guard = TEST_KITTY.lock().unwrap();
        if kitty_guard.is_some() {
            return Ok(()); // Already started
        }

        // Create a temporary socket path for testing
        let socket_path = "/tmp/kitty-ah.sock";

        // Remove any existing socket
        let _ = std::fs::remove_file(socket_path);

        // Create a temporary config directory with remote control enabled
        let config_dir = "/tmp/kitty-test-config";
        std::fs::create_dir_all(config_dir)?;
        let config_file = format!("{}/kitty.conf", config_dir);
        std::fs::write(
            &config_file,
            "allow_remote_control yes\nlisten_on unix:/tmp/kitty-ah.sock\nenabled_layouts splits\n",
        )?;

        // Try to start Kitty in hidden mode with custom config directory
        tracing::debug!(socket=?socket_path, config=?config_file, "Attempting to start Kitty");
        let mut child = match std::process::Command::new("kitty")
            .args([
                "--listen-on",
                &format!("unix:{}", socket_path),
                "--start-as=hidden",
                "--config",
                &config_file,
            ])
            .env("KITTY_CONFIG_DIRECTORY", config_dir)
            .spawn()
        {
            Ok(child) => {
                tracing::debug!(pid=?child.id(), "Kitty spawned successfully");
                child
            }
            Err(e) => {
                tracing::error!(error=%e, "Failed to spawn Kitty");
                return Err(format!("Failed to spawn Kitty: {}", e).into());
            }
        };

        // Give Kitty time to start up
        std::thread::sleep(Duration::from_secs(2));

        // Check if Kitty is still running (it should be running indefinitely)
        match child.try_wait() {
            Ok(Some(status)) => {
                tracing::error!(status=?status, "Kitty exited unexpectedly");
                return Err(format!("Kitty exited immediately with status: {}", status).into());
            }
            Ok(None) => {
                tracing::debug!("Kitty still running after initial wait; checking socket");
                // Kitty is still running, check if socket was created
                if !std::path::Path::new(socket_path).exists() {
                    tracing::debug!(socket=?socket_path, "Socket not found yet");
                    // Wait a bit more and check again
                    std::thread::sleep(Duration::from_secs(3));
                    if !std::path::Path::new(socket_path).exists() {
                        tracing::error!(socket=?socket_path, "Socket not created after extended wait");
                        let _ = child.kill();
                        return Err("Kitty started but failed to create socket".into());
                    }
                }
                tracing::debug!(socket=?socket_path, "Socket found; Kitty setup successful");
            }
            Err(e) => {
                let _ = child.kill();
                return Err(format!("Failed to check Kitty status: {}", e).into());
            }
        }

        *kitty_guard = Some(child);
        Ok(())
    }

    /// Stop the test Kitty instance
    #[cfg(test)]
    #[allow(dead_code)]
    fn stop_test_kitty() {
        let mut kitty_guard = TEST_KITTY.lock().unwrap();
        if let Some(mut child) = kitty_guard.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }

    #[test]
    #[serial_test::serial(env)]
    fn test_kitty_multiplexer_creation() {
        // Set up a test environment with proper kitty configuration
        let test_config = "# Test kitty configuration\nallow_remote_control yes\nenabled_layouts splits\nlisten_on unix:/tmp/kitty-ah.sock\n";
        let _guard = setup_test_home_with_config(test_config);

        {
            let kitty = KittyMultiplexer::new().unwrap();
            assert_eq!(kitty.id(), "kitty");
            assert_eq!(kitty.socket_path, std::env::var("KITTY_LISTEN_ON").ok());
        }

        {
            std::env::set_var("KITTY_LISTEN_ON", "/tmp/test-kitty-ah.sock");
            let kitty = KittyMultiplexer::new().unwrap();
            assert_eq!(kitty.id(), "kitty");
            assert_eq!(kitty.socket_path, std::env::var("KITTY_LISTEN_ON").ok());
        }

        {
            let test_socket = "/tmp/test-kitty-ah.sock".to_string();
            let kitty = KittyMultiplexer::with_socket_path(test_socket.clone());
            assert_eq!(kitty.id(), "kitty");
            assert_eq!(kitty.socket_path.unwrap(), test_socket);
        }
    }

    #[test]
    #[serial_test::serial(env)]
    fn test_check_configuration_collects_multiple_errors() {
        // Set up test environment with incomplete kitty configuration
        // (missing remote control and socket settings)
        let incomplete_config = "# Incomplete kitty configuration\n# Missing: allow_remote_control yes\n# Missing: listen_on unix:/tmp/kitty-ah.sock\n# Has only layout setting\nenabled_layouts splits\n";
        let _guard = setup_test_home_with_config(incomplete_config);

        let kitty = KittyMultiplexer::default();

        // Check configuration should collect all issues, not just the first one
        let result = kitty.check_configuration();

        match result {
            Err(MuxError::ConfigurationError(error_msg)) => {
                // The error message should contain all the configuration issues
                assert!(
                    error_msg.contains("Remote control"),
                    "Error should mention remote control issue: {}",
                    error_msg
                );
                assert!(
                    error_msg.contains("Socket listening"),
                    "Error should mention socket issue: {}",
                    error_msg
                );
                // Layout splitting should NOT be mentioned since it's properly configured
                assert!(
                    !error_msg.contains("Layout splitting"),
                    "Error should not mention layout splitting since it's configured: {}",
                    error_msg
                );

                // Verify the error message provides helpful configuration guidance
                assert!(
                    error_msg.contains("allow_remote_control yes"),
                    "Error should include configuration guidance: {}",
                    error_msg
                );
                assert!(
                    error_msg.contains("listen_on unix:/tmp/kitty-ah.sock"),
                    "Error should include socket configuration guidance: {}",
                    error_msg
                );
            }
            Err(other_error) => panic!("Expected ConfigurationError, got: {:?}", other_error),
            Ok(()) => panic!("Expected configuration errors but check passed"),
        }
    }

    #[test]
    #[serial_test::serial(env)]
    fn test_check_configuration_with_complete_config() {
        // Set up test environment with complete kitty configuration
        let complete_config = "# Complete kitty configuration\nallow_remote_control yes\nlisten_on unix:/tmp/kitty-ah.sock\nenabled_layouts splits\n";
        let _guard = setup_test_home_with_config(complete_config);

        let kitty = KittyMultiplexer::default();

        // With complete configuration, check should pass (assuming kitty is installed)
        let result = kitty.check_configuration();

        match result {
            Ok(()) => {
                // Configuration check passed as expected
            }
            Err(MuxError::ConfigurationError(error_msg)) => {
                // If kitty is not installed, that's expected, but the error should only mention installation
                if !error_msg.contains("not installed") {
                    panic!(
                        "Unexpected configuration error with complete config: {}",
                        error_msg
                    );
                }
            }
            Err(other_error) => panic!("Unexpected error type: {:?}", other_error),
        }
    }

    #[test]
    #[serial_test::serial(env)]
    fn test_check_configuration_without_config_file() {
        // Set up test environment without any configuration file
        let _guard = setup_test_home_without_config();

        let kitty = KittyMultiplexer::default();

        // Without any config file, should collect all configuration issues
        let result = kitty.check_configuration();

        match result {
            Err(MuxError::ConfigurationError(error_msg)) => {
                // Should mention all missing configuration items
                assert!(
                    error_msg.contains("Remote control") || error_msg.contains("not installed"),
                    "Error should mention remote control or installation: {}",
                    error_msg
                );
                // The error should provide helpful guidance about all required settings
                assert!(
                    error_msg.contains("allow_remote_control yes"),
                    "Error should include remote control configuration guidance: {}",
                    error_msg
                );
                assert!(
                    error_msg.contains("listen_on unix:/tmp/kitty-ah.sock"),
                    "Error should include socket configuration guidance: {}",
                    error_msg
                );
                assert!(
                    error_msg.contains("enabled_layouts splits"),
                    "Error should include layout configuration guidance: {}",
                    error_msg
                );
            }
            Err(other_error) => panic!("Expected ConfigurationError, got: {:?}", other_error),
            Ok(()) => panic!("Expected configuration errors but check passed"),
        }
    }

    #[test]
    fn test_start_kitty_instance() {
        tracing::debug!("Testing start_test_kitty function");
        match start_test_kitty() {
            Ok(()) => tracing::debug!("start_test_kitty succeeded"),
            Err(e) => tracing::error!(error=%e, "start_test_kitty failed"),
        }
    }

    // #[test]
    // fn test_kitty_with_custom_socket() {
    //     let socket_path = "/tmp/test-kitty.sock".to_string();
    //     let kitty = KittyMultiplexer::with_socket_path(socket_path.clone());
    //     assert_eq!(kitty.socket_path, Some(socket_path));
    // }

    #[test]
    fn test_kitty_availability() {
        // Skip kitty tests in CI environments where kitty remote control is not available
        if std::env::var("CI").is_ok() {
            tracing::info!("Skipping kitty test in CI environment");
            return;
        }

        {
            // When kitty is configured correctly is should be available
            let complete_config = "# Complete kitty configuration\nallow_remote_control yes\nlisten_on unix:/tmp/kitty-ah.sock\nenabled_layouts splits\n";
            let _guard = setup_test_home_with_config(complete_config);

            let kitty = KittyMultiplexer::default();
            let available = kitty.is_available();
            assert!(available);
        }

        {
            // When kitty is not configured it is NOT available
            let _guard = setup_test_home_without_config();

            let kitty = KittyMultiplexer::default();
            let available = kitty.is_available();
            assert!(!available);
        }
    }

    #[test]
    fn test_parse_window_id() {
        let kitty = KittyMultiplexer::default();

        // Test valid window ID
        let result = kitty.parse_window_id_from_output("42\n");
        assert_eq!(result.unwrap(), "42");

        // Test empty output (should fail)
        let result = kitty.parse_window_id_from_output("");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_pane_id() {
        let kitty = KittyMultiplexer::default();

        // Test valid pane ID (same as window ID in kitty)
        let result = kitty.parse_pane_id_from_output("42\n");
        assert_eq!(result.unwrap(), "42");

        // Test empty output (should fail)
        let result = kitty.parse_pane_id_from_output("");
        assert!(result.is_err());
    }

    #[test]
    fn test_open_window_with_title_and_cwd() {
        // Skip kitty tests in CI environments where kitty remote control is not available
        if std::env::var("CI").is_ok() {
            tracing::info!("Skipping kitty test in CI environment");
            return;
        }

        let _ = start_test_kitty();
        let kitty = KittyMultiplexer::with_socket_path("/tmp/kitty-ah.sock".to_string());

        let opts = WindowOptions {
            title: Some("my-test-window-001"),
            cwd: Some(Path::new("/tmp")),
            profile: None,
            focus: false,
            init_command: None,
        };

        let window_id = kitty.open_window(&opts).unwrap();

        // Verify the window ID is numeric
        assert!(window_id.parse::<u32>().is_ok());

        // Verify the window actually exists in kitty
        assert!(
            kitty.window_exists(&window_id).unwrap_or(false),
            "Window {} should exist after creation",
            window_id
        );

        // Verify the window has the correct title
        let title = kitty.get_window_title(window_id).unwrap_or_default();
        assert_eq!(
            title, "my-test-window-001",
            "Window should have the correct title"
        );
        stop_test_kitty();
    }

    #[test]
    fn test_open_window_focus() {
        // Skip kitty tests in CI environments where kitty remote control is not available
        if std::env::var("CI").is_ok() {
            tracing::info!("Skipping kitty test in CI environment");
            return;
        }

        let _ = start_test_kitty();
        let kitty = KittyMultiplexer::with_socket_path("/tmp/kitty-ah.sock".to_string());

        let opts = WindowOptions {
            title: Some("focus-test-002"),
            cwd: Some(Path::new("/tmp")),
            profile: None,
            focus: true, // Should focus the window
            init_command: None,
        };

        // This will open new tab and focus it
        let window_id = kitty.open_window(&opts).unwrap();
        println!("Created window: {}", window_id);
        // Verify the window was created
        assert!(kitty.window_exists(&window_id).unwrap_or(false));

        // Verify the window is now focused
        let focused_id = kitty.get_focused_tab_window_id().unwrap_or_default();
        assert_eq!(
            focused_id, window_id,
            "Window {} should be focused after creation with focus=true",
            window_id
        );

        let current_window = kitty.current_window().unwrap();
        assert_eq!(
            current_window,
            Some(window_id),
            "Window should be focused after creation with focus=true",
        );
    }

    #[test]
    #[ignore = "TODO: Fix test and re-enable in CI"]
    fn test_split_pane_horizontal() {
        // Skip kitty tests in CI environments where kitty remote control is not available
        if std::env::var("CI").is_ok() {
            tracing::info!("Skipping kitty test in CI environment");
            return;
        }

        let kitty = KittyMultiplexer::new().unwrap();
        if kitty.is_available() {
            // Get initial window count
            let initial_windows = kitty.list_windows_detailed().unwrap_or_default();
            let initial_count = initial_windows.len();

            // Create a window to split from
            let window_opts = WindowOptions {
                title: Some("split-test-003"),
                cwd: Some(Path::new("/tmp")),
                profile: None,
                focus: false,
                init_command: None,
            };

            let window_result = kitty.open_window(&window_opts);
            match window_result {
                Ok(window_id) => {
                    // Verify window was created
                    assert!(kitty.window_exists(&window_id).unwrap_or(false));

                    // Now try to split it
                    let split_result = kitty.split_pane(
                        Some(&window_id),
                        Some(&window_id), // In kitty, panes are windows, so use window_id as pane_id
                        SplitDirection::Horizontal,
                        Some(60),
                        &CommandOptions::default(),
                        None,
                    );

                    match split_result {
                        Ok(new_pane_id) => {
                            // Verify the new pane ID is numeric and different
                            assert!(new_pane_id.parse::<u32>().is_ok());
                            assert_ne!(new_pane_id, window_id);

                            // Verify the new window actually exists
                            assert!(
                                kitty.window_exists(&new_pane_id).unwrap_or(false),
                                "New pane window {} should exist after split",
                                new_pane_id
                            );

                            // Verify we now have more windows
                            let final_windows = kitty.list_windows_detailed().unwrap_or_default();
                            assert!(
                                final_windows.len() > initial_count,
                                "Should have more than {} windows after split, got {}",
                                initial_count,
                                final_windows.len()
                            );
                        }
                        Err(MuxError::CommandFailed(_)) => {
                            // Expected when remote control fails
                        }
                        Err(e) => panic!("Unexpected error: {:?}", e),
                    }
                }
                Err(MuxError::CommandFailed(_)) => {
                    // Can't test splitting if we can't create windows
                }
                Err(e) => panic!("Unexpected error creating window: {:?}", e),
            }
        }
    }

    #[test]
    #[ignore = "TODO: Fix test and re-enable in CI"]
    fn test_split_pane_vertical() {
        // Skip kitty tests in CI environments where kitty remote control is not available
        if std::env::var("CI").is_ok() {
            tracing::info!("Skipping kitty test in CI environment");
            return;
        }

        let kitty = KittyMultiplexer::new().unwrap();
        if kitty.is_available() {
            // Get initial window count
            let initial_windows = kitty.list_windows_detailed().unwrap_or_default();
            let initial_count = initial_windows.len();

            let window_opts = WindowOptions {
                title: Some("split-v-test-004"),
                cwd: Some(Path::new("/tmp")),
                profile: None,
                focus: false,
                init_command: None,
            };

            let window_result = kitty.open_window(&window_opts);
            match window_result {
                Ok(window_id) => {
                    // Verify window was created
                    assert!(kitty.window_exists(&window_id).unwrap_or(false));

                    let split_result = kitty.split_pane(
                        Some(&window_id),
                        Some(&window_id),
                        SplitDirection::Vertical,
                        Some(70),
                        &CommandOptions::default(),
                        None,
                    );

                    match split_result {
                        Ok(new_pane_id) => {
                            assert!(new_pane_id.parse::<u32>().is_ok());
                            assert_ne!(new_pane_id, window_id);

                            // Verify the new window actually exists
                            assert!(
                                kitty.window_exists(&new_pane_id).unwrap_or(false),
                                "New pane window {} should exist after vertical split",
                                new_pane_id
                            );

                            // Verify window count increased
                            let final_windows = kitty.list_windows_detailed().unwrap_or_default();
                            assert!(
                                final_windows.len() > initial_count,
                                "Should have more than {} windows after vertical split, got {}",
                                initial_count,
                                final_windows.len()
                            );
                        }
                        Err(MuxError::CommandFailed(_)) => {
                            // Expected when remote control fails
                        }
                        Err(e) => panic!("Unexpected error: {:?}", e),
                    }
                }
                Err(MuxError::CommandFailed(_)) => {
                    // Can't test splitting if we can't create windows
                }
                Err(e) => panic!("Unexpected error creating window: {:?}", e),
            }
        }
    }

    #[test]
    #[ignore = "TODO: Fix test and re-enable in CI"]
    fn test_split_pane_with_initial_command() {
        // Skip kitty tests in CI environments where kitty remote control is not available
        if std::env::var("CI").is_ok() {
            tracing::info!("Skipping kitty test in CI environment");
            return;
        }

        let kitty = KittyMultiplexer::new().unwrap();
        if kitty.is_available() {
            let window_opts = WindowOptions {
                title: Some("split-cmd-test"),
                cwd: Some(Path::new("/tmp")),
                profile: None,
                focus: false,
                init_command: None,
            };

            let window_result = kitty.open_window(&window_opts);
            match window_result {
                Ok(window_id) => {
                    // Split with initial command that should keep the pane alive
                    let split_result = kitty.split_pane(
                        Some(&window_id),
                        Some(&window_id),
                        SplitDirection::Horizontal,
                        None,
                        &CommandOptions::default(),
                        Some("sleep 1"), // Short sleep to test command execution
                    );

                    match split_result {
                        Ok(new_pane_id) => {
                            assert!(new_pane_id.parse::<u32>().is_ok());
                            assert_ne!(new_pane_id, window_id);

                            // Give the command a moment to start
                            thread::sleep(Duration::from_millis(200));
                        }
                        Err(MuxError::CommandFailed(_)) => {
                            // Expected when remote control fails
                        }
                        Err(e) => panic!("Unexpected error: {:?}", e),
                    }
                }
                Err(MuxError::CommandFailed(_)) => {
                    // Can't test splitting if we can't create windows
                }
                Err(e) => panic!("Unexpected error creating window: {:?}", e),
            }
        }
    }

    #[test]
    #[ignore = "TODO: Fix test and re-enable in CI"]
    fn test_run_command_and_send_text() {
        // Skip kitty tests in CI environments where kitty remote control is not available
        if std::env::var("CI").is_ok() {
            tracing::info!("Skipping kitty test in CI environment");
            return;
        }

        let kitty = KittyMultiplexer::new().unwrap();
        if kitty.is_available() {
            let window_opts = WindowOptions {
                title: Some("cmd-text-test"),
                cwd: Some(Path::new("/tmp")),
                profile: None,
                focus: false,
                init_command: None,
            };

            let window_result = kitty.open_window(&window_opts);
            match window_result {
                Ok(window_id) => {
                    // Test run_command
                    let cmd_result = kitty.run_command(
                        &window_id,
                        "echo 'hello world'",
                        &CommandOptions::default(),
                    );
                    match cmd_result {
                        Ok(()) => {
                            // Command executed successfully
                            thread::sleep(Duration::from_millis(100));
                        }
                        Err(MuxError::CommandFailed(_)) => {
                            // Expected when remote control fails
                        }
                        Err(e) => panic!("Unexpected error running command: {:?}", e),
                    }

                    // Test send_text
                    let text_result = kitty.send_text(&window_id, "some input text");
                    match text_result {
                        Ok(()) => {
                            // Text sent successfully
                            thread::sleep(Duration::from_millis(100));
                        }
                        Err(MuxError::CommandFailed(_)) => {
                            // Expected when remote control fails
                        }
                        Err(e) => panic!("Unexpected error sending text: {:?}", e),
                    }

                    // Verify window still exists (if we can list windows)
                    let list_result = kitty.list_windows(Some("cmd-text-test"));
                    match list_result {
                        Ok(windows) => {
                            // Should find our test window
                            assert!(windows.iter().any(|w| w.parse::<u32>().is_ok()));
                        }
                        Err(MuxError::CommandFailed(_)) => {
                            // Expected when remote control fails
                        }
                        Err(e) => panic!("Unexpected error listing windows: {:?}", e),
                    }
                }
                Err(MuxError::CommandFailed(_)) => {
                    // Can't test commands if we can't create windows
                }
                Err(e) => panic!("Unexpected error creating window: {:?}", e),
            }
        }
    }

    #[test]
    #[ignore = "TODO: Fix test and re-enable in CI"]
    fn test_focus_window_and_pane() {
        // Skip kitty tests in CI environments where kitty remote control is not available
        if std::env::var("CI").is_ok() {
            tracing::warn!("skipping kitty test in CI environment");
            return;
        }

        let kitty = KittyMultiplexer::new().unwrap();
        if kitty.is_available() {
            let window_opts1 = WindowOptions {
                title: Some("window1-005"),
                cwd: Some(Path::new("/tmp")),
                profile: None,
                focus: false,
                init_command: None,
            };

            let window_opts2 = WindowOptions {
                title: Some("window2-005"),
                cwd: Some(Path::new("/tmp")),
                profile: None,
                focus: false,
                init_command: None,
            };

            let window1_result = kitty.open_window(&window_opts1);
            let window2_result = kitty.open_window(&window_opts2);

            match (window1_result, window2_result) {
                (Ok(window1), Ok(window2)) => {
                    // Test window focusing - focus window1 first
                    let focus1_result = kitty.focus_window(&window1);
                    match focus1_result {
                        Ok(()) => {
                            // Verify window1 is now focused
                            let focused_id = kitty.get_focused_window_id().unwrap_or_default();
                            assert_eq!(
                                focused_id, window1,
                                "Window {} should be focused after focus_window call",
                                window1
                            );
                        }
                        Err(MuxError::CommandFailed(_)) => {
                            // Expected when remote control fails
                            return;
                        }
                        Err(e) => panic!("Unexpected error focusing window1: {:?}", e),
                    }

                    // Now focus window2
                    let focus2_result = kitty.focus_window(&window2);
                    match focus2_result {
                        Ok(()) => {
                            // Verify window2 is now focused
                            let focused_id = kitty.get_focused_window_id().unwrap_or_default();
                            assert_eq!(
                                focused_id, window2,
                                "Window {} should be focused after focus_window call",
                                window2
                            );
                        }
                        Err(MuxError::CommandFailed(_)) => {
                            // Expected when remote control fails
                        }
                        Err(e) => panic!("Unexpected error focusing window2: {:?}", e),
                    }

                    // Test pane focusing (same as window focusing in kitty) - focus back to window1
                    let pane_focus_result = kitty.focus_pane(&window1);
                    match pane_focus_result {
                        Ok(()) => {
                            // Verify window1 is focused again
                            let focused_id = kitty.get_focused_window_id().unwrap_or_default();
                            assert_eq!(
                                focused_id, window1,
                                "Pane/window {} should be focused after focus_pane call",
                                window1
                            );
                        }
                        Err(MuxError::CommandFailed(_)) => {
                            // Expected when remote control fails
                        }
                        Err(e) => panic!("Unexpected error focusing pane: {:?}", e),
                    }
                }
                _ => {
                    // Can't test focusing if we can't create windows
                }
            }
        }
    }

    #[test]
    #[ignore = "TODO: Fix test and re-enable in CI"]
    fn test_list_windows_filtering() {
        // Skip kitty tests in CI environments where kitty remote control is not available
        if std::env::var("CI").is_ok() {
            tracing::warn!("skipping kitty test in CI environment");
            return;
        }

        let kitty = KittyMultiplexer::new().unwrap();
        if kitty.is_available() {
            // Create test windows
            let window_opts = vec![
                WindowOptions {
                    title: Some("alpha-window-006"),
                    cwd: Some(Path::new("/tmp")),
                    profile: None,
                    focus: false,
                    init_command: None,
                },
                WindowOptions {
                    title: Some("beta-window-006"),
                    cwd: Some(Path::new("/tmp")),
                    profile: None,
                    focus: false,
                    init_command: None,
                },
                WindowOptions {
                    title: Some("alpha-other-006"),
                    cwd: Some(Path::new("/tmp")),
                    profile: None,
                    focus: false,
                    init_command: None,
                },
            ];

            let mut created_windows = Vec::new();
            for opts in window_opts {
                match kitty.open_window(&opts) {
                    Ok(window_id) => {
                        created_windows.push(window_id);
                    }
                    Err(MuxError::CommandFailed(_)) => {
                        // Skip if remote control not available
                        return;
                    }
                    Err(e) => panic!("Unexpected error creating window: {:?}", e),
                }
            }

            // Give windows time to be created
            thread::sleep(Duration::from_millis(200));

            // List all windows
            let all_windows_result = kitty.list_windows(None);
            match all_windows_result {
                Ok(all_windows) => {
                    assert!(!all_windows.is_empty());
                    // All window IDs should be numeric
                    for window in &all_windows {
                        assert!(window.parse::<u32>().is_ok());
                    }
                }
                Err(MuxError::CommandFailed(_)) => {
                    // Expected when remote control fails
                    return;
                }
                Err(e) => panic!("Unexpected error listing all windows: {:?}", e),
            }

            // Filter by "alpha"
            let alpha_windows_result = kitty.list_windows(Some("alpha"));
            match alpha_windows_result {
                Ok(alpha_windows) => {
                    // Should find at least the alpha windows we created
                    assert!(!alpha_windows.is_empty());
                    for window in &alpha_windows {
                        assert!(window.parse::<u32>().is_ok());
                    }
                }
                Err(MuxError::CommandFailed(_)) => {
                    // Expected when remote control fails
                }
                Err(e) => panic!("Unexpected error listing alpha windows: {:?}", e),
            }

            // Filter by "beta"
            let beta_windows_result = kitty.list_windows(Some("beta"));
            match beta_windows_result {
                Ok(beta_windows) => {
                    assert!(!beta_windows.is_empty());
                    for window in &beta_windows {
                        assert!(window.parse::<u32>().is_ok());
                    }
                }
                Err(MuxError::CommandFailed(_)) => {
                    // Expected when remote control fails
                }
                Err(e) => panic!("Unexpected error listing beta windows: {:?}", e),
            }

            // Filter by non-existent title
            let none_windows_result = kitty.list_windows(Some("nonexistent"));
            match none_windows_result {
                Ok(none_windows) => {
                    // Should be empty or not contain our test windows
                    assert!(
                        none_windows.is_empty()
                            || !none_windows.iter().any(|w| created_windows.contains(w))
                    );
                }
                Err(MuxError::CommandFailed(_)) => {
                    // Expected when remote control fails
                }
                Err(e) => panic!("Unexpected error listing nonexistent windows: {:?}", e),
            }
        }
    }

    #[test]
    #[ignore = "TODO: Fix test and re-enable in CI"]
    fn test_error_handling_invalid_window() {
        // Skip kitty tests in CI environments where kitty remote control is not available
        if std::env::var("CI").is_ok() {
            tracing::warn!("skipping kitty test in CI environment");
            return;
        }

        let kitty = KittyMultiplexer::new().unwrap();
        if kitty.is_available() {
            // Try to focus a non-existent window
            let invalid_window = "99999".to_string();
            let result = kitty.focus_window(&invalid_window);
            // Should either succeed (if window exists) or fail gracefully
            match result {
                Ok(()) => {
                    // Window might exist
                }
                Err(MuxError::CommandFailed(_)) => {
                    // Expected when window doesn't exist or remote control fails
                }
                Err(e) => panic!("Unexpected error: {:?}", e),
            }
        }
    }

    #[test]
    #[ignore = "TODO: Fix test and re-enable in CI"]
    fn test_error_handling_invalid_pane() {
        // Skip kitty tests in CI environments where kitty remote control is not available
        if std::env::var("CI").is_ok() {
            tracing::warn!("skipping kitty test in CI environment");
            return;
        }

        let kitty = KittyMultiplexer::new().unwrap();
        if kitty.is_available() {
            // Try to focus a non-existent pane
            let invalid_pane = "99999".to_string();
            let result = kitty.focus_pane(&invalid_pane);
            match result {
                Ok(()) => {
                    // Pane might exist
                }
                Err(MuxError::CommandFailed(_)) => {
                    // Expected when pane doesn't exist or remote control fails
                }
                Err(e) => panic!("Unexpected error: {:?}", e),
            }

            // Try to send text to non-existent pane
            let result = kitty.send_text(&invalid_pane, "test");
            match result {
                Ok(()) => {
                    // Pane might exist
                }
                Err(MuxError::CommandFailed(_)) => {
                    // Expected when pane doesn't exist or remote control fails
                }
                Err(e) => panic!("Unexpected error: {:?}", e),
            }
        }
    }

    #[test]
    #[ignore = "TODO: Fix test and re-enable in CI"]
    fn test_complex_layout_creation() {
        // Skip kitty tests in CI environments where kitty remote control is not available
        if std::env::var("CI").is_ok() {
            tracing::warn!("skipping kitty test in CI environment");
            return;
        }

        let kitty = KittyMultiplexer::new().unwrap();
        if kitty.is_available() {
            // Get initial window count
            let initial_windows = kitty.list_windows_detailed().unwrap_or_default();
            let initial_count = initial_windows.len();

            // Create a main window
            let window_opts = WindowOptions {
                title: Some("complex-layout-008"),
                cwd: Some(Path::new("/tmp")),
                profile: None,
                focus: false,
                init_command: None,
            };

            let window_result = kitty.open_window(&window_opts);
            match window_result {
                Ok(window_id) => {
                    // Verify main window was created
                    assert!(kitty.window_exists(&window_id).unwrap_or(false));
                    assert_eq!(
                        kitty.get_window_title(window_id.clone()).unwrap_or_default(),
                        "complex-layout-008"
                    );

                    // Create a 3-"pane" layout: editor (left), agent (top-right), logs (bottom-right)
                    // In kitty terms, this means creating separate windows positioned relative to each other

                    // Create agent pane (top-right of main window)
                    let agent_result = kitty.split_pane(
                        Some(&window_id),
                        Some(&window_id),
                        SplitDirection::Horizontal,
                        Some(70), // 70% for editor (main window)
                        &CommandOptions::default(),
                        None,
                    );

                    match agent_result {
                        Ok(agent_pane) => {
                            // Verify agent pane was created
                            assert!(kitty.window_exists(&agent_pane).unwrap_or(false));
                            assert_ne!(agent_pane, window_id);

                            // Create logs pane (bottom-right, split from agent pane)
                            let logs_result = kitty.split_pane(
                                Some(&window_id),
                                Some(&agent_pane),
                                SplitDirection::Vertical,
                                Some(60), // 60% for agent, 40% for logs
                                &CommandOptions::default(),
                                None,
                            );

                            match logs_result {
                                Ok(logs_pane) => {
                                    // Verify logs pane was created
                                    assert!(kitty.window_exists(&logs_pane).unwrap_or(false));
                                    assert_ne!(logs_pane, window_id);
                                    assert_ne!(logs_pane, agent_pane);

                                    // Give panes time to be created and verify final state
                                    thread::sleep(Duration::from_millis(200));

                                    // Verify all "panes" (windows) exist
                                    let all_panes_result = kitty.list_panes(&window_id);
                                    match all_panes_result {
                                        Ok(all_panes) => {
                                            assert!(!all_panes.is_empty());
                                            // Should contain our main window and created panes
                                            assert!(all_panes.contains(&window_id));
                                            assert!(all_panes.contains(&agent_pane));
                                            assert!(all_panes.contains(&logs_pane));
                                        }
                                        Err(MuxError::CommandFailed(_)) => {
                                            // Expected when remote control fails
                                            return;
                                        }
                                        Err(e) => panic!("Unexpected error listing panes: {:?}", e),
                                    }

                                    // Verify we have the expected number of windows
                                    let final_windows =
                                        kitty.list_windows_detailed().unwrap_or_default();
                                    assert!(
                                        final_windows.len() >= initial_count + 2,
                                        "Should have at least {} windows after creating 2 splits, got {}",
                                        initial_count + 2,
                                        final_windows.len()
                                    );

                                    // Test focusing different panes and verify focus changes
                                    let _ = kitty.focus_window(&window_id);
                                    let focused = kitty.get_focused_window_id().unwrap_or_default();
                                    assert_eq!(focused, window_id);

                                    let _ = kitty.focus_pane(&agent_pane);
                                    let focused = kitty.get_focused_window_id().unwrap_or_default();
                                    assert_eq!(focused, agent_pane);

                                    let _ = kitty.focus_pane(&logs_pane);
                                    let focused = kitty.get_focused_window_id().unwrap_or_default();
                                    assert_eq!(focused, logs_pane);
                                }
                                Err(MuxError::CommandFailed(_)) => {
                                    // Expected when remote control fails
                                }
                                Err(e) => panic!("Unexpected error creating logs pane: {:?}", e),
                            }
                        }
                        Err(MuxError::CommandFailed(_)) => {
                            // Expected when remote control fails
                        }
                        Err(e) => panic!("Unexpected error creating agent pane: {:?}", e),
                    }
                }
                Err(MuxError::CommandFailed(_)) => {
                    // Can't test complex layout if we can't create windows
                }
                Err(e) => panic!("Unexpected error creating main window: {:?}", e),
            }
        }
    }


    #[test]
    #[serial_test::serial(env)]
    fn test_read_default_config() {
        let test_config = "# Test kitty configuration\nallow_remote_control yes\nlisten_on unix:/tmp/kitty-ah.sock\n";
        let _guard = setup_test_home_with_config(test_config);

        let kitty = KittyMultiplexer::default();
        let result = kitty.read_default_config();

        // Verify the config was read correctly
        match result {
            Ok(config) => {
                assert!(!config.is_empty(), "Config file should not be empty");
                assert_eq!(
                    config, test_config,
                    "Config content should match what was written"
                );
                tracing::debug!(bytes = config.len(), "successfully read kitty config");
            }
            Err(e) => {
                panic!("Failed to read config file: {:?}", e);
            }
        }
    }

    #[test]
    #[serial_test::serial(env)]
    fn test_read_default_config_missing_file() {
        let _guard = setup_test_home_without_config();

        let kitty = KittyMultiplexer::default();
        let result = kitty.read_default_config();

        // Verify we get an appropriate error
        match result {
            Ok(_) => {
                panic!("Should have failed when config file doesn't exist");
            }
            Err(MuxError::CommandFailed(msg)) => {
                assert!(
                    msg.contains("kitty.conf"),
                    "Error message should mention kitty.conf: {}",
                    msg
                );
                tracing::debug!(error = msg, "expected error for missing config");
            }
            Err(e) => {
                panic!("Unexpected error type: {:?}", e);
            }
        }
    }

    #[test]
    #[serial_test::serial(env)]
    fn test_check_remote_control_from_config() {
        // Test case 1: Config with remote control enabled
        {
            let test_config = "# Kitty configuration\nallow_remote_control yes\nlisten_on unix:/tmp/kitty-ah.sock\n";
            let _guard = setup_test_home_with_config(test_config);

            let kitty = KittyMultiplexer::default();
            let result = kitty.check_remote_control_from_config();

            assert!(result.is_ok(), "Should successfully read config");
            assert!(
                result.unwrap(),
                "Should detect 'allow_remote_control yes' in config"
            );
            tracing::debug!("remote control enabled detected");
        }

        // Test case 2: Config without remote control enabled (commented out)
        {
            let test_config = "# Kitty configuration\n# allow_remote_control yes\nlisten_on unix:/tmp/kitty-ah.sock\n";
            let _guard = setup_test_home_with_config(test_config);

            let kitty = KittyMultiplexer::default();
            let result = kitty.check_remote_control_from_config();

            assert!(result.is_ok(), "Should successfully read config");
            assert!(
                !result.unwrap(),
                "Should not detect remote control when commented out"
            );
            tracing::debug!("remote control disabled detected");
        }

        // Test case 3: Config with 'allow_remote_control no'
        {
            let test_config = "# Kitty configuration\nallow_remote_control no\n";
            let _guard = setup_test_home_with_config(test_config);

            let kitty = KittyMultiplexer::default();
            let result = kitty.check_remote_control_from_config();

            assert!(result.is_ok(), "Should successfully read config");
            assert!(
                !result.unwrap(),
                "Should not detect remote control when set to 'no'"
            );
            tracing::debug!("'allow_remote_control no' handled correctly");
        }
    }

    #[test]
    #[serial_test::serial(env)]
    fn test_check_enable_layout_split_from_config() {
        // Test case 1: Config with layout split enabled
        {
            let test_config =
                "# Kitty configuration\nenabled_layouts splits\nallow_remote_control yes\n";
            let _guard = setup_test_home_with_config(test_config);

            let kitty = KittyMultiplexer::default();
            let result = kitty.check_enable_layout_split_from_config();

            assert!(result.is_ok(), "Should successfully read config");
            assert!(
                result.unwrap(),
                "Should detect 'enabled_layouts splits' in config"
            );
            tracing::debug!("layout split enabled detected");
        }

        // Test case 2: Config without layout split setting
        {
            let test_config = "# Kitty configuration\nallow_remote_control yes\n";
            let _guard = setup_test_home_with_config(test_config);

            let kitty = KittyMultiplexer::default();
            let result = kitty.check_enable_layout_split_from_config();

            assert!(result.is_ok(), "Should successfully read config");
            assert!(
                !result.unwrap(),
                "Should not detect layout split when not present"
            );
            tracing::debug!("missing layout split setting handled");
        }

        // Test case 3: Config with layout split commented out
        {
            let test_config =
                "# Kitty configuration\n# enabled_layouts splits\nallow_remote_control yes\n";
            let _guard = setup_test_home_with_config(test_config);

            let kitty = KittyMultiplexer::default();
            let result = kitty.check_enable_layout_split_from_config();

            assert!(result.is_ok(), "Should successfully read config");
            assert!(
                !result.unwrap(),
                "Should not detect layout split when commented out"
            );
            tracing::debug!("commented layout split handled");
        }

        // Test case 4: Both settings enabled
        {
            let test_config = "# Kitty configuration\nallow_remote_control yes\nenabled_layouts splits\nlisten_on unix:/tmp/kitty-ah.sock\n";
            let _guard = setup_test_home_with_config(test_config);

            let kitty = KittyMultiplexer::default();

            let remote_result = kitty.check_remote_control_from_config();
            let layout_result = kitty.check_enable_layout_split_from_config();

            assert!(
                remote_result.is_ok() && layout_result.is_ok(),
                "Should successfully read config"
            );
            assert!(
                remote_result.unwrap() && layout_result.unwrap(),
                "Should detect both settings when enabled"
            );
            tracing::debug!("both settings detected correctly");
        }
    }

    #[test]
    #[serial_test::serial(env)]
    fn test_config_methods_with_missing_file() {
        let _guard = setup_test_home_without_config();

        let kitty = KittyMultiplexer::default();

        // Both methods should return an error when config file doesn't exist
        let remote_result = kitty.check_remote_control_from_config();
        let layout_result = kitty.check_enable_layout_split_from_config();

        // Verify both methods return errors
        assert!(
            remote_result.is_err(),
            "check_remote_control_from_config should fail when config file doesn't exist"
        );
        assert!(
            layout_result.is_err(),
            "check_enable_layout_split_from_config should fail when config file doesn't exist"
        );

        // Verify error messages are helpful
        match remote_result {
            Err(MuxError::CommandFailed(msg)) => {
                assert!(
                    msg.contains("kitty.conf"),
                    "Error should mention config file: {}",
                    msg
                );
                tracing::debug!(
                    error = msg,
                    "expected error for missing config (remote_control)"
                );
            }
            _ => panic!("Expected MuxError::CommandFailed"),
        }

        match layout_result {
            Err(MuxError::CommandFailed(msg)) => {
                assert!(
                    msg.contains("kitty.conf"),
                    "Error should mention config file: {}",
                    msg
                );
                tracing::debug!(
                    error = msg,
                    "expected error for missing config (layout_split)"
                );
            }
            _ => panic!("Expected MuxError::CommandFailed"),
        }
    }
}
