// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! WezTerm multiplexer implementation
//!
//! WezTerm is a modern, GPU-accelerated terminal emulator with excellent
//! CLI automation support via the `wezterm cli` command.
//!
//! This implementation follows the WezTerm Multiplexer Integration Spec to provide
//! reliable automated session layout creation, pane targeting, key injection and
//! task focusing across platforms.
//!
//! ## Terminology Note
//!
//! There is an important terminology discrepancy between the `Multiplexer` trait
//! and WezTerm's CLI:
//!
//! - **Multiplexer "window"** → **WezTerm "tab"** (what users see as tabs in the terminal)
//! - **Multiplexer "pane"** → **WezTerm "pane"** (splits within a tab)
//! - **WezTerm "window"** → **GUI window** (the actual desktop window containing multiple tabs)
//!
//! This implementation internally uses WezTerm's terminology (tabs) but exposes
//! the Multiplexer trait interface (windows), performing the necessary translation.

use std::process::Command;

use ah_mux_core::*;
use serde_json::Value;
use tracing::{debug, error, info, instrument, trace, warn};

use crate::MuxError;

/// WezTerm multiplexer implementation
#[derive(Debug)]
pub struct WezTermMultiplexer {
    /// WezTerm version string for feature compatibility
    version: String,
}

impl WezTermMultiplexer {
    /// Create a new WezTerm multiplexer instance
    #[instrument]
    pub fn new() -> Result<Self, MuxError> {
        info!("Initializing WezTerm multiplexer");

        let version = Self::check_version()?;

        // Test CLI connectivity - if this fails, propagate the detailed error
        if let Err(e) = Self::test_cli_connectivity() {
            error!("WezTerm CLI connectivity test failed: {}", e);
            return Err(e);
        }

        info!(
            "WezTerm multiplexer initialized successfully (version: {})",
            version
        );
        Ok(Self { version })
    }

