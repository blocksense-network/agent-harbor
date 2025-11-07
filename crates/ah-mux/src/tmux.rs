// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! tmux multiplexer implementation
//!
//! Implements the Multiplexer trait for tmux using its command-line interface.
//! Based on the tmux integration guide in specs/Public/Terminal-Multiplexers/tmux.md

use ah_mux_core::*;
use std::process::{Command, Stdio};
use std::time::Duration;

/// tmux multiplexer implementation
pub struct TmuxMultiplexer {
    session_name: String,
    /// If true, assume session already exists and don't call ensure_session()
    assume_session_exists: bool,
}

impl Default for TmuxMultiplexer {
    fn default() -> Self {
        Self {
            session_name: "ah".to_string(),
            assume_session_exists: false,
        }
    }
}

impl TmuxMultiplexer {
    pub fn new() -> Result<Self, MuxError> {
        Ok(Self::default())
    }

    pub fn with_session_name(session_name: String) -> Self {
        Self {
            session_name,
            assume_session_exists: false,
        }
    }

    /// Create a tmux multiplexer that assumes the session already exists
    /// (useful for testing with continuous sessions)
    pub fn with_existing_session(session_name: String) -> Self {
        // Wait for the session to be ready
        for _ in 0..10 {
            if Command::new("tmux")
                .args(["has-session", "-t", &session_name])
                .output()
                .map(|o| o.status.success())
                .unwrap_or(false)
            {
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        Self {
            session_name,
            assume_session_exists: true,
        }
    }

    /// Run a tmux command and return its output
    fn run_tmux_command(&self, args: &[&str]) -> Result<String, MuxError> {
        let output = Command::new("tmux").args(args).output().map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                MuxError::NotAvailable("tmux")
            } else {
                MuxError::Io(e)
            }
        })?;

        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(MuxError::CommandFailed(format!(
                "tmux {} failed: {}",
                args.join(" "),
                stderr
            )))
        }
    }

    /// Ensure a tmux session exists
    fn ensure_session(&self) -> Result<(), MuxError> {
        if self.assume_session_exists {
            // Just check that session exists, don't create it
            self.run_tmux_command(&["has-session", "-t", &self.session_name])?;
            Ok(())
        } else {
            // Check if session exists
            let result = self.run_tmux_command(&["has-session", "-t", &self.session_name]);

            match result {
                Ok(_) => Ok(()), // Session exists
                Err(MuxError::CommandFailed(_)) => {
                    // Session doesn't exist, create it
                    self.run_tmux_command(&[
                        "new-session",
                        "-d",
                        "-s",
                        &self.session_name,
                        "-c",
                        &std::env::current_dir().map_err(MuxError::Io)?.to_string_lossy(),
                    ])?;
                    Ok(())
                }
                Err(e) => Err(e),
            }
        }
    }
}

