// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! WezTerm multiplexer implementation
//!
//! WezTerm is a modern, GPU-accelerated terminal emulator with excellent
//! CLI automation support via the `wezterm cli` command.

use std::process::Command;

use ah_mux_core::*;
use tracing::{debug, error, info, instrument, warn};

use crate::MuxError;

/// WezTerm multiplexer implementation
#[derive(Debug)]
pub struct WezTermMultiplexer;

impl WezTermMultiplexer {
    /// Create a new WezTerm multiplexer instance
    #[instrument]
    pub fn new() -> Result<Self, MuxError> {
        debug!("Initializing WezTerm multiplexer");

        // Check if wezterm is available
        let output = Command::new("wezterm").arg("--version").output().map_err(|e| {
            error!("Failed to run wezterm --version: {}", e);
            MuxError::Other(format!("Failed to run wezterm --version: {}", e))
        })?;

        if !output.status.success() {
            warn!("WezTerm is not available");
            return Err(MuxError::NotAvailable("wezterm"));
        }

        info!("WezTerm multiplexer initialized successfully");
        Ok(Self)
    }
}

impl Multiplexer for WezTermMultiplexer {
    fn id(&self) -> &'static str {
        "wezterm"
    }

    #[instrument]
    fn is_available(&self) -> bool {
        debug!("Checking if WezTerm is available");

        let available = Command::new("wezterm")
            .arg("--version")
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false);

        if available {
            debug!("WezTerm is available");
        } else {
            debug!("WezTerm is not available");
        }

        available
    }

    #[instrument(skip(self))]
    fn open_window(&self, opts: &WindowOptions) -> Result<WindowId, MuxError> {
        let title = opts.title.unwrap_or("ah-session");
        info!("Opening WezTerm window with title: {}", title);

        let mut cmd = Command::new("wezterm");
        cmd.arg("cli").arg("spawn").arg("--new-window");

        if let Some(cwd) = opts.cwd {
            debug!("Setting working directory: {}", cwd.display());
            cmd.arg("--cwd").arg(cwd);
        }

        // Set title using OSC escape sequence
        let command = format!("printf '\\e]2;{}\\a'; bash", title);
        cmd.arg("--").arg("bash").arg("-lc").arg(command);

        let output = cmd.output().map_err(|e| {
            error!("Failed to spawn WezTerm window: {}", e);
            MuxError::Other(format!("Failed to spawn wezterm window: {}", e))
        })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            error!("WezTerm spawn failed: {}", stderr);
            return Err(MuxError::CommandFailed(format!(
                "wezterm spawn failed: {}",
                stderr
            )));
        }

        // Extract window ID from output if possible
        let stdout = String::from_utf8_lossy(&output.stdout);
        let window_id = stdout.trim().parse::<u64>().unwrap_or(0);
        let window_id_str = format!("{}", window_id);

        info!("WezTerm window created with ID: {}", window_id_str);
        Ok(window_id_str)
    }

    #[instrument(skip(self))]
    fn split_pane(
        &self,
        _window: Option<&WindowId>,
        _target: Option<&PaneId>,
        dir: SplitDirection,
        percent: Option<u8>,
        opts: &CommandOptions,
        initial_cmd: Option<&str>,
    ) -> Result<PaneId, MuxError> {
        debug!("Splitting WezTerm pane in direction: {:?}", dir);

        let mut cmd = Command::new("wezterm");
        cmd.arg("cli");

        match dir {
            SplitDirection::Horizontal => {
                cmd.arg("split-pane").arg("--bottom");
            }
            SplitDirection::Vertical => {
                cmd.arg("split-pane").arg("--right");
            }
        }

        if let Some(pct) = percent {
            debug!("Setting split percentage: {}%", pct);
            cmd.arg("--percent").arg(pct.to_string());
        }

        // Add command to run in the new pane
        if let Some(cwd) = opts.cwd {
            let cmd_str = if let Some(initial_cmd_val) = initial_cmd {
                format!("cd '{}' && {}", cwd.display(), initial_cmd_val)
            } else {
                format!("cd '{}' && bash", cwd.display())
            };
            debug!("Setting working directory and command: {}", cmd_str);
            cmd.arg("--").arg("bash").arg("-lc").arg(cmd_str);
        } else if let Some(initial_cmd_val) = initial_cmd {
            debug!("Setting initial command: {}", initial_cmd_val);
            cmd.arg("--").arg("bash").arg("-lc").arg(initial_cmd_val);
        } else {
            cmd.arg("--").arg("bash");
        }

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

        // Extract pane ID from output
        let stdout = String::from_utf8_lossy(&output.stdout);
        let pane_id = stdout.trim().parse::<u64>().unwrap_or(0);
        let pane_id_str = format!("{}", pane_id);

        info!("WezTerm pane split successful, pane ID: {}", pane_id_str);
        Ok(pane_id_str)
    }

    #[instrument(skip(self))]
    fn run_command(
        &self,
        pane: &PaneId,
        cmd: &str,
        _opts: &CommandOptions,
    ) -> Result<(), MuxError> {
        debug!("Running command in WezTerm pane '{}': {}", pane, cmd);

        let mut command = Command::new("wezterm");
        command
            .arg("cli")
            .arg("send-text")
            .arg("--no-paste")
            .arg("--pane-id")
            .arg(pane)
            .arg("--")
            .arg(format!("{}\n", cmd));

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

        info!("Command executed successfully in WezTerm pane '{}'", pane);
        Ok(())
    }

    #[instrument(skip(self))]
    fn send_text(&self, pane: &PaneId, text: &str) -> Result<(), MuxError> {
        debug!("Sending text to WezTerm pane '{}': {}", pane, text);

        let mut command = Command::new("wezterm");
        command
            .arg("cli")
            .arg("send-text")
            .arg("--no-paste")
            .arg("--pane-id")
            .arg(pane)
            .arg("--")
            .arg(text);

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

        info!("Text sent successfully to WezTerm pane '{}'", pane);
        Ok(())
    }

    #[instrument(skip(self))]
    fn focus_window(&self, window: &WindowId) -> Result<(), MuxError> {
        info!("Focusing WezTerm window: {}", window);

        let output = Command::new("wezterm")
            .arg("cli")
            .arg("activate")
            .arg("--window-id")
            .arg(window)
            .output()
            .map_err(|e| {
                error!("Failed to activate WezTerm window '{}': {}", window, e);
                MuxError::Other(format!("Failed to activate wezterm window: {}", e))
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            error!(
                "WezTerm activate failed for window '{}': {}",
                window, stderr
            );
            return Err(MuxError::CommandFailed(format!(
                "wezterm activate failed: {}",
                stderr
            )));
        }

        info!("WezTerm window '{}' focused successfully", window);
        Ok(())
    }

    #[instrument(skip(self))]
    fn focus_pane(&self, pane: &PaneId) -> Result<(), MuxError> {
        info!("Focusing WezTerm pane: {}", pane);

        let output = Command::new("wezterm")
            .arg("cli")
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

        info!("WezTerm pane '{}' focused successfully", pane);
        Ok(())
    }

    #[instrument(skip(self))]
    fn list_windows(&self, title_substr: Option<&str>) -> Result<Vec<WindowId>, MuxError> {
        debug!(
            "Listing WezTerm windows with title filter: {:?}",
            title_substr
        );

        let output = Command::new("wezterm")
            .arg("cli")
            .arg("list")
            .arg("--format")
            .arg("json")
            .output()
            .map_err(|e| {
                error!("Failed to list WezTerm windows: {}", e);
                MuxError::Other(format!("Failed to list wezterm windows: {}", e))
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            error!("WezTerm list failed: {}", stderr);
            return Err(MuxError::CommandFailed(format!(
                "wezterm list failed: {}",
                stderr
            )));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);

        // Parse JSON output
        let windows: Vec<serde_json::Value> = serde_json::from_str(&stdout).map_err(|e| {
            error!("Failed to parse WezTerm list output: {}", e);
            MuxError::Other(format!("Failed to parse wezterm list output: {}", e))
        })?;

        let mut result = Vec::new();

        for window in windows {
            if let Some(window_id) = window.get("window_id") {
                if let Some(window_id_str) = window_id.as_u64() {
                    if let Some(substr) = title_substr {
                        // Check if title matches
                        if let Some(title) = window.get("title").and_then(|t| t.as_str()) {
                            if title.contains(substr) {
                                result.push(format!("{}", window_id_str));
                            }
                        }
                    } else {
                        result.push(format!("{}", window_id_str));
                    }
                }
            }
        }

        info!("Found {} WezTerm windows", result.len());
        Ok(result)
    }

    #[instrument(skip(self))]
    fn list_panes(&self, window: &WindowId) -> Result<Vec<PaneId>, MuxError> {
        debug!("Listing WezTerm panes for window: {}", window);

        let output = Command::new("wezterm")
            .arg("cli")
            .arg("list")
            .arg("--format")
            .arg("json")
            .output()
            .map_err(|e| {
                error!("Failed to list WezTerm panes: {}", e);
                MuxError::Other(format!("Failed to list wezterm panes: {}", e))
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            error!("WezTerm list failed: {}", stderr);
            return Err(MuxError::CommandFailed(format!(
                "wezterm list failed: {}",
                stderr
            )));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);

        // Parse JSON output
        let panes: Vec<serde_json::Value> = serde_json::from_str(&stdout).map_err(|e| {
            error!("Failed to parse WezTerm list output: {}", e);
            MuxError::Other(format!("Failed to parse wezterm list output: {}", e))
        })?;

        let mut result = Vec::new();

        for pane in panes {
            // Filter by window_id if provided
            let window_matches = if let Ok(window_id_num) = window.parse::<u64>() {
                pane.get("window_id").and_then(|w| w.as_u64()) == Some(window_id_num)
            } else {
                true // If window ID is not a number, include all panes
            };

            if window_matches {
                if let Some(pane_id) = pane.get("pane_id") {
                    if let Some(pane_id_str) = pane_id.as_u64() {
                        result.push(format!("{}", pane_id_str));
                    }
                }
            }
        }

        info!(
            "Found {} WezTerm panes for window '{}'",
            result.len(),
            window
        );
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wezterm_id() {
        // Only run this test if wezterm is available
        if let Ok(mux) = WezTermMultiplexer::new() {
            assert_eq!(mux.id(), "wezterm");
        }
    }
}