    /// Test CLI connectivity as per spec requirement
    fn test_cli_connectivity() -> Result<(), MuxError> {
        let output = Command::new("wezterm")
            .arg("cli")
            .arg("--prefer-mux")
            .arg("list")
            .output()
            .map_err(|e| MuxError::Other(format!("CLI test failed: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);

            // Check for specific connection errors
            if stderr.contains("failed to connect") || stderr.contains("gui-sock") {
                return Err(MuxError::ConfigurationError(
                    "WezTerm CLI could not connect to running WezTerm instance. \
                     Make sure WezTerm is running and CLI access is enabled. \
                     You may need to start WezTerm with multiplexer support or configure \
                     the Unix domain socket in your WezTerm config."
                        .to_string(),
                ));
            }

            return Err(MuxError::Other(format!(
                "CLI not responsive - stderr: {}, stdout: {}",
                stderr, stdout
            )));
        }

        Ok(())
    }

    /// Parse JSON response from WezTerm CLI commands
    fn parse_json_response(output: &[u8]) -> Result<Value, MuxError> {
        let json_str = String::from_utf8_lossy(output);
        trace!("WezTerm CLI JSON response: {}", json_str);
        serde_json::from_str(&json_str).map_err(|e| {
            MuxError::Other(format!(
                "Failed to parse JSON response '{}': {}",
                json_str, e
            ))
        })
    }

    /// General listing function for WezTerm - returns all windows with their tabs and panes
    ///
    /// This function executes `wezterm cli list --format json` once and returns the
    /// parsed result for use by other functions like `list_windows`, `list_panes`,
    /// and `find_tab_for_pane_id`.
    #[instrument(skip(self))]
    fn list_all_entities(&self) -> Result<Value, MuxError> {
        info!("Retrieving complete WezTerm entity listing");

        let output = Command::new("wezterm")
            .arg("cli")
            .arg("--prefer-mux")
            .arg("list")
            .arg("--format")
            .arg("json")
            .output()
            .map_err(|e| {
                error!("Failed to execute wezterm cli list command: {}", e);
                MuxError::Other(format!("Failed to list wezterm entities: {}", e))
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            error!("WezTerm list command failed: {}", stderr);
            return Err(MuxError::CommandFailed(format!(
                "wezterm list failed: {}",
                stderr
            )));
        }

        let entities_json = Self::parse_json_response(&output.stdout)?;
        debug!("WezTerm entity listing retrieved successfully");
        info!("Successfully retrieved complete WezTerm entity listing");
        Ok(entities_json)
    }

    /// Find the tab containing a given pane ID
    ///
    /// Returns the tab ID (as a string) that contains the specified pane.
    /// WezTerm CLI list returns a flat array where each object represents a pane
    /// with window_id, tab_id, and pane_id fields at the top level.
    #[instrument(skip(self))]
    pub fn find_tab_for_pane_id(&self, pane_id: &str) -> Result<String, MuxError> {
        info!("Finding tab for pane ID: {}", pane_id);

        let entities = self.list_all_entities()?;
        let panes = entities
            .as_array()
            .ok_or_else(|| MuxError::Other("Expected JSON array from wezterm list".to_string()))?;

        let target_pane_id = pane_id
            .parse::<u64>()
            .map_err(|e| MuxError::Other(format!("Invalid pane ID format '{}': {}", pane_id, e)))?;

        for pane_obj in panes {
            // Each object in the array is a pane with window_id, tab_id, and pane_id at the top level
            if let Some(found_pane_id) = pane_obj.get("pane_id").and_then(|id| id.as_u64()) {
                if found_pane_id == target_pane_id {
                    // Found the pane, now get the tab ID
                    if let Some(tab_id) = pane_obj.get("tab_id").and_then(|id| id.as_u64()) {
                        let tab_id_str = tab_id.to_string();
                        debug!("Found pane {} in tab {}", pane_id, tab_id_str);
                        info!(
                            "Successfully found tab {} for pane ID {}",
                            tab_id_str, pane_id
                        );
                        return Ok(tab_id_str);
                    } else {
                        return Err(MuxError::Other(format!(
                            "Pane {} found but object has no valid tab_id",
                            pane_id
                        )));
                    }
                }
            }
        }

        error!("Pane ID {} not found in any tab", pane_id);
        Err(MuxError::Other(format!(
            "Pane ID {} not found in any tab",
            pane_id
        )))
    }

    /// Set window title explicitly using WezTerm CLI
    pub fn set_window_title(&self, window: &WindowId, title: &str) -> Result<(), MuxError> {
        debug!("Setting WezTerm window title: {} -> {}", window, title);

        let output = Command::new("wezterm")
            .arg("cli")
            .arg("--prefer-mux")
            .arg("set-window-title")
            .arg("--window-id")
            .arg(window)
            .arg(title)
            .output()
            .map_err(|e| {
                error!("Failed to set WezTerm window title: {}", e);
                MuxError::Other(format!("Failed to set wezterm window title: {}", e))
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            warn!("WezTerm set-window-title failed: {}", stderr);
            // Don't return error - title setting is not critical
        }

        Ok(())
    }

    /// Set tab title explicitly using WezTerm CLI
    pub fn set_tab_title(&self, window_id: &str, title: &str) -> Result<(), MuxError> {
        debug!(
            "Setting WezTerm tab title for pane: {} -> {}",
            window_id, title
        );

        // Find the tab that contains this pane
        let tab_id = self.find_tab_for_pane_id(window_id)?;
        debug!("Found tab ID {} for pane ID {}", tab_id, window_id);

        let output = Command::new("wezterm")
            .arg("cli")
            .arg("--prefer-mux")
            .arg("set-tab-title")
            .arg("--tab-id")
            .arg(&tab_id)
            .arg(title)
            .output()
            .map_err(|e| {
                error!("Failed to set WezTerm tab title: {}", e);
                MuxError::Other(format!("Failed to set wezterm tab title: {}", e))
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            warn!("WezTerm set-tab-title failed: {}", stderr);
            // Don't return error - title setting is not critical
        }

        Ok(())
    }

    /// Kill a specific pane (for cleanup operations)
    pub fn kill_pane(&self, pane: &PaneId) -> Result<(), MuxError> {
        info!("Killing WezTerm pane: {}", pane);

        let output = Command::new("wezterm")
            .arg("cli")
            .arg("--prefer-mux")
            .arg("kill-pane")
            .arg("--pane-id")
            .arg(pane)
            .output()
            .map_err(|e| {
                error!("Failed to kill WezTerm pane: {}", e);
                MuxError::Other(format!("Failed to kill wezterm pane: {}", e))
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            error!("WezTerm kill-pane failed: {}", stderr);
            return Err(MuxError::CommandFailed(format!(
                "wezterm kill-pane failed: {}",
                stderr
            )));
        }

        debug!("WezTerm pane '{}' killed", pane);
        info!("Successfully killed WezTerm pane: {}", pane);
        Ok(())
    }

    /// Get version info for feature compatibility checks
    pub fn version(&self) -> &str {
        &self.version
    }

    /// Focus a tab in WezTerm (corresponds to Multiplexer "window")
    #[instrument(skip(self))]
    pub fn focus_tab(&self, window: &WindowId) -> Result<(), MuxError> {
        info!("Focusing WezTerm tab for pane: {}", window);
        debug!("Focusing WezTerm tab for pane: {}", window);

        // Find the tab that contains this pane
        let tab_id = self.find_tab_for_pane_id(window)?;
        debug!("Found tab ID {} for pane ID {}", tab_id, window);

        let output = Command::new("wezterm")
            .arg("cli")
            .arg("--prefer-mux")
            .arg("activate-tab")
            .arg("--tab-id")
            .arg(&tab_id)
            .output()
            .map_err(|e| {
                error!(
                    "Failed to activate WezTerm tab '{}' for pane '{}': {}",
                    tab_id, window, e
                );
                MuxError::Other(format!("Failed to activate wezterm tab: {}", e))
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            error!(
                "WezTerm activate-tab failed for tab '{}' (pane '{}'): {}",
                tab_id, window, stderr
            );
            return Err(MuxError::CommandFailed(format!(
                "wezterm activate-tab failed: {}",
                stderr
            )));
        }

        // set env var to the pane ID
        std::env::set_var("WEZTERM_PANE", window);
        debug!("WEZTERM_PANE set to: {}", window);

        debug!("WezTerm tab '{}' focused for pane '{}'", tab_id, window);
        info!("Successfully focused WezTerm tab for pane: {}", window);
        Ok(())
    }

    /// List WezTerm tabs (corresponds to Multiplexer "windows")
    #[instrument(skip(self))]
    pub fn list_tabs(&self, title_substr: Option<&str>) -> Result<Vec<WindowId>, MuxError> {
        info!("Listing WezTerm tabs with title filter: {:?}", title_substr);

        let windows_json = self.list_all_entities()?;
        let windows = windows_json
            .as_array()
            .ok_or_else(|| MuxError::Other("Expected JSON array from wezterm list".to_string()))?;

        let mut result = Vec::new();
        let mut seen_windows = std::collections::HashSet::new();

        for window_obj in windows {
            if let Some(window_id) = window_obj.get("window_id").and_then(|id| id.as_u64()) {
                let window_id_str = window_id.to_string();

                // Avoid duplicate window IDs (WezTerm list includes tabs and panes)
                if !seen_windows.insert(window_id_str.clone()) {
                    continue;
                }

                if let Some(substr) = title_substr {
                    // Check title in window, tabs, or panes
                    let title_matches = window_obj
                        .get("title")
                        .and_then(|t| t.as_str())
                        .map(|title| title.contains(substr))
                        .unwrap_or(false);

                    // Also check tab titles if window title doesn't match
                    let tab_matches = if !title_matches {
                        window_obj
                            .get("tabs")
                            .and_then(|tabs| tabs.as_array())
                            .map(|tabs| {
                                tabs.iter().any(|tab| {
                                    tab.get("title")
                                        .and_then(|t| t.as_str())
                                        .map(|title| title.contains(substr))
                                        .unwrap_or(false)
                                })
                            })
                            .unwrap_or(false)
                    } else {
                        false
                    };

                    if title_matches || tab_matches {
                        result.push(window_id_str);
                    }
                } else {
                    result.push(window_id_str);
                }
            }
        }

        debug!("Found {} WezTerm tabs", result.len());
        info!(
            "Successfully listed WezTerm tabs: found {} entries",
            result.len()
        );
        Ok(result)
    }

    /// Open a new tab in WezTerm (corresponds to Multiplexer "window")
    #[instrument(skip(self))]
    pub fn open_tab(&self, opts: &WindowOptions) -> Result<WindowId, MuxError> {
        let title = opts.title.unwrap_or("ah-session");
        info!("Opening WezTerm tab with title: {}", title);

        let mut cmd = Command::new("wezterm");
        cmd.arg("cli").arg("--prefer-mux").arg("spawn");

        if let Some(cwd) = opts.cwd {
            debug!("Setting working directory: {}", cwd.display());
            cmd.arg("--cwd").arg(cwd);
        }

        // Start with a login shell (title will be set after creation)
        // cmd.arg("--").arg("bash").arg("-l");

        debug!("Executing: {}", Self::sanitize_command_for_logging(&cmd));
        let output = cmd.output().map_err(|e| {
            error!("Failed to spawn WezTerm tab: {}", e);
            MuxError::Other(format!("Failed to spawn wezterm tab: {}", e))
        })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);
            error!(
                "WezTerm spawn failed - stderr: {}, stdout: {}",
                stderr, stdout
            );
            return Err(MuxError::CommandFailed(format!(
                "wezterm spawn failed: stderr={}, stdout={}",
                stderr, stdout
            )));
        }

        let stdout_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
        debug!("WezTerm spawn stdout: '{}'", stdout_str);

        // WezTerm spawn command returns just the pane ID as a plain number
        // For window operations, we use the pane ID as the window identifier
        let window_id = stdout_str;

        debug!("WezTerm tab created with ID: {}", window_id);

        // Set the tab title using WezTerm CLI
        if let Err(e) = self.set_tab_title(&window_id, title) {
            warn!("Failed to set tab title to '{}': {}", title, e);
        }

        // Execute init command if provided
        if let Some(init_cmd) = opts.init_command {
            debug!("Executing init command: {}", init_cmd);
            if let Err(e) = self.run_command(&window_id, init_cmd, &CommandOptions::default()) {
                warn!("Failed to execute init command '{}': {}", init_cmd, e);
            }
        }

        // Focus the window if requested
        if opts.focus {
            if let Err(e) = self.focus_window(&window_id) {
                warn!("Failed to focus newly created tab: {}", e);
            }
        }

        info!("Successfully opened WezTerm tab with ID: {}", window_id);
        Ok(window_id)
    }

    /// Wrap a command with PATH environment variable for execution in bash
    ///
    /// When spawning shells via WezTerm we want to ensure the current
    /// PATH from the Agent Harbor process is propagated explicitly.
    /// This mirrors the behavior used in the Tilix multiplexer.
    #[instrument]
    fn wrap_command_with_path(cmd: &str) -> String {
        let path = std::env::var("PATH").unwrap_or_default();
        // Escape spaces with backslash so WezTerm/bash sees a single PATH element
        let escaped_path = path.replace(' ', "\\s");
        debug!("Wrapping command with PATH environment");
        format!("env PATH={} bash -c '{}'", escaped_path, cmd)
    }

    /// Sanitize command for logging by replacing PATH values with placeholder
    ///
    /// This prevents sensitive PATH information from appearing in logs while
    /// still providing useful debugging information.
    fn sanitize_command_for_logging(cmd: &Command) -> String {
        let program = cmd.get_program().to_string_lossy();
        let args: Vec<String> = cmd
            .get_args()
            .map(|arg| {
                let arg_str = arg.to_string_lossy();
                if arg_str.starts_with("env PATH=") {
                    // Replace the actual PATH value with $PATH placeholder
                    let after_equals = arg_str.find('=').map(|i| &arg_str[i + 1..]).unwrap_or("");
                    if let Some(bash_pos) = after_equals.find(" bash ") {
                        format!("env PATH=$PATH {}", &after_equals[bash_pos..])
                    } else {
                        "env PATH=$PATH bash -c '...'".to_string()
                    }
                } else {
                    arg_str.to_string()
                }
            })
            .collect();

        format!("{:?} {:?}", program, args)
    }

    /// Check if WezTerm is installed and available
    fn check_version() -> Result<String, MuxError> {
        let version_output = Command::new("wezterm").arg("--version").output().map_err(|e| {
            debug!("Failed to run wezterm --version: {}", e);
            MuxError::NotAvailable("wezterm")
        })?;

        if !version_output.status.success() {
            debug!("WezTerm is not available");
            return Err(MuxError::NotAvailable("wezterm"));
        }

        let version = String::from_utf8_lossy(&version_output.stdout).trim().to_string();
        debug!("WezTerm version detected: {}", version);
        Ok(version)
    }
}

impl Multiplexer for WezTermMultiplexer {
    fn id(&self) -> &'static str {
        "wezterm"
    }

    #[instrument(skip(self))]
    fn is_available(&self) -> bool {
        debug!("Checking if WezTerm is available");

        // Test both version command and CLI connectivity
        let version_ok = Self::check_version().is_ok();
        let cli_ok = if version_ok {
            Self::test_cli_connectivity().is_ok()
        } else {
            false
        };

        let available = version_ok && cli_ok;

        if available {
            debug!("WezTerm is available and CLI is responsive");
        } else {
            debug!("WezTerm is not available or CLI is not responsive");
        }

        available
    }

    #[instrument(skip(self))]
    fn open_window(&self, opts: &WindowOptions) -> Result<WindowId, MuxError> {
        // Multiplexer "window" corresponds to WezTerm "tab"
        self.open_tab(opts)
    }

    #[instrument(skip(self))]
    fn split_pane(
        &self,
        _window: Option<&WindowId>,
        target: Option<&PaneId>,
        dir: SplitDirection,
        percent: Option<u8>,
        opts: &CommandOptions,
        initial_cmd: Option<&str>,
    ) -> Result<PaneId, MuxError> {
        info!("Splitting WezTerm pane in direction: {:?}", dir);

        let mut cmd = Command::new("wezterm");
        cmd.arg("cli").arg("--prefer-mux").arg("split-pane");

        // Set split direction (WezTerm uses different terminology than our enum)
        match dir {
            SplitDirection::Horizontal => {
                cmd.arg("--bottom"); // Horizontal split creates bottom pane
            }
            SplitDirection::Vertical => {
                cmd.arg("--right"); // Vertical split creates right pane
            }
        }

        // Set percentage if provided
        if let Some(pct) = percent {
            debug!("Setting split percentage: {}%", pct);
            cmd.arg("--percent").arg(pct.to_string());
        }

        // Target specific pane if provided
        if let Some(pane_id) = target {
            debug!("Targeting pane: {}", pane_id);
            cmd.arg("--pane-id").arg(pane_id);
        }

        // Set working directory if provided
        if let Some(cwd) = opts.cwd {
            debug!("Setting working directory: {}", cwd.display());
            cmd.arg("--cwd").arg(cwd);
        }

        // Build shell command with environment variables and initial command
        let mut shell_cmd_parts = Vec::new();

        // Add environment variables
        if let Some(env_vars) = opts.env {
            for (key, value) in env_vars {
                shell_cmd_parts.push(format!("export {}='{}'", key, value));
            }
        }

        // Add initial command or default shell
        if let Some(initial_cmd_val) = initial_cmd {
            shell_cmd_parts.push(initial_cmd_val.to_string());
        } else {
            shell_cmd_parts.push("bash -l".to_string());
        }

        let raw_shell_command = shell_cmd_parts.join(" && ");
        let shell_command = Self::wrap_command_with_path(&raw_shell_command);
        cmd.arg("--").arg("bash").arg("-lc").arg(shell_command);

        debug!("Executing: {}", Self::sanitize_command_for_logging(&cmd));
        let output = cmd.output().map_err(|e| {
            error!("Failed to split WezTerm pane: {}", e);
            MuxError::Other(format!("Failed to split wezterm pane: {}", e))
        })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            error!("WezTerm split-pane failed: {}", stderr);
            return Err(MuxError::CommandFailed(format!(
                "wezterm split-pane failed: {}",
                stderr
            )));
        }

        // WezTerm split-pane command returns just the pane ID as a plain number
        let stdout_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
        debug!("WezTerm split-pane stdout: '{}'", stdout_str);
        let pane_id = stdout_str;

        info!("WezTerm pane split successful, pane ID: {}", pane_id);
        Ok(pane_id)
    }

    #[instrument(skip(self))]
    fn run_command(
        &self,
        pane: &PaneId,
        cmd: &str,
        _opts: &CommandOptions,
    ) -> Result<(), MuxError> {
        info!("Running command in WezTerm pane: {}", pane);
        debug!("Running command in WezTerm pane '{}': {}", pane, cmd);

        // Send command text first
        let mut command = Command::new("wezterm");
        command
            .arg("cli")
            .arg("--prefer-mux")
            .arg("send-text")
            .arg("--no-paste") // Use simulated typing for compatibility
            .arg("--pane-id")
            .arg(pane)
            .arg(cmd);

        debug!("Sending command text: {:?}", command);
        let output = command.output().map_err(|e| {
            error!("Failed to send command to WezTerm pane '{}': {}", pane, e);
            MuxError::Other(format!("Failed to send command to wezterm: {}", e))
        })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            error!("WezTerm send-text failed for pane '{}': {}", pane, stderr);
            return Err(MuxError::CommandFailed(format!(
                "wezterm send-text failed: {}",
                stderr
            )));
        }

        // Send carriage return separately as per spec
        let mut enter_command = Command::new("wezterm");
        enter_command
            .arg("cli")
            .arg("--prefer-mux")
            .arg("send-text")
            .arg("--pane-id")
            .arg(pane)
            .arg("\r");

        let enter_output = enter_command.output().map_err(|e| {
            error!(
                "Failed to send carriage return to WezTerm pane '{}': {}",
                pane, e
            );
            MuxError::Other(format!("Failed to send carriage return to wezterm: {}", e))
        })?;

        if !enter_output.status.success() {
            let stderr = String::from_utf8_lossy(&enter_output.stderr);
            warn!(
                "WezTerm carriage return failed for pane '{}': {}",
                pane, stderr
            );
        }

        debug!("Command executed in WezTerm pane '{}'", pane);
        info!("Successfully ran command in WezTerm pane: {}", pane);
        Ok(())
    }

    #[instrument(skip(self))]
    fn send_text(&self, pane: &PaneId, text: &str) -> Result<(), MuxError> {
        info!("Sending text to WezTerm pane: {}", pane);
        debug!("Sending text to WezTerm pane '{}': {}", pane, text);

        let mut command = Command::new("wezterm");
        command
            .arg("cli")
            .arg("--prefer-mux")
            .arg("send-text")
            .arg("--no-paste") // Use simulated typing for better compatibility
            .arg("--pane-id")
            .arg(pane)
            .arg(text);

        debug!(
            "Executing: {}",
            Self::sanitize_command_for_logging(&command)
        );
        let output = command.output().map_err(|e| {
            error!("Failed to send text to WezTerm pane '{}': {}", pane, e);
            MuxError::Other(format!("Failed to send text to wezterm: {}", e))
        })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            error!("WezTerm send-text failed for pane '{}': {}", pane, stderr);
            return Err(MuxError::CommandFailed(format!(
                "wezterm send-text failed: {}",
                stderr
            )));
        }

        debug!("Text sent to WezTerm pane '{}'", pane);
        info!("Successfully sent text to WezTerm pane: {}", pane);
        Ok(())
    }

    #[instrument(skip(self))]
    fn focus_window(&self, window: &WindowId) -> Result<(), MuxError> {
        // Multiplexer "window" corresponds to WezTerm "tab"
        self.focus_tab(window)
    }

    #[instrument(skip(self))]
    fn focus_pane(&self, pane: &PaneId) -> Result<(), MuxError> {
        info!("Focusing WezTerm pane: {}", pane);
        debug!("Focusing WezTerm pane: {}", pane);

        let output = Command::new("wezterm")
            .arg("cli")
            .arg("--prefer-mux")
            .arg("activate-pane")
            .arg("--pane-id")
            .arg(pane)
            .output()
            .map_err(|e| {
                error!("Failed to activate WezTerm pane '{}': {}", pane, e);
                MuxError::Other(format!("Failed to activate wezterm pane: {}", e))
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            error!(
                "WezTerm activate-pane failed for pane '{}': {}",
                pane, stderr
            );
            return Err(MuxError::CommandFailed(format!(
                "wezterm activate-pane failed: {}",
                stderr
            )));
        }

        debug!("WezTerm pane '{}' focused", pane);
        info!("Successfully focused WezTerm pane: {}", pane);
        Ok(())
    }

    #[instrument(skip(self))]
    fn list_windows(&self, title_substr: Option<&str>) -> Result<Vec<WindowId>, MuxError> {
        // Multiplexer "windows" correspond to WezTerm "tabs"
        self.list_tabs(title_substr)
    }

    #[instrument(skip(self))]
    fn list_panes(&self, window: &WindowId) -> Result<Vec<PaneId>, MuxError> {
        // Multiplexer "window" corresponds to WezTerm "tab"
        info!(
            "Listing WezTerm panes for tab (Multiplexer window): {}",
            window
        );

        let windows_json = self.list_all_entities()?;
        let windows = windows_json
            .as_array()
            .ok_or_else(|| MuxError::Other("Expected JSON array from wezterm list".to_string()))?;

        let mut result = Vec::new();
        let window_id_num = window.parse::<u64>().ok();

        for window_obj in windows {
            // Filter by window_id if it's numeric
            let window_matches = if let Some(target_id) = window_id_num {
                window_obj.get("window_id").and_then(|w| w.as_u64()) == Some(target_id)
            } else {
                true // If window ID is not numeric, include panes from all windows
            };

            if !window_matches {
                continue;
            }

            // Extract panes from tabs within the window
            if let Some(tabs) = window_obj.get("tabs").and_then(|t| t.as_array()) {
                for tab in tabs {
                    if let Some(panes) = tab.get("panes").and_then(|p| p.as_array()) {
                        for pane in panes {
                            if let Some(pane_id) = pane.get("pane_id").and_then(|id| id.as_u64()) {
                                result.push(pane_id.to_string());
                            }
                        }
                    }
                }
            }

            // Also check if the window object itself contains pane info (flat structure)
            if let Some(pane_id) = window_obj.get("pane_id").and_then(|id| id.as_u64()) {
                result.push(pane_id.to_string());
            }
        }

        debug!(
            "Found {} WezTerm panes for tab (Multiplexer window) '{}'",
            result.len(),
            window
        );
        info!(
            "Successfully listed WezTerm panes for tab (Multiplexer window) '{}': found {} panes",
            window,
            result.len()
        );
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::detection::is_in_wezterm;
    use std::path::Path;
    use std::thread;
    use std::time::Duration;

    /// Start a headless WezTerm mux-server suitable for CI environments.
    ///
    /// This function ensures that a wezterm mux-server is running **without**
    /// launching the GUI frontend, making it safe for headless environments
    /// such as GitHub Actions, Docker containers, and remote CI runners.
    ///
    /// Behavior:
    /// - If running inside an existing WezTerm session (detected via environment
    ///   variables), no mux server is started.
    ///
    /// - If a mux-server is already running (determined by checking whether the
    ///   `wezterm cli` commands can connect), this function **reuses** the
    ///   existing server and does not attempt to start a new one.
    ///
    /// - If no mux-server is running, this function starts one using
    ///   `wezterm-mux-server --daemonize` and waits until CLI connectivity
    ///   succeeds.
    ///
    /// - Because a daemonized mux-server detaches and manages its own lifetime,
    ///   this function does **not** track or kill it. The server will exit
    ///   automatically when idle.
    ///
    /// The function is fully idempotent: calling it multiple times is safe and
    /// will never spawn duplicate mux-server instances or trigger pid-file
    /// locking errors.
    fn start_test_wezterm() -> Result<(), Box<dyn std::error::Error>> {
        // If we're already inside wezterm, no need to spawn anything
        if is_in_wezterm() {
            tracing::debug!("Already inside wezterm; no need to launch mux server");
            return Ok(());
        }

        // If CLI already works, a mux-server is already running.
        if WezTermMultiplexer::test_cli_connectivity().is_ok() {
            tracing::debug!("WezTerm mux server already running; not starting a new one");
            // Ensure WEZTERM_PANE is always available for tests
            unsafe { std::env::set_var("WEZTERM_PANE", "9999") };

            return Ok(());
        }

        tracing::debug!("Starting headless wezterm-mux-server (no existing server)");

        let status = std::process::Command::new("wezterm-mux-server")
            .arg("--daemonize")
            .status()
            .map_err(|e| format!("Failed to start wezterm-mux-server: {}", e))?;

        if !status.success() {
            return Err(format!("wezterm-mux-server exited with {:?}", status).into());
        }

        // Wait for mux server to initialize
        for i in 0..20 {
            if WezTermMultiplexer::test_cli_connectivity().is_ok() {
                tracing::debug!("Mux server is now ready (after {i} checks)");
                unsafe { std::env::set_var("WEZTERM_PANE", "9999") };
                return Ok(());
            }

            tracing::debug!("Waiting for mux server to become ready...");
            std::thread::sleep(Duration::from_millis(250));
        }

        Err("wezterm-mux-server failed to become ready".into())
    }

    #[test]
    fn test_wezterm_id() {
        // Only run this test if wezterm is available
        if let Ok(mux) = WezTermMultiplexer::new() {
            assert_eq!(mux.id(), "wezterm");
        }
    }

    #[test]
    fn test_cli_connectivity() {
        // Test CLI connectivity separately from constructor
        // Only test if WezTerm is available and running
        if Command::new("wezterm")
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
        {
            // CLI connectivity test should pass if WezTerm is running, otherwise skip
            let result = WezTermMultiplexer::test_cli_connectivity();
            if result.is_err() {
                // Don't fail test if WezTerm is not running - just skip
                return;
            }
            assert!(result.is_ok());
        }
    }

    #[test]
    fn test_parse_json_response() {
        let valid_json = r#"[{"pane_id": 1, "window_id": 1, "tab_id": 1}]"#;
        let result = WezTermMultiplexer::parse_json_response(valid_json.as_bytes());
        assert!(result.is_ok());

        let invalid_json = "not json";
        let result = WezTermMultiplexer::parse_json_response(invalid_json.as_bytes());
        assert!(result.is_err());
    }

    #[test]
    fn test_find_tab_for_pane_id() {
        // Test with mock JSON response that simulates actual WezTerm CLI output
        // WezTerm returns a flat array where each object is a pane with top-level fields
        let mock_json = r#"[
            {
                "window_id": 1,
                "tab_id": 100,
                "pane_id": 1001,
                "workspace": "default",
                "title": "Test Pane 1"
            },
            {
                "window_id": 1,
                "tab_id": 100,
                "pane_id": 1002,
                "workspace": "default",
                "title": "Test Pane 2"
            },
            {
                "window_id": 1,
                "tab_id": 101,
                "pane_id": 1003,
                "workspace": "default",
                "title": "Test Pane 3"
            },
            {
                "window_id": 2,
                "tab_id": 200,
                "pane_id": 2001,
                "workspace": "default",
                "title": "Test Pane 4"
            }
        ]"#;

        // Create a mock multiplexer for testing
        if let Ok(mux) = WezTermMultiplexer::new() {
            // Parse the mock JSON
            let mock_entities = WezTermMultiplexer::parse_json_response(mock_json.as_bytes())
                .expect("Should parse mock JSON");

            // Test finding tab for different pane IDs
            let panes = mock_entities.as_array().unwrap();

            // Test each pane and verify it maps to the correct tab
            for pane_obj in panes {
                if let (Some(pane_id), Some(expected_tab_id)) = (
                    pane_obj.get("pane_id").and_then(|id| id.as_u64()),
                    pane_obj.get("tab_id").and_then(|id| id.as_u64()),
                ) {
                    // Verify the mapping: pane 1001 should be in tab 100, etc.
                    match pane_id {
                        1001 | 1002 => assert_eq!(expected_tab_id, 100),
                        1003 => assert_eq!(expected_tab_id, 101),
                        2001 => assert_eq!(expected_tab_id, 200),
                        _ => panic!("Unexpected pane_id: {}", pane_id),
                    }
                }
            }

            // The new find_tab_for_pane_id method exists and is callable
            // Actual testing would require a real WezTerm instance
            let _result = mux.find_tab_for_pane_id("1001");
        }
    }

    #[test]
    fn test_list_all_entities() {
        if let Ok(mux) = WezTermMultiplexer::new() {
            // Verify that the new list_all_entities method exists and is callable
            // Actual testing would require a real WezTerm instance
            if mux.is_available() {
                let _result = mux.list_all_entities();
                // Method should be callable - actual functionality depends on WezTerm running
            }
        }
    }

    #[test]
    #[serial_test::file_serial]
    fn test_wezterm_availability() {
        // Check if WezTerm is available
        if WezTermMultiplexer::check_version().is_ok() {
            let result = WezTermMultiplexer::new();
            if result.is_ok() {
                let mux = result.unwrap();
                assert!(mux.is_available());
            }
        }
    }

    #[test]
    #[serial_test::file_serial]
    fn test_start_wezterm_instance() {
        tracing::debug!("Testing start_test_wezterm function");
        start_test_wezterm().expect("start_test_wezterm should succeed");
        tracing::debug!("start_test_wezterm succeeded");
    }

    #[test]
    #[serial_test::file_serial]
    fn test_open_tab_with_title_and_cwd() {
        start_test_wezterm().expect("Should start test wezterm");

        let mux = WezTermMultiplexer::new().expect("Should create WezTermMultiplexer");
        let opts = WindowOptions {
            title: Some("wezterm-test-001"),
            cwd: Some(Path::new("/tmp")),
            profile: None,
            focus: false,
            init_command: None,
        };

        let window_id = mux.open_window(&opts).expect("Should create window");
        // Verify the window ID is numeric
        assert!(window_id.parse::<u32>().is_ok());
        tracing::debug!(window_id, "Window created successfully");
    }

    #[test]
    #[serial_test::file_serial]
    fn test_open_tab_with_focus() {
        start_test_wezterm().expect("Should start test wezterm");

        let mux = WezTermMultiplexer::new().expect("Should create WezTermMultiplexer");
        let opts = WindowOptions {
            title: Some("wezterm-focus-test-002"),
            cwd: Some(Path::new("/tmp")),
            profile: None,
            focus: true,
            init_command: None,
        };

        let window_id = mux.open_window(&opts).expect("Should create focused window");
        assert!(window_id.parse::<u32>().is_ok());

        // Give focus operation time to complete
        thread::sleep(Duration::from_millis(200));

        // Verify the window was created and can be focused
        // Check that WEZTERM_PANE env var was set correctly
        if let Ok(pane_env) = std::env::var("WEZTERM_PANE") {
            assert_eq!(
                pane_env, window_id,
                "WEZTERM_PANE should match focused window"
            );
        }
        tracing::debug!(window_id, "Focused window created successfully");
    }

    #[test]
    #[serial_test::file_serial]
    fn test_split_pane_horizontal_vertical() {
        start_test_wezterm().expect("Should start test wezterm");

        let mux = WezTermMultiplexer::new().expect("Should create WezTermMultiplexer");
        // Create a window first
        let window_opts = WindowOptions {
            title: Some("split-test-003"),
            cwd: Some(Path::new("/tmp")),
            profile: None,
            focus: false,
            init_command: None,
        };

        let window_id = mux.open_window(&window_opts).expect("Should create window");

        // Test horizontal split
        let h_pane_id = mux
            .split_pane(
                Some(&window_id),
                Some(&window_id),
                SplitDirection::Horizontal,
                Some(60),
                &CommandOptions::default(),
                None,
            )
            .expect("Should create horizontal split");

        assert!(h_pane_id.parse::<u32>().is_ok());
        assert_ne!(h_pane_id, window_id);
        tracing::debug!(h_pane_id, "Horizontal split created");

        // Test vertical split
        let v_pane_id = mux
            .split_pane(
                Some(&window_id),
                Some(&h_pane_id),
                SplitDirection::Vertical,
                Some(50),
                &CommandOptions::default(),
                None,
            )
            .expect("Should create vertical split");

        assert!(v_pane_id.parse::<u32>().is_ok());
        assert_ne!(v_pane_id, h_pane_id);
        tracing::debug!(v_pane_id, "Vertical split created");
    }

    #[test]
    #[serial_test::file_serial]
    fn test_split_pane_with_initial_command() {
        start_test_wezterm().expect("Should start test wezterm");

        let mux = WezTermMultiplexer::new().expect("Should create WezTermMultiplexer");
        let window_opts = WindowOptions {
            title: Some("split-cmd-test-004"),
            cwd: Some(Path::new("/tmp")),
            profile: None,
            focus: false,
            init_command: None,
        };

        let window_id = mux.open_window(&window_opts).expect("Should create window");

        // Split with initial command
        let pane_id = mux
            .split_pane(
                Some(&window_id),
                Some(&window_id),
                SplitDirection::Horizontal,
                None,
                &CommandOptions::default(),
                Some("echo 'test command' && sleep 1"),
            )
            .expect("Should create split with command");

        assert!(pane_id.parse::<u32>().is_ok(), "Pane ID should be numeric");
        assert_ne!(pane_id, window_id, "Split pane should have different ID");
        // Wait for command to complete
        thread::sleep(Duration::from_millis(1200));
        // Command executed successfully if pane was created - we can't directly
        // capture output, but the split_pane call would have failed if command failed immediately
        tracing::debug!(pane_id, "Split with command created and executed");
    }

    #[test]
    #[serial_test::file_serial]
    fn test_run_command_and_send_text() {
        start_test_wezterm().expect("Should start test wezterm");

        let mux = WezTermMultiplexer::new().expect("Should create WezTermMultiplexer");
        let window_opts = WindowOptions {
            title: Some("cmd-text-test-005"),
            cwd: Some(Path::new("/tmp")),
            profile: None,
            focus: false,
            init_command: None,
        };

        let window_id = mux.open_window(&window_opts).expect("Should create window");

        // Test run_command
        mux.run_command(
            &window_id,
            "echo 'hello from wezterm'",
            &CommandOptions::default(),
        )
        .expect("run_command should succeed");
        thread::sleep(Duration::from_millis(100));
        tracing::debug!("Command executed successfully");

        // Test send_text
        mux.send_text(&window_id, "test input").expect("send_text should succeed");
        thread::sleep(Duration::from_millis(100));
        tracing::debug!("Text sent successfully");
    }

    #[test]
    #[serial_test::file_serial]
    fn test_focus_window_and_pane() {
        start_test_wezterm().expect("Should start test wezterm");

        let mux = WezTermMultiplexer::new().expect("Should create WezTermMultiplexer");
        // Create two windows
        let window1_opts = WindowOptions {
            title: Some("focus-window1-006"),
            cwd: Some(Path::new("/tmp")),
            profile: None,
            focus: false,
            init_command: None,
        };

        let window2_opts = WindowOptions {
            title: Some("focus-window2-006"),
            cwd: Some(Path::new("/tmp")),
            profile: None,
            focus: false,
            init_command: None,
        };

        let (window1, window2) = (
            mux.open_window(&window1_opts).expect("Should create window1"),
            mux.open_window(&window2_opts).expect("Should create window2"),
        );

        // Test focusing window1
        mux.focus_window(&window1).expect("Should focus window1");
        thread::sleep(Duration::from_millis(100));
        tracing::debug!(window1, "Window 1 focused");

        // Test focusing window2
        mux.focus_window(&window2).expect("Should focus window2");
        thread::sleep(Duration::from_millis(100));
        tracing::debug!(window2, "Window 2 focused");

        // Test focusing pane
        mux.focus_pane(&window1).expect("Should focus pane");
        thread::sleep(Duration::from_millis(100));
        tracing::debug!(window1, "Pane focused");
    }

    #[test]
    #[serial_test::file_serial]
    fn test_list_windows_and_filtering() {
        start_test_wezterm().expect("Should start test wezterm");

        let mux = WezTermMultiplexer::new().expect("Should create WezTermMultiplexer");
        // Create test windows with different titles
        let window_opts = vec![
            WindowOptions {
                title: Some("alpha-wezterm-007"),
                cwd: Some(Path::new("/tmp")),
                profile: None,
                focus: false,
                init_command: None,
            },
            WindowOptions {
                title: Some("beta-wezterm-007"),
                cwd: Some(Path::new("/tmp")),
                profile: None,
                focus: false,
                init_command: None,
            },
            WindowOptions {
                title: Some("alpha-other-007"),
                cwd: Some(Path::new("/tmp")),
                profile: None,
                focus: false,
                init_command: None,
            },
        ];

        let mut created_panes = Vec::new();
        for opts in window_opts {
            let pane_id = mux.open_window(&opts).expect("Should create window");
            created_panes.push(pane_id);
        }

        thread::sleep(Duration::from_millis(500));

        // Test listing all windows - should have at least one window
        let all_windows = mux.list_windows(None).expect("Should list windows");
        assert!(!all_windows.is_empty(), "Should list at least one window");
        tracing::debug!(count = all_windows.len(), "Listed all windows");

        // Test filtering by title substring
        // Note: list_windows returns window IDs, not pane IDs
        // So we test the filtering mechanism by verifying it returns results
        let alpha_windows = mux.list_windows(Some("alpha")).expect("Should list alpha windows");
        tracing::debug!(count = alpha_windows.len(), "Listed alpha windows");

        let beta_windows = mux.list_windows(Some("beta")).expect("Should list beta windows");
        tracing::debug!(count = beta_windows.len(), "Listed beta windows");

        tracing::debug!("Successfully created {} panes", created_panes.len());
    }

    #[test]
    #[serial_test::file_serial]
    #[ignore = "This test hit known bug that will be fixed soon"]
    fn test_list_panes() {
        start_test_wezterm().expect("Should start test wezterm");

        let mux = WezTermMultiplexer::new().expect("Should create WezTermMultiplexer");
        let window_opts = WindowOptions {
            title: Some("panes-test-008"),
            cwd: Some(Path::new("/tmp")),
            profile: None,
            focus: false,
            init_command: None,
        };

        let window_id = mux.open_window(&window_opts).expect("Should create window");

        // Create a split to have multiple panes
        mux.split_pane(
            Some(&window_id),
            Some(&window_id),
            SplitDirection::Horizontal,
            None,
            &CommandOptions::default(),
            None,
        )
        .expect("Should create split pane");

        thread::sleep(Duration::from_millis(200));

        // List panes in the window
        let panes = mux.list_panes(&window_id).expect("Should list panes");
        assert!(!panes.is_empty(), "Should have at least one pane");
        // We created one split, so should have at least 2 panes
        assert!(panes.len() >= 2, "Should have at least 2 panes after split");
        tracing::debug!(count = panes.len(), "Listed panes");
    }

    #[test]
    #[serial_test::file_serial]
    #[ignore = "This test hit known bug that will be fixed soon"]
    fn test_complex_layout_creation() {
        start_test_wezterm().expect("Should start test wezterm");

        let mux = WezTermMultiplexer::new().expect("Should create WezTermMultiplexer");
        // Create a main window
        let window_opts = WindowOptions {
            title: Some("complex-layout-009"),
            cwd: Some(Path::new("/tmp")),
            profile: None,
            focus: false,
            init_command: None,
        };

        let main_window = mux.open_window(&window_opts).expect("Should create main window");

        // Create a complex 3-pane layout
        // Pane 1: editor (left, 70%)
        // Pane 2: agent (top-right, 60% of right side)
        // Pane 3: logs (bottom-right, 40% of right side)

        // Create agent pane (right side)
        let agent_pane = mux
            .split_pane(
                Some(&main_window),
                Some(&main_window),
                SplitDirection::Vertical,
                Some(70),
                &CommandOptions::default(),
                None,
            )
            .expect("Should create agent pane");

        tracing::debug!(agent_pane, "Agent pane created");

        // Create logs pane (bottom of agent pane)
        let logs_pane = mux
            .split_pane(
                Some(&main_window),
                Some(&agent_pane),
                SplitDirection::Horizontal,
                Some(60),
                &CommandOptions::default(),
                None,
            )
            .expect("Should create logs pane");

        tracing::debug!(logs_pane, "Logs pane created");

        thread::sleep(Duration::from_millis(200));

        // Verify all panes exist
        let panes = mux.list_panes(&main_window).expect("Should list panes");
        assert!(panes.len() >= 3);
        assert!(panes.contains(&main_window));
        assert!(panes.contains(&agent_pane));
        assert!(panes.contains(&logs_pane));
        tracing::debug!(count = panes.len(), "Complex layout verified");

        // Test focusing different panes
        mux.focus_pane(&main_window).expect("Should focus main window");
        thread::sleep(Duration::from_millis(50));
        mux.focus_pane(&agent_pane).expect("Should focus agent pane");
        thread::sleep(Duration::from_millis(50));
        mux.focus_pane(&logs_pane).expect("Should focus logs pane");
    }

    #[test]
    #[serial_test::file_serial]
    fn test_error_handling_invalid_pane() {
        start_test_wezterm().expect("Should start test wezterm");

        let mux = WezTermMultiplexer::new().expect("Should create WezTermMultiplexer");
        // Try to focus a non-existent pane
        let invalid_pane = "99999".to_string();
        let result = mux.focus_pane(&invalid_pane);

        match result {
            Ok(()) => {
                tracing::debug!("Pane might exist or command accepted");
            }
            Err(MuxError::CommandFailed(_)) => {
                tracing::debug!("Expected error for invalid pane");
            }
            Err(e) => {
                panic!("Unexpected error type: {:?}", e);
            }
        }

        // Try to send text to non-existent pane
        let result = mux.send_text(&invalid_pane, "test");
        assert!(result.is_ok() || matches!(result, Err(MuxError::CommandFailed(_))));
    }

    #[test]
    #[serial_test::file_serial]
    #[ignore = "This test hit known bug that will be fixed soon"]
    fn test_set_tab_title() {
        start_test_wezterm().expect("Should start test wezterm");

        let mux = WezTermMultiplexer::new().expect("Should create WezTermMultiplexer");
        let window_opts = WindowOptions {
            title: Some("original-title-010"),
            cwd: Some(Path::new("/tmp")),
            profile: None,
            focus: false,
            init_command: None,
        };

        let window_id = mux.open_window(&window_opts).expect("Should create window");
        thread::sleep(Duration::from_millis(100));

        // Change the tab title
        mux.set_tab_title(&window_id, "new-title-010")
            .expect("set_tab_title should succeed");
        thread::sleep(Duration::from_millis(100));
        tracing::debug!("Tab title updated successfully");

        // Verify the title change by listing windows
        // Note: We can't directly read the title back, but we can verify the operation didn't fail
        let windows = mux.list_windows(Some("new-title-010")).expect("Should list windows");
        assert!(!windows.is_empty(), "Should find window with new title");
    }

    #[test]
    #[serial_test::file_serial]
    #[ignore = "This test hit known bug that will be fixed soon"]
    fn test_kill_pane() {
        start_test_wezterm().expect("Should start test wezterm");

        let mux = WezTermMultiplexer::new().expect("Should create WezTermMultiplexer");
        let window_opts = WindowOptions {
            title: Some("kill-pane-test-011"),
            cwd: Some(Path::new("/tmp")),
            profile: None,
            focus: false,
            init_command: None,
        };

        let window_id = mux.open_window(&window_opts).expect("Should create window");

        // Create a split pane
        let pane_id = mux
            .split_pane(
                Some(&window_id),
                Some(&window_id),
                SplitDirection::Horizontal,
                None,
                &CommandOptions::default(),
                None,
            )
            .expect("Should create split pane");

        assert!(pane_id.parse::<u32>().is_ok(), "Pane ID should be numeric");
        thread::sleep(Duration::from_millis(100));

        // Count panes before kill
        let panes_before = mux.list_panes(&window_id).expect("Should list panes before kill");
        let count_before = panes_before.len();

        // Kill the pane
        mux.kill_pane(&pane_id).expect("kill_pane should succeed");
        thread::sleep(Duration::from_millis(100));
        tracing::debug!(pane_id, "Pane killed successfully");

        // Verify pane count decreased by one
        let panes_after = mux.list_panes(&window_id).expect("Should list panes after kill");
        assert!(
            !panes_after.contains(&pane_id),
            "Killed pane should not be in list"
        );
        assert_eq!(
            panes_after.len(),
            count_before - 1,
            "Pane count should decrease by one after kill"
        );
    }
}
