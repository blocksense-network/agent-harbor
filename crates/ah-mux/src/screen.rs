// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! GNU Screen multiplexer implementation
//!
//! GNU Screen is a classic terminal multiplexer with support for sessions,
//! windows, and regions. It uses the `-X` command interface for automation.

use std::env;
use std::process::Command;
use std::sync::Once;

use ah_mux_core::*;
use regex::Regex;
use tracing::{debug, error, info, instrument, warn};

use crate::MuxError;

static INIT: Once = Once::new();

/// GNU Screen multiplexer implementation
#[derive(Debug)]
pub struct ScreenMultiplexer;

impl ScreenMultiplexer {
    /// Create a new GNU Screen multiplexer instance
    #[instrument(fields(component = "ah-mux", operation = "screen_init"))]
    pub fn new() -> Result<Self, MuxError> {
        debug!("Checking GNU Screen availability");

        // Check if screen is available
        let output = Command::new("screen").arg("--version").output().map_err(|e| {
            error!(error = %e, component = "ah-mux", "Failed to run screen --version");
            MuxError::Other(format!("Failed to run screen --version: {}", e))
        })?;

        if !output.status.success() {
            warn!(
                component = "ah-mux",
                "GNU Screen is not available or failed version check"
            );
            return Err(MuxError::NotAvailable("screen"));
        }

        info!(
            component = "ah-mux",
            "GNU Screen multiplexer initialized successfully"
        );
        Ok(Self)
    }

    /// Initialize the agent-harbor layout in the current Screen session
    /// This is called automatically only once during the first task layout creation
    #[instrument(fields(component = "ah-mux", operation = "init_layout"))]
    fn init_agent_harbor_screen_layout() -> Result<(), MuxError> {
        let session_name = std::env::var("STY").map_err(|_| {
            error!(
                component = "ah-mux",
                "Not running inside a GNU Screen session"
            );
            MuxError::Other("Not running inside a GNU Screen session".to_string())
        })?;

        debug!(session_name = %session_name, "Initializing agent-harbor layout");

        // Create agent-harbor layout
        // screen -S <session_name> -X layout new agent-harbor
        let output = Command::new("screen")
            .arg("-S")
            .arg(&session_name)
            .arg("-X")
            .arg("layout")
            .arg("new")
            .arg("agent-harbor")
            .output()
            .map_err(|e| {
                error!(error = %e, session_name = %session_name, "Failed to create agent-harbor layout");
                MuxError::Other(format!("Failed to create agent-harbor layout: {}", e))
            })?;

        if !output.status.success() {
            debug!(session_name = %session_name, "Layout creation returned non-success status (might already exist)");
        }

        // Select window 0 (original agent-harbor-tui)
        // screen -S <session_name> -X select 0
        let output = Command::new("screen")
            .arg("-S")
            .arg(&session_name)
            .arg("-X")
            .arg("select")
            .arg("0")
            .output()
            .map_err(|e| {
                error!(error = %e, session_name = %session_name, "Failed to select window 0");
                MuxError::Other(format!("Failed to select window 0: {}", e))
            })?;

        if !output.status.success() {
            debug!(session_name = %session_name, "Window 0 selection returned non-success status");
        }

        info!(session_name = %session_name, "Agent-harbor layout initialized successfully");
        Ok(())
    }
}

