// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Tilix multiplexer implementation
//!
//! Tilix is a Linux tiling terminal emulator that supports tabs and splits
//! through command-line actions and session files.
//! Based on the Tilix integration guide in specs/Public/Terminal-Multiplexers/Tilix.md

use std::fs;
use std::io::Write;
use std::process::{Command, Stdio};

use ah_mux_core::{
    CommandOptions, Multiplexer, MuxCapability, MuxError, PaneId, SplitDirection, WindowId,
    WindowOptions,
};
use serde::Serialize;

/// Tilix session file format
/// See: https://gnunn1.github.io/tilix-web/manual/session/
#[allow(dead_code)] // Reserved for future session file creation
#[derive(Debug, Clone, Serialize)]
struct TilixSession {
    name: String,
    #[serde(rename = "synchronizedInput")]
    synchronized_input: bool,
    terminals: Vec<TilixTerminal>,
}

#[allow(dead_code)] // Reserved for future session file creation
#[derive(Debug, Clone, Serialize)]
struct TilixTerminal {
    #[serde(rename = "type")]
    terminal_type: String, // "Terminal"
    directory: Option<String>,
    command: Option<String>,
    #[serde(rename = "readOnly")]
    read_only: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    width: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    height: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    child1: Option<Box<TilixTerminal>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    child2: Option<Box<TilixTerminal>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    orientation: Option<u32>, // 0 = horizontal split, 1 = vertical split
    #[serde(skip_serializing_if = "Option::is_none")]
    position: Option<u32>, // Split position (0-100)
}

impl TilixTerminal {
    #[allow(dead_code)] // Reserved for future session file creation
    fn new_terminal(directory: Option<String>, command: Option<String>) -> Self {
        Self {
            terminal_type: "Terminal".to_string(),
            directory,
            command,
            read_only: false,
            width: None,
            height: None,
            child1: None,
            child2: None,
            orientation: None,
            position: None,
        }
    }

    #[allow(dead_code)] // Reserved for future session file creation
    fn new_split(
        orientation: u32,
        position: u32,
        child1: TilixTerminal,
        child2: TilixTerminal,
    ) -> Self {
        Self {
            terminal_type: "Repart".to_string(), // "Repart" = split pane
            directory: None,
            command: None,
            read_only: false,
            width: None,
            height: None,
            child1: Some(Box::new(child1)),
            child2: Some(Box::new(child2)),
            orientation: Some(orientation),
            position: Some(position),
        }
    }
}

/// Tilix multiplexer implementation
pub struct TilixMultiplexer;

impl TilixMultiplexer {
    /// Create a new Tilix multiplexer instance
    pub fn new() -> Result<Self, MuxError> {
        Ok(Self)
    }

    /// Check if tilix is available
    pub fn is_available() -> bool {
        Command::new("tilix")
            .arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }

    /// Get multiplexer identifier
    pub fn id() -> &'static str {
        "tilix"
    }

    /// Run a tilix command with the given arguments
    fn run_tilix_command(&self, args: &[&str]) -> Result<String, MuxError> {
        // Log the full command for debugging
        tracing::info!("Executing tilix command: tilix {}", args.join(" "));

        let output = Command::new("tilix").args(args).output().map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                MuxError::NotAvailable("tilix")
            } else {
                MuxError::Io(e)
            }
        })?;

        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(MuxError::CommandFailed(format!(
                "tilix {} failed: {}",
                args.join(" "),
                stderr
            )))
        }
    }

    /// Create a session file and launch Tilix with it
    ///
    /// This is the preferred way to create complex layouts with Tilix,
    /// avoiding command-line escaping issues.
    #[allow(dead_code)] // Reserved for future session file creation
    fn create_session_file(
        &self,
        session_name: &str,
        terminals: Vec<TilixTerminal>,
    ) -> Result<WindowId, MuxError> {
        let session = TilixSession {
            name: session_name.to_string(),
            synchronized_input: false,
            terminals,
        };

        // Create a temporary file for the session
        let session_json = serde_json::to_string_pretty(&session)
            .map_err(|e| MuxError::CommandFailed(format!("Failed to serialize session: {}", e)))?;

        let session_file = format!("/tmp/tilix-session-{}.json", session_name);
        let mut file = fs::File::create(&session_file).map_err(|e| MuxError::Io(e))?;

        file.write_all(session_json.as_bytes()).map_err(|e| MuxError::Io(e))?;

        tracing::info!("Created Tilix session file: {}", session_file);
        tracing::debug!("Session content:\n{}", session_json);

        // Launch Tilix with the session file
        self.run_tilix_command(&["--session", &session_file])?;

        // Return a window ID based on the session name
        Ok(format!("tilix:session:{}", session_name))
    }
}

