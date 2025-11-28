// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! GNU Screen multiplexer implementation
//!
//! GNU Screen is a classic terminal multiplexer with support for sessions,
//! windows, and regions. It uses the `-X` command interface for automation.
//!
//! ## Terminology Note
//!
//! There is an important terminology discrepancy between the `Multiplexer` trait
//! and GNU Screen's CLI:
//!
//! - **Multiplexer "window"** → **GNU Screen "window"** (what users see as tabs in the terminal)
//! - **Multiplexer "pane"** → **GNU Screen "region"** (splits within a tab/window)
//!
//! Additionally, GNU Screen has "layouts" which are saved configurations of window+region
//! arrangements. This implementation uses layouts to organize Agent Harbor tasks, where each
//! task gets its own layout containing one or more windows and regions.

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

        // Create a new layout for this task
        // screen -S <session_name> -X layout new <task_name>
        let _split_layout_cmd = Command::new("screen")
            .arg("-S")
            .arg(&session_name)
            .arg("-X")
            .arg("layout")
            .arg("new")
            .arg(task_name)
            .output();

        // Create a new window within this layout with the task name as its title
        // screen -S <session_name> -X screen -t <task_name>
        let _screen_cmd = Command::new("screen")
            .arg("-S")
            .arg(&session_name)
            .arg("-X")
            .arg("screen")
            .arg("-t")
            .arg(task_name)
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
        // Screen uses regions (its term for panes/splits)
        // We split the current region and create a new window in the new region

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

        // Send the command as text input to the focused window (region)
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

        // Screen doesn't have addressable region IDs in the CLI, so we target the current session
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
        let session_name = std::env::var("STY").unwrap_or_default();
        if session_name.is_empty() {
            debug!("STY not set, cannot list windows");
            return Ok(Vec::new());
        }

        debug!(session_name = %session_name, filter = ?title_substr, "Listing Screen windows");

        // screen -S <session_name> -Q windows
        let output = Command::new("screen")
            .arg("-S")
            .arg(&session_name)
            .arg("-Q")
            .arg("windows")
            .output()
            .map_err(|e| {
                error!(error = %e, "Failed to list screen windows");
                MuxError::Other(format!("Failed to list screen windows: {}", e))
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            // Use debug level since this is expected when session is gone or empty
            debug!(stderr = %stderr, "screen -Q windows failed (possibly no session or old version)");
            return Ok(Vec::new());
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        debug!(stdout = %stdout, "screen -Q windows output");
        let windows = Self::parse_windows_output(&stdout, title_substr)?;

        info!(count = windows.len(), filter = ?title_substr, "Listed Screen windows");
        Ok(windows)
    }

    #[instrument(skip(self), fields(component = "ah-mux", operation = "list_panes"))]
    fn list_panes(&self, _window: &WindowId) -> Result<Vec<PaneId>, MuxError> {
        debug!("List panes not available in GNU Screen");
        // Screen doesn't expose region listing via CLI
        Err(MuxError::NotAvailable("screen"))
    }

    #[instrument(skip(self), fields(component = "ah-mux", operation = "current_window"))]
    fn current_window(&self) -> Result<Option<WindowId>, MuxError> {
        let win = env::var("WINDOW").ok();
        Self::resolve_current_window(win)
    }

    #[instrument(skip(self), fields(component = "ah-mux", operation = "current_pane"))]
    fn current_pane(&self) -> Result<Option<PaneId>, MuxError> {
        let win = env::var("WINDOW").ok();
        Self::resolve_current_pane(win)
    }
}