impl Multiplexer for ScreenMultiplexer {
    fn id(&self) -> &'static str {
        "screen"
    }

    #[instrument(
        skip(self),
        fields(component = "ah-mux", operation = "check_availability")
    )]
    fn is_available(&self) -> bool {
        let available = Command::new("screen")
            .arg("--version")
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false);

        debug!(available = %available, "GNU Screen availability check");
        available
    }

    #[instrument(skip(self, opts), fields(component = "ah-mux", operation = "open_window", window_title = opts.title))]
    fn open_window(&self, opts: &WindowOptions) -> Result<WindowId, MuxError> {
        // Initialize the agent-harbor layout once
        INIT.call_once(|| {
            let _ = Self::init_agent_harbor_screen_layout();
        });

        let session_name = std::env::var("STY").unwrap_or_default();
        let task_name = opts.title.unwrap_or("ah-task");

        debug!(session_name = %session_name, task_name = %task_name, "Opening window in Screen session");

        // Check if session already exists
        if self.list_windows(Some(task_name)).is_ok_and(|windows| !windows.is_empty()) {
            info!(task_name = %task_name, "Window already exists, reusing");
            return Ok(task_name.to_string());
        }

        // -- Start new task layout --
        debug!(session_name = %session_name, task_name = %task_name, "Creating new task layout");

        // screen -S <session_name> -X layout new split
        let _split_layout_cmd = Command::new("screen")
            .arg("-S")
            .arg(&session_name)
            .arg("-X")
            .arg("layout")
            .arg("new")
            .arg(task_name)
            .output();

        // screen -S <session_name> -X screen
        let _screen_cmd = Command::new("screen")
            .arg("-S")
            .arg(&session_name)
            .arg("-X")
            .arg("screen")
            .output();
        // -- Finish new task layout --

        info!(session_name = %session_name, task_name = %task_name, "Window opened successfully");
        Ok(task_name.to_string())
    }

    #[instrument(skip(self, window, opts), fields(component = "ah-mux", operation = "split_pane", direction = ?dir))]
    fn split_pane(
        &self,
        window: Option<&WindowId>,
        _target: Option<&PaneId>,
        dir: SplitDirection,
        _percent: Option<u8>,
        opts: &CommandOptions,
        initial_cmd: Option<&str>,
    ) -> Result<PaneId, MuxError> {
        // Screen uses regions, not panes in the same way as modern multiplexers
        // We need to split the current region and create a new window in it

        let session_name = std::env::var("STY").unwrap_or_default();
        match window {
            Some(w) => {
                let _select_window_cmd = Command::new("screen")
                    .arg("-S")
                    .arg(&session_name)
                    .arg("-X")
                    .arg("select")
                    .arg(w)
                    .output();
            }
            None => {
                debug!("No window specified for GNU Screen split operation");
            }
        };

        debug!(session_name = %session_name, direction = ?dir, "Splitting pane in Screen session");

        // If we split vertically (side-by-side, 'split -v'), the new region is to the right.
        // If we split horizontally (top/bottom, 'split'), the new region is down.
        let focus_dir = match dir {
            SplitDirection::Horizontal => "down",
            SplitDirection::Vertical => "right",
        };

        // -- Start split --
        debug!(direction = ?dir, focus_dir = %focus_dir, "Executing split command");

        // screen -S <session_name> -X split (or split -v for vertical)
        let mut split_cmd = Command::new("screen");
        split_cmd.arg("-S").arg(&session_name).arg("-X").arg("split");
        if let SplitDirection::Vertical = dir {
            split_cmd.arg("-v");
        }
        let _split_result = split_cmd.output();

        // screen -S <session_name> -X focus <direction>
        let _focus_cmd = Command::new("screen")
            .arg("-S")
            .arg(&session_name)
            .arg("-X")
            .arg("focus")
            .arg(focus_dir)
            .output();

        // screen -S <session_name> -X screen
        let _screen_cmd = Command::new("screen")
            .arg("-S")
            .arg(&session_name)
            .arg("-X")
            .arg("screen")
            .output();
        // -- Finish split --

        let mut stuff_command = Command::new("screen");
        stuff_command.arg("-S").arg(&session_name).arg("-X").arg("stuff");

        if let Some(cwd) = opts.cwd {
            debug!(cwd = %cwd.display(), "Setting working directory for pane");

            // Screen doesn't support setting CWD directly, so we use bash -lc with cd
            let cmd_str = if let Some(cmd) = initial_cmd {
                format!("cd '{}' && {}", cwd.display(), cmd)
            } else {
                format!("cd '{}' && bash", cwd.display())
            };

            // For Screen's stuff command, we need to properly escape the string.
            // Screen uses $ for special escape sequences. We need to escape:
            // - Single quotes: use '\'' (end quote, escaped quote, start quote)
            // - Dollar signs: use \$
            // The entire command is wrapped in single quotes for Screen
            let escaped_cmd = cmd_str
                .replace('\\', "\\\\") // Escape backslashes first
                .replace('$', "\\$") // Escape dollar signs
                .replace('\'', "'\\''"); // Escape single quotes using shell-safe method

            // Build the command: bash -lc '<escaped_cmd>' followed by newline
            // The newline (\n) tells Screen to press Enter after typing
            let full_cmd = format!("bash -lc '{}'\n", escaped_cmd);

            stuff_command.arg(full_cmd);
        } else if let Some(cmd) = initial_cmd {
            // Escape for Screen's stuff command
            let escaped_cmd = cmd
                .replace('\\', "\\\\") // Escape backslashes first
                .replace('$', "\\$") // Escape dollar signs
                .replace('\'', "'\\''"); // Escape single quotes

            let full_cmd = format!("bash -lc '{}'\n", escaped_cmd);
            stuff_command.arg(full_cmd);
        } else {
            // Just start bash with a newline
            let _ = stuff_command.arg("bash\n");
        }

        let _ = stuff_command.output();

        let pane_id = std::env::var("WINDOW").unwrap_or_default();
        info!(pane_id = %pane_id, direction = ?dir, "Pane split successfully");
        Ok(pane_id)
    }

    #[instrument(skip(self, pane, _opts), fields(component = "ah-mux", operation = "run_command", pane_id = %pane, command = %cmd))]
    fn run_command(
        &self,
        pane: &PaneId,
        cmd: &str,
        _opts: &CommandOptions,
    ) -> Result<(), MuxError> {
        // Extract session name from pane ID
        let session_name = std::env::var("STY").unwrap_or_default();
        debug!(session_name = %session_name, pane_id = %pane, command = %cmd, "Running command in pane");

        // Send the command as text input to the focused window
        let stuff_command = format!("{}\n", cmd);

        let mut command = Command::new("screen");
        command.arg("-S").arg(&session_name).arg("-X").arg("stuff").arg(stuff_command);

        let output = command.output().map_err(|e| {
            error!(error = %e, session_name = %session_name, pane_id = %pane, "Failed to send command to screen");
            MuxError::Other(format!("Failed to send command to screen: {}", e))
        })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            error!(error_details = %stderr, session_name = %session_name, pane_id = %pane, "Screen stuff command failed");
            return Err(MuxError::CommandFailed(format!(
                "screen stuff failed: {}",
                stderr
            )));
        }

        info!(session_name = %session_name, pane_id = %pane, "Command executed successfully");
        Ok(())
    }

    #[instrument(skip(self, pane, text), fields(component = "ah-mux", operation = "send_text", pane_id = %pane))]
    fn send_text(&self, pane: &PaneId, text: &str) -> Result<(), MuxError> {
        debug!(pane_id = %pane, text_length = text.len(), "Sending text to pane");

        // Screen doesn't have addressable pane IDs in the CLI, so we use the current session
        let session_name = std::env::var("STY").unwrap_or_default();

        // Use screen's stuff command to send text
        let mut command = Command::new("screen");
        command.arg("-S").arg(&session_name).arg("-X").arg("stuff").arg(text);

        let output = command.output().map_err(|e| {
            error!(error = %e, session_name = %session_name, pane_id = %pane, "Failed to send text to screen");
            MuxError::Other(format!("Failed to send text to screen: {}", e))
        })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            error!(error_details = %stderr, session_name = %session_name, pane_id = %pane, "Screen stuff command failed");
            return Err(MuxError::CommandFailed(format!(
                "screen stuff failed: {}",
                stderr
            )));
        }

        info!(session_name = %session_name, pane_id = %pane, "Text sent successfully");
        Ok(())
    }

    #[instrument(skip(self), fields(component = "ah-mux", operation = "focus_window", window_id = %window))]
    fn focus_window(&self, window: &WindowId) -> Result<(), MuxError> {
        debug!(window_id = %window, "Focusing window (reattaching to session)");

        // Reattach to the session (this brings it to the foreground)
        // Note: This behaves differently depending on if we are in a terminal or not.
        // If running programmatically, this might hang until detach.
        let output = Command::new("screen").arg("-r").arg(window).output().map_err(|e| {
            error!(error = %e, window_id = %window, "Failed to reattach to screen session");
            MuxError::Other(format!("Failed to reattach to screen session: {}", e))
        })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            error!(stderr = %stderr, window_id = %window, "Screen reattach failed");
            return Err(MuxError::CommandFailed(format!(
                "screen reattach failed: {}",
                stderr
            )));
        }

        info!(window_id = %window, "Window focused successfully");
        Ok(())
    }

    #[instrument(skip(self), fields(component = "ah-mux", operation = "focus_pane"))]
    fn focus_pane(&self, _pane: &PaneId) -> Result<(), MuxError> {
        debug!("Focus pane not available in GNU Screen");
        // Screen doesn't have direct pane focusing via CLI
        // Users can navigate regions manually
        Err(MuxError::NotAvailable("screen"))
    }

    #[instrument(skip(self), fields(component = "ah-mux", operation = "list_windows", filter = ?title_substr))]
    fn list_windows(&self, title_substr: Option<&str>) -> Result<Vec<WindowId>, MuxError> {
        debug!(filter = ?title_substr, "Listing Screen windows/sessions");

        // Screen doesn't have a direct way to list sessions from CLI
        // We use `screen -ls` and parse the output
        let output = Command::new("screen").arg("-ls").output().map_err(|e| {
            error!(error = %e, "Failed to list screen sessions");
            MuxError::Other(format!("Failed to list screen sessions: {}", e))
        })?;

        // screen -ls returns 1 if no sessions found (sometimes)
        let stdout = String::from_utf8_lossy(&output.stdout);

        // Handle failure cases that are not just "No Sockets found"
        if !output.status.success() && !stdout.contains("No Sockets found") {
            let stderr = String::from_utf8_lossy(&output.stderr);
            error!(stderr = %stderr, "screen -ls command failed");
            return Err(MuxError::CommandFailed(format!(
                "screen -ls failed: {}",
                stderr
            )));
        }

        let sessions = Self::parse_ls_output(&stdout, title_substr)?;

        info!(count = sessions.len(), filter = ?title_substr, "Listed Screen sessions");
        Ok(sessions)
    }

    #[instrument(skip(self), fields(component = "ah-mux", operation = "list_panes"))]
    fn list_panes(&self, _window: &WindowId) -> Result<Vec<PaneId>, MuxError> {
        debug!("List panes not available in GNU Screen");
        // Screen doesn't expose pane/region listing via CLI
        Err(MuxError::NotAvailable("screen"))
    }

    #[instrument(skip(self), fields(component = "ah-mux", operation = "current_window"))]
    fn current_window(&self) -> Result<Option<WindowId>, MuxError> {
        let win = env::var("WINDOW").ok();
        Self::resolve_current_window(win)
    }

    #[instrument(skip(self), fields(component = "ah-mux", operation = "current_pane"))]
    fn current_pane(&self) -> Result<Option<PaneId>, MuxError> {
        let sty = env::var("STY").ok();
        Self::resolve_current_pane(sty)
    }
}