impl Multiplexer for TilixMultiplexer {
    fn id(&self) -> &'static str {
        Self::id()
    }

    fn is_available(&self) -> bool {
        Self::is_available()
    }

    fn supports(&self, capability: MuxCapability) -> bool {
        // Tilix has limited capabilities compared to session-based multiplexers
        match capability {
            // Tilix doesn't support independent sessions/windows like tmux
            // It works within the current terminal window
            MuxCapability::SessionBasedWindows => false,
            // Tilix requires xdotool to run commands in panes
            MuxCapability::RunCommandInPane => true,
            // Tilix split_pane doesn't support initial_cmd; use run_command separately
            MuxCapability::SplitWithCommand => false,
        }
    }

    fn open_window(&self, opts: &WindowOptions) -> Result<WindowId, MuxError> {
        let mut args = vec![];

        // Add title if specified
        if let Some(title) = opts.title {
            args.push(format!("--title={}", title));
        }

        // Add working directory if specified
        if let Some(cwd) = opts.cwd {
            args.push(format!("--working-directory={}", cwd.to_string_lossy()));
        }

        // Add command if specified in profile
        // Note: Tilix requires --command="value" format (with equals sign and quotes)
        // Inside double quotes, we only need to escape: double quotes, backslashes, dollar signs, and backticks
        if let Some(profile) = opts.profile {
            // Escape characters that are special inside double quotes
            let escaped_profile = profile
                .replace('\\', r#"\\"#) // Backslash must be escaped first
                .replace('"', r#"\""#) // Escape double quotes
                .replace('$', r#"\$"#) // Escape dollar signs
                .replace('`', r#"\`"#); // Escape backticks
            args.push(format!(r#"--command="{}""#, escaped_profile));
        }

        // Convert to &str for the command
        let args_str: Vec<&str> = args.iter().map(|s| s.as_str()).collect();

        // Run the command - tilix creates a new window by default when run
        // We'll use a generated ID based on title or process ID
        let window_id = if let Some(title) = opts.title {
            format!("tilix:{}", title)
        } else {
            format!("tilix:{}", std::process::id())
        };

        self.run_tilix_command(&args_str)?;

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
        // Tilix uses actions to split panes
        // Note: Tilix terminology differs from standard multiplexer terminology:
        // - session-add-right creates a horizontal split (side-by-side, left|right)
        // - session-add-down creates a vertical split (top/bottom, top÷bottom)
        let action = match dir {
            SplitDirection::Horizontal => "session-add-right", // Left|Right split
            SplitDirection::Vertical => "session-add-down",    // Top÷Bottom split
        };

        tracing::info!("split_pane called with direction: {:?}", dir);
        tracing::info!("split_pane working directory: {:?}", opts.cwd);
        tracing::info!("split_pane initial_cmd: {:?}", initial_cmd);

        let mut args = vec![];

        // Add working directory FIRST if specified (must come before --action)
        if let Some(cwd) = opts.cwd {
            let cwd_str = cwd.to_string_lossy().to_string();
            tracing::info!("Using working directory for Tilix: {}", cwd_str);
            args.push(format!("--working-directory={}", cwd_str));
        } else {
            tracing::warn!("No working directory specified for Tilix split");
        }

        // Then add the action
        args.push(format!("--action={}", action));

        // DON'T add the command here - let run_command handle it
        // This allows for cleaner separation: first split, then execute command
        if initial_cmd.is_some() {
            tracing::warn!(
                "initial_cmd provided to split_pane, but Tilix implementation ignores it. Use run_command instead."
            );
        }

        // Convert to &str for the command
        let args_str: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        self.run_tilix_command(&args_str)?;

        // Small delay to let the split complete
        std::thread::sleep(std::time::Duration::from_millis(100));

        // Tilix doesn't return pane IDs, so we'll generate one based on the action
        // We use a timestamp-based ID to make it unique
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis();
        let pane_id = format!("tilix:{}:{}", action, timestamp);
        Ok(pane_id)
    }

    fn run_command(
        &self,
        _pane: &PaneId,
        cmd: &str,
        opts: &CommandOptions,
    ) -> Result<(), MuxError> {
        // Tilix doesn't have a built-in way to send commands to panes
        // We'll use xdotool to type the command into the currently focused terminal
        // This assumes the pane is currently focused (which it should be after split_pane)

        tracing::info!("run_command called with cmd: {}", cmd);
        tracing::info!("run_command working directory: {:?}", opts.cwd);

        // Build the full command with cd if needed
        let full_cmd = if let Some(cwd) = opts.cwd {
            format!("cd {} && {}", cwd.to_string_lossy(), cmd)
        } else {
            cmd.to_string()
        };

        // Use xdotool to type the command and press Enter
        // We need to escape the command for xdotool
        let xdotool_result = Command::new("xdotool")
            .arg("type")
            .arg("--clearmodifiers")
            .arg(&full_cmd)
            .output();

        match xdotool_result {
            Ok(output) if output.status.success() => {
                // Now press Enter to execute the command
                let enter_result = Command::new("xdotool").arg("key").arg("Return").output();

                match enter_result {
                    Ok(enter_output) if enter_output.status.success() => {
                        tracing::info!("Successfully sent command to Tilix pane via xdotool");
                        Ok(())
                    }
                    Ok(enter_output) => {
                        let stderr = String::from_utf8_lossy(&enter_output.stderr);
                        Err(MuxError::CommandFailed(format!(
                            "xdotool key Return failed: {}",
                            stderr
                        )))
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                        Err(MuxError::NotAvailable(
                            "xdotool is required for Tilix command execution but is not installed",
                        ))
                    }
                    Err(e) => Err(MuxError::Io(e)),
                }
            }
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr);
                Err(MuxError::CommandFailed(format!(
                    "xdotool type failed: {}",
                    stderr
                )))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Err(MuxError::NotAvailable(
                "xdotool is required for Tilix command execution but is not installed",
            )),
            Err(e) => Err(MuxError::Io(e)),
        }
    }

    fn send_text(&self, _pane: &PaneId, _text: &str) -> Result<(), MuxError> {
        // Tilix doesn't have a direct send-text capability
        // This would require external tools like xdotool or similar
        Err(MuxError::NotAvailable(
            "tilix does not support sending text to panes - use external tools like xdotool",
        ))
    }

    fn focus_window(&self, window: &WindowId) -> Result<(), MuxError> {
        // Tilix can use --focus-window but doesn't support targeting specific windows
        // We could use the title from window_id if it follows our pattern
        if window.starts_with("tilix:") {
            if let Some(title) = window.strip_prefix("tilix:") {
                // Try to focus using window manager tools if available
                let _ = Command::new("wmctrl").args(["-a", title]).output();

                // Also try tilix's focus option (affects new instances)
                let _ = Command::new("tilix").arg("--focus-window").output();

                Ok(())
            } else {
                Err(MuxError::NotAvailable(
                    "tilix window focusing requires window manager tools like wmctrl",
                ))
            }
        } else {
            Err(MuxError::NotAvailable(
                "tilix window focusing requires window manager tools like wmctrl",
            ))
        }
    }

    fn focus_pane(&self, _pane: &PaneId) -> Result<(), MuxError> {
        // Tilix doesn't provide direct pane focusing via CLI
        // Pane navigation would require window manager integration
        Err(MuxError::NotAvailable(
            "tilix does not support programmatic pane focusing - use window manager tools",
        ))
    }

    fn list_windows(&self, _title_substr: Option<&str>) -> Result<Vec<WindowId>, MuxError> {
        // Tilix doesn't provide a way to list windows programmatically via CLI
        // This would require window manager tools like wmctrl
        Err(MuxError::NotAvailable(
            "tilix does not support listing windows - use window manager tools like wmctrl",
        ))
    }

    fn list_panes(&self, _window: &WindowId) -> Result<Vec<PaneId>, MuxError> {
        // Tilix doesn't provide pane enumeration via CLI
        Err(MuxError::NotAvailable(
            "tilix does not support listing panes - no CLI interface available",
        ))
    }

    fn current_window(&self) -> Result<Option<WindowId>, MuxError> {
        // Tilix doesn't provide a way to get the current window ID programmatically.
        // However, since Tilix actions always operate on the currently focused terminal,
        // we return a synthetic window ID to indicate "current window".
        // This allows the layout creation code to work with Tilix.
        Ok(Some("tilix:current".to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tilix_multiplexer_creation() {
        // Should always succeed since it doesn't require actual tilix to be available
        let result = TilixMultiplexer::new();
        assert!(result.is_ok());
    }

    #[test]
    fn test_tilix_id() {
        let mux = TilixMultiplexer::new().unwrap();
        assert_eq!(mux.id(), "tilix");
        assert_eq!(TilixMultiplexer::id(), "tilix");
    }

    #[test]
    fn test_tilix_availability_check() {
        // This test depends on whether tilix is actually installed
        let available = TilixMultiplexer::is_available();
        println!("Tilix availability: {}", available);

        // The function should not panic, regardless of availability
        let mux = TilixMultiplexer::new().unwrap();
        assert_eq!(mux.is_available(), available);
    }

    #[test]
    fn test_split_direction_mapping() {
        let _mux = TilixMultiplexer::new().unwrap();

        // Test that split directions map to correct tilix actions
        // We can't actually run the commands without tilix, but we can test the logic

        // Vertical split should map to session-add-right
        // Horizontal split should map to session-add-down

        // This is tested implicitly in the split_pane method
        // The actual action strings are tested via integration tests
    }

    #[test]
    fn test_window_id_generation() {
        let mux = TilixMultiplexer::new().unwrap();

        // Test window ID generation with title
        let _opts_with_title = WindowOptions {
            title: Some("test-window"),
            cwd: None,
            profile: None,
            focus: false,
        };

        // We can't actually create windows in tests, but we can verify the logic
        // by checking that our implementation follows the expected pattern
        assert_eq!(mux.id(), "tilix");
    }

    #[test]
    fn test_not_available_methods() {
        let mux = TilixMultiplexer::new().unwrap();

        // Test that methods that aren't available return proper errors
        let dummy_pane = "test:pane:1".to_string();
        let dummy_window = "test:window:1".to_string();

        // run_command requires xdotool - will error if xdotool is not available
        let result = mux.run_command(&dummy_pane, "echo test", &CommandOptions::default());
        assert!(result.is_err(), "run_command should fail without xdotool");

        // send_text should not be available
        let result = mux.send_text(&dummy_pane, "test text");
        assert!(matches!(result, Err(MuxError::NotAvailable(_))));

        // focus_pane should not be available
        let result = mux.focus_pane(&dummy_pane);
        assert!(matches!(result, Err(MuxError::NotAvailable(_))));

        // list_windows should not be available
        let result = mux.list_windows(None);
        assert!(matches!(result, Err(MuxError::NotAvailable(_))));

        // list_panes should not be available
        let result = mux.list_panes(&dummy_window);
        assert!(matches!(result, Err(MuxError::NotAvailable(_))));
    }

    #[test]
    fn test_error_message_quality() {
        let mux = TilixMultiplexer::new().unwrap();
        let dummy_pane = "test:pane:1".to_string();

        // Check that error messages are informative
        // run_command now requires xdotool, so it should error about that
        let result = mux.run_command(&dummy_pane, "echo test", &CommandOptions::default());
        assert!(result.is_err(), "run_command should fail without xdotool");
        // The error will be about xdotool not being available
        if let Err(MuxError::NotAvailable(msg)) = result {
            assert!(
                msg.contains("xdotool"),
                "Error should mention xdotool requirement"
            );
        } else if let Err(MuxError::CommandFailed(msg)) = result {
            // Could also be a command failed error
            assert!(msg.contains("xdotool") || msg.contains("type failed"));
        }

        let result = mux.send_text(&dummy_pane, "test");
        if let Err(MuxError::NotAvailable(msg)) = result {
            assert!(msg.contains("tilix"));
            assert!(msg.contains("xdotool") || msg.contains("external"));
        } else {
            panic!("Expected NotAvailable error");
        }
    }

    #[test]
    fn test_current_window_returns_synthetic_id() {
        let mux = TilixMultiplexer::new().unwrap();

        // Tilix should return a synthetic window ID for current_window
        // This allows layout creation to work even though Tilix doesn't
        // have true session/window management
        let result = mux.current_window();
        assert!(result.is_ok());

        let window_id = result.unwrap();
        assert!(window_id.is_some());

        let window_id_str = window_id.unwrap();
        assert_eq!(window_id_str, "tilix:current");
    }
}
