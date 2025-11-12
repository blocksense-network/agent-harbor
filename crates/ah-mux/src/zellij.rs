// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Zellij multiplexer implementation
//!
//! Zellij is a terminal workspace that uses KDL layout files for defining
//! complex pane arrangements and CLI commands for session management.

use std::process::Command;

use ah_mux_core::*;

use crate::MuxError;

/// Zellij multiplexer implementation
pub struct ZellijMultiplexer;

impl ZellijMultiplexer {
    /// Create a new Zellij multiplexer instance
    pub fn new() -> Result<Self, MuxError> {
        // Check if zellij is available
        let output = Command::new("zellij")
            .arg("--version")
            .output()
            .map_err(|e| MuxError::Other(format!("Failed to run zellij --version: {}", e)))?;

        if !output.status.success() {
            return Err(MuxError::NotAvailable("zellij"));
        }

        Ok(Self)
    }
}

impl Multiplexer for ZellijMultiplexer {
    fn id(&self) -> &'static str {
        "zellij"
    }

    fn is_available(&self) -> bool {
        Command::new("zellij")
            .arg("--version")
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    }

    fn open_window(&self, opts: &WindowOptions) -> Result<WindowId, MuxError> {
        // Zellij doesn't have a direct "open window" command like tmux.
        // Instead, we create a new session with a layout.
        // For simplicity, we'll create a session and return its name as the window ID.

        let session_name = opts.title.unwrap_or("ah-session");

        // Check if session already exists
        if self.list_windows(Some(session_name)).is_ok_and(|windows| !windows.is_empty()) {
            return Ok(session_name.to_string());
        }

        // Create a new session with a basic layout
        let mut cmd = Command::new("zellij");
        cmd.arg("--session").arg(session_name);

        if let Some(cwd) = opts.cwd {
            cmd.arg("--cwd").arg(cwd);
        }

        // Start with a default layout (just a single pane)
        cmd.arg("--layout").arg("default");

        let output = cmd
            .output()
            .map_err(|e| MuxError::Other(format!("Failed to start zellij session: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(MuxError::CommandFailed(format!(
                "zellij session creation failed: {}",
                stderr
            )));
        }

        Ok(session_name.to_string())
    }

    fn split_pane(
        &self,
        window: Option<&WindowId>,
        _target: Option<&PaneId>,
        _dir: SplitDirection,
        _percent: Option<u8>,
        opts: &CommandOptions,
        initial_cmd: Option<&str>,
    ) -> Result<PaneId, MuxError> {
        // Zellij doesn't have direct split commands like tmux.
        // We need to use layouts or the `zellij run` command.
        // For now, we'll use `zellij run` to create a new pane.

        let mut cmd = Command::new("zellij");
        if let Some(session) = window {
            cmd.arg("--session").arg(session);
        }
        cmd.arg("run");

        if let Some(cwd) = opts.cwd {
            cmd.arg("--cwd").arg(cwd);
        }

        // Set up environment variables
        if let Some(env) = opts.env {
            for (key, value) in env {
                cmd.env(key, value);
            }
        }

        // Add the command to run
        if let Some(cmd_str) = initial_cmd {
            cmd.arg("--").arg(cmd_str);
        } else {
            // Default to shell if no command provided
            cmd.arg("--").arg("sh");
        }

        let output = cmd
            .output()
            .map_err(|e| MuxError::Other(format!("Failed to run zellij command: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(MuxError::CommandFailed(format!(
                "zellij run failed: {}",
                stderr
            )));
        }

        // Zellij doesn't return pane IDs in a parseable way from CLI.
        // We'll return a placeholder ID that won't be usable for targeting.
        // In practice, AH workflows should use layouts instead of individual splits.
        Ok(format!("zellij-pane-{:?}", window))
    }

    fn run_command(&self, pane: &PaneId, cmd: &str, opts: &CommandOptions) -> Result<(), MuxError> {
        // Zellij doesn't have a way to run commands in existing panes via CLI.
        // The best we can do is create a new pane with the command.
        // This is a limitation of Zellij's CLI interface.

        let session_name = pane.strip_prefix("zellij-pane-").ok_or(MuxError::NotFound)?;

        let mut zellij_cmd = Command::new("zellij");
        zellij_cmd.arg("--session").arg(session_name);
        zellij_cmd.arg("run");

        if let Some(cwd) = opts.cwd {
            zellij_cmd.arg("--cwd").arg(cwd);
        }

        if let Some(env) = opts.env {
            for (key, value) in env {
                zellij_cmd.env(key, value);
            }
        }

        zellij_cmd.arg("--").arg(cmd);

        let output = zellij_cmd
            .output()
            .map_err(|e| MuxError::Other(format!("Failed to run zellij command: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(MuxError::CommandFailed(format!(
                "zellij run failed: {}",
                stderr
            )));
        }

        Ok(())
    }

    fn send_text(&self, _pane: &PaneId, _text: &str) -> Result<(), MuxError> {
        // Zellij does not expose a stable "send keys" CLI API.
        // According to the documentation, direct text injection is not supported.
        Err(MuxError::NotAvailable("zellij"))
    }

    fn focus_window(&self, window: &WindowId) -> Result<(), MuxError> {
        // Attach to the session (this brings it to the foreground)
        let output =
            Command::new("zellij").arg("attach").arg(window).output().map_err(|e| {
                MuxError::Other(format!("Failed to attach to zellij session: {}", e))
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(MuxError::CommandFailed(format!(
                "zellij attach failed: {}",
                stderr
            )));
        }

        Ok(())
    }

    fn focus_pane(&self, _pane: &PaneId) -> Result<(), MuxError> {
        // Zellij doesn't have direct pane focusing via CLI.
        // Panes are focused through the layout or user interaction.
        Err(MuxError::NotAvailable("zellij"))
    }

    fn list_windows(&self, title_substr: Option<&str>) -> Result<Vec<WindowId>, MuxError> {
        let output = Command::new("zellij")
            .arg("list-sessions")
            .output()
            .map_err(|e| MuxError::Other(format!("Failed to list zellij sessions: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(MuxError::CommandFailed(format!(
                "zellij list-sessions failed: {}",
                stderr
            )));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut sessions = Vec::new();

        for line in stdout.lines() {
            // Parse session names from the output
            // Format is typically: "session_name EXITED|ATTACHED|DETACHED"
            let parts: Vec<&str> = line.split_whitespace().collect();
            if let Some(session_name) = parts.first() {
                if let Some(substr) = title_substr {
                    if session_name.contains(substr) {
                        sessions.push(session_name.to_string());
                    }
                } else {
                    sessions.push(session_name.to_string());
                }
            }
        }

        Ok(sessions)
    }

    fn list_panes(&self, _window: &WindowId) -> Result<Vec<PaneId>, MuxError> {
        // Zellij doesn't expose pane listing via CLI.
        // This is a limitation of the CLI interface.
        Err(MuxError::NotAvailable("zellij"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_zellij_id() {
        let mux = ZellijMultiplexer::new().unwrap();
        assert_eq!(mux.id(), "zellij");
    }

    #[test]
    fn test_zellij_send_text_not_available() {
        let mux = ZellijMultiplexer::new().unwrap();
        let result = mux.send_text("dummy", "text");
        assert!(matches!(result, Err(MuxError::NotAvailable("zellij"))));
    }

    #[test]
    fn test_zellij_focus_pane_not_available() {
        let mux = ZellijMultiplexer::new().unwrap();
        let result = mux.focus_pane("dummy");
        assert!(matches!(result, Err(MuxError::NotAvailable("zellij"))));
    }

    #[test]
    fn test_zellij_list_panes_not_available() {
        let mux = ZellijMultiplexer::new().unwrap();
        let result = mux.list_panes("dummy");
        assert!(matches!(result, Err(MuxError::NotAvailable("zellij"))));
    }
}