impl ScreenMultiplexer {
    /// Helper to parse output from `screen -ls`
    pub(crate) fn parse_ls_output(
        output: &str,
        title_substr: Option<&str>,
    ) -> Result<Vec<String>, MuxError> {
        if output.contains("No Sockets found") {
            debug!("No Screen sessions found");
            return Ok(Vec::new());
        }

        let mut sessions = Vec::new();
        // Regex to parse session line: "\t12345.session_name\t(Detached)"
        // Matches start of line, optional whitespace, digits, dot, capture name, whitespace, open paren
        let re = Regex::new(r"^\s*\d+\.([^\s]+)\s+\(").map_err(|e| {
            error!(error = %e, "Failed to compile regex for parsing screen -ls output");
            MuxError::Other(format!("Failed to compile regex: {}", e))
        })?;

        for line in output.lines() {
            if let Some(captures) = re.captures(line) {
                if let Some(session_name_match) = captures.get(1) {
                    let session_name = session_name_match.as_str();
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

        Ok(sessions)
    }

    /// Helper to resolve current window from WINDOW env var
    pub(crate) fn resolve_current_window(
        window_env: Option<String>,
    ) -> Result<Option<WindowId>, MuxError> {
        match window_env {
            Some(w) if !w.is_empty() => {
                debug!(window_id = %w, "Current window detected");
                Ok(Some(w))
            }
            _ => {
                debug!("Not running inside a Screen session or window");
                Ok(None)
            }
        }
    }

    /// Helper to resolve current pane from STY env var
    pub(crate) fn resolve_current_pane(sty: Option<String>) -> Result<Option<PaneId>, MuxError> {
        // We can't easily determine the "pane ID" as we format it (screen-region-<session>)
        // But we can verify if we are in screen
        match sty {
            Some(sty) if !sty.is_empty() => {
                let pane_id = format!("screen-region-{}", sty);
                debug!(pane_id = %pane_id, "Current pane detected");
                Ok(Some(pane_id))
            }
            _ => {
                debug!("Not running inside a Screen session");
                Ok(None)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_screen_id() {
        // Only run if screen is available or just create instance if possible
        // Since new() checks availability, we can assume if it fails, we can't test id() via instance
        // But we can check strictly if we can instantiate it
        if let Ok(mux) = ScreenMultiplexer::new() {
            assert_eq!(mux.id(), "screen");
        }
    }

    #[test]
    fn test_parse_ls_output_empty() {
        let output = "No Sockets found in /var/run/screen/S-user.\n";
        let sessions = ScreenMultiplexer::parse_ls_output(output, None).unwrap();
        assert!(sessions.is_empty());
    }

    #[test]
    fn test_parse_ls_output_single() {
        let output = "There is a screen on:\n\t12345.ah-task-1\t(Detached)\n1 Socket in /var/run/screen/S-user.\n";
        let sessions = ScreenMultiplexer::parse_ls_output(output, None).unwrap();
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0], "ah-task-1");
    }

    #[test]
    fn test_parse_ls_output_multiple() {
        let output = "There are screens on:\n\t12345.ah-task-1\t(Detached)\n\t67890.ah-task-2\t(Attached)\n2 Sockets in ...";
        let sessions = ScreenMultiplexer::parse_ls_output(output, None).unwrap();
        assert_eq!(sessions.len(), 2);
        assert_eq!(sessions[0], "ah-task-1");
        assert_eq!(sessions[1], "ah-task-2");
    }

    #[test]
    fn test_parse_ls_output_filter() {
        let output = "There are screens on:\n\t12345.ah-task-1\t(Detached)\n\t67890.other-session\t(Attached)\n";
        let sessions = ScreenMultiplexer::parse_ls_output(output, Some("ah-task")).unwrap();
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0], "ah-task-1");
    }

    #[test]
    fn test_resolve_current_window() {
        // Test with valid session
        let res = ScreenMultiplexer::resolve_current_window(Some("session1".to_string())).unwrap();
        assert_eq!(res, Some("session1".to_string()));

        // Test with empty string
        let res = ScreenMultiplexer::resolve_current_window(Some("".to_string())).unwrap();
        assert_eq!(res, None);

        // Test with None
        let res = ScreenMultiplexer::resolve_current_window(None).unwrap();
        assert_eq!(res, None);
    }

    #[test]
    fn test_resolve_current_pane() {
        // Test with valid session
        let res = ScreenMultiplexer::resolve_current_pane(Some("session1".to_string())).unwrap();
        assert_eq!(res, Some("screen-region-session1".to_string()));

        // Test with empty string
        let res = ScreenMultiplexer::resolve_current_pane(Some("".to_string())).unwrap();
        assert_eq!(res, None);

        // Test with None
        let res = ScreenMultiplexer::resolve_current_pane(None).unwrap();
        assert_eq!(res, None);
    }

    #[test]
    fn test_screen_not_available_methods() {
        // Even if screen is not available, we can test the behavior of these methods if we could instantiate it.
        // Since we can't easily instantiate ScreenMultiplexer without availability check passing,
        // we rely on the fact that list_panes and focus_pane return NotAvailable error.
        // We can assume if we have an instance, it behaves this way.
        if let Ok(mux) = ScreenMultiplexer::new() {
            assert!(matches!(
                mux.list_panes(&"w".to_string()),
                Err(MuxError::NotAvailable(_))
            ));
            assert!(matches!(
                mux.focus_pane(&"p".to_string()),
                Err(MuxError::NotAvailable(_))
            ));
        }
    }
}
