// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! GNU Screen multiplexer implementation
//!
//! GNU Screen is a classic terminal multiplexer with support for sessions,
//! windows, and regions. It uses the `-X` command interface for automation.

use std::process::Command;

use ah_mux_core::*;
use tracing::{debug, error, info, instrument, warn};

use crate::MuxError;

/// GNU Screen multiplexer implementation
#[derive(Debug)]
pub struct ScreenMultiplexer;

impl ScreenMultiplexer {
    /// Create a new GNU Screen multiplexer instance
    #[instrument]
    pub fn new() -> Result<Self, MuxError> {
        debug!("Initializing GNU Screen multiplexer");

        // Check if screen is available
        let output = Command::new("screen").arg("--version").output().map_err(|e| {
            error!("Failed to run screen --version: {}", e);
            MuxError::Other(format!("Failed to run screen --version: {}", e))
        })?;

        if !output.status.success() {
            warn!("GNU Screen is not available");
            return Err(MuxError::NotAvailable("screen"));
        }

        info!("GNU Screen multiplexer initialized successfully");
        Ok(Self)
    }
}

impl Multiplexer for ScreenMultiplexer {
    fn id(&self) -> &'static str {
        "screen"
    }

    #[instrument]
    fn is_available(&self) -> bool {
        debug!("Checking if GNU Screen is available");

        let available = Command::new("screen")
            .arg("--version")
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false);

        if available {
            debug!("GNU Screen is available");
        } else {
            debug!("GNU Screen is not available");
        }

        available
    }

    #[instrument(skip(self))]
    fn open_window(&self, opts: &WindowOptions) -> Result<WindowId, MuxError> {
        let session_name = opts.title.unwrap_or("ah-session");
        info!(
            "Opening GNU Screen window with session name: {}",
            session_name
        );

        // Check if session already exists
        if self.list_windows(Some(session_name)).is_ok_and(|windows| !windows.is_empty()) {
            info!("GNU Screen session '{}' already exists", session_name);
            return Ok(session_name.to_string());
        }

        debug!("Creating new GNU Screen session: {}", session_name);

        // Start a detached session
        let mut cmd = Command::new("screen");
        cmd.arg("-dmS").arg(session_name);

        // Add command if specified
        cmd.arg("bash").arg("-lc").arg("echo 'Agent Harbor session started'");

        let output = cmd.output().map_err(|e| {
            error!(
                "Failed to start GNU Screen session '{}': {}",
                session_name, e
            );
            MuxError::Other(format!("Failed to start screen session: {}", e))
        })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            error!(
                "GNU Screen session '{}' creation failed: {}",
                session_name, stderr
            );
            return Err(MuxError::CommandFailed(format!(
                "screen session creation failed: {}",
                stderr
            )));
        }

        info!("GNU Screen session '{}' created successfully", session_name);
        Ok(session_name.to_string())
    }

    #[instrument(skip(self))]
    fn split_pane(
        &self,
        window: Option<&WindowId>,
        _target: Option<&PaneId>,
        dir: SplitDirection,
        _percent: Option<u8>,
        opts: &CommandOptions,
        initial_cmd: Option<&str>,
    ) -> Result<PaneId, MuxError> {
        debug!("Splitting GNU Screen pane in direction: {:?}", dir);

        // Screen uses regions, not panes in the same way as modern multiplexers
        // We need to split the current region and create a new window in it

        // Get the session name from window parameter or use default
        let session_name = window.map(|w| w.as_str()).unwrap_or("default");

        // First split the region
        let split_cmd = match dir {
            SplitDirection::Horizontal => "split",
            SplitDirection::Vertical => "split -v",
        };

        let _session = match window {
            Some(w) => w,
            None => {
                error!("No window specified for GNU Screen split operation");
                return Err(MuxError::NotFound);
            }
        };

        debug!("Executing GNU Screen split command: {}", split_cmd);

        let mut split_command = Command::new("screen");
        split_command.arg("-S").arg(session_name).arg("-X").arg(split_cmd);

        let output = split_command.output().map_err(|e| {
            error!("Failed to split GNU Screen region: {}", e);
            MuxError::Other(format!("Failed to split screen region: {}", e))
        })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            error!("GNU Screen split failed: {}", stderr);
            return Err(MuxError::CommandFailed(format!(
                "screen split failed: {}",
                stderr
            )));
        }

        // Focus the new region
        let focus_dir = match dir {
            SplitDirection::Horizontal => "focus down",
            SplitDirection::Vertical => "focus right",
        };

        debug!("Focusing new GNU Screen region with command: {}", focus_dir);

        let mut focus_command = Command::new("screen");
        focus_command.arg("-S").arg(session_name).arg("-X").arg(focus_dir);

        let output = focus_command.output().map_err(|e| {
            error!("Failed to focus GNU Screen region: {}", e);
            MuxError::Other(format!("Failed to focus screen region: {}", e))
        })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            error!("GNU Screen focus failed: {}", stderr);
            return Err(MuxError::CommandFailed(format!(
                "screen focus failed: {}",
                stderr
            )));
        }

        // Create a new window in the new region
        debug!("Creating new GNU Screen window in the new region");
        let mut screen_command = Command::new("screen");
        screen_command.arg("-S").arg(session_name).arg("-X").arg("screen");

        if let Some(cwd) = opts.cwd {
            // Screen doesn't support setting CWD directly, so we use bash -lc with cd
            let cmd_str = if let Some(cmd) = initial_cmd {
                format!("cd '{}' && {}", cwd.display(), cmd)
            } else {
                format!("cd '{}' && bash", cwd.display())
            };
            debug!("Setting working directory and command: {}", cmd_str);
            screen_command.arg("bash").arg("-lc").arg(cmd_str);
        } else if let Some(cmd) = initial_cmd {
            debug!("Setting initial command: {}", cmd);
            screen_command.arg("bash").arg("-lc").arg(cmd);
        } else {
            screen_command.arg("bash");
        }

        let output = screen_command.output().map_err(|e| {
            error!("Failed to create GNU Screen window: {}", e);
            MuxError::Other(format!("Failed to create screen window: {}", e))
        })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            error!("GNU Screen window creation failed: {}", stderr);
            return Err(MuxError::CommandFailed(format!(
                "screen window creation failed: {}",
                stderr
            )));
        }

        // Screen doesn't provide pane IDs in a programmatic way
        // We'll return a placeholder ID
        let pane_id = format!("screen-region-{}", session_name);
        info!("GNU Screen pane split successful, pane ID: {}", pane_id);
        Ok(pane_id)
    }

    #[instrument(skip(self))]
    fn run_command(
        &self,
        pane: &PaneId,
        cmd: &str,
        _opts: &CommandOptions,
    ) -> Result<(), MuxError> {
        debug!("Running command in GNU Screen pane '{}': {}", pane, cmd);

        // Extract session name from pane ID
        let session_name = pane.strip_prefix("screen-region-").ok_or_else(|| {
            error!("Invalid GNU Screen pane ID format: {}", pane);
            MuxError::NotFound
        })?;

        // Send the command as text input to the focused window
        let stuff_command = format!("{}\n", cmd);

        let mut command = Command::new("screen");
        command.arg("-S").arg(session_name).arg("-X").arg("stuff").arg(stuff_command);

        let output = command.output().map_err(|e| {
            error!(
                "Failed to send command to GNU Screen session '{}': {}",
                session_name, e
            );
            MuxError::Other(format!("Failed to send command to screen: {}", e))
        })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            error!(
                "GNU Screen stuff command failed for session '{}': {}",
                session_name, stderr
            );
            return Err(MuxError::CommandFailed(format!(
                "screen stuff failed: {}",
                stderr
            )));
        }

        info!(
            "Command executed successfully in GNU Screen pane '{}'",
            pane
        );
        Ok(())
    }

    #[instrument(skip(self))]
    fn send_text(&self, pane: &PaneId, text: &str) -> Result<(), MuxError> {
        debug!("Sending text to GNU Screen pane '{}': {}", pane, text);

        // Extract session name from pane ID
        let session_name = pane.strip_prefix("screen-region-").ok_or_else(|| {
            error!("Invalid GNU Screen pane ID format: {}", pane);
            MuxError::NotFound
        })?;

        // Use screen's stuff command to send text
        let mut command = Command::new("screen");
        command.arg("-S").arg(session_name).arg("-X").arg("stuff").arg(text);

        let output = command.output().map_err(|e| {
            error!(
                "Failed to send text to GNU Screen session '{}': {}",
                session_name, e
            );
            MuxError::Other(format!("Failed to send text to screen: {}", e))
        })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            error!(
                "GNU Screen text sending failed for session '{}': {}",
                session_name, stderr
            );
            return Err(MuxError::CommandFailed(format!(
                "screen stuff failed: {}",
                stderr
            )));
        }

        info!("Text sent successfully to GNU Screen pane '{}'", pane);
        Ok(())
    }

    #[instrument(skip(self))]
    fn focus_window(&self, window: &WindowId) -> Result<(), MuxError> {
        info!("Focusing GNU Screen window: {}", window);

        // Reattach to the session (this brings it to the foreground)
        let output = Command::new("screen").arg("-r").arg(window).output().map_err(|e| {
            error!(
                "Failed to reattach to GNU Screen session '{}': {}",
                window, e
            );
            MuxError::Other(format!("Failed to reattach to screen session: {}", e))
        })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            error!(
                "GNU Screen reattach failed for session '{}': {}",
                window, stderr
            );
            return Err(MuxError::CommandFailed(format!(
                "screen reattach failed: {}",
                stderr
            )));
        }

        info!("GNU Screen window '{}' focused successfully", window);
        Ok(())
    }

    #[instrument(skip(self))]
    fn focus_pane(&self, _pane: &PaneId) -> Result<(), MuxError> {
        warn!("GNU Screen does not support direct pane focusing via CLI");
        // Screen doesn't have direct pane focusing via CLI
        // Users can navigate regions manually
        Err(MuxError::NotAvailable("screen"))
    }

    #[instrument(skip(self))]
    fn list_windows(&self, title_substr: Option<&str>) -> Result<Vec<WindowId>, MuxError> {
        debug!(
            "Listing GNU Screen sessions with title filter: {:?}",
            title_substr
        );

        // Screen doesn't have a direct way to list sessions from CLI
        // We can try to use `screen -ls` and parse the output
        let output = Command::new("screen").arg("-ls").output().map_err(|e| {
            error!("Failed to list GNU Screen sessions: {}", e);
            MuxError::Other(format!("Failed to list screen sessions: {}", e))
        })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            error!("GNU Screen -ls command failed: {}", stderr);
            return Err(MuxError::CommandFailed(format!(
                "screen -ls failed: {}",
                stderr
            )));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut sessions = Vec::new();

        for line in stdout.lines() {
            // Parse session names from output like:
            // "There is a screen on:"
            // "        12345.ah-session        (Detached)"
            if line.contains('.') {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if let Some(session_part) = parts.last() {
                    if let Some(session_name) = session_part.split('.').nth(1) {
                        if let Some(substr) = title_substr {
                            if session_name.contains(substr) {
                                sessions.push(session_name.to_string());
                            }
                        } else {
                            sessions.push(session_name.to_string());
                        }
                    }
                }
            }
        }

        info!("Found {} GNU Screen sessions", sessions.len());
        Ok(sessions)
    }

    #[instrument(skip(self))]
    fn list_panes(&self, _window: &WindowId) -> Result<Vec<PaneId>, MuxError> {
        warn!("GNU Screen does not expose pane/region listing via CLI");
        // Screen doesn't expose pane/region listing via CLI
        Err(MuxError::NotAvailable("screen"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_screen_id() {
        // Only run if screen is available
        if let Ok(mux) = ScreenMultiplexer::new() {
            assert_eq!(mux.id(), "screen");
        }
    }

    #[test]
    fn test_screen_focus_pane_not_available() {
        // Only run if screen is available
        if let Ok(mux) = ScreenMultiplexer::new() {
            let result = mux.focus_pane(&"dummy".to_string());
            assert!(matches!(result, Err(MuxError::NotAvailable("screen"))));
        }
    }

    #[test]
    fn test_screen_list_panes_not_available() {
        // Only run if screen is available
        if let Ok(mux) = ScreenMultiplexer::new() {
            let result = mux.list_panes(&"dummy".to_string());
            assert!(matches!(result, Err(MuxError::NotAvailable("screen"))));
        }
    }
}
