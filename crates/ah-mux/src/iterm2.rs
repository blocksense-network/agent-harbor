// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! iTerm2 multiplexer implementation
//!
//! Implements the Multiplexer trait for iTerm2 using its AppleScript interface.
//! Based on the iTerm2 integration guide in specs/Public/Terminal-Multiplexers/iTerm2.md

use ah_mux_core::*;
use std::process::{Command, Stdio};
use std::time::Duration;

/// iTerm2 multiplexer implementation using AppleScript
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
    pub fn new() -> Result<Self, MuxError> {
        let mux = Self::default();
        if !mux.is_available() {
            return Err(MuxError::NotAvailable("iTerm2"));
        }
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
    fn run_applescript(&self, script: &str) -> Result<String, MuxError> {
        let child = Command::new("osascript")
            .arg("-e")
            .arg(script)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| MuxError::CommandFailed(format!("Failed to run osascript: {}", e)))?;

        let output = child
            .wait_with_output()
            .map_err(|e| MuxError::CommandFailed(format!("Failed to wait for osascript: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(MuxError::CommandFailed(format!(
                "osascript failed: {}",
                stderr
            )));
        }

        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
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
    fn get_window_count(&self) -> Result<usize, MuxError> {
        let script = r#"
            tell application "iTerm2"
                count of windows
            end tell
        "#;
        self.run_applescript(script)?
            .parse()
            .map_err(|_| MuxError::Other("Failed to parse window count".to_string()))
    }

    /// Find a window by title substring
    fn find_window_by_title(&self, title_substr: &str) -> Result<Option<WindowId>, MuxError> {
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
            Ok(None)
        } else {
            Ok(Some(result))
        }
    }
}

impl Multiplexer for ITerm2Multiplexer {
    fn id(&self) -> &'static str {
        "iterm2"
    }

    fn is_available(&self) -> bool {
        // Check if osascript is available
        Command::new("osascript")
            .arg("-e")
            .arg("return \"test\"")
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
        &&
        // Check if iTerm2 is installed (try to get its version)
        self.run_applescript(r#"tell application "iTerm2" to version"#)
            .is_ok()
    }

    fn open_window(&self, opts: &WindowOptions) -> Result<WindowId, MuxError> {
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

        Ok(window_id)
    }

    fn split_pane(
        &self,
        _window: Option<&WindowId>,
        _target: Option<&PaneId>,
        dir: SplitDirection,
        _percent: Option<u8>,
        opts: &CommandOptions,
        initial_cmd: Option<&str>,
    ) -> Result<PaneId, MuxError> {
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

        Ok(pane_id)
    }

    fn run_command(&self, pane: &PaneId, cmd: &str, opts: &CommandOptions) -> Result<(), MuxError> {
        // Determine which session to target based on the pane ID
        // Pane IDs ending with ".0" are the left pane (current session)
        // Other pane IDs are the right pane (second session)
        let session_target = if pane.ends_with(".0") {
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
        Ok(())
    }

    fn send_text(&self, pane: &PaneId, text: &str) -> Result<(), MuxError> {
        // Determine which session to target based on the pane ID
        let session_target = if pane.ends_with(".0") {
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
        Ok(())
    }

    fn focus_window(&self, _window: &WindowId) -> Result<(), MuxError> {
        let script = r#"
            tell application "iTerm2"
                activate
                -- Focus the specified window (simplified - focuses the first window)
                -- In a full implementation, you'd parse the window ID and focus the right one
            end tell
        "#;

        self.run_applescript(script)?;
        Ok(())
    }

    fn focus_pane(&self, _pane: &PaneId) -> Result<(), MuxError> {
        // For now, just focus the current session
        let script = r#"
            tell application "iTerm2"
                tell current session of current tab of window 1
                    select
                end tell
            end tell
        "#;

        self.run_applescript(script)?;
        Ok(())
    }

    fn current_window(&self) -> Result<Option<WindowId>, MuxError> {
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
            Ok(None)
        } else {
            Ok(Some(result.trim().to_string()))
        }
    }

    fn list_windows(&self, title_substr: Option<&str>) -> Result<Vec<WindowId>, MuxError> {
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

        Ok(windows)
    }

    fn list_panes(&self, _window: &WindowId) -> Result<Vec<PaneId>, MuxError> {
        // For simplicity, return a basic pane ID
        // A full implementation would enumerate actual panes in the window
        Ok(vec!["pane-1".to_string()])
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
