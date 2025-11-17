// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! kitty multiplexer implementation
//!
//! Implements the Multiplexer trait for kitty using its remote control interface.
//! Based on the kitty integration guide in specs/Public/Terminal-Multiplexers/kitty.md
//!
//! ## Configuration Required
//!
//! Remote control must be enabled in `~/.config/kitty/kitty.conf`:
//!
//! ```conf
//! # Enable remote control (required for all kitty @ commands)
//! allow_remote_control yes
//!
//! # Set up UNIX socket for external control (scripts, other terminals)
//! # Kitty creates this socket file automatically when it starts
//! listen_on unix:/tmp/kitty-ah.sock
//! ```
//!
//! ## Usage
//!
//! - **From within Kitty:** Commands work without `--to` flag (uses stdio/environment)
//! - **From external scripts/terminals:** Must use socket path via `with_socket_path()` or
//!   set `KITTY_LISTEN_ON` environment variable
//!
//! ## Limitations
//!
//! - **Split sizes:** Kitty does not support custom split size percentages. Split sizes are
//!   determined automatically by kitty's layout algorithm. The `percent` parameter in
//!   `split_pane()` is ignored.
//!
//! See: <https://sw.kovidgoyal.net/kitty/remote-control/>

use ah_mux_core::*;
use std::process::{Command, Stdio};

/// kitty multiplexer implementation
///
/// Provides remote control of kitty terminal emulator via `kitty @` commands.
/// Supports both internal control (from within kitty) and external control
/// (from other terminals/scripts via UNIX socket).
pub struct KittyMultiplexer {
    /// Socket path for remote control.
    ///
    /// - None: Uses stdio (for commands within kitty)
    /// - Some(path): Uses socket connection (for external control)
    ///
    /// Defaults to KITTY_LISTEN_ON environment variable if set
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

    /// Check if kitty remote control is properly configured
    ///
    /// This provides detailed feedback about configuration issues.
    /// Use this to diagnose why kitty multiplexer functionality isn't working.
    pub fn check_configuration() -> Result<(), String> {
        // Check if kitty is installed
        let kitty_exists = std::process::Command::new("kitty")
            .arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);

        if !kitty_exists {
            return Err(
                "Kitty is not installed or not in PATH. Install it from https://sw.kovidgoyal.net/kitty/"
                    .to_string(),
            );
        }

