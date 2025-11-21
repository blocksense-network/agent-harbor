// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! iTerm2 multiplexer implementation
//!
//! Implements the Multiplexer trait for iTerm2 using its AppleScript interface.
//! Based on the iTerm2 integration guide in specs/Public/Terminal-Multiplexers/iTerm2.md

use ah_mux_core::*;
use std::process::{Command, Stdio};
use std::time::Duration;
use tracing::{debug, error, info, instrument, warn};

/// iTerm2 multiplexer implementation using AppleScript
#[derive(Debug)]
pub struct ITerm2Multiplexer {
    window_counter: std::sync::atomic::AtomicU32,
    pane_counter: std::sync::atomic::AtomicU32,
}

impl Default for ITerm2Multiplexer {
    fn default() -> Self {
        Self {
            window_counter: std::sync::atomic::AtomicU32::new(0),
            pane_counter: std::sync::atomic::AtomicU32::new(0),
        }
    }
}

impl ITerm2Multiplexer {
    #[instrument]
    pub fn new() -> Result<Self, MuxError> {
        debug!("Creating new iTerm2 multiplexer");
        let mux = Self::default();
        if !mux.is_available() {
            error!("iTerm2 is not available");
            return Err(MuxError::NotAvailable("iTerm2"));
        }
        info!("iTerm2 multiplexer created successfully");
        Ok(mux)
    }

    /// Generate a unique window ID
    fn next_window_id(&self) -> WindowId {
        let id = self.window_counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        format!("iterm2-window-{}", id)
    }

    /// Generate a unique pane ID
    fn next_pane_id(&self) -> PaneId {
        let id = self.pane_counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        format!("iterm2-pane-{}", id)
    }

    /// Execute an AppleScript command and return the output
    #[instrument(skip(script))]
    fn run_applescript(&self, script: &str) -> Result<String, MuxError> {
        debug!("Executing AppleScript command");
        let child = Command::new("osascript")
            .arg("-e")
            .arg(script)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| {
                error!("Failed to spawn osascript process: {}", e);
                MuxError::CommandFailed(format!("Failed to run osascript: {}", e))
            })?;

        let output = child.wait_with_output().map_err(|e| {
            error!("Failed to wait for osascript process: {}", e);
            MuxError::CommandFailed(format!("Failed to wait for osascript: {}", e))
        })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            error!("AppleScript execution failed: {}", stderr);
            return Err(MuxError::CommandFailed(format!(
                "osascript failed: {}",
                stderr
            )));
        }

        let result = String::from_utf8_lossy(&output.stdout).trim().to_string();
        debug!("AppleScript command completed successfully");
        Ok(result)
    }

    /// Check if iTerm2 application is running
    fn _is_iterm2_running(&self) -> bool {
        self.run_applescript(
            r#"tell application "System Events" to (name of processes) contains "iTerm2""#,
        )
        .map(|output| output == "true")
        .unwrap_or(false)
    }

    /// Get the number of windows in iTerm2
    #[instrument]
    fn get_window_count(&self) -> Result<usize, MuxError> {
        debug!("Getting iTerm2 window count");
        let script = r#"
            tell application "iTerm2"
                count of windows
            end tell
        "#;
        let count = self.run_applescript(script)?.parse().map_err(|_| {
            error!("Failed to parse window count");
            MuxError::Other("Failed to parse window count".to_string())
        })?;
        debug!("Found {} windows", count);
        Ok(count)
    }

    /// Find a window by title substring
    #[instrument]
    fn find_window_by_title(&self, title_substr: &str) -> Result<Option<WindowId>, MuxError> {
        debug!("Finding window by title substring: {}", title_substr);
        let script = format!(
            r#"
            tell application "iTerm2"
                set windowList to windows
                repeat with w in windowList
                    tell current session of current tab of w
                        if name contains "{}" then
                            return "window-" & (index of w)
                        end if
                    end tell
                end repeat
                return ""
            end tell
        "#,
            title_substr.replace("\"", "\\\"")
        );

        let result = self.run_applescript(&script)?;
        if result.is_empty() {
            debug!("No window found with title substring: {}", title_substr);
            Ok(None)
        } else {
            debug!("Found window: {}", result);
            Ok(Some(result))
        }
    }
}