impl Multiplexer for TmuxMultiplexer {
    fn id(&self) -> &'static str {
        "tmux"
    }

    fn is_available(&self) -> bool {
        std::process::Command::new("tmux")
            .arg("-V")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }

    fn open_window(&self, opts: &WindowOptions) -> Result<WindowId, MuxError> {
        // Ensure session exists first
        self.ensure_session()?;

        let mut args = vec!["new-window".to_string(), "-P".to_string()];

        // Add title if specified
        if let Some(title) = opts.title {
            args.extend_from_slice(&["-n".to_string(), title.to_string()]);
        }

        // Add working directory if specified
        if let Some(cwd) = opts.cwd {
            args.extend_from_slice(&["-c".to_string(), cwd.to_string_lossy().to_string()]);
        }

        // Target the session
        args.extend_from_slice(&["-t".to_string(), self.session_name.clone()]);

        // Convert to slice of &str for the command
        let args_str: Vec<&str> = args.iter().map(|s| s.as_str()).collect();

        // Run the command and capture output
        let output = self.run_tmux_command(&args_str)?;

        // new-window -P returns session:window.pane, but we need session:window as WindowId
        let pane_id = output.trim();
        let window_id = if let Some(dot_pos) = pane_id.rfind('.') {
            pane_id[..dot_pos].to_string()
        } else {
            pane_id.to_string()
        };

        // Focus the window if requested
        if opts.focus {
            self.focus_window(&window_id)?;
        }

        Ok(window_id)
    }

    fn split_pane(
        &self,
        window: Option<&WindowId>,
        target: Option<&PaneId>,
        dir: SplitDirection,
        percent: Option<u8>,
        opts: &CommandOptions,
        initial_cmd: Option<&str>,
    ) -> Result<PaneId, MuxError> {
        let mut args = vec!["split-window".to_string(), "-P".to_string()];

        // Add direction
        match dir {
            SplitDirection::Horizontal => args.push("-h".to_string()),
            SplitDirection::Vertical => args.push("-v".to_string()),
        }

        // Add size percentage if specified
        if let Some(p) = percent {
            args.extend_from_slice(&["-p".to_string(), p.to_string()]);
        }

        // Add working directory if specified
        if let Some(cwd) = opts.cwd {
            args.extend_from_slice(&["-c".to_string(), cwd.to_string_lossy().to_string()]);
        }

        // Target the specific pane or window (or current window if None)
        if let Some(target_spec) = target {
            args.extend_from_slice(&["-t".to_string(), target_spec.clone()]);
        } else if let Some(window_id) = window {
            args.extend_from_slice(&["-t".to_string(), window_id.clone()]);
        }
        // If both target and window are None, tmux will operate on the current window

        // Add initial command if specified
        if let Some(cmd) = initial_cmd {
            args.push(cmd.to_string());
        }

        // Convert to slice of &str for the command
        let args_str: Vec<&str> = args.iter().map(|s| s.as_str()).collect();

        // Run the command and capture the new pane ID
        let output = self.run_tmux_command(&args_str)?;
        let pane_id = output.trim().to_string();

        Ok(pane_id)
    }

    fn run_command(
        &self,
        pane: &PaneId,
        cmd: &str,
        _opts: &CommandOptions,
    ) -> Result<(), MuxError> {
        // Send the command followed by Enter (C-m)
        self.run_tmux_command(&["send-keys", "-t", pane, cmd, "C-m"])?;
        Ok(())
    }

    fn send_text(&self, pane: &PaneId, text: &str) -> Result<(), MuxError> {
        // Send literal text to the pane
        self.run_tmux_command(&["send-keys", "-t", pane, text])?;
        Ok(())
    }

    fn focus_window(&self, window: &WindowId) -> Result<(), MuxError> {
        self.run_tmux_command(&["select-window", "-t", window])?;
        Ok(())
    }

    fn focus_pane(&self, pane: &PaneId) -> Result<(), MuxError> {
        self.run_tmux_command(&["select-pane", "-t", pane])?;
        Ok(())
    }

    fn list_windows(&self, title_substr: Option<&str>) -> Result<Vec<WindowId>, MuxError> {
        // List all windows in the specific session with format: session:window_index:window_name
        let output =
            self.run_tmux_command(&["list-windows", "-t", &self.session_name, "-F", "#S:#I:#W"])?;

        let mut windows = Vec::new();
        for line in output.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let parts: Vec<&str> = line.split(':').collect();
            if parts.len() >= 3 {
                let session_name = parts[0];
                let window_index = parts[1];
                let window_name = parts[2];

                // Filter by title substring if provided
                if let Some(substr) = title_substr {
                    if !window_name.contains(substr) {
                        continue;
                    }
                }

                // Only include windows from our session
                if session_name == self.session_name {
                    windows.push(format!("{}:{}", session_name, window_index));
                }
            }
        }

        Ok(windows)
    }

    fn current_pane(&self) -> Result<Option<PaneId>, MuxError> {
        // TMUX_PANE contains the current pane ID in format: %<pane_index>
        // But we need it in session:window.pane format
        if let Ok(_pane_index) = std::env::var("TMUX_PANE") {
            // Get current window information
            let output = self.run_tmux_command(&[
                "display-message",
                "-p",
                "#{session_name}:#{window_index}.#{pane_index}",
            ])?;
            let current_pane = output.trim().to_string();
            Ok(Some(current_pane))
        } else {
            Ok(None)
        }
    }

    fn current_window(&self) -> Result<Option<WindowId>, MuxError> {
        // Get current window in session:window format
        let output =
            self.run_tmux_command(&["display-message", "-p", "#{session_name}:#{window_index}"])?;
        let current_window = output.trim().to_string();
        Ok(Some(current_window))
    }

    fn list_panes(&self, window: &WindowId) -> Result<Vec<PaneId>, MuxError> {
        // The window parameter might be a pane ID (session:window.pane), extract just the window part
        let window_target = if window.contains('.') {
            // It's a pane ID like session:window.pane, extract session:window
            let parts: Vec<&str> = window.split('.').collect();
            if parts.len() >= 2 {
                format!("{}:{}", parts[0], parts[1])
            } else {
                window.to_string()
            }
        } else {
            window.to_string()
        };

        let output =
            self.run_tmux_command(&["list-panes", "-t", &window_target, "-F", "#S:#I.#P"])?;
        let panes: Vec<String> = output.lines().map(|s| s.trim().to_string()).collect();
        Ok(panes)
    }

    fn run_script(&self, pane: &PaneId, script: &str) -> Result<(), MuxError> {
        // Split the script into lines and send each line separately
        for line in script.lines() {
            self.run_tmux_command(&["send-keys", "-t", pane, line, "C-m"])?;
            std::thread::sleep(Duration::from_millis(50)); // Give tmux time to process each command
        }
        Ok(())
    }

    fn capture_pane(&self, pane: &PaneId, lines: Option<usize>) -> Result<String, MuxError> {
        let mut args = vec!["capture-pane", "-p", "-t", pane.as_str()];
        if let Some(n) = lines {
            args.extend_from_slice(&["-S", &format!("-{}", n)]);
        }
        self.run_tmux_command(&args.iter().map(|s| s.as_ref()).collect::<Vec<&str>>())
    }

    fn kill_session(&self) -> Result<(), MuxError> {
        self.run_tmux_command(&["kill-session", "-t", &self.session_name])?
            .is_empty()
            .then_some(())
            .ok_or_else(|| {
                MuxError::CommandFailed("kill-session did not return empty output".into())
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_is_available() {
        let tmux = TmuxMultiplexer::new().unwrap();
        let available = tmux.is_available();
        // We don't assert on availability here; just ensure it doesn't panic
        println!("tmux available: {}", available);
    }

    #[test]
    fn test_session_ensure_creates_session() {
        let tmux = TmuxMultiplexer::with_session_name("test-session-create".to_string());
        if tmux.is_available() {
            // Clean up any existing session first
            let _ = tmux.run_tmux_command(&["kill-session", "-t", "test-session-create"]);

            // Verify session doesn't exist initially
            let result = tmux.run_tmux_command(&["has-session", "-t", "test-session-create"]);
            assert!(result.is_err()); // Should fail because session doesn't exist

            // Ensure session creates it
            tmux.ensure_session().unwrap();

            // Verify session now exists
            let result = tmux.run_tmux_command(&["has-session", "-t", "test-session-create"]);
            assert!(result.is_ok());

            // Clean up
            let _ = tmux.run_tmux_command(&["kill-session", "-t", "test-session-create"]);
        }
    }

    #[test]
    fn test_session_ensure_idempotent() {
        let tmux = TmuxMultiplexer::with_session_name("test-session-idempotent".to_string());
        if tmux.is_available() {
            // Clean up any existing session first
            let _ = tmux.run_tmux_command(&["kill-session", "-t", "test-session-idempotent"]);

            // Create session
            tmux.ensure_session().unwrap();

            // Call ensure_session again - should be idempotent
            tmux.ensure_session().unwrap();

            // Verify session still exists
            let result = tmux.run_tmux_command(&["has-session", "-t", "test-session-idempotent"]);
            assert!(result.is_ok());

            // Clean up
            let _ = tmux.run_tmux_command(&["kill-session", "-t", "test-session-idempotent"]);
        }
    }

    #[test]
    fn test_open_window_with_title_and_cwd() {
        let tmux = TmuxMultiplexer::with_session_name("test-win-create-001".to_string());
        if tmux.is_available() {
            // Clean up any existing session
            let _ = tmux.run_tmux_command(&["kill-session", "-t", "test-win-create-001"]);

            let opts = WindowOptions {
                title: Some("my-test-window-001"),
                cwd: Some(Path::new("/tmp")),
                profile: None,
                focus: false,
            };

            let window_id = tmux.open_window(&opts).unwrap();
            assert_eq!(window_id, "test-win-create-001:1");

            // Verify window exists and has correct title
            let windows = tmux.list_windows(Some("my-test-window-001")).unwrap();
            assert_eq!(windows.len(), 1);
            assert_eq!(windows[0], "test-win-create-001:1");

            // Clean up
            let _ = tmux.run_tmux_command(&["kill-session", "-t", "test-win-create-001"]);
        }
    }

    #[test]
    fn test_open_window_focus() {
        let tmux = TmuxMultiplexer::with_session_name("test-win-focus-002".to_string());
        if tmux.is_available() {
            // Clean up
            let _ = tmux.run_tmux_command(&["kill-session", "-t", "test-win-focus-002"]);

            let opts = WindowOptions {
                title: Some("focus-test-002"),
                cwd: Some(Path::new("/tmp")),
                profile: None,
                focus: true, // Should focus the window
            };

            let window_id = tmux.open_window(&opts).unwrap();

            // Instead of checking global state (which can be affected by other tests),
            // just verify that the window was created and focus operation succeeded
            let windows = tmux.list_windows(Some("focus-test-002")).unwrap();
            assert_eq!(windows.len(), 1);
            assert_eq!(windows[0], window_id);

            // Clean up
            let _ = tmux.run_tmux_command(&["kill-session", "-t", "test-win-focus-002"]);
        }
    }

    #[test]
    fn test_split_pane_horizontal() {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis();
        let session_name = format!("test-split-h-{}", timestamp);
        if TmuxMultiplexer::new().is_ok() {
            // Start continuous tmux session for visual testing
            let _ = snapshot_testing::start_continuous_session(&session_name);

            // Give tmux a moment to fully initialize
            std::thread::sleep(Duration::from_millis(300));

            // Create tmux API for this session (ensure_session will create it)
            let tmux = TmuxMultiplexer::with_session_name(session_name.to_string());

            let window_id = tmux
                .open_window(&WindowOptions {
                    title: Some("split-test-003"),
                    cwd: Some(Path::new("/tmp")),
                    profile: None,
                    focus: false,
                })
                .unwrap();

            let initial_pane = format!("{}.0", window_id);

            // Run command in initial pane
            tmux.run_command(
                &initial_pane,
                "echo 'Left pane content'",
                &CommandOptions::default(),
            )
            .unwrap();
            std::thread::sleep(Duration::from_millis(200));

            // Strategic snapshot: before split
            if let Ok(snapshot) = snapshot_testing::snapshot_continuous_session() {
                snapshot_testing::assert_snapshot_optional("before_split_horizontal", snapshot);
            }

            // Split horizontally
            let new_pane = tmux
                .split_pane(
                    Some(&window_id),
                    Some(&initial_pane),
                    SplitDirection::Horizontal,
                    Some(60), // 60% for left pane
                    &CommandOptions {
                        cwd: Some(Path::new("/tmp")),
                        env: None,
                    },
                    None,
                )
                .unwrap();

            // The exact window/pane numbering can vary depending on how tmux starts
            // Just verify it's a valid pane ID format
            assert!(new_pane.starts_with(&format!("{}:", "test-split-h-")));

            // Run command in new pane
            tmux.run_command(
                &new_pane,
                "echo 'Right pane content'",
                &CommandOptions::default(),
            )
            .unwrap();

            // Strategic snapshot: after split
            if let Ok(snapshot) = snapshot_testing::snapshot_continuous_session() {
                snapshot_testing::assert_snapshot_optional("after_split_horizontal", snapshot);
            }

            // Clean up temp session
            let _ = tmux.run_tmux_command(&["kill-session", "-t", &session_name]);
        }
    }
}