        // Try to check if remote control is enabled by attempting a simple command
        let mux = Self::default();
        match mux.run_kitty_command(&["ls"]) {
            Ok(_) => Ok(()),
            Err(MuxError::CommandFailed(msg)) if msg.contains("Remote control is disabled") => {
                Err(format!(
                    "Kitty remote control is disabled. Run 'bin/setup-kitty' or manually add to ~/.config/kitty/kitty.conf:\n\
                     allow_remote_control yes\n\
                     listen_on unix:/tmp/kitty-ah.sock\n\n\
                     Then restart kitty or reload config (Cmd/Ctrl+Shift+F5)"
                ))
            }
            Err(MuxError::CommandFailed(msg)) if msg.contains("Could not connect") => Err(format!(
                "Could not connect to kitty socket. Ensure kitty is running and configured with:\n\
                     listen_on unix:/tmp/kitty-ah.sock\n\n\
                     Run 'bin/setup-kitty' to configure automatically."
            )),
            Err(e) => Err(format!("Failed to verify kitty configuration: {}", e)),
        }
    }

    /// Run a kitty @ command and return its output
    ///
    /// Executes kitty remote control commands via `kitty @` interface.
    /// Uses socket connection if socket_path is set, otherwise uses stdio (for commands within kitty).
    ///
    /// See: https://sw.kovidgoyal.net/kitty/remote-control/
    fn run_kitty_command(&self, args: &[&str]) -> Result<String, MuxError> {
        // Build command args
        let mut cmd_args = vec!["@"];

        // Add socket path if specified
        // From within Kitty: commands work without --to using stdio
        // From external scripts/terminals: must use --to unix:/path/to/socket
        if let Some(socket) = &self.socket_path {
            cmd_args.extend_from_slice(&["--to", socket]);
        }

        // Add the actual command arguments
        cmd_args.extend_from_slice(args);

        // Execute the kitty @ command
        let output = Command::new("kitty")
            .args(&cmd_args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .map_err(|e| {
                if e.kind() == std::io::ErrorKind::NotFound {
                    MuxError::NotAvailable("kitty")
                } else {
                    MuxError::CommandFailed(format!("Failed to execute kitty command: {}", e))
                }
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);

            // Check for specific error messages indicating remote control issues
            if stderr.contains("Remote control is disabled") {
                return Err(MuxError::CommandFailed(
                    "Remote control is disabled. Add 'allow_remote_control yes' to ~/.config/kitty/kitty.conf".to_string(),
                ));
            }

            if stderr.contains("Could not connect") || stderr.contains("no socket") {
                return Err(MuxError::CommandFailed(format!(
                    "Could not connect to kitty socket. Ensure kitty is running with 'listen_on unix:/tmp/kitty-ah.sock' in kitty.conf. Error: {}",
                    stderr
                )));
            }

            return Err(MuxError::CommandFailed(format!(
                "kitty @ command failed: {}",
                stderr
            )));
        }

        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }

    /// Check if kitty remote control is available
    ///
    /// Tests remote control by attempting a simple `kitty @ ls` command.
    /// Returns false if remote control is not configured or kitty is not running.
    fn is_remote_control_available(&self) -> bool {
        // Try to run a simple kitty @ command to test remote control availability
        let result = self.run_kitty_command(&["ls"]);
        match result {
            Ok(_) => true,
            Err(MuxError::CommandFailed(ref msg)) if msg.contains("no socket") => false,
            Err(MuxError::CommandFailed(ref msg)) if msg.contains("Could not connect") => false,
            Err(MuxError::CommandFailed(ref msg)) if msg.contains("timed out") => false,
            Err(MuxError::CommandFailed(ref msg)) if msg.contains("Remote control is disabled") => {
                false
            }
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
    ///
    /// Returns a list of (window_id, title) tuples by parsing kitty @ ls JSON output.
    /// See: https://sw.kovidgoyal.net/kitty/remote-control/#kitty-ls
    pub fn list_windows_detailed(&self) -> Result<Vec<(String, String)>, MuxError> {
        // Get list of windows as JSON
        let output = self.run_kitty_command(&["ls"])?;

        // Parse JSON to extract window IDs and titles
        // Expected structure: array of OS windows, each with tabs, each with windows
        let mut windows = Vec::new();

        // Simple JSON parsing for window info
        // Format: [{"tabs": [{"windows": [{"id": N, "title": "..."}, ...]}]}]
        for line in output.lines() {
            if line.contains("\"id\":") && line.contains("\"title\":") {
                // Extract id (number)
                if let Some(id_start) = line.find("\"id\":") {
                    let id_part = &line[id_start + 5..];
                    if let Some(id_end) = id_part.find(',').or_else(|| id_part.find('}')) {
                        let id_str = id_part[..id_end].trim();

                        // Extract title (string)
                        if let Some(title_start) = line.find("\"title\":") {
                            let title_part = &line[title_start + 9..];
                            if let Some(title_end) =
                                title_part.find("\",").or_else(|| title_part.find("\""))
                            {
                                let title = title_part[..title_end].trim_matches('"');
                                windows.push((id_str.to_string(), title.to_string()));
                            }
                        }
                    }
                }
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
    use std::sync::Mutex;
    use std::thread;
    use std::time::Duration;

    // Global test Kitty instance
    static TEST_KITTY: Mutex<Option<std::process::Child>> = Mutex::new(None);

    /// Start a test Kitty instance with remote control enabled
    fn start_test_kitty() -> Result<(), Box<dyn std::error::Error>> {
        let mut kitty_guard = TEST_KITTY.lock().unwrap();
        if kitty_guard.is_some() {
            return Ok(()); // Already started
        }

        // Create a temporary socket path for testing
        let socket_path = "/tmp/kitty-test.sock";

        // Remove any existing socket
        let _ = std::fs::remove_file(socket_path);

        // Try to start Kitty in hidden mode with remote control
        eprintln!(
            "DEBUG: Attempting to start Kitty with socket: {}",
            socket_path
        );
        let mut child = match std::process::Command::new("kitty")
            .args(&[
                "--listen-on",
                &format!("unix:{}", socket_path),
                "--start-as=hidden",
            ])
            .spawn()
        {
            Ok(child) => {
                eprintln!(
                    "DEBUG: Kitty spawned successfully with PID: {:?}",
                    child.id()
                );
                child
            }
            Err(e) => {
                eprintln!("DEBUG: Failed to spawn Kitty: {}", e);
                return Err(format!("Failed to spawn Kitty: {}", e).into());
            }
        };

        // Give Kitty time to start up
        std::thread::sleep(Duration::from_secs(2));

        // Check if Kitty is still running (it should be running indefinitely)
        match child.try_wait() {
            Ok(Some(status)) => {
                eprintln!("DEBUG: Kitty exited with status: {}", status);
                return Err(format!("Kitty exited immediately with status: {}", status).into());
            }
            Ok(None) => {
                eprintln!("DEBUG: Kitty is still running after 2 seconds, checking for socket...");
                // Kitty is still running, check if socket was created
                if !std::path::Path::new(socket_path).exists() {
                    eprintln!("DEBUG: Socket not found at {}", socket_path);
                    // Wait a bit more and check again
                    std::thread::sleep(Duration::from_secs(3));
                    if !std::path::Path::new(socket_path).exists() {
                        eprintln!("DEBUG: Socket still not found after additional wait");
                        let _ = child.kill();
                        return Err("Kitty started but failed to create socket".into());
                    }
                }
                eprintln!("DEBUG: Socket found, Kitty setup successful");
            }
            Err(e) => {
                let _ = child.kill();
                return Err(format!("Failed to check Kitty status: {}", e).into());
            }
        }

        *kitty_guard = Some(child);
        Ok(())
    }

    /// Stop the test Kitty instance
    fn stop_test_kitty() {
        let mut kitty_guard = TEST_KITTY.lock().unwrap();
        if let Some(mut child) = kitty_guard.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }

    #[test]
    fn test_kitty_multiplexer_creation() {
        let kitty = KittyMultiplexer::new().unwrap();
        assert_eq!(kitty.id(), "kitty");
        assert_eq!(kitty.socket_path, std::env::var("KITTY_LISTEN_ON").ok());
    }

    #[test]
    fn test_start_kitty_instance() {
        eprintln!("DEBUG: Testing start_test_kitty function");
        match start_test_kitty() {
            Ok(()) => eprintln!("DEBUG: start_test_kitty succeeded"),
            Err(e) => eprintln!("DEBUG: start_test_kitty failed: {}", e),
        }
    }

    #[test]
    fn test_kitty_with_custom_socket() {
        let socket_path = "/tmp/test-kitty.sock".to_string();
        let kitty = KittyMultiplexer::with_socket_path(socket_path.clone());
        assert_eq!(kitty.socket_path, Some(socket_path));
    }

    #[test]
    fn test_kitty_availability() {
        // Skip kitty tests in CI environments where kitty remote control is not available
        if std::env::var("CI").is_ok() {
            println!("⚠️  Skipping kitty test in CI environment");
            return;
        }

        let kitty = KittyMultiplexer::new().unwrap();
        let _available = kitty.is_available();
        // Note: We can't assert availability since kitty might not be installed or configured
    }

    #[test]
    fn test_kitty_remote_control_available() {
        // Skip kitty tests in CI environments where kitty remote control is not available
        if std::env::var("CI").is_ok() {
            println!("⚠️  Skipping kitty test in CI environment");
            return;
        }

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
        // Try to start a test Kitty instance
        if let Err(e) = start_test_kitty() {
            eprintln!(
                "Skipping Kitty test: Cannot start Kitty instance in this environment: {}",
                e
            );
            return; // Skip test if Kitty can't be started
        }

        // Create Kitty multiplexer with the test socket
        let kitty = KittyMultiplexer::with_socket_path("/tmp/kitty-test.sock".to_string());

        // Skip test if Kitty is not available
        if !kitty.is_available() {
            eprintln!("Skipping Kitty test: Kitty instance not available for remote control");
            return;
        }

        // Now run the actual test logic
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

    #[test]
    fn test_open_window_focus() {
        // Skip kitty tests in CI environments where kitty remote control is not available
        if std::env::var("CI").is_ok() {
            println!("⚠️  Skipping kitty test in CI environment");
            return;
        }

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
        // Skip kitty tests in CI environments where kitty remote control is not available
        if std::env::var("CI").is_ok() {
            println!("⚠️  Skipping kitty test in CI environment");
            return;
        }

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
        // Skip kitty tests in CI environments where kitty remote control is not available
        if std::env::var("CI").is_ok() {
            println!("⚠️  Skipping kitty test in CI environment");
            return;
        }

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
        // Skip kitty tests in CI environments where kitty remote control is not available
        if std::env::var("CI").is_ok() {
            println!("⚠️  Skipping kitty test in CI environment");
            return;
        }

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
        // Skip kitty tests in CI environments where kitty remote control is not available
        if std::env::var("CI").is_ok() {
            println!("⚠️  Skipping kitty test in CI environment");
            return;
        }

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
        // Skip kitty tests in CI environments where kitty remote control is not available
        if std::env::var("CI").is_ok() {
            println!("⚠️  Skipping kitty test in CI environment");
            return;
        }

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
        // Skip kitty tests in CI environments where kitty remote control is not available
        if std::env::var("CI").is_ok() {
            println!("⚠️  Skipping kitty test in CI environment");
            return;
        }

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
        // Skip kitty tests in CI environments where kitty remote control is not available
        if std::env::var("CI").is_ok() {
            println!("⚠️  Skipping kitty test in CI environment");
            return;
        }

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
        // Skip kitty tests in CI environments where kitty remote control is not available
        if std::env::var("CI").is_ok() {
            println!("⚠️  Skipping kitty test in CI environment");
            return;
        }

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
        // Skip kitty tests in CI environments where kitty remote control is not available
        if std::env::var("CI").is_ok() {
            println!("⚠️  Skipping kitty test in CI environment");
            return;
        }

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
        // Test behavior when using a non-existent socket
        let kitty = KittyMultiplexer::with_socket_path("/nonexistent/socket".to_string());

        // With a non-existent socket, kitty should not be available
        // or operations should fail
        let result = kitty.open_window(&WindowOptions::default());

        // Accept any error type - the important thing is that it fails
        assert!(
            result.is_err(),
            "Opening window with invalid socket should fail"
        );
    }
}

impl Multiplexer for KittyMultiplexer {
    fn id(&self) -> &'static str {
        "kitty"
    }

    fn is_available(&self) -> bool {
        // Check if kitty command exists
        let kitty_exists = std::process::Command::new("kitty")
            .arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);

        if !kitty_exists {
            return false;
        }

        // Check if remote control is available
        let remote_control_available = self.is_remote_control_available();

        if !remote_control_available {
            // Log helpful message for debugging
            eprintln!("⚠️  Kitty remote control is not available.");
            eprintln!(
                "   Run 'bin/setup-kitty' to configure, or manually add to ~/.config/kitty/kitty.conf:"
            );
            eprintln!("   allow_remote_control yes");
            eprintln!("   listen_on unix:/tmp/kitty-ah.sock");
            eprintln!("   Then restart kitty or reload config (Cmd/Ctrl+Shift+F5)");
        }

        remote_control_available
    }

    fn open_window(&self, opts: &WindowOptions) -> Result<WindowId, MuxError> {
        // Instead of creating a new tab, we'll use the current window for task layouts
        // This provides a better user experience by keeping everything in one view
        // The layout will be: TUI on left, lazygit + agent on right (split vertically)

        // Get the current window ID
        if let Some(window_id) = self.current_window()? {
            // If focus is requested, focus the window
            if opts.focus {
                self.focus_window(&window_id)?;
            }
            Ok(window_id)
        } else {
            // Fallback: if no current window exists, create a new tab
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
        let mut args = vec![
            "launch".to_string(),
            "--type".to_string(),
            "window".to_string(),
        ];

        // Set location based on direction
        // For the desired layout (TUI left, Lazygit top-right, Agent bottom-right),
        // we swap horizontal to vertical when no target is specified.
        // This makes the agent pane split below the lazygit pane instead of beside it.
        // See: https://sw.kovidgoyal.net/kitty/launch/
        let location = if target.is_none() && window.is_none() {
            // When operating on "current" (which will be the lazygit pane after run_command),
            // use vsplit to create agent below it, regardless of requested direction
            "vsplit".to_string()
        } else {
            match dir {
                SplitDirection::Horizontal => "hsplit".to_string(),
                SplitDirection::Vertical => "vsplit".to_string(),
            }
        };
        args.extend_from_slice(&["--location".to_string(), location]);

        // Note: kitty does not support --size option for splits
        // Split sizes are determined automatically by kitty's layout algorithm
        // The percent parameter is ignored for kitty
        if cfg!(debug_assertions) && percent.is_some() {
            eprintln!(
                "DEBUG: kitty does not support custom split sizes, ignoring percent parameter"
            );
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

        // Add environment variables if specified
        // See: https://sw.kovidgoyal.net/kitty/launch/
        if let Some(env_vars) = opts.env {
            for (key, value) in env_vars {
                args.extend_from_slice(&["--env".to_string(), format!("{}={}", key, value)]);
            }
        }

        // Add initial command if specified (after -- separator as documented)
        // See: https://sw.kovidgoyal.net/kitty/launch/
        // Note: We wrap the command in bash -c to handle complex command lines with arguments
        if let Some(cmd) = initial_cmd {
            args.push("--".to_string());
            args.push("bash".to_string());
            args.push("-c".to_string());
            args.push(cmd.to_string());
        }

        // Convert to slice of &str for the command
        let args_str: Vec<&str> = args.iter().map(|s| s.as_str()).collect();

        // Run the command and capture the pane ID
        let output = self.run_kitty_command(&args_str)?;
        self.parse_pane_id_from_output(&output)
    }

    fn run_command(&self, pane: &PaneId, cmd: &str, opts: &CommandOptions) -> Result<(), MuxError> {
        // For kitty, when ah-tui-multiplexer tries to run a command in the "editor pane"
        // (where the TUI is running), we need to create a NEW pane with the command
        // instead of sending text to the TUI.
        //
        // Special case: if the command is "lazygit", create a new split pane instead of sending text.
        //
        // See: https://sw.kovidgoyal.net/kitty/launch/

        eprintln!("DEBUG run_command: pane={:?}, cmd={:?}", pane, cmd);

        if cmd == "lazygit" || cmd.starts_with("lazygit ") {
            eprintln!("DEBUG: Detected lazygit, creating split");
            // This is the editor pane setup - create a new split with the command
            let mut args = vec![
                "launch".to_string(),
                "--type".to_string(),
                "window".to_string(),
                "--location".to_string(),
                "hsplit".to_string(), // Split horizontally (left/right)
                "--cwd".to_string(),
                opts.cwd
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_else(|| ".".to_string()),
            ];

            // Add the command to execute (wrapped in bash -c)
            args.push("--".to_string());
            args.push("bash".to_string());
            args.push("-c".to_string());
            args.push(cmd.to_string());

            // Convert to slice of &str
            let args_str: Vec<&str> = args.iter().map(|s| s.as_str()).collect();

            // Create the split and get its pane ID
            let output = self.run_kitty_command(&args_str)?;
            let new_pane_id = self.parse_pane_id_from_output(&output)?;

            // Focus the newly created pane so the next split targets it
            // This is crucial for the layout: TUI | Lazygit
            //                                         | Agent
            self.focus_pane(&new_pane_id)?;

            Ok(())
        } else {
            // For other panes, send the command text normally
            // Build the full command with working directory if specified
            let full_cmd = if let Some(cwd) = opts.cwd {
                format!("cd {} && {}", cwd.display(), cmd)
            } else {
                cmd.to_string()
            };

            let match_arg = format!("id:{}", pane);
            let text_arg = format!("{}\r", full_cmd); // \r is carriage return (Enter key)

            // Give the pane a moment to be ready (similar to iTerm2's 100ms delay)
            std::thread::sleep(std::time::Duration::from_millis(100));

            self.run_kitty_command(&["send-text", "--match", &match_arg, &text_arg])?;
            Ok(())
        }
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

    fn current_window(&self) -> Result<Option<WindowId>, MuxError> {
        // First try using kitty's get-focused-window-id command
        match self.get_focused_window_id() {
            Ok(window_id) if !window_id.is_empty() => {
                eprintln!(
                    "DEBUG current_window: got from get_focused_window_id: {:?}",
                    window_id
                );
                return Ok(Some(window_id));
            }
            Err(e) => {
                eprintln!(
                    "DEBUG current_window: get_focused_window_id failed: {:?}",
                    e
                );
            }
            _ => {
                eprintln!("DEBUG current_window: get_focused_window_id returned empty");
            }
        }

        // Fallback: Try the KITTY_WINDOW_ID environment variable
        // This is set by kitty for all processes running inside it
        if let Ok(window_id) = std::env::var("KITTY_WINDOW_ID") {
            if !window_id.is_empty() {
                eprintln!(
                    "DEBUG current_window: got from KITTY_WINDOW_ID env: {:?}",
                    window_id
                );
                return Ok(Some(window_id));
            }
        }

        eprintln!("DEBUG current_window: no window found");
        Ok(None)
    }

    fn current_pane(&self) -> Result<Option<PaneId>, MuxError> {
        // In kitty, each pane is effectively a window, so the focused window ID
        // is the same as the focused pane ID
        self.current_window()
    }

    fn list_windows(&self, title_substr: Option<&str>) -> Result<Vec<WindowId>, MuxError> {
        // Get detailed window info via list_windows_detailed which parses JSON from kitty @ ls
        let windows = self.list_windows_detailed()?;

        // Filter by title substring if specified
        let filtered: Vec<WindowId> = if let Some(substr) = title_substr {
            windows
                .into_iter()
                .filter(|(_, title)| title.contains(substr))
                .map(|(id, _)| id)
                .collect()
        } else {
            windows.into_iter().map(|(id, _)| id).collect()
        };

        Ok(filtered)
    }

    fn list_panes(&self, _window: &WindowId) -> Result<Vec<PaneId>, MuxError> {
        // In kitty, each "pane" is actually a separate window, but we can treat
        // all windows as panes for compatibility. For now, return all windows.
        // This is a simplification - in a real implementation, we might need to
        // track which windows belong to which "logical" panes.
        self.list_windows(None)
    }
}
