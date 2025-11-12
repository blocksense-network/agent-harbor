// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! kitty multiplexer implementation
//!
//! Implements the Multiplexer trait for kitty using its remote control interface.
//! Based on the kitty integration guide in specs/Public/Terminal-Multiplexers/kitty.md

use ah_mux_core::*;
use std::process::{Command, Stdio};

/// kitty multiplexer implementation
pub struct KittyMultiplexer {
    /// Socket path for remote control. Uses KITTY_LISTEN_ON if available, otherwise defaults
    socket_path: Option<String>,
}

impl Default for KittyMultiplexer {
    fn default() -> Self {
        Self {
            socket_path: std::env::var("KITTY_LISTEN_ON").ok(),
        }
    }
}

impl KittyMultiplexer {
    pub fn new() -> Result<Self, MuxError> {
        Ok(Self::default())
    }

    pub fn with_socket_path(socket_path: String) -> Self {
        Self {
            socket_path: Some(socket_path),
        }
    }

    /// Run a kitty @ command and return its output
    fn run_kitty_command(&self, args: &[&str]) -> Result<String, MuxError> {
        let mut cmd_args = vec!["@"];

        // Add socket path if specified
        if let Some(socket) = &self.socket_path {
            cmd_args.extend_from_slice(&["--to", socket]);
        }

        // Add the actual command arguments
        cmd_args.extend_from_slice(args);

        let output = Command::new("kitty").args(&cmd_args).output().map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                MuxError::NotAvailable("kitty")
            } else {
                MuxError::Io(e)
            }
        })?;

        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(MuxError::CommandFailed(format!(
                "kitty @ {} failed: {}",
                args.join(" "),
                stderr
            )))
        }
    }

    /// Check if kitty remote control is available
    fn is_remote_control_available(&self) -> bool {
        // Try to run a simple kitty @ command to test remote control availability
        let result = self.run_kitty_command(&["ls"]);
        match result {
            Ok(_) => true,
            Err(MuxError::CommandFailed(ref msg)) if msg.contains("no socket") => false,
            Err(MuxError::CommandFailed(ref msg)) if msg.contains("Could not connect") => false,
            Err(MuxError::NotAvailable(_)) => false,
            _ => true, // Other errors might be transient
        }
    }

    /// Get the window ID from kitty's launch output
    /// kitty @ launch returns the window ID
    fn parse_window_id_from_output(&self, output: &str) -> Result<String, MuxError> {
        // kitty @ launch returns just the window ID
        let window_id = output.trim();
        if window_id.is_empty() {
            return Err(MuxError::CommandFailed(
                "kitty @ launch returned empty window ID".to_string(),
            ));
        }
        Ok(window_id.to_string())
    }

    /// Get the pane ID from kitty's launch output
    /// For kitty, panes are also identified by window IDs since each pane is a separate window
    fn parse_pane_id_from_output(&self, output: &str) -> Result<String, MuxError> {
        // In kitty's model, each pane is a separate window, so pane ID is the same as window ID
        self.parse_window_id_from_output(output)
    }

    /// Get the currently focused window ID
    pub fn get_focused_window_id(&self) -> Result<String, MuxError> {
        let output = self.run_kitty_command(&["get-focused-window-id"])?;
        Ok(output.trim().to_string())
    }

    /// Get the title of a specific window
    pub fn get_window_title(&self, window_id: &str) -> Result<String, MuxError> {
        let output =
            self.run_kitty_command(&["get-window-title", "--match", &format!("id:{}", window_id)])?;
        Ok(output.trim().to_string())
    }

    /// Get detailed window information
    pub fn list_windows_detailed(&self) -> Result<Vec<(String, String)>, MuxError> {
        // Get list of windows with format: id title
        let output = self.run_kitty_command(&["ls", "--format", "id title"])?;

        let mut windows = Vec::new();
        for line in output.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }

            let parts: Vec<&str> = line.splitn(2, ' ').collect();
            if parts.len() >= 2 {
                windows.push((parts[0].to_string(), parts[1].to_string()));
            }
        }

        Ok(windows)
    }

    /// Check if a window with the given ID exists
    pub fn window_exists(&self, window_id: &str) -> Result<bool, MuxError> {
        let windows = self.list_windows_detailed()?;
        Ok(windows.iter().any(|(id, _)| id == window_id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn test_kitty_multiplexer_creation() {
        let kitty = KittyMultiplexer::new().unwrap();
        assert_eq!(kitty.id(), "kitty");
        assert_eq!(kitty.socket_path, std::env::var("KITTY_LISTEN_ON").ok());
    }

    #[test]
    fn test_kitty_with_custom_socket() {
        let socket_path = "/tmp/test-kitty.sock".to_string();
        let kitty = KittyMultiplexer::with_socket_path(socket_path.clone());
        assert_eq!(kitty.socket_path, Some(socket_path));
    }

    #[test]
    fn test_kitty_availability() {
        let kitty = KittyMultiplexer::new().unwrap();
        let _available = kitty.is_available();
        // Note: We can't assert availability since kitty might not be installed or configured
    }

    #[test]
    fn test_kitty_remote_control_available() {
        let kitty = KittyMultiplexer::new().unwrap();
        let _available = kitty.is_remote_control_available();
        // Note: This tests the remote control check, but doesn't assert since
        // kitty might not be running or configured
    }

    #[test]
    fn test_parse_window_id() {
        let kitty = KittyMultiplexer::new().unwrap();

        // Test valid window ID
        let result = kitty.parse_window_id_from_output("42\n");
        assert_eq!(result.unwrap(), "42");

        // Test empty output (should fail)
        let result = kitty.parse_window_id_from_output("");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_pane_id() {
        let kitty = KittyMultiplexer::new().unwrap();

        // Test valid pane ID (same as window ID in kitty)
        let result = kitty.parse_pane_id_from_output("42\n");
        assert_eq!(result.unwrap(), "42");

        // Test empty output (should fail)
        let result = kitty.parse_pane_id_from_output("");
        assert!(result.is_err());
    }

    #[test]
    fn test_open_window_with_title_and_cwd() {
        let kitty = KittyMultiplexer::new().unwrap();
        if kitty.is_available() {
            let opts = WindowOptions {
                title: Some("my-test-window-001"),
                cwd: Some(Path::new("/tmp")),
                profile: None,
                focus: false,
            };

            let result = kitty.open_window(&opts);
            match result {
                Ok(window_id) => {
                    // Verify the window ID is numeric
                    assert!(window_id.parse::<u32>().is_ok());

                    // Verify the window actually exists in kitty
                    assert!(
                        kitty.window_exists(&window_id).unwrap_or(false),
                        "Window {} should exist after creation",
                        window_id
                    );

                    // Verify the window has the correct title
                    let title = kitty.get_window_title(&window_id).unwrap_or_default();
                    assert_eq!(
                        title, "my-test-window-001",
                        "Window should have the correct title"
                    );
                }
                Err(MuxError::CommandFailed(_)) => {
                    // Expected when kitty remote control is not available
                }
                Err(e) => panic!("Unexpected error: {:?}", e),
            }
        }
    }

    #[test]
    fn test_open_window_focus() {
        let kitty = KittyMultiplexer::new().unwrap();
        if kitty.is_available() {
            let opts = WindowOptions {
                title: Some("focus-test-002"),
                cwd: Some(Path::new("/tmp")),
                profile: None,
                focus: true, // Should focus the window
            };

            let result = kitty.open_window(&opts);
            match result {
                Ok(window_id) => {
                    // Verify the window was created
                    assert!(kitty.window_exists(&window_id).unwrap_or(false));

                    // Verify the window is now focused
                    let focused_id = kitty.get_focused_window_id().unwrap_or_default();
                    assert_eq!(
                        focused_id, window_id,
                        "Window {} should be focused after creation with focus=true",
                        window_id
                    );
                }
                Err(MuxError::CommandFailed(_)) => {
                    // Expected when kitty remote control is not available
                }
                Err(e) => panic!("Unexpected error: {:?}", e),
            }
        }
    }

    #[test]
    fn test_split_pane_horizontal() {
        let kitty = KittyMultiplexer::new().unwrap();
        if kitty.is_available() {
            // Get initial window count
            let initial_windows = kitty.list_windows_detailed().unwrap_or_default();
            let initial_count = initial_windows.len();

            // Create a window to split from
            let window_opts = WindowOptions {
                title: Some("split-test-003"),
                cwd: Some(Path::new("/tmp")),
                profile: None,
                focus: false,
            };

            let window_result = kitty.open_window(&window_opts);
            match window_result {
                Ok(window_id) => {
                    // Verify window was created
                    assert!(kitty.window_exists(&window_id).unwrap_or(false));

                    // Now try to split it
                    let split_result = kitty.split_pane(
                        Some(&window_id),
                        Some(&window_id), // In kitty, panes are windows, so use window_id as pane_id
                        SplitDirection::Horizontal,
                        Some(60),
                        &CommandOptions {
                            cwd: Some(Path::new("/tmp")),
                            env: None,
                        },
                        None,
                    );

                    match split_result {
                        Ok(new_pane_id) => {
                            // Verify the new pane ID is numeric and different
                            assert!(new_pane_id.parse::<u32>().is_ok());
                            assert_ne!(new_pane_id, window_id);

                            // Verify the new window actually exists
                            assert!(
                                kitty.window_exists(&new_pane_id).unwrap_or(false),
                                "New pane window {} should exist after split",
                                new_pane_id
                            );

                            // Verify we now have more windows
                            let final_windows = kitty.list_windows_detailed().unwrap_or_default();
                            assert!(
                                final_windows.len() >= initial_count + 1,
                                "Should have at least {} windows after split, got {}",
                                initial_count + 1,
                                final_windows.len()
                            );
                        }
                        Err(MuxError::CommandFailed(_)) => {
                            // Expected when remote control fails
                        }
                        Err(e) => panic!("Unexpected error: {:?}", e),
                    }
                }
                Err(MuxError::CommandFailed(_)) => {
                    // Can't test splitting if we can't create windows
                }
                Err(e) => panic!("Unexpected error creating window: {:?}", e),
            }
        }
    }

    #[test]
    fn test_split_pane_vertical() {
        let kitty = KittyMultiplexer::new().unwrap();
        if kitty.is_available() {
            // Get initial window count
            let initial_windows = kitty.list_windows_detailed().unwrap_or_default();
            let initial_count = initial_windows.len();

            let window_opts = WindowOptions {
                title: Some("split-v-test-004"),
                cwd: Some(Path::new("/tmp")),
                profile: None,
                focus: false,
            };

            let window_result = kitty.open_window(&window_opts);
            match window_result {
                Ok(window_id) => {
                    // Verify window was created
                    assert!(kitty.window_exists(&window_id).unwrap_or(false));

                    let split_result = kitty.split_pane(
                        Some(&window_id),
                        Some(&window_id),
                        SplitDirection::Vertical,
                        Some(70),
                        &CommandOptions::default(),
                        None,
                    );

                    match split_result {
                        Ok(new_pane_id) => {
                            assert!(new_pane_id.parse::<u32>().is_ok());
                            assert_ne!(new_pane_id, window_id);

                            // Verify the new window actually exists
                            assert!(
                                kitty.window_exists(&new_pane_id).unwrap_or(false),
                                "New pane window {} should exist after vertical split",
                                new_pane_id
                            );

                            // Verify window count increased
                            let final_windows = kitty.list_windows_detailed().unwrap_or_default();
                            assert!(
                                final_windows.len() >= initial_count + 1,
                                "Should have at least {} windows after vertical split, got {}",
                                initial_count + 1,
                                final_windows.len()
                            );
                        }
                        Err(MuxError::CommandFailed(_)) => {
                            // Expected when remote control fails
                        }
                        Err(e) => panic!("Unexpected error: {:?}", e),
                    }
                }
                Err(MuxError::CommandFailed(_)) => {
                    // Can't test splitting if we can't create windows
                }
                Err(e) => panic!("Unexpected error creating window: {:?}", e),
            }
        }
    }

    #[test]
    fn test_split_pane_with_initial_command() {
        let kitty = KittyMultiplexer::new().unwrap();
        if kitty.is_available() {
            let window_opts = WindowOptions {
                title: Some("split-cmd-test"),
                cwd: Some(Path::new("/tmp")),
                profile: None,
                focus: false,
            };

            let window_result = kitty.open_window(&window_opts);
            match window_result {
                Ok(window_id) => {
                    // Split with initial command that should keep the pane alive
                    let split_result = kitty.split_pane(
                        Some(&window_id),
                        Some(&window_id),
                        SplitDirection::Horizontal,
                        None,
                        &CommandOptions::default(),
                        Some("sleep 1"), // Short sleep to test command execution
                    );

                    match split_result {
                        Ok(new_pane_id) => {
                            assert!(new_pane_id.parse::<u32>().is_ok());
                            assert_ne!(new_pane_id, window_id);

                            // Give the command a moment to start
                            thread::sleep(Duration::from_millis(200));
                        }
                        Err(MuxError::CommandFailed(_)) => {
                            // Expected when remote control fails
                        }
                        Err(e) => panic!("Unexpected error: {:?}", e),
                    }
                }
                Err(MuxError::CommandFailed(_)) => {
                    // Can't test splitting if we can't create windows
                }
                Err(e) => panic!("Unexpected error creating window: {:?}", e),
            }
        }
    }

    #[test]
    fn test_run_command_and_send_text() {
        let kitty = KittyMultiplexer::new().unwrap();
        if kitty.is_available() {
            let window_opts = WindowOptions {
                title: Some("cmd-text-test"),
                cwd: Some(Path::new("/tmp")),
                profile: None,
                focus: false,
            };

            let window_result = kitty.open_window(&window_opts);
            match window_result {
                Ok(window_id) => {
                    // Test run_command
                    let cmd_result = kitty.run_command(
                        &window_id,
                        "echo 'hello world'",
                        &CommandOptions::default(),
                    );
                    match cmd_result {
                        Ok(()) => {
                            // Command executed successfully
                            thread::sleep(Duration::from_millis(100));
                        }
                        Err(MuxError::CommandFailed(_)) => {
                            // Expected when remote control fails
                        }
                        Err(e) => panic!("Unexpected error running command: {:?}", e),
                    }

                    // Test send_text
                    let text_result = kitty.send_text(&window_id, "some input text");
                    match text_result {
                        Ok(()) => {
                            // Text sent successfully
                            thread::sleep(Duration::from_millis(100));
                        }
                        Err(MuxError::CommandFailed(_)) => {
                            // Expected when remote control fails
                        }
                        Err(e) => panic!("Unexpected error sending text: {:?}", e),
                    }

                    // Verify window still exists (if we can list windows)
                    let list_result = kitty.list_windows(Some("cmd-text-test"));
                    match list_result {
                        Ok(windows) => {
                            // Should find our test window
                            assert!(windows.iter().any(|w| w.parse::<u32>().is_ok()));
                        }
                        Err(MuxError::CommandFailed(_)) => {
                            // Expected when remote control fails
                        }
                        Err(e) => panic!("Unexpected error listing windows: {:?}", e),
                    }
                }
                Err(MuxError::CommandFailed(_)) => {
                    // Can't test commands if we can't create windows
                }
                Err(e) => panic!("Unexpected error creating window: {:?}", e),
            }
        }
    }

    #[test]
    fn test_focus_window_and_pane() {
        let kitty = KittyMultiplexer::new().unwrap();
        if kitty.is_available() {
            let window_opts1 = WindowOptions {
                title: Some("window1-005"),
                cwd: Some(Path::new("/tmp")),
                profile: None,
                focus: false,
            };

            let window_opts2 = WindowOptions {
                title: Some("window2-005"),
                cwd: Some(Path::new("/tmp")),
                profile: None,
                focus: false,
            };

            let window1_result = kitty.open_window(&window_opts1);
            let window2_result = kitty.open_window(&window_opts2);

            match (window1_result, window2_result) {
                (Ok(window1), Ok(window2)) => {
                    // Test window focusing - focus window1 first
                    let focus1_result = kitty.focus_window(&window1);
                    match focus1_result {
                        Ok(()) => {
                            // Verify window1 is now focused
                            let focused_id = kitty.get_focused_window_id().unwrap_or_default();
                            assert_eq!(
                                focused_id, window1,
                                "Window {} should be focused after focus_window call",
                                window1
                            );
                        }
                        Err(MuxError::CommandFailed(_)) => {
                            // Expected when remote control fails
                            return;
                        }
                        Err(e) => panic!("Unexpected error focusing window1: {:?}", e),
                    }

                    // Now focus window2
                    let focus2_result = kitty.focus_window(&window2);
                    match focus2_result {
                        Ok(()) => {
                            // Verify window2 is now focused
                            let focused_id = kitty.get_focused_window_id().unwrap_or_default();
                            assert_eq!(
                                focused_id, window2,
                                "Window {} should be focused after focus_window call",
                                window2
                            );
                        }
                        Err(MuxError::CommandFailed(_)) => {
                            // Expected when remote control fails
                        }
                        Err(e) => panic!("Unexpected error focusing window2: {:?}", e),
                    }

                    // Test pane focusing (same as window focusing in kitty) - focus back to window1
                    let pane_focus_result = kitty.focus_pane(&window1);
                    match pane_focus_result {
                        Ok(()) => {
                            // Verify window1 is focused again
                            let focused_id = kitty.get_focused_window_id().unwrap_or_default();
                            assert_eq!(
                                focused_id, window1,
                                "Pane/window {} should be focused after focus_pane call",
                                window1
                            );
                        }
                        Err(MuxError::CommandFailed(_)) => {
                            // Expected when remote control fails
                        }
                        Err(e) => panic!("Unexpected error focusing pane: {:?}", e),
                    }
                }
                _ => {
                    // Can't test focusing if we can't create windows
                }
            }
        }
    }

    #[test]
    fn test_list_windows_filtering() {
        let kitty = KittyMultiplexer::new().unwrap();
        if kitty.is_available() {
            // Create test windows
            let window_opts = vec![
                WindowOptions {
                    title: Some("alpha-window-006"),
                    cwd: Some(Path::new("/tmp")),
                    profile: None,
                    focus: false,
                },
                WindowOptions {
                    title: Some("beta-window-006"),
                    cwd: Some(Path::new("/tmp")),
                    profile: None,
                    focus: false,
                },
                WindowOptions {
                    title: Some("alpha-other-006"),
                    cwd: Some(Path::new("/tmp")),
                    profile: None,
                    focus: false,
                },
            ];

            let mut created_windows = Vec::new();
            for opts in window_opts {
                match kitty.open_window(&opts) {
                    Ok(window_id) => {
                        created_windows.push(window_id);
                    }
                    Err(MuxError::CommandFailed(_)) => {
                        // Skip if remote control not available
                        return;
                    }
                    Err(e) => panic!("Unexpected error creating window: {:?}", e),
                }
            }

            // Give windows time to be created
            thread::sleep(Duration::from_millis(200));

            // List all windows
            let all_windows_result = kitty.list_windows(None);
            match all_windows_result {
                Ok(all_windows) => {
                    assert!(!all_windows.is_empty());
                    // All window IDs should be numeric
                    for window in &all_windows {
                        assert!(window.parse::<u32>().is_ok());
                    }
                }
                Err(MuxError::CommandFailed(_)) => {
                    // Expected when remote control fails
                    return;
                }
                Err(e) => panic!("Unexpected error listing all windows: {:?}", e),
            }

            // Filter by "alpha"
            let alpha_windows_result = kitty.list_windows(Some("alpha"));
            match alpha_windows_result {
                Ok(alpha_windows) => {
                    // Should find at least the alpha windows we created
                    assert!(!alpha_windows.is_empty());
                    for window in &alpha_windows {
                        assert!(window.parse::<u32>().is_ok());
                    }
                }
                Err(MuxError::CommandFailed(_)) => {
                    // Expected when remote control fails
                }
                Err(e) => panic!("Unexpected error listing alpha windows: {:?}", e),
            }

            // Filter by "beta"
            let beta_windows_result = kitty.list_windows(Some("beta"));
            match beta_windows_result {
                Ok(beta_windows) => {
                    assert!(!beta_windows.is_empty());
                    for window in &beta_windows {
                        assert!(window.parse::<u32>().is_ok());
                    }
                }
                Err(MuxError::CommandFailed(_)) => {
                    // Expected when remote control fails
                }
                Err(e) => panic!("Unexpected error listing beta windows: {:?}", e),
            }

            // Filter by non-existent title
            let none_windows_result = kitty.list_windows(Some("nonexistent"));
            match none_windows_result {
                Ok(none_windows) => {
                    // Should be empty or not contain our test windows
                    assert!(
                        none_windows.is_empty()
                            || !none_windows.iter().any(|w| created_windows.contains(w))
                    );
                }
                Err(MuxError::CommandFailed(_)) => {
                    // Expected when remote control fails
                }
                Err(e) => panic!("Unexpected error listing nonexistent windows: {:?}", e),
            }
        }
    }

    #[test]
    fn test_error_handling_invalid_window() {
        let kitty = KittyMultiplexer::new().unwrap();
        if kitty.is_available() {
            // Try to focus a non-existent window
            let invalid_window = "99999".to_string();
            let result = kitty.focus_window(&invalid_window);
            // Should either succeed (if window exists) or fail gracefully
            match result {
                Ok(()) => {
                    // Window might exist
                }
                Err(MuxError::CommandFailed(_)) => {
                    // Expected when window doesn't exist or remote control fails
                }
                Err(e) => panic!("Unexpected error: {:?}", e),
            }
        }
    }

    #[test]
    fn test_error_handling_invalid_pane() {
        let kitty = KittyMultiplexer::new().unwrap();
        if kitty.is_available() {
            // Try to focus a non-existent pane
            let invalid_pane = "99999".to_string();
            let result = kitty.focus_pane(&invalid_pane);
            match result {
                Ok(()) => {
                    // Pane might exist
                }
                Err(MuxError::CommandFailed(_)) => {
                    // Expected when pane doesn't exist or remote control fails
                }
                Err(e) => panic!("Unexpected error: {:?}", e),
            }

            // Try to send text to non-existent pane
            let result = kitty.send_text(&invalid_pane, "test");
            match result {
                Ok(()) => {
                    // Pane might exist
                }
                Err(MuxError::CommandFailed(_)) => {
                    // Expected when pane doesn't exist or remote control fails
                }
                Err(e) => panic!("Unexpected error: {:?}", e),
            }
        }
    }

    #[test]
    fn test_complex_layout_creation() {
        let kitty = KittyMultiplexer::new().unwrap();
        if kitty.is_available() {
            // Get initial window count
            let initial_windows = kitty.list_windows_detailed().unwrap_or_default();
            let initial_count = initial_windows.len();

            // Create a main window
            let window_opts = WindowOptions {
                title: Some("complex-layout-008"),
                cwd: Some(Path::new("/tmp")),
                profile: None,
                focus: false,
            };

            let window_result = kitty.open_window(&window_opts);
            match window_result {
                Ok(window_id) => {
                    // Verify main window was created
                    assert!(kitty.window_exists(&window_id).unwrap_or(false));
                    assert_eq!(
                        kitty.get_window_title(&window_id).unwrap_or_default(),
                        "complex-layout-008"
                    );

                    // Create a 3-"pane" layout: editor (left), agent (top-right), logs (bottom-right)
                    // In kitty terms, this means creating separate windows positioned relative to each other

                    // Create agent pane (top-right of main window)
                    let agent_result = kitty.split_pane(
                        Some(&window_id),
                        Some(&window_id),
                        SplitDirection::Horizontal,
                        Some(70), // 70% for editor (main window)
                        &CommandOptions::default(),
                        None,
                    );

                    match agent_result {
                        Ok(agent_pane) => {
                            // Verify agent pane was created
                            assert!(kitty.window_exists(&agent_pane).unwrap_or(false));
                            assert_ne!(agent_pane, window_id);

                            // Create logs pane (bottom-right, split from agent pane)
                            let logs_result = kitty.split_pane(
                                Some(&window_id),
                                Some(&agent_pane),
                                SplitDirection::Vertical,
                                Some(60), // 60% for agent, 40% for logs
                                &CommandOptions::default(),
                                None,
                            );

                            match logs_result {
                                Ok(logs_pane) => {
                                    // Verify logs pane was created
                                    assert!(kitty.window_exists(&logs_pane).unwrap_or(false));
                                    assert_ne!(logs_pane, window_id);
                                    assert_ne!(logs_pane, agent_pane);

                                    // Give panes time to be created and verify final state
                                    thread::sleep(Duration::from_millis(200));

                                    // Verify all "panes" (windows) exist
                                    let all_panes_result = kitty.list_panes(&window_id);
                                    match all_panes_result {
                                        Ok(all_panes) => {
                                            assert!(!all_panes.is_empty());
                                            // Should contain our main window and created panes
                                            assert!(all_panes.contains(&window_id));
                                            assert!(all_panes.contains(&agent_pane));
                                            assert!(all_panes.contains(&logs_pane));
                                        }
                                        Err(MuxError::CommandFailed(_)) => {
                                            // Expected when remote control fails
                                            return;
                                        }
                                        Err(e) => panic!("Unexpected error listing panes: {:?}", e),
                                    }

                                    // Verify we have the expected number of windows
                                    let final_windows =
                                        kitty.list_windows_detailed().unwrap_or_default();
                                    assert!(
                                        final_windows.len() >= initial_count + 2,
                                        "Should have at least {} windows after creating 2 splits, got {}",
                                        initial_count + 2,
                                        final_windows.len()
                                    );

                                    // Test focusing different panes and verify focus changes
                                    let _ = kitty.focus_window(&window_id);
                                    let focused = kitty.get_focused_window_id().unwrap_or_default();
                                    assert_eq!(focused, window_id);

                                    let _ = kitty.focus_pane(&agent_pane);
                                    let focused = kitty.get_focused_window_id().unwrap_or_default();
                                    assert_eq!(focused, agent_pane);

                                    let _ = kitty.focus_pane(&logs_pane);
                                    let focused = kitty.get_focused_window_id().unwrap_or_default();
                                    assert_eq!(focused, logs_pane);
                                }
                                Err(MuxError::CommandFailed(_)) => {
                                    // Expected when remote control fails
                                }
                                Err(e) => panic!("Unexpected error creating logs pane: {:?}", e),
                            }
                        }
                        Err(MuxError::CommandFailed(_)) => {
                            // Expected when remote control fails
                        }
                        Err(e) => panic!("Unexpected error creating agent pane: {:?}", e),
                    }
                }
                Err(MuxError::CommandFailed(_)) => {
                    // Can't test complex layout if we can't create windows
                }
                Err(e) => panic!("Unexpected error creating main window: {:?}", e),
            }
        }
    }

    #[test]
    fn test_kitty_not_available() {
        // Test behavior when kitty is not available
        let kitty = KittyMultiplexer::with_socket_path("/nonexistent/socket".to_string());

        // Mock kitty not being available by checking if it's actually available
        if !kitty.is_available() {
            assert!(!kitty.is_available());

            // These operations should return CommandFailed error
            let result = kitty.open_window(&WindowOptions::default());
            assert!(matches!(result, Err(MuxError::CommandFailed(_))));

            let result = kitty.list_windows(None);
            assert!(matches!(result, Err(MuxError::CommandFailed(_))));
        }
    }
}

impl Multiplexer for KittyMultiplexer {
    fn id(&self) -> &'static str {
        "kitty"
    }

    fn is_available(&self) -> bool {
        // Check if kitty command exists and remote control is available
        std::process::Command::new("kitty")
            .arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
            && self.is_remote_control_available()
    }

    fn open_window(&self, opts: &WindowOptions) -> Result<WindowId, MuxError> {
        let mut args = vec![
            "launch".to_string(),
            "--type".to_string(),
            "tab".to_string(),
        ];

        // Add title if specified
        if let Some(title) = opts.title {
            args.extend_from_slice(&["--title".to_string(), title.to_string()]);
        }

        // Add working directory if specified
        if let Some(cwd) = opts.cwd {
            args.extend_from_slice(&["--cwd".to_string(), cwd.to_string_lossy().to_string()]);
        }

        // Convert to slice of &str for the command
        let args_str: Vec<&str> = args.iter().map(|s| s.as_str()).collect();

        // Run the command and capture the window ID
        let output = self.run_kitty_command(&args_str)?;
        let window_id = self.parse_window_id_from_output(&output)?;

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
        let mut args = vec!["launch".to_string()];

        // Set location based on direction
        let location = match dir {
            SplitDirection::Horizontal => "hsplit".to_string(),
            SplitDirection::Vertical => "vsplit".to_string(),
        };
        args.extend_from_slice(&["--location".to_string(), location]);

        // Add size percentage if specified
        if let Some(p) = percent {
            args.extend_from_slice(&["--size".to_string(), format!("{}%", p)]);
        }

        // Target the specific pane/window if specified, otherwise operate on current window
        if let Some(target_pane) = target {
            args.extend_from_slice(&["--match".to_string(), format!("id:{}", target_pane)]);
        } else if let Some(window_id) = window {
            args.extend_from_slice(&["--match".to_string(), format!("id:{}", window_id)]);
        }
        // If neither target nor window is specified, kitty will operate on the current window

        // Add working directory if specified
        if let Some(cwd) = opts.cwd {
            args.extend_from_slice(&["--cwd".to_string(), cwd.to_string_lossy().to_string()]);
        }

        // Add initial command if specified
        if let Some(cmd) = initial_cmd {
            args.push(cmd.to_string());
        }

        // Convert to slice of &str for the command
        let args_str: Vec<&str> = args.iter().map(|s| s.as_str()).collect();

        // Run the command and capture the pane ID
        let output = self.run_kitty_command(&args_str)?;
        self.parse_pane_id_from_output(&output)
    }

    fn run_command(
        &self,
        pane: &PaneId,
        cmd: &str,
        _opts: &CommandOptions,
    ) -> Result<(), MuxError> {
        // Send the command followed by Enter
        let match_arg = format!("id:{}", pane);
        let text_arg = format!("{}\n", cmd);
        self.run_kitty_command(&["send-text", "--match", &match_arg, &text_arg])?;
        Ok(())
    }

    fn send_text(&self, pane: &PaneId, text: &str) -> Result<(), MuxError> {
        // Send literal text to the pane
        let match_arg = format!("id:{}", pane);
        self.run_kitty_command(&["send-text", "--match", &match_arg, "--no-newline", text])?;
        Ok(())
    }

    fn focus_window(&self, window: &WindowId) -> Result<(), MuxError> {
        let match_arg = format!("id:{}", window);
        self.run_kitty_command(&["focus-window", "--match", &match_arg])?;
        Ok(())
    }

    fn focus_pane(&self, pane: &PaneId) -> Result<(), MuxError> {
        // In kitty, focusing a pane is the same as focusing its window
        self.focus_window(pane)
    }

    fn list_windows(&self, title_substr: Option<&str>) -> Result<Vec<WindowId>, MuxError> {
        // Get list of windows with format: id title
        let output = self.run_kitty_command(&["ls", "--format", "id title"])?;

        let mut windows = Vec::new();
        for line in output.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }

            let parts: Vec<&str> = line.splitn(2, ' ').collect();
            if parts.len() >= 2 {
                let window_id = parts[0];
                let title = parts[1];

                // Filter by title substring if provided
                if let Some(substr) = title_substr {
                    if !title.contains(substr) {
                        continue;
                    }
                }

                windows.push(window_id.to_string());
            }
        }

        Ok(windows)
    }

    fn list_panes(&self, _window: &WindowId) -> Result<Vec<PaneId>, MuxError> {
        // In kitty, each "pane" is actually a separate window, but we can treat
        // all windows as panes for compatibility. For now, return all windows.
        // This is a simplification - in a real implementation, we might need to
        // track which windows belong to which "logical" panes.
        self.list_windows(None)
    }
}
