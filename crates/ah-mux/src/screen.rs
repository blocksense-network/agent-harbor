// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! GNU Screen multiplexer implementation
//!
//! GNU Screen is a classic terminal multiplexer with support for sessions,
//! windows, and regions. It uses the `-X` command interface for automation.

use std::process::Command;

use ah_mux_core::*;

use crate::MuxError;

/// GNU Screen multiplexer implementation
pub struct ScreenMultiplexer;

impl ScreenMultiplexer {
    /// Create a new GNU Screen multiplexer instance
    pub fn new() -> Result<Self, MuxError> {
        // Check if screen is available
        let output = Command::new("screen")
            .arg("--version")
            .output()
            .map_err(|e| MuxError::Other(format!("Failed to run screen --version: {}", e)))?;

        if !output.status.success() {
            return Err(MuxError::NotAvailable("screen"));
        }

        Ok(Self)
    }
}

impl Multiplexer for ScreenMultiplexer {
    fn id(&self) -> &'static str {
        "screen"
    }

    fn is_available(&self) -> bool {
        Command::new("screen")
            .arg("--version")
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    }

    fn open_window(&self, opts: &WindowOptions) -> Result<WindowId, MuxError> {
        let session_name = opts.title.unwrap_or("ah-session");

        // Check if session already exists
        if self
            .list_windows(Some(session_name))
            .map_or(false, |windows| !windows.is_empty())
        {
            return Ok(session_name.to_string());
        }

        // Start a detached session
        let mut cmd = Command::new("screen");
        cmd.arg("-dmS").arg(session_name);

        // Add command if specified
        cmd.arg("bash").arg("-lc").arg("echo 'Agent Harbor session started'");

        let output = cmd
            .output()
            .map_err(|e| MuxError::Other(format!("Failed to start screen session: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(MuxError::CommandFailed(format!(
                "screen session creation failed: {}",
                stderr
            )));
        }

        Ok(session_name.to_string())
    }

    fn split_pane(
        &self,
        window: &WindowId,
        _target: Option<&PaneId>,
        dir: SplitDirection,
        percent: Option<u8>,
        opts: &CommandOptions,
        initial_cmd: Option<&str>,
    ) -> Result<PaneId, MuxError> {
        // Screen uses regions, not panes in the same way as modern multiplexers
        // We need to split the current region and create a new window in it

        // First split the region
        let split_cmd = match dir {
            SplitDirection::Horizontal => "split",
            SplitDirection::Vertical => "split -v",
        };

        let mut split_command = Command::new("screen");
        split_command.arg("-S").arg(window).arg("-X").arg(split_cmd);

        let output = split_command
            .output()
            .map_err(|e| MuxError::Other(format!("Failed to split screen region: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
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

        let mut focus_command = Command::new("screen");
        focus_command.arg("-S").arg(window).arg("-X").arg(focus_dir);

        let output = focus_command
            .output()
            .map_err(|e| MuxError::Other(format!("Failed to focus screen region: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(MuxError::CommandFailed(format!(
                "screen focus failed: {}",
                stderr
            )));
        }

        // Create a new window in the new region
        let mut screen_command = Command::new("screen");
        screen_command.arg("-S").arg(window).arg("-X").arg("screen");

        if let Some(cwd) = opts.cwd {
            // Screen doesn't support setting CWD directly, so we use bash -lc with cd
            let cmd_str = if let Some(cmd) = initial_cmd {
                format!("cd '{}' && {}", cwd.display(), cmd)
            } else {
                format!("cd '{}' && bash", cwd.display())
            };
            screen_command.arg("bash").arg("-lc").arg(cmd_str);
        } else if let Some(cmd) = initial_cmd {
            screen_command.arg("bash").arg("-lc").arg(cmd);
        } else {
            screen_command.arg("bash");
        }

        let output = screen_command
            .output()
            .map_err(|e| MuxError::Other(format!("Failed to create screen window: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(MuxError::CommandFailed(format!(
                "screen window creation failed: {}",
                stderr
            )));
        }

        // Screen doesn't provide pane IDs in a programmatic way
        // We'll return a placeholder ID
        Ok(format!("screen-region-{}", window))
    }

    fn run_command(&self, pane: &PaneId, cmd: &str, opts: &CommandOptions) -> Result<(), MuxError> {
        // Extract session name from pane ID
        let session_name = pane.strip_prefix("screen-region-").ok_or_else(|| MuxError::NotFound)?;

        // Send the command as text input to the focused window
        let mut stuff_command = format!("{}\n", cmd);

        let mut command = Command::new("screen");
        command.arg("-S").arg(session_name).arg("-X").arg("stuff").arg(stuff_command);

        let output = command
            .output()
            .map_err(|e| MuxError::Other(format!("Failed to send command to screen: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(MuxError::CommandFailed(format!(
                "screen stuff failed: {}",
                stderr
            )));
        }

        Ok(())
    }

    fn send_text(&self, pane: &PaneId, text: &str) -> Result<(), MuxError> {
        // Extract session name from pane ID
        let session_name = pane.strip_prefix("screen-region-").ok_or_else(|| MuxError::NotFound)?;

        // Use screen's stuff command to send text
        let mut command = Command::new("screen");
        command.arg("-S").arg(session_name).arg("-X").arg("stuff").arg(text);

        let output = command
            .output()
            .map_err(|e| MuxError::Other(format!("Failed to send text to screen: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(MuxError::CommandFailed(format!(
                "screen stuff failed: {}",
                stderr
            )));
        }

        Ok(())
    }

    fn focus_window(&self, window: &WindowId) -> Result<(), MuxError> {
        // Reattach to the session (this brings it to the foreground)
        let output =
            Command::new("screen").arg("-r").arg(window).output().map_err(|e| {
                MuxError::Other(format!("Failed to reattach to screen session: {}", e))
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(MuxError::CommandFailed(format!(
                "screen reattach failed: {}",
                stderr
            )));
        }

        Ok(())
    }

    fn focus_pane(&self, _pane: &PaneId) -> Result<(), MuxError> {
        // Screen doesn't have direct pane focusing via CLI
        // Users can navigate regions manually
        Err(MuxError::NotAvailable("screen"))
    }

    fn list_windows(&self, title_substr: Option<&str>) -> Result<Vec<WindowId>, MuxError> {
        // Screen doesn't have a direct way to list sessions from CLI
        // We can try to use `screen -ls` and parse the output
        let output = Command::new("screen")
            .arg("-ls")
            .output()
            .map_err(|e| MuxError::Other(format!("Failed to list screen sessions: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
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

        Ok(sessions)
    }

    fn list_panes(&self, _window: &WindowId) -> Result<Vec<PaneId>, MuxError> {
        // Screen doesn't expose pane/region listing via CLI
        Err(MuxError::NotAvailable("screen"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_screen_id() {
        let mux = ScreenMultiplexer::new().unwrap();
        assert_eq!(mux.id(), "screen");
    }

    #[test]
    fn test_screen_focus_pane_not_available() {
        let mux = ScreenMultiplexer::new().unwrap();
        let result = mux.focus_pane("dummy");
        assert!(matches!(result, Err(MuxError::NotAvailable("screen"))));
    }

    #[test]
    fn test_screen_list_panes_not_available() {
        let mux = ScreenMultiplexer::new().unwrap();
        let result = mux.list_panes("dummy");
        assert!(matches!(result, Err(MuxError::NotAvailable("screen"))));
    }
}
