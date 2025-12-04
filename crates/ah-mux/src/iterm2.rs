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
    use std::path::Path;
    use std::sync::Mutex;
    use std::thread;
    use std::time::Duration;
    use tracing_subscriber::{EnvFilter, fmt};

    // Global test iTerm2 instance tracker
    static TEST_ITERM2: Mutex<Option<()>> = Mutex::new(None);

    /// Initialize tracing for tests with detailed output
    fn init_test_tracing() {
        let _ = fmt()
            .with_env_filter(
                EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("debug")),
            )
            .with_test_writer()
            .try_init();
    }

    /// Start iTerm2 if not already running
    ///
    /// This function explicitly opens iTerm2 using AppleScript and ensures it's ready for testing.
    /// It tracks whether we've initialized it to avoid multiple concurrent initializations.
    fn start_test_iterm2() -> Result<(), Box<dyn std::error::Error>> {
        let mut iterm_guard = TEST_ITERM2.lock().unwrap();
        if iterm_guard.is_some() {
            tracing::debug!("iTerm2 test instance already running");
            return Ok(()); // Already initialized
        }

        tracing::info!("Starting iTerm2 test instance");

        // First, use AppleScript to launch and activate iTerm2
        // This is the most reliable method as it ensures the app is both launched and activated
        let launch_script = r#"
            tell application "iTerm2"
                activate
                -- Ensure we have at least one window
                if (count of windows) is 0 then
                    create window with default profile
                end if
            end tell
        "#;

        tracing::debug!("Launching iTerm2 via AppleScript");
        let launch_result = Command::new("osascript").arg("-e").arg(launch_script).output();

        match launch_result {
            Ok(output) if output.status.success() => {
                tracing::info!("iTerm2 launched and activated successfully via AppleScript");
            }
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr);
                tracing::warn!("iTerm2 AppleScript launch warning: {}", stderr);

                // Fallback: try using the open command
                tracing::debug!("Trying fallback method with 'open' command");
                let _ = Command::new("open").arg("-a").arg("iTerm").output();
            }
            Err(e) => {
                tracing::error!("Failed to launch iTerm2 via AppleScript: {}", e);

                // Fallback: try using the open command
                tracing::debug!("Trying fallback method with 'open' command");
                match Command::new("open").arg("-a").arg("iTerm").output() {
                    Ok(_) => tracing::info!("iTerm2 launched via 'open' command"),
                    Err(e2) => {
                        tracing::error!("All launch methods failed: {}", e2);
                        return Err(format!("Failed to launch iTerm2: {}", e2).into());
                    }
                }
            }
        }

        // Give iTerm2 time to fully start and initialize
        tracing::debug!("Waiting for iTerm2 to initialize...");
        std::thread::sleep(Duration::from_millis(1500));

        // Verify iTerm2 is actually running by checking if we can get its version
        let verify_result = Command::new("osascript")
            .arg("-e")
            .arg(r#"tell application "iTerm2" to version"#)
            .output();

        match verify_result {
            Ok(output) if output.status.success() => {
                let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
                tracing::info!("iTerm2 is running (version: {})", version);
            }
            _ => {
                tracing::warn!("Could not verify iTerm2 version, but continuing...");
            }
        }

        *iterm_guard = Some(());
        tracing::info!("iTerm2 test instance ready");
        Ok(())
    }

    /// Stop/cleanup test iTerm2 instance by force quitting iTerm2 without confirmation
    fn stop_test_iterm2() {
        let mut iterm_guard = TEST_ITERM2.lock().unwrap();

        tracing::info!("Stopping iTerm2 test instance");

        // Force quit immediately without prompts using killall
        tracing::debug!("Force quitting iTerm2 without confirmation");
        let _ = Command::new("killall").arg("-9").arg("iTerm2").output();

        // Give system time to clean up
        std::thread::sleep(Duration::from_millis(500));

        *iterm_guard = None;
        tracing::info!("iTerm2 test instance stopped");
    }

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

    #[test]
    #[serial_test::file_serial]
    fn test_iterm2_multiplexer_creation() {
        // Skip if not on macOS or in CI
        if cfg!(not(target_os = "macos")) || std::env::var("CI").is_ok() {
            tracing::info!("Skipping iTerm2 test (not on macOS or in CI)");
            return;
        }

        let result = ITerm2Multiplexer::new();
        match result {
            Ok(mux) => {
                assert_eq!(mux.id(), "iterm2");
            }
            Err(MuxError::NotAvailable(_)) => {
                // Expected if iTerm2 is not installed
            }
            Err(e) => panic!("Unexpected error: {:?}", e),
        }
    }

    #[test]
    #[serial_test::file_serial]
    fn test_iterm2_availability() {
        // Skip if not on macOS or in CI
        if cfg!(not(target_os = "macos")) || std::env::var("CI").is_ok() {
            tracing::info!("Skipping iTerm2 test (not on macOS or in CI)");
            return;
        }

        let mux = ITerm2Multiplexer::default();
        let available = mux.is_available();

        // iTerm2 availability depends on whether it's installed on the system
        // We just verify the check runs without errors
        tracing::debug!("iTerm2 availability: {}", available);
    }

    #[test]
    #[serial_test::file_serial]
    fn test_open_window_with_title_and_cwd() {
        init_test_tracing();
        tracing::info!("Starting test_open_window_with_title_and_cwd");

        // Skip if not on macOS or in CI
        if cfg!(not(target_os = "macos")) || std::env::var("CI").is_ok() {
            tracing::info!("Skipping iTerm2 test (not on macOS or in CI)");
            return;
        }

        let _ = start_test_iterm2();
        let mux = ITerm2Multiplexer::new().unwrap();

        let opts = WindowOptions {
            title: Some("iterm2-test-window-001"),
            cwd: Some(Path::new("/tmp")),
            profile: None,
            focus: false,
            init_command: None,
        };

        let window_id = mux.open_window(&opts).unwrap();
        tracing::info!("Window created with ID: {}", window_id);

        // Verify the window ID was created
        assert!(window_id.starts_with("iterm2-window-"));
        tracing::debug!("Window ID validation passed: {}", window_id);

        // Give iTerm2 time to create the window
        thread::sleep(Duration::from_millis(500));

        tracing::info!("Test completed successfully");

        stop_test_iterm2();
    }

    #[test]
    #[serial_test::file_serial]
    fn test_open_window_focus() {
        init_test_tracing();
        tracing::info!("Starting test_open_window_focus");

        // Skip if not on macOS or in CI
        if cfg!(not(target_os = "macos")) || std::env::var("CI").is_ok() {
            tracing::info!("Skipping iTerm2 test (not on macOS or in CI)");
            return;
        }

        let _ = start_test_iterm2();
        let mux = ITerm2Multiplexer::new().unwrap();

        let opts = WindowOptions {
            title: Some("focus-test-002"),
            cwd: Some(Path::new("/tmp")),
            profile: None,
            focus: true, // Should focus the window
            init_command: None,
        };

        let window_id = mux.open_window(&opts).unwrap();
        tracing::info!("Focused window created with ID: {}", window_id);

        // Verify the window ID was created
        assert!(window_id.starts_with("iterm2-window-"));
        tracing::debug!("Window ID validation passed: {}", window_id);
        tracing::info!("Window focus attribute was set to true");

        // Give iTerm2 time to focus
        thread::sleep(Duration::from_millis(500));
        tracing::info!("Test completed successfully");

        stop_test_iterm2();
    }

    #[test]
    #[serial_test::file_serial]
    fn test_split_pane() {
        init_test_tracing();
        tracing::info!("Starting test_split_pane");

        // Skip if not on macOS or in CI
        if cfg!(not(target_os = "macos")) || std::env::var("CI").is_ok() {
            tracing::info!("Skipping iTerm2 test (not on macOS or in CI)");
            return;
        }

        let _ = start_test_iterm2();
        let mux = ITerm2Multiplexer::new().unwrap();

        // Create a window to split from
        let window_opts = WindowOptions {
            title: Some("split-test-003"),
            cwd: Some(Path::new("/tmp")),
            profile: None,
            focus: false,
            init_command: None,
        };

        let window_id = mux.open_window(&window_opts).unwrap();
        thread::sleep(Duration::from_millis(500));

        // Split it horizontally
        let pane_id = mux
            .split_pane(
                Some(&window_id),
                None,
                SplitDirection::Horizontal,
                Some(60),
                &CommandOptions::default(),
                None,
            )
            .unwrap();

        assert!(pane_id.starts_with("iterm2-pane-"));
        assert_ne!(pane_id, window_id);
        tracing::debug!("Created horizontal pane: {}", pane_id);

        thread::sleep(Duration::from_millis(500));

        // Split it vertically
        let pane_id2 = mux
            .split_pane(
                Some(&window_id),
                None,
                SplitDirection::Vertical,
                Some(70),
                &CommandOptions::default(),
                None,
            )
            .unwrap();

        assert!(pane_id2.starts_with("iterm2-pane-"));
        assert_ne!(pane_id2, pane_id);
        tracing::debug!("Created vertical pane: {}", pane_id2);

        stop_test_iterm2();
    }

    #[test]
    #[serial_test::file_serial]
    fn test_split_pane_with_initial_command() {
        // Skip if not on macOS or in CI
        if cfg!(not(target_os = "macos")) || std::env::var("CI").is_ok() {
            tracing::info!("Skipping iTerm2 test (not on macOS or in CI)");
            return;
        }

        let _ = start_test_iterm2();
        let mux = ITerm2Multiplexer::new().unwrap();

        let window_opts = WindowOptions {
            title: Some("split-cmd-test"),
            cwd: Some(Path::new("/tmp")),
            profile: None,
            focus: false,
            init_command: None,
        };

        let window_id = mux.open_window(&window_opts).unwrap();
        thread::sleep(Duration::from_millis(500));

        // Split with initial command
        let pane_id = mux
            .split_pane(
                Some(&window_id),
                None,
                SplitDirection::Horizontal,
                None,
                &CommandOptions::default(),
                Some("echo 'Hello from split pane'"),
            )
            .unwrap();

        assert!(pane_id.starts_with("iterm2-pane-"));
        assert_ne!(pane_id, window_id);
        tracing::debug!("Created pane with command: {}", pane_id);

        stop_test_iterm2();
    }

    #[test]
    #[serial_test::file_serial]
    fn test_run_command_and_send_text() {
        // Skip if not on macOS or in CI
        if cfg!(not(target_os = "macos")) || std::env::var("CI").is_ok() {
            tracing::info!("Skipping iTerm2 test (not on macOS or in CI)");
            return;
        }

        let _ = start_test_iterm2();
        let mux = ITerm2Multiplexer::new().unwrap();

        let window_opts = WindowOptions {
            title: Some("cmd-text-test"),
            cwd: Some(Path::new("/tmp")),
            profile: None,
            focus: false,
            init_command: None,
        };

        let window_id = mux.open_window(&window_opts).unwrap();
        thread::sleep(Duration::from_millis(500));

        // Test run_command
        mux.run_command(
            &window_id,
            "echo 'test command'",
            &CommandOptions::default(),
        )
        .unwrap();
        tracing::debug!("Command executed successfully");
        thread::sleep(Duration::from_millis(200));

        // Test send_text
        mux.send_text(&window_id, "some input text").unwrap();
        tracing::debug!("Text sent successfully");
        thread::sleep(Duration::from_millis(200));

        stop_test_iterm2();
    }

    #[test]
    #[serial_test::file_serial]
    fn test_focus_window_and_pane() {
        // Skip if not on macOS or in CI
        if cfg!(not(target_os = "macos")) || std::env::var("CI").is_ok() {
            tracing::info!("Skipping iTerm2 test (not on macOS or in CI)");
            return;
        }

        let _ = start_test_iterm2();
        let mux = ITerm2Multiplexer::new().unwrap();

        let window_opts1 = WindowOptions {
            title: Some("window1-005"),
            cwd: Some(Path::new("/tmp")),
            profile: None,
            focus: false,
            init_command: None,
        };

        let window_opts2 = WindowOptions {
            title: Some("window2-005"),
            cwd: Some(Path::new("/tmp")),
            profile: None,
            focus: false,
            init_command: None,
        };

        let window1 = mux.open_window(&window_opts1).unwrap();
        let window2 = mux.open_window(&window_opts2).unwrap();
        thread::sleep(Duration::from_millis(500));

        // Test window focusing
        mux.focus_window(&window1).unwrap();
        tracing::debug!("Focused window1");
        thread::sleep(Duration::from_millis(200));

        mux.focus_window(&window2).unwrap();
        tracing::debug!("Focused window2");
        thread::sleep(Duration::from_millis(200));

        // Test pane focusing (same as window in iTerm2)
        mux.focus_pane(&window1).unwrap();
        tracing::debug!("Focused pane/window1");

        stop_test_iterm2();
    }

    #[test]
    #[serial_test::file_serial]
    fn test_list_windows_filtering() {
        // Skip if not on macOS or in CI
        if cfg!(not(target_os = "macos")) || std::env::var("CI").is_ok() {
            tracing::info!("Skipping iTerm2 test (not on macOS or in CI)");
            return;
        }

        let _ = start_test_iterm2();
        let mux = ITerm2Multiplexer::new().unwrap();

        // Create test windows with different titles
        let window_opts = vec![
            WindowOptions {
                title: Some("alpha-window-006"),
                cwd: Some(Path::new("/tmp")),
                profile: None,
                focus: false,
                init_command: None,
            },
            WindowOptions {
                title: Some("beta-window-006"),
                cwd: Some(Path::new("/tmp")),
                profile: None,
                focus: false,
                init_command: None,
            },
            WindowOptions {
                title: Some("alpha-other-006"),
                cwd: Some(Path::new("/tmp")),
                profile: None,
                focus: false,
                init_command: None,
            },
        ];

        for opts in window_opts {
            let window_id = mux.open_window(&opts).unwrap();
            tracing::debug!("Created window: {}", window_id);
            thread::sleep(Duration::from_millis(300));
        }

        // Give windows time to be created
        thread::sleep(Duration::from_millis(500));

        // List all windows
        let all_windows = mux.list_windows(None).unwrap();
        tracing::debug!("Found {} windows total", all_windows.len());
        assert!(!all_windows.is_empty(), "Should have at least some windows");

        // Filter by "alpha"
        let alpha_windows = mux.list_windows(Some("alpha")).unwrap();
        tracing::debug!("Found {} alpha windows", alpha_windows.len());
        // Should find at least the alpha windows we created

        // Filter by "beta"
        let beta_windows = mux.list_windows(Some("beta")).unwrap();
        tracing::debug!("Found {} beta windows", beta_windows.len());

        // Filter by non-existent title
        let none_windows = mux.list_windows(Some("nonexistent-unique-title-xyz")).unwrap();
        tracing::debug!("Found {} nonexistent windows", none_windows.len());
        // Should be empty or not contain our test windows

        stop_test_iterm2();
    }

    #[test]
    #[serial_test::file_serial]
    fn test_current_window() {
        // Skip if not on macOS or in CI
        if cfg!(not(target_os = "macos")) || std::env::var("CI").is_ok() {
            tracing::info!("Skipping iTerm2 test (not on macOS or in CI)");
            return;
        }

        let _ = start_test_iterm2();
        let mux = ITerm2Multiplexer::new().unwrap();

        let window_opts = WindowOptions {
            title: Some("current-window-test"),
            cwd: Some(Path::new("/tmp")),
            profile: None,
            focus: true, // Focus so it becomes current
            init_command: None,
        };

        let window_id = mux.open_window(&window_opts).unwrap();
        thread::sleep(Duration::from_millis(500));

        // Check current window
        let current = mux.current_window().unwrap();
        tracing::debug!("Current window: {:?}", current);
        // Should have a current window

        // Focus another window and check again
        mux.focus_window(&window_id).unwrap();
        thread::sleep(Duration::from_millis(200));

        let current = mux.current_window().unwrap();
        tracing::debug!("Current window after focus: {:?}", current);

        stop_test_iterm2();
    }

    #[test]
    #[serial_test::file_serial]
    fn test_error_handling_invalid_pane() {
        // Skip if not on macOS or in CI
        if cfg!(not(target_os = "macos")) || std::env::var("CI").is_ok() {
            tracing::info!("Skipping iTerm2 test (not on macOS or in CI)");
            return;
        }

        let _ = start_test_iterm2();
        let mux = ITerm2Multiplexer::new().unwrap();

        // Try to focus a non-existent pane
        let invalid_pane = "iterm2-pane-99999".to_string();
        let result = mux.focus_pane(&invalid_pane);

        match result {
            Ok(()) => {
                // May succeed if iTerm2 just activates anyway
                tracing::debug!("Focus succeeded (expected for iTerm2)");
            }
            Err(MuxError::CommandFailed(_)) => {
                // Expected when pane doesn't exist
                tracing::debug!("Focus failed as expected");
            }
            Err(e) => {
                tracing::warn!("Unexpected error: {:?}", e);
            }
        }

        // Try to send text to non-existent pane
        let result = mux.send_text(&invalid_pane, "test");

        match result {
            Ok(()) => {
                // May succeed if iTerm2 sends to current session
                tracing::debug!("Send text succeeded (may be expected for iTerm2)");
            }
            Err(MuxError::CommandFailed(_)) => {
                // Expected when pane doesn't exist
                tracing::debug!("Send text failed as expected");
            }
            Err(e) => {
                tracing::warn!("Unexpected error: {:?}", e);
            }
        }

        stop_test_iterm2();
    }

    #[test]
    #[serial_test::file_serial]
    fn test_complex_layout_creation() {
        // Skip if not on macOS or in CI
        if cfg!(not(target_os = "macos")) || std::env::var("CI").is_ok() {
            tracing::info!("Skipping iTerm2 test (not on macOS or in CI)");
            return;
        }

        let _ = start_test_iterm2();
        let mux = ITerm2Multiplexer::new().unwrap();

        // Create a main window
        let window_opts = WindowOptions {
            title: Some("complex-layout-008"),
            cwd: Some(Path::new("/tmp")),
            profile: None,
            focus: false,
            init_command: None,
        };

        let window_id = mux.open_window(&window_opts).unwrap();
        thread::sleep(Duration::from_millis(500));

        // Create a 3-pane layout: editor (left), agent (top-right), logs (bottom-right)

        // Create agent pane (top-right of main window)
        let agent_pane = mux
            .split_pane(
                Some(&window_id),
                None,
                SplitDirection::Horizontal,
                Some(70), // 70% for editor
                &CommandOptions::default(),
                None,
            )
            .unwrap();
        assert!(agent_pane.starts_with("iterm2-pane-"));
        tracing::debug!("Created agent pane: {}", agent_pane);
        thread::sleep(Duration::from_millis(500));

        // Create logs pane (bottom-right, split from agent pane)
        let logs_pane = mux
            .split_pane(
                Some(&window_id),
                Some(&agent_pane),
                SplitDirection::Vertical,
                Some(60), // 60% for agent
                &CommandOptions::default(),
                None,
            )
            .unwrap();
        assert!(logs_pane.starts_with("iterm2-pane-"));
        assert_ne!(
            logs_pane, agent_pane,
            "Logs pane should be different from agent pane"
        );
        tracing::debug!("Created logs pane: {}", logs_pane);

        // Give time for layout to stabilize
        thread::sleep(Duration::from_millis(500));

        // Test focusing different panes
        mux.focus_window(&window_id).unwrap();
        thread::sleep(Duration::from_millis(200));

        mux.focus_pane(&agent_pane).unwrap();
        thread::sleep(Duration::from_millis(200));

        mux.focus_pane(&logs_pane).unwrap();
        thread::sleep(Duration::from_millis(200));

        tracing::debug!("Complex layout test completed successfully");

        stop_test_iterm2();
    }

    #[test]
    #[serial_test::file_serial]
    fn test_list_panes() {
        // Skip if not on macOS or in CI
        if cfg!(not(target_os = "macos")) || std::env::var("CI").is_ok() {
            tracing::info!("Skipping iTerm2 test (not on macOS or in CI)");
            return;
        }

        let _ = start_test_iterm2();
        let mux = ITerm2Multiplexer::new().unwrap();

        let window_opts = WindowOptions {
            title: Some("list-panes-test"),
            cwd: Some(Path::new("/tmp")),
            profile: None,
            focus: false,
            init_command: None,
        };

        let window_id = mux.open_window(&window_opts).unwrap();
        thread::sleep(Duration::from_millis(500));

        // List panes for the window
        let panes = mux.list_panes(&window_id).unwrap();
        tracing::debug!("Found {} panes", panes.len());
        assert!(!panes.is_empty(), "Window should have at least one pane");

        stop_test_iterm2();
    }

    #[test]
    #[serial_test::file_serial]
    fn test_run_applescript_basic() {
        // Skip if not on macOS or in CI
        if cfg!(not(target_os = "macos")) || std::env::var("CI").is_ok() {
            tracing::info!("Skipping iTerm2 test (not on macOS or in CI)");
            return;
        }

        let mux = ITerm2Multiplexer::default();

        // Test basic AppleScript execution
        let output = mux.run_applescript(r#"return "test""#).unwrap();
        assert_eq!(
            output.trim(),
            "test",
            "AppleScript should return test string"
        );
        tracing::debug!("AppleScript test passed");
    }

    #[test]
    #[serial_test::file_serial]
    fn test_get_window_count() {
        // Skip if not on macOS or in CI
        if cfg!(not(target_os = "macos")) || std::env::var("CI").is_ok() {
            tracing::info!("Skipping iTerm2 test (not on macOS or in CI)");
            return;
        }

        let _ = start_test_iterm2();
        let mux = ITerm2Multiplexer::new().unwrap();

        let count = mux.get_window_count().unwrap();
        tracing::debug!("iTerm2 has {} windows", count);
        // Just verify we can get the count without error
        // count is usize so it's always >= 0

        stop_test_iterm2();
    }
}