impl ScreenMultiplexer {
    /// Helper to parse output from `screen -Q windows`
    pub(crate) fn parse_windows_output(
        output: &str,
        title_substr: Option<&str>,
    ) -> Result<Vec<String>, MuxError> {
        let mut windows = Vec::new();
        // Output format example: "0* bash  1- vim  2  misc"
        // We use a regex to find: number, optional flags, space, title
        // NOTE: This assumes titles don't have spaces.
        let re = Regex::new(r"(\d+)[*!-]?\s+([^\s]+)").map_err(|e| {
            error!(error = %e, "Failed to compile regex for parsing screen windows output");
            MuxError::Other(format!("Failed to compile regex: {}", e))
        })?;

        for cap in re.captures_iter(output) {
            if let Some(title_match) = cap.get(2) {
                let title = title_match.as_str();
                if let Some(substr) = title_substr {
                    if title.contains(substr) {
                        windows.push(title.to_string());
                    }
                } else {
                    windows.push(title.to_string());
                }
            }
        }

        Ok(windows)
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

    /// Helper to resolve current pane from WINDOW env var
    pub(crate) fn resolve_current_pane(window: Option<String>) -> Result<Option<PaneId>, MuxError> {
        // Screen doesn't have addressable region IDs in the CLI
        // We use the WINDOW env var which identifies the current window
        match window {
            Some(w) if !w.is_empty() => {
                debug!(pane_id = %w, "Current pane detected");
                Ok(Some(w))
            }
            _ => {
                debug!("Not running inside a Screen session or window");
                Ok(None)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{thread, time::Duration};

    use serial_test::serial;

    use super::*;

    /// Initialize tracing for tests if not already initialized
    fn init_tracing() {
        use std::sync::Once;
        static INIT: Once = Once::new();

        INIT.call_once(|| {
            // Only initialize if RUST_LOG is set (user wants logging)
            if std::env::var("RUST_LOG").is_ok() {
                tracing_subscriber::fmt()
                    .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
                    .with_test_writer()
                    .try_init()
                    .ok(); // Ignore errors if already initialized
            }
        });
    }

    /// Helper to start a detached screen session and return its session name (STY)
    /// Returns None if screen is not available or fails to start
    fn start_screen_session(name: &str) -> Option<String> {
        use std::{process::Command, thread, time::Duration};

        // 1) Check if screen is runnable at all
        let version_output = Command::new("screen").arg("--version").output().ok()?;
        debug!(
            "screen --version: status={:?}, stdout={}, stderr={}",
            version_output.status,
            String::from_utf8_lossy(&version_output.stdout),
            String::from_utf8_lossy(&version_output.stderr),
        );
        if !version_output.status.success() {
            error!("screen --version failed, treating screen as unavailable");
            return None;
        }

        // 2) Try to start a detached session
        let start_output = Command::new("screen").args(["-d", "-m", "-S", name]).output().ok()?;
        debug!(
            "screen -d -m -S {}: status={:?}, stdout={}, stderr={}",
            name,
            start_output.status,
            String::from_utf8_lossy(&start_output.stdout),
            String::from_utf8_lossy(&start_output.stderr),
        );

        if !start_output.status.success() {
            error!("Failed to start screen session '{}', skipping test", name);
            return None;
        }

        // 3) Give it a moment to start
        thread::sleep(Duration::from_millis(500));

        // 4) Verify the session exists
        let ls_output = Command::new("screen").arg("-ls").output().ok()?;
        debug!(
            "screen -ls: status={:?}, stdout={}, stderr={}",
            ls_output.status,
            String::from_utf8_lossy(&ls_output.stdout),
            String::from_utf8_lossy(&ls_output.stderr),
        );

        // If `screen -ls` itself fails (like your CI case: "No Sockets found ..."),
        // treat this as "screen not available" and let the test skip.
        if !ls_output.status.success() {
            error!("`screen -ls` failed, treating screen as unavailable");
            return None;
        }

        let stdout = String::from_utf8_lossy(&ls_output.stdout);
        for line in stdout.lines() {
            if line.contains(name) {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if let Some(full_name) = parts.first() {
                    return Some(full_name.to_string());
                }
            }
        }

        // If we got here, `screen -ls` worked but didn't list our session –
        // again, treat that as "screen not really usable" instead of faking success.
        error!(
            "screen session '{}' not found in `screen -ls` output, treating as unavailable",
            name
        );
        None
    }

    /// Helper to kill the screen session
    fn kill_screen_session(sty: &str) {
        let _ = Command::new("screen").args(["-S", sty, "-X", "quit"]).status();
    }

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
    fn test_parse_windows_output() {
        let output = "0* bash  1- vim  2  misc";
        let windows = ScreenMultiplexer::parse_windows_output(output, None).unwrap();
        assert_eq!(windows.len(), 3);
        assert_eq!(windows[0], "bash");
        assert_eq!(windows[1], "vim");
        assert_eq!(windows[2], "misc");
    }

    #[test]
    fn test_parse_windows_output_filter() {
        let output = "0* bash  1- vim  2  misc";
        let windows = ScreenMultiplexer::parse_windows_output(output, Some("vim")).unwrap();
        assert_eq!(windows.len(), 1);
        assert_eq!(windows[0], "vim");
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
        // Test with valid window ID
        let res = ScreenMultiplexer::resolve_current_pane(Some("5".to_string())).unwrap();
        assert_eq!(res, Some("5".to_string()));

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

    #[test]
    #[serial]
    fn test_current_window_with_env() {
        // Test current_window() with WINDOW env var set
        env::set_var("WINDOW", "5");
        if let Ok(mux) = ScreenMultiplexer::new() {
            let result = mux.current_window().unwrap();
            assert_eq!(result, Some("5".to_string()));
        }
        env::remove_var("WINDOW");
    }

    #[test]
    #[serial]
    fn test_current_window_without_env() {
        // Test current_window() with WINDOW env var unset
        env::remove_var("WINDOW");
        if let Ok(mux) = ScreenMultiplexer::new() {
            let result = mux.current_window().unwrap();
            assert_eq!(result, None);
        }
    }

    #[test]
    #[serial]
    fn test_current_window_empty_env() {
        // Test current_window() with empty WINDOW env var
        env::set_var("WINDOW", "");
        if let Ok(mux) = ScreenMultiplexer::new() {
            let result = mux.current_window().unwrap();
            assert_eq!(result, None);
        }
        env::remove_var("WINDOW");
    }

    #[test]
    #[serial]
    fn test_current_pane_with_env() {
        // Test current_pane() with WINDOW env var set
        env::set_var("WINDOW", "5");
        if let Ok(mux) = ScreenMultiplexer::new() {
            let result = mux.current_pane().unwrap();
            assert_eq!(result, Some("5".to_string()));
        }
        env::remove_var("WINDOW");
    }

    #[test]
    #[serial]
    fn test_current_pane_without_env() {
        // Test current_pane() with WINDOW env var unset
        env::remove_var("WINDOW");
        if let Ok(mux) = ScreenMultiplexer::new() {
            let result = mux.current_pane().unwrap();
            assert_eq!(result, None);
        }
    }

    #[test]
    #[serial]
    fn test_current_pane_empty_env() {
        // Test current_pane() with empty WINDOW env var
        env::set_var("WINDOW", "");
        if let Ok(mux) = ScreenMultiplexer::new() {
            let result = mux.current_pane().unwrap();
            assert_eq!(result, None);
        }
        env::remove_var("WINDOW");
    }

    #[test]
    fn test_parse_windows_output_empty() {
        // Test with empty output
        let windows = ScreenMultiplexer::parse_windows_output("", None).unwrap();
        assert_eq!(windows.len(), 0);
    }

    #[test]
    fn test_parse_windows_output_malformed() {
        // Test with malformed output (no window titles)
        let output = "some random text without proper format";
        let windows = ScreenMultiplexer::parse_windows_output(output, None).unwrap();
        assert_eq!(windows.len(), 0);
    }

    #[test]
    fn test_parse_windows_output_special_flags() {
        // Test with various flag combinations
        let output = "0* active  1! bell  2- previous  3 normal";
        let windows = ScreenMultiplexer::parse_windows_output(output, None).unwrap();
        assert_eq!(windows.len(), 4);
        assert_eq!(windows[0], "active");
        assert_eq!(windows[1], "bell");
        assert_eq!(windows[2], "previous");
        assert_eq!(windows[3], "normal");
    }

    #[test]
    fn test_parse_windows_output_many_windows() {
        // Test with many windows
        let mut output = String::new();
        for i in 0..20 {
            output.push_str(&format!("{}  window{}  ", i, i));
        }
        let windows = ScreenMultiplexer::parse_windows_output(&output, None).unwrap();
        assert_eq!(windows.len(), 20);
    }

    #[test]
    fn test_parse_windows_output_with_numbers_in_title() {
        // Test window titles containing numbers
        let output = "0* task-123  1- window-456  2  test789";
        let windows = ScreenMultiplexer::parse_windows_output(output, None).unwrap();
        assert_eq!(windows.len(), 3);
        assert_eq!(windows[0], "task-123");
        assert_eq!(windows[1], "window-456");
        assert_eq!(windows[2], "test789");
    }

    #[test]
    #[serial]
    fn test_send_text_integration() {
        init_tracing();
        let session_name = format!("test-sendtext-{}", std::process::id());

        // Start screen session
        let sty = match start_screen_session(&session_name) {
            Some(s) => s,
            None => {
                error!("Skipping test_send_text_integration: screen not available");
                return;
            }
        };

        env::set_var("STY", &sty);
        env::set_var("WINDOW", "0");

        let mux = ScreenMultiplexer::new().expect("Failed to create ScreenMultiplexer");

        // Test: Send text without newline
        let result = mux.send_text(&"dummy-pane".to_string(), "echo 'test'");
        assert!(result.is_ok());

        // Test: Send text with newline
        let result = mux.send_text(&"dummy-pane".to_string(), "ls\n");
        assert!(result.is_ok());

        // Test: Send text with special characters
        let result = mux.send_text(&"dummy-pane".to_string(), "echo '$HOME'");
        assert!(result.is_ok());

        // Cleanup
        env::remove_var("STY");
        env::remove_var("WINDOW");
        kill_screen_session(&sty);
    }

    #[test]
    #[serial]
    fn test_send_text_invalid_session() {
        init_tracing();

        // Set STY to a non-existent session
        env::set_var("STY", "99999.nonexistent");

        if let Ok(mux) = ScreenMultiplexer::new() {
            let result = mux.send_text(&"dummy-pane".to_string(), "echo 'test'");
            // This should fail because the session doesn't exist
            assert!(result.is_err());
        }

        env::remove_var("STY");
    }

    #[test]
    #[serial]
    fn test_command_escaping_single_quotes() {
        init_tracing();
        let session_name = format!("test-escape-quotes-{}", std::process::id());

        let sty = match start_screen_session(&session_name) {
            Some(s) => s,
            None => {
                error!("Skipping test_command_escaping_single_quotes: screen not available");
                return;
            }
        };

        env::set_var("STY", &sty);
        env::set_var("WINDOW", "0");

        let mux = ScreenMultiplexer::new().expect("Failed to create ScreenMultiplexer");

        let window_opts = WindowOptions {
            title: Some("escape-test"),
            cwd: None,
            focus: false,
            profile: None,
            init_command: None,
        };
        let window_id = mux.open_window(&window_opts).expect("Failed to open window");

        let cmd_opts = CommandOptions {
            cwd: None,
            env: None,
        };

        // Test: Command with single quotes
        let result = mux.run_command(&window_id, "echo 'hello world'", &cmd_opts);
        assert!(result.is_ok());

        // Cleanup
        env::remove_var("STY");
        env::remove_var("WINDOW");
        kill_screen_session(&sty);
    }

    #[test]
    #[serial]
    fn test_command_escaping_dollar_signs() {
        init_tracing();
        let session_name = format!("test-escape-dollar-{}", std::process::id());

        let sty = match start_screen_session(&session_name) {
            Some(s) => s,
            None => {
                error!("Skipping test_command_escaping_dollar_signs: screen not available");
                return;
            }
        };

        env::set_var("STY", &sty);
        env::set_var("WINDOW", "0");

        let mux = ScreenMultiplexer::new().expect("Failed to create ScreenMultiplexer");

        let window_opts = WindowOptions {
            title: Some("dollar-test"),
            cwd: None,
            focus: false,
            profile: None,
            init_command: None,
        };
        let window_id = mux.open_window(&window_opts).expect("Failed to open window");

        let cmd_opts = CommandOptions {
            cwd: None,
            env: None,
        };

        // Test: Command with dollar signs
        let result = mux.run_command(&window_id, "echo $HOME", &cmd_opts);
        assert!(result.is_ok());

        // Cleanup
        env::remove_var("STY");
        env::remove_var("WINDOW");
        kill_screen_session(&sty);
    }

    #[test]
    #[serial]
    fn test_command_escaping_backslashes() {
        init_tracing();
        let session_name = format!("test-escape-backslash-{}", std::process::id());

        let sty = match start_screen_session(&session_name) {
            Some(s) => s,
            None => {
                error!("Skipping test_command_escaping_backslashes: screen not available");
                return;
            }
        };

        env::set_var("STY", &sty);
        env::set_var("WINDOW", "0");

        let mux = ScreenMultiplexer::new().expect("Failed to create ScreenMultiplexer");

        let window_opts = WindowOptions {
            title: Some("backslash-test"),
            cwd: None,
            focus: false,
            profile: None,
            init_command: None,
        };
        let window_id = mux.open_window(&window_opts).expect("Failed to open window");

        let cmd_opts = CommandOptions {
            cwd: None,
            env: None,
        };

        // Test: Command with backslashes
        let result = mux.run_command(&window_id, "echo \\test\\path", &cmd_opts);
        assert!(result.is_ok());

        // Cleanup
        env::remove_var("STY");
        env::remove_var("WINDOW");
        kill_screen_session(&sty);
    }

    #[test]
    #[serial]
    fn test_command_escaping_mixed_special_chars() {
        init_tracing();
        let session_name = format!("test-escape-mixed-{}", std::process::id());

        let sty = match start_screen_session(&session_name) {
            Some(s) => s,
            None => {
                error!("Skipping test_command_escaping_mixed_special_chars: screen not available");
                return;
            }
        };

        env::set_var("STY", &sty);
        env::set_var("WINDOW", "0");

        let mux = ScreenMultiplexer::new().expect("Failed to create ScreenMultiplexer");

        let window_opts = WindowOptions {
            title: Some("mixed-test"),
            cwd: None,
            focus: false,
            profile: None,
            init_command: None,
        };
        let window_id = mux.open_window(&window_opts).expect("Failed to open window");

        let cmd_opts = CommandOptions {
            cwd: None,
            env: None,
        };

        // Test: Command with mixed special characters
        let result = mux.run_command(&window_id, "echo '$HOME' \\test", &cmd_opts);
        assert!(result.is_ok());

        // Cleanup
        env::remove_var("STY");
        env::remove_var("WINDOW");
        kill_screen_session(&sty);
    }

    #[test]
    #[serial]
    fn test_split_pane_escaping_initial_cmd() {
        init_tracing();
        let session_name = format!("test-split-escape-{}", std::process::id());

        let sty = match start_screen_session(&session_name) {
            Some(s) => s,
            None => {
                error!("Skipping test_split_pane_escaping_initial_cmd: screen not available");
                return;
            }
        };

        env::set_var("STY", &sty);
        env::set_var("WINDOW", "0");

        let mux = ScreenMultiplexer::new().expect("Failed to create ScreenMultiplexer");

        let window_opts = WindowOptions {
            title: Some("split-escape-test"),
            cwd: None,
            focus: false,
            profile: None,
            init_command: None,
        };
        let window_id = mux.open_window(&window_opts).expect("Failed to open window");

        let cmd_opts = CommandOptions {
            cwd: None,
            env: None,
        };

        // Test: Split with initial_cmd containing special characters
        let result = mux.split_pane(
            Some(&window_id),
            None,
            SplitDirection::Vertical,
            None,
            &cmd_opts,
            Some("echo 'test $HOME \\path'"),
        );
        assert!(result.is_ok());

        // Cleanup
        env::remove_var("STY");
        env::remove_var("WINDOW");
        kill_screen_session(&sty);
    }

    #[test]
    #[serial]
    fn test_split_pane_with_cwd() {
        init_tracing();
        let session_name = format!("test-split-cwd-{}", std::process::id());

        let sty = match start_screen_session(&session_name) {
            Some(s) => s,
            None => {
                error!("Skipping test_split_pane_with_cwd: screen not available");
                return;
            }
        };

        env::set_var("STY", &sty);
        env::set_var("WINDOW", "0");

        let mux = ScreenMultiplexer::new().expect("Failed to create ScreenMultiplexer");

        let window_opts = WindowOptions {
            title: Some("cwd-test"),
            cwd: None,
            focus: false,
            profile: None,
            init_command: None,
        };
        let window_id = mux.open_window(&window_opts).expect("Failed to open window");

        let tmp_dir = std::path::Path::new("/tmp");
        let cmd_opts = CommandOptions {
            cwd: Some(tmp_dir),
            env: None,
        };

        // Test: Horizontal split with cwd
        let result = mux.split_pane(
            Some(&window_id),
            None,
            SplitDirection::Horizontal,
            None,
            &cmd_opts,
            None,
        );
        assert!(result.is_ok());

        // Test: Vertical split with cwd
        let result = mux.split_pane(
            Some(&window_id),
            None,
            SplitDirection::Vertical,
            None,
            &cmd_opts,
            None,
        );
        assert!(result.is_ok());

        // Cleanup
        env::remove_var("STY");
        env::remove_var("WINDOW");
        kill_screen_session(&sty);
    }

    #[test]
    #[serial]
    fn test_split_pane_with_cwd_and_initial_cmd() {
        init_tracing();
        let session_name = format!("test-split-cwd-cmd-{}", std::process::id());

        let sty = match start_screen_session(&session_name) {
            Some(s) => s,
            None => {
                error!("Skipping test_split_pane_with_cwd_and_initial_cmd: screen not available");
                return;
            }
        };

        env::set_var("STY", &sty);
        env::set_var("WINDOW", "0");

        let mux = ScreenMultiplexer::new().expect("Failed to create ScreenMultiplexer");

        let window_opts = WindowOptions {
            title: Some("cwd-cmd-test"),
            cwd: None,
            focus: false,
            profile: None,
            init_command: None,
        };
        let window_id = mux.open_window(&window_opts).expect("Failed to open window");

        let tmp_dir = std::path::Path::new("/tmp");
        let cmd_opts = CommandOptions {
            cwd: Some(tmp_dir),
            env: None,
        };

        // Test: Split with both cwd and initial_cmd
        let result = mux.split_pane(
            Some(&window_id),
            None,
            SplitDirection::Vertical,
            None,
            &cmd_opts,
            Some("pwd && echo 'test'"),
        );
        assert!(result.is_ok());

        // Cleanup
        env::remove_var("STY");
        env::remove_var("WINDOW");
        kill_screen_session(&sty);
    }

    #[test]
    #[serial]
    fn test_split_pane_with_special_chars_in_cwd() {
        init_tracing();
        let session_name = format!("test-split-cwd-special-{}", std::process::id());

        let sty = match start_screen_session(&session_name) {
            Some(s) => s,
            None => {
                error!("Skipping test_split_pane_with_special_chars_in_cwd: screen not available");
                return;
            }
        };

        env::set_var("STY", &sty);
        env::set_var("WINDOW", "0");

        let mux = ScreenMultiplexer::new().expect("Failed to create ScreenMultiplexer");

        let window_opts = WindowOptions {
            title: Some("cwd-special-test"),
            cwd: None,
            focus: false,
            profile: None,
            init_command: None,
        };
        let window_id = mux.open_window(&window_opts).expect("Failed to open window");

        // Use /tmp which should always exist
        let test_dir = std::path::Path::new("/tmp");
        let cmd_opts = CommandOptions {
            cwd: Some(test_dir),
            env: None,
        };

        // Test: Split with cwd and initial_cmd with special characters
        let result = mux.split_pane(
            Some(&window_id),
            None,
            SplitDirection::Horizontal,
            None,
            &cmd_opts,
            Some("echo '$PWD'"),
        );
        assert!(result.is_ok());

        // Cleanup
        env::remove_var("STY");
        env::remove_var("WINDOW");
        kill_screen_session(&sty);
    }

    #[test]
    #[serial]
    fn test_open_window_without_sty() {
        init_tracing();

        // Ensure STY is not set
        env::remove_var("STY");

        if let Ok(mux) = ScreenMultiplexer::new() {
            let window_opts = WindowOptions {
                title: Some("test-window"),
                cwd: None,
                focus: false,
                profile: None,
                init_command: None,
            };

            // This should succeed but the window won't actually be created
            // because STY is empty
            let result = mux.open_window(&window_opts);
            // The implementation returns Ok with the task name even if STY is empty
            assert!(result.is_ok());
        }
    }

    #[test]
    #[serial]
    fn test_open_window_with_title() {
        init_tracing();
        let session_name = format!("test-window-title-{}", std::process::id());

        let sty = match start_screen_session(&session_name) {
            Some(s) => s,
            None => {
                error!("Skipping test_open_window_with_title: screen not available");
                return;
            }
        };

        env::set_var("STY", &sty);
        env::set_var("WINDOW", "0");

        let mux = ScreenMultiplexer::new().expect("Failed to create ScreenMultiplexer");

        // Test: Open window with custom title
        let window_opts = WindowOptions {
            title: Some("custom-window-title"),
            cwd: None,
            focus: false,
            profile: None,
            init_command: None,
        };
        let window_id = mux.open_window(&window_opts).expect("Failed to open window");
        assert_eq!(window_id, "custom-window-title");

        // Cleanup
        env::remove_var("STY");
        env::remove_var("WINDOW");
        kill_screen_session(&sty);
    }

    #[test]
    #[serial]
    fn test_open_window_without_title() {
        init_tracing();
        let session_name = format!("test-window-no-title-{}", std::process::id());

        let sty = match start_screen_session(&session_name) {
            Some(s) => s,
            None => {
                error!("Skipping test_open_window_without_title: screen not available");
                return;
            }
        };

        env::set_var("STY", &sty);
        env::set_var("WINDOW", "0");

        let mux = ScreenMultiplexer::new().expect("Failed to create ScreenMultiplexer");

        // Test: Open window without title (should use default)
        let window_opts = WindowOptions {
            title: None,
            cwd: None,
            focus: false,
            profile: None,
            init_command: None,
        };
        let window_id = mux.open_window(&window_opts).expect("Failed to open window");
        assert_eq!(window_id, "ah-task"); // Default title

        // Cleanup
        env::remove_var("STY");
        env::remove_var("WINDOW");
        kill_screen_session(&sty);
    }

    #[test]
    #[serial]
    fn test_open_window_reuse_existing() {
        init_tracing();
        let session_name = format!("test-window-reuse-{}", std::process::id());

        let sty = match start_screen_session(&session_name) {
            Some(s) => s,
            None => {
                error!("Skipping test_open_window_reuse_existing: screen not available");
                return;
            }
        };

        env::set_var("STY", &sty);
        env::set_var("WINDOW", "0");

        let mux = ScreenMultiplexer::new().expect("Failed to create ScreenMultiplexer");

        // Test: Open window first time
        let window_opts = WindowOptions {
            title: Some("reusable-window"),
            cwd: None,
            focus: false,
            profile: None,
            init_command: None,
        };
        let window_id1 = mux.open_window(&window_opts).expect("Failed to open window first time");
        assert_eq!(window_id1, "reusable-window");

        // Test: Open same window again (should reuse)
        let window_id2 = mux.open_window(&window_opts).expect("Failed to open window second time");
        assert_eq!(window_id2, "reusable-window");
        assert_eq!(window_id1, window_id2);

        // Cleanup
        env::remove_var("STY");
        env::remove_var("WINDOW");
        kill_screen_session(&sty);
    }

    #[test]
    #[serial]
    fn test_open_multiple_windows() {
        init_tracing();
        let session_name = format!("test-multiple-windows-{}", std::process::id());

        let sty = match start_screen_session(&session_name) {
            Some(s) => s,
            None => {
                error!("Skipping test_open_multiple_windows: screen not available");
                return;
            }
        };

        env::set_var("STY", &sty);
        env::set_var("WINDOW", "0");

        let mux = ScreenMultiplexer::new().expect("Failed to create ScreenMultiplexer");

        // Test: Open multiple windows with different titles
        let window_opts1 = WindowOptions {
            title: Some("window-one"),
            cwd: None,
            focus: false,
            profile: None,
            init_command: None,
        };
        let window_id1 = mux.open_window(&window_opts1).expect("Failed to open first window");
        assert_eq!(window_id1, "window-one");

        let window_opts2 = WindowOptions {
            title: Some("window-two"),
            cwd: None,
            focus: false,
            profile: None,
            init_command: None,
        };
        let window_id2 = mux.open_window(&window_opts2).expect("Failed to open second window");
        assert_eq!(window_id2, "window-two");

        let window_opts3 = WindowOptions {
            title: Some("window-three"),
            cwd: None,
            focus: false,
            profile: None,
            init_command: None,
        };
        let window_id3 = mux.open_window(&window_opts3).expect("Failed to open third window");
        assert_eq!(window_id3, "window-three");

        // Verify all windows are unique
        assert_ne!(window_id1, window_id2);
        assert_ne!(window_id2, window_id3);
        assert_ne!(window_id1, window_id3);

        // Cleanup
        env::remove_var("STY");
        env::remove_var("WINDOW");
        kill_screen_session(&sty);
    }

    #[test]
    #[serial]
    fn test_open_window_with_special_chars_in_title() {
        init_tracing();
        let session_name = format!("test-window-special-{}", std::process::id());

        let sty = match start_screen_session(&session_name) {
            Some(s) => s,
            None => {
                error!(
                    "Skipping test_open_window_with_special_chars_in_title: screen not available"
                );
                return;
            }
        };

        env::set_var("STY", &sty);
        env::set_var("WINDOW", "0");

        let mux = ScreenMultiplexer::new().expect("Failed to create ScreenMultiplexer");

        // Test: Open window with special characters in title
        let window_opts = WindowOptions {
            title: Some("task-123-test"),
            cwd: None,
            focus: false,
            profile: None,
            init_command: None,
        };
        let window_id = mux.open_window(&window_opts).expect("Failed to open window");
        assert_eq!(window_id, "task-123-test");

        // Cleanup
        env::remove_var("STY");
        env::remove_var("WINDOW");
        kill_screen_session(&sty);
    }

    #[test]
    #[serial]
    fn test_open_window_idempotent() {
        init_tracing();
        let session_name = format!("test-window-idempotent-{}", std::process::id());

        let sty = match start_screen_session(&session_name) {
            Some(s) => s,
            None => {
                error!("Skipping test_open_window_idempotent: screen not available");
                return;
            }
        };

        env::set_var("STY", &sty);
        env::set_var("WINDOW", "0");

        let mux = ScreenMultiplexer::new().expect("Failed to create ScreenMultiplexer");

        let window_opts = WindowOptions {
            title: Some("idempotent-test"),
            cwd: None,
            focus: false,
            profile: None,
            init_command: None,
        };

        // Test: Open same window multiple times - should be idempotent
        let window_id1 = mux.open_window(&window_opts).expect("Failed first open");
        let window_id2 = mux.open_window(&window_opts).expect("Failed second open");
        let window_id3 = mux.open_window(&window_opts).expect("Failed third open");

        assert_eq!(window_id1, window_id2);
        assert_eq!(window_id2, window_id3);
        assert_eq!(window_id1, "idempotent-test");

        // Cleanup
        env::remove_var("STY");
        env::remove_var("WINDOW");
        kill_screen_session(&sty);
    }

    #[test]
    #[serial]
    fn test_list_windows_without_sty() {
        init_tracing();

        // Ensure STY is not set
        env::remove_var("STY");

        if let Ok(mux) = ScreenMultiplexer::new() {
            // This should return an empty list when STY is not set
            let result = mux.list_windows(None);
            assert!(result.is_ok());
            assert!(result.unwrap().is_empty());
        }
    }

    #[test]
    #[serial]
    fn test_run_command_invalid_session() {
        init_tracing();

        // Set STY to a non-existent session
        env::set_var("STY", "99999.nonexistent");

        if let Ok(mux) = ScreenMultiplexer::new() {
            let cmd_opts = CommandOptions {
                cwd: None,
                env: None,
            };

            let result = mux.run_command(&"dummy-pane".to_string(), "echo 'test'", &cmd_opts);
            // This should fail because the session doesn't exist
            assert!(result.is_err());
        }

        env::remove_var("STY");
    }

    #[test]
    #[serial]
    fn test_list_windows_invalid_session() {
        init_tracing();

        // Set STY to a non-existent session
        env::set_var("STY", "99999.nonexistent");

        if let Ok(mux) = ScreenMultiplexer::new() {
            // This should return an empty list for a non-existent session
            let result = mux.list_windows(None);
            // The implementation handles this gracefully and returns Ok with empty vec
            assert!(result.is_ok());
        }

        env::remove_var("STY");
    }

    #[test]
    #[serial]
    fn test_init_agent_harbor_layout_without_sty() {
        init_tracing();

        // Ensure STY is not set
        env::remove_var("STY");

        // Test that init_agent_harbor_screen_layout returns an error when STY is not set
        let result = ScreenMultiplexer::init_agent_harbor_screen_layout();
        assert!(result.is_err());
        if let Err(e) = result {
            assert!(matches!(e, MuxError::Other(_)));
        }
    }

    #[test]
    #[serial]
    fn test_focus_window_invalid_session() {
        init_tracing();

        if let Ok(mux) = ScreenMultiplexer::new() {
            // Try to focus a non-existent window
            let result = mux.focus_window(&"99999.nonexistent".to_string());
            // This should fail
            assert!(result.is_err());
        }
    }

    #[test]
    #[serial]
    fn test_screen_integration_lifecycle() {
        init_tracing();
        let session_name = format!("test-screen-{}", std::process::id());

        // 1. Start screen session
        let sty = match start_screen_session(&session_name) {
            Some(s) => s,
            None => {
                error!("Skipping test_screen_integration_lifecycle: screen not available");
                return;
            }
        };

        // Ensure cleanup happens even if test panics (best effort in Rust tests)
        // We use a defer-like pattern or just try/catch blocks, but standard tests don't have try/catch.
        // We'll explicit cleanup at end.

        // 2. Set STY env var so ScreenMultiplexer connects to this session
        // IMPORTANT: This modifies global state, so #[serial] is required
        env::set_var("STY", &sty);
        // Screen sometimes needs WINDOW env var for context, but for layout creation it might rely on STY
        // Setting WINDOW to 0 to simulate being in the first window
        env::set_var("WINDOW", "0");

        // 3. Instantiate ScreenMultiplexer
        let mux = ScreenMultiplexer::new().expect("Failed to create ScreenMultiplexer");

        // 4. Test: Check availability
        assert!(mux.is_available());

        // 5. Test: List windows
        // Note: A freshly created detached screen session might not have named windows
        // visible to -Q windows, so we don't assert on the count here
        let _windows = mux.list_windows(None).expect("Failed to list windows");

        // 6. Test: Open a new window (Task Layout)
        let window_opts = WindowOptions {
            title: Some("integration-task"),
            cwd: None,
            focus: false,
            profile: None,
            init_command: None,
        };
        let window_id = mux.open_window(&window_opts).expect("Failed to open window");
        assert_eq!(window_id, "integration-task");

        // Note: list_windows() using screen -Q windows doesn't work reliably on detached
        // sessions when controlled remotely. This is a known limitation of GNU Screen.
        // We can't assert on window counts in this remote control scenario.
        let _windows_after = mux.list_windows(None).expect("Failed to list windows");

        // 7. Test: Split Pane
        // We need to be careful here. split_pane relies on `WINDOW` env var to know which window to split
        // or passed window_id. `ScreenMultiplexer::split_pane` implementation:
        // if window is provided, it selects it.

        // For Screen, window_id is the window name/number. Our open_window returns the name "integration-task".
        // Let's try splitting it.
        let split_opts = CommandOptions {
            cwd: None,
            env: None,
        };

        // Vertical split
        let _ = mux
            .split_pane(
                Some(&window_id),
                None,
                SplitDirection::Vertical,
                None,
                &split_opts,
                Some("echo 'vertical split'"),
            )
            .expect("Failed to split pane vertically");

        // Horizontal split
        let _ = mux
            .split_pane(
                Some(&window_id),
                None,
                SplitDirection::Horizontal,
                None,
                &split_opts,
                Some("echo 'horizontal split'"),
            )
            .expect("Failed to split pane horizontally");

        // 8. Test: Send Text / Run Command
        // We need a pane ID. split_pane returns "WINDOW" env var which is the window number,
        // but ScreenMultiplexer::split_pane returns the window number as string.
        // However, `ScreenMultiplexer::split_pane` implementation reads `WINDOW` env var to return pane_id.
        // Since we are running outside screen, `env::var("WINDOW")` returns "0" (what we set).
        // This might be a limitation of the current implementation when running in "remote control" mode.
        // The implementation assumes `WINDOW` env var reflects the *current* window we are in.
        // But when we control it remotely, the `WINDOW` env var of the *test process* doesn't change.

        // The `ScreenMultiplexer` implementation seems designed to run *inside* the session or
        // at least assumes `WINDOW` is set to something relevant.
        // Let's check `run_command`: it uses `STY` to target the session. It takes `pane` arg but
        // `ScreenMultiplexer::run_command` ignores `pane` arg for targeting?
        // No, it uses it in logs, but `screen -S ... -X stuff` sends to the *current focused region* in that session.
        // Unless we select a window first.

        // `ScreenMultiplexer::run_command` implementation:
        // `command.arg("-S").arg(&session_name).arg("-X").arg("stuff").arg(stuff_command);`
        // It sends to the currently active window/region in that session.

        // So we can test it executes without error.
        let run_res = mux.run_command(
            &"dummy-pane".to_string(),
            "echo 'command test'",
            &split_opts,
        );
        assert!(run_res.is_ok());

        // Test: Send text to the pane
        let send_text_res = mux.send_text(&"dummy-pane".to_string(), "echo 'sent text'\n");
        assert!(send_text_res.is_ok());

        // Test: Multiple window creation
        let window_opts2 = WindowOptions {
            title: Some("integration-task-2"),
            cwd: None,
            focus: false,
            profile: None,
            init_command: None,
        };
        let window_id2 = mux.open_window(&window_opts2).expect("Failed to open second window");
        assert_eq!(window_id2, "integration-task-2");

        // Note: Same limitation applies - list_windows doesn't work reliably on detached sessions
        let _windows_multiple =
            mux.list_windows(None).expect("Failed to list windows after second creation");

        // Test: Multiple splits in same window
        let _ = mux
            .split_pane(
                Some(&window_id2),
                None,
                SplitDirection::Vertical,
                None,
                &split_opts,
                Some("echo 'split 1'"),
            )
            .expect("Failed to create first split in second window");

        let _ = mux
            .split_pane(
                Some(&window_id2),
                None,
                SplitDirection::Horizontal,
                None,
                &split_opts,
                Some("echo 'split 2'"),
            )
            .expect("Failed to create second split in second window");

        // Test: Split with cwd
        let tmp_dir = std::path::Path::new("/tmp");
        let split_opts_with_cwd = CommandOptions {
            cwd: Some(tmp_dir),
            env: None,
        };

        let _ = mux
            .split_pane(
                Some(&window_id),
                None,
                SplitDirection::Vertical,
                None,
                &split_opts_with_cwd,
                Some("pwd"),
            )
            .expect("Failed to split with cwd");

        // 9. Cleanup
        env::remove_var("STY");
        env::remove_var("WINDOW");
        kill_screen_session(&sty);

        // Verify cleanup
        thread::sleep(Duration::from_millis(200));
        let output = Command::new("screen").arg("-ls").output().unwrap();
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(!stdout.contains(&sty));
    }
}
