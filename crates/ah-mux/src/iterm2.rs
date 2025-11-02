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
    fn is_iterm2_running(&self) -> bool {
        self.run_applescript(
            r#"tell application "System Events" to (name of processes) contains "iTerm2""#,
        )
        .map(|output| output == "true")
        .unwrap_or(false)
    }

    /// Get the number of windows in iTerm2
    fn get_window_count(&self) -> Result<usize, MuxError> {
        let script = r#"
            tell application "iTerm"
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
            tell application "iTerm"
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
        self.run_applescript(r#"tell application "iTerm" to version"#)
            .is_ok()
    }

    fn open_window(&self, opts: &WindowOptions) -> Result<WindowId, MuxError> {
        let window_id = self.next_window_id();

        let mut script = format!(
            r#"
            tell application "iTerm"
                activate
                set newWindow to (create window with default profile)
        "#
        );

        if let Some(title) = opts.title {
            script.push_str(&format!(
                r#"
                tell current session of current tab of newWindow
                    write text "printf '\\e]1;{}\\a'"
                end tell
            "#,
                title.replace("\"", "\\\"").replace("'", "\\'")
            ));
        }

        script.push_str(
            r#"
                return "window-1"
            end tell
        "#,
        );

        let result = self.run_applescript(&script)?;
        if result.is_empty() {
            return Err(MuxError::CommandFailed(
                "Failed to create window".to_string(),
            ));
        }

        // Give iTerm2 a moment to create the window
        std::thread::sleep(Duration::from_millis(200));

        Ok(window_id)
    }

    fn split_pane(
        &self,
        _window: Option<&WindowId>,
        _target: Option<&PaneId>,
        dir: SplitDirection,
        _percent: Option<u8>,
        _opts: &CommandOptions,
        initial_cmd: Option<&str>,
    ) -> Result<PaneId, MuxError> {
        let pane_id = self.next_pane_id();

        let direction = match dir {
            SplitDirection::Horizontal => "horizontally",
            SplitDirection::Vertical => "vertically",
        };

        let mut script = format!(
            r#"
            tell application "iTerm"
                tell current tab of window 1
                    set newPane to (split {} with default profile)
        "#,
            direction
        );

        if let Some(cmd) = initial_cmd {
            script.push_str(&format!(
                r#"
                    tell newPane
                        write text "{}"
                    end tell
            "#,
                cmd.replace("\"", "\\\"").replace("'", "\\'")
            ));
        }

        script.push_str(
            r#"
                end tell
            end tell
        "#,
        );

        self.run_applescript(&script)?;

        // Give iTerm2 a moment to create the pane
        std::thread::sleep(Duration::from_millis(100));

        Ok(pane_id)
    }

    fn run_command(
        &self,
        pane: &PaneId,
        cmd: &str,
        _opts: &CommandOptions,
    ) -> Result<(), MuxError> {
        // For simplicity, we'll target the current session in the current tab
        // A more sophisticated implementation would track pane IDs
        let script = format!(
            r#"
            tell application "iTerm"
                tell current session of current tab of window 1
                    write text "{}"
                end tell
            end tell
        "#,
            cmd.replace("\"", "\\\"").replace("'", "\\'")
        );

        self.run_applescript(&script)?;
        Ok(())
    }

    fn send_text(&self, pane: &PaneId, text: &str) -> Result<(), MuxError> {
        // Similar to run_command but for sending literal text
        let script = format!(
            r#"
            tell application "iTerm"
                tell current session of current tab of window 1
                    write text "{}"
                end tell
            end tell
        "#,
            text.replace("\"", "\\\"").replace("'", "\\'")
        );

        self.run_applescript(&script)?;
        Ok(())
    }

    fn focus_window(&self, window: &WindowId) -> Result<(), MuxError> {
        let script = r#"
            tell application "iTerm"
                activate
                -- Focus the specified window (simplified - focuses the first window)
                -- In a full implementation, you'd parse the window ID and focus the right one
            end tell
        "#;

        self.run_applescript(script)?;
        Ok(())
    }

    fn focus_pane(&self, pane: &PaneId) -> Result<(), MuxError> {
        // For now, just focus the current session
        let script = r#"
            tell application "iTerm"
                tell current session of current tab of window 1
                    select
                end tell
            end tell
        "#;

        self.run_applescript(script)?;
        Ok(())
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

    #[ah_test_utils::logged_test]
    fn test_id() {
        let mux = ITerm2Multiplexer::default();
        assert_eq!(mux.id(), "iterm2");
    }

    #[ah_test_utils::logged_test]
    fn test_next_window_id() {
        let mux = ITerm2Multiplexer::default();
        let id1 = mux.next_window_id();
        let id2 = mux.next_window_id();
        assert_ne!(id1, id2);
        assert!(id1.starts_with("iterm2-window-"));
        assert!(id2.starts_with("iterm2-window-"));
    }

    #[ah_test_utils::logged_test]
    fn test_next_pane_id() {
        let mux = ITerm2Multiplexer::default();
        let id1 = mux.next_pane_id();
        let id2 = mux.next_pane_id();
        assert_ne!(id1, id2);
        assert!(id1.starts_with("iterm2-pane-"));
        assert!(id2.starts_with("iterm2-pane-"));
    }
}