impl Multiplexer for ITerm2Multiplexer {
    fn id(&self) -> &'static str {
        "iterm2"
    }

    #[instrument]
    fn is_available(&self) -> bool {
        debug!("Checking iTerm2 availability");

        // Check if osascript is available
        let osascript_available = Command::new("osascript")
            .arg("-e")
            .arg("return \"test\"")
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false);

        if !osascript_available {
            debug!("osascript is not available");
            return false;
        }

        // Check if iTerm2 is installed (try to get its version)
        let iterm2_available =
            self.run_applescript(r#"tell application "iTerm2" to version"#).is_ok();

        debug!("iTerm2 availability: {}", iterm2_available);
        iterm2_available
    }

    #[instrument(skip(opts))]
    fn open_window(&self, opts: &WindowOptions) -> Result<WindowId, MuxError> {
        debug!("Opening new iTerm2 window");
        let window_id = self.next_window_id();

        let script = r#"
            tell application "iTerm2"
                activate
                if not (exists current window) then
                    create window with default profile
                end if
                tell current window
                    create tab with default profile
                end tell
            end tell
        "#;

        self.run_applescript(script)?;

        // Give iTerm2 a moment to create the tab
        std::thread::sleep(Duration::from_millis(200));

        // If a working directory is specified, cd to it
        if let Some(cwd) = &opts.cwd {
            debug!("Setting working directory to: {}", cwd.display());
            let cd_script = format!(
                r#"
                tell application "iTerm2"
                    tell current session of current tab of current window
                        write text "cd {}"
                    end tell
                end tell
            "#,
                cwd.display()
            );

            self.run_applescript(&cd_script)?;
            std::thread::sleep(Duration::from_millis(100));
        }

        info!("Opened iTerm2 window: {}", window_id);
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
        debug!("Splitting pane in direction: {:?}", dir);
        let pane_id = self.next_pane_id();

        let direction = match dir {
            SplitDirection::Horizontal => "horizontally",
            SplitDirection::Vertical => "vertically",
        };

        let mut script = format!(
            r#"
            tell application "iTerm2"
                tell current session of current tab of current window
                    split {} with default profile
                end tell
        "#,
            direction
        );

        if let Some(cmd) = initial_cmd {
            debug!("Running initial command in new pane: {}", cmd);
            // Build the command with working directory if specified
            let full_cmd = if let Some(cwd) = &opts.cwd {
                format!("cd {} && {}", cwd.display(), cmd)
            } else {
                cmd.to_string()
            };

            script.push_str(&format!(
                r#"
                tell second session of current tab of current window
                    write text "{}"
                end tell
            "#,
                full_cmd.replace("\"", "\\\"")
            ));
        }

        script.push_str(
            r#"
            end tell
        "#,
        );

        self.run_applescript(&script)?;

        // Give iTerm2 a moment to create the pane
        std::thread::sleep(Duration::from_millis(100));

        info!("Split pane created: {}", pane_id);
        Ok(pane_id)
    }

    #[instrument(skip(opts))]
    fn run_command(&self, pane: &PaneId, cmd: &str, opts: &CommandOptions) -> Result<(), MuxError> {
        debug!("Running command in pane {}: {}", pane, cmd);

        // Determine which session to target based on the pane ID.
        let session_target = if pane.starts_with("iterm2-window-") || pane.ends_with(".0") {
            "current session"
        } else {
            "second session"
        };

        // Build the command with working directory if specified
        let full_cmd = if let Some(cwd) = &opts.cwd {
            format!("cd {} && {}", cwd.display(), cmd)
        } else {
            cmd.to_string()
        };

        let script = format!(
            r#"
            tell application "iTerm2"
                tell {} of current tab of current window
                    write text "{}"
                end tell
            end tell
        "#,
            session_target,
            full_cmd.replace("\"", "\\\"")
        );

        self.run_applescript(&script)?;
        info!("Command executed in pane {}", pane);
        Ok(())
    }

    #[instrument]
    fn send_text(&self, pane: &PaneId, text: &str) -> Result<(), MuxError> {
        debug!("Sending text to pane {}: {}", pane, text);

        // Determine which session to target based on the pane ID.
        let session_target = if pane.starts_with("iterm2-window-") || pane.ends_with(".0") {
            "current session"
        } else {
            "second session"
        };

        // Similar to run_command but for sending literal text
        let script = format!(
            r#"
            tell application "iTerm2"
                tell {} of current tab of current window
                    write text "{}"
                end tell
            end tell
        "#,
            session_target,
            text.replace("\"", "\\\"")
        );

        self.run_applescript(&script)?;
        info!("Text sent to pane {}", pane);
        Ok(())
    }

    #[instrument]
    fn focus_window(&self, _window: &WindowId) -> Result<(), MuxError> {
        debug!("Focusing iTerm2 window: {}", _window);
        let script = r#"
            tell application "iTerm2"
                activate
                -- Focus the specified window (simplified - focuses the first window)
                -- In a full implementation, you'd parse the window ID and focus the right one
            end tell
        "#;

        self.run_applescript(script)?;
        info!("Window focused: {}", _window);
        Ok(())
    }

    #[instrument]
    fn focus_pane(&self, _pane: &PaneId) -> Result<(), MuxError> {
        debug!("Focusing iTerm2 pane: {}", _pane);
        // For now, just focus the current session
        let script = r#"
            tell application "iTerm2"
                tell current session of current tab of window 1
                    select
                end tell
            end tell
        "#;

        self.run_applescript(script)?;
        info!("Pane focused: {}", _pane);
        Ok(())
    }

    #[instrument]
    fn current_window(&self) -> Result<Option<WindowId>, MuxError> {
        debug!("Getting current iTerm2 window");
        // For iTerm2, return the frontmost window
        let script = r#"
            tell application "iTerm2"
                if (count of windows) > 0 then
                    return "window-1"
                else
                    return ""
                end if
            end tell
        "#;

        let result = self.run_applescript(script)?;
        if result.trim().is_empty() {
            debug!("No current window found");
            Ok(None)
        } else {
            let window_id = result.trim().to_string();
            debug!("Current window: {}", window_id);
            Ok(Some(window_id))
        }
    }

    #[instrument]
    fn list_windows(&self, title_substr: Option<&str>) -> Result<Vec<WindowId>, MuxError> {
        debug!("Listing iTerm2 windows, title filter: {:?}", title_substr);
        let window_count = self.get_window_count()?;

        let mut windows = Vec::new();
        for i in 1..=window_count {
            let window_id = format!("window-{}", i);
            if let Some(substr) = title_substr {
                if let Some(found_window) = self.find_window_by_title(substr)? {
                    if found_window == window_id {
                        windows.push(window_id);
                    }
                }
            } else {
                windows.push(window_id);
            }
        }

        debug!("Found {} windows", windows.len());
        Ok(windows)
    }

    #[instrument]
    fn list_panes(&self, _window: &WindowId) -> Result<Vec<PaneId>, MuxError> {
        debug!("Listing panes for window: {}", _window);
        // For simplicity, return a basic pane ID
        // A full implementation would enumerate actual panes in the window
        let panes = vec!["pane-1".to_string()];
        debug!("Found {} panes", panes.len());
        Ok(panes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_id() {
        let mux = ITerm2Multiplexer::default();
        assert_eq!(mux.id(), "iterm2");
    }

    #[test]
    fn test_next_window_id() {
        let mux = ITerm2Multiplexer::default();
        let id1 = mux.next_window_id();
        let id2 = mux.next_window_id();
        assert_ne!(id1, id2);
        assert!(id1.starts_with("iterm2-window-"));
        assert!(id2.starts_with("iterm2-window-"));
    }

    #[test]
    fn test_next_pane_id() {
        let mux = ITerm2Multiplexer::default();
        let id1 = mux.next_pane_id();
        let id2 = mux.next_pane_id();
        assert_ne!(id1, id2);
        assert!(id1.starts_with("iterm2-pane-"));
        assert!(id2.starts_with("iterm2-pane-"));
    }
}
