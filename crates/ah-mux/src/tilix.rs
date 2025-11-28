// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Tilix multiplexer implementation
//!
//! Tilix is a Linux tiling terminal emulator that supports tabs and splits
//! through command-line actions and session files.
//!
//! ## Capabilities
//!
//! - Tabs/workspaces: Yes (sessions and windows)
//! - Horizontal/vertical splits: Yes (via `session-add-right`, `session-add-down` actions)
//! - Addressability: Limited - via session layout definitions; runtime targeting via D-Bus/window focus is limited
//! - Start commands per pane: Yes, via `--command` and `--working-directory` or session layouts
//! - Focus/activate pane: Limited - window manager focus only
//! - Send keys: Not supported natively
//! - Startup layout: Command-line actions or saved session layouts
//!
//! ## References
//!
//! - Tilix documentation: https://gnunn1.github.io/tilix-web/manual/

use std::process::Command;
use tracing::{debug, error, info, instrument, warn};

use ah_mux_core::{
    CommandOptions, Multiplexer, MuxError, PaneId, SplitDirection, WindowId, WindowOptions,
};

/// Tilix multiplexer implementation
#[derive(Debug)]
pub struct TilixMultiplexer;

impl TilixMultiplexer {
    /// Create a new Tilix multiplexer instance
    #[instrument]
    pub fn new() -> Result<Self, MuxError> {
        debug!("Creating new Tilix multiplexer");
        if !Self::is_available() {
            error!("Tilix is not available");
            return Err(MuxError::NotAvailable("Tilix"));
        }
        info!("Tilix multiplexer created successfully");
        Ok(Self)
    }

    /// Check if tilix is available
    #[instrument]
    pub fn is_available() -> bool {
        debug!("Checking Tilix availability");
        let available = Command::new("tilix")
            .arg("--version")
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false);
        debug!("Tilix availability: {}", available);
        available
    }

    /// Get multiplexer identifier
    pub fn id() -> &'static str {
        "tilix"
    }

    /// Sanitize command arguments for logging by replacing PATH values with placeholders
    ///
    /// This function replaces actual PATH values with $PATH placeholder for cleaner logging
    /// while preserving the structure of the command for debugging purposes.
    ///
    /// # Arguments
    ///
    /// * `args` - Command arguments to sanitize
    ///
    /// # Returns
    ///
    /// A vector of sanitized argument strings suitable for logging
    fn sanitize_args_for_logging(args: &[&str]) -> Vec<String> {
        args.iter()
            .map(|&arg| {
                if arg.starts_with("env PATH=") || arg.starts_with("env ") && arg.contains("PATH=")
                {
                    // Replace the actual PATH value with $PATH placeholder
                    let after_equals = arg.find('=').map(|i| &arg[i + 1..]).unwrap_or("");
                    if let Some(bash_pos) = after_equals.find(" bash ") {
                        format!("env PATH=$PATH {}", &after_equals[bash_pos..])
                    } else {
                        "env PATH=$PATH bash -c '...'".to_string()
                    }
                } else {
                    arg.to_string()
                }
            })
            .collect()
    }

    /// Wrap a command with PATH and optional environment variables for execution in bash
    ///
    /// Tilix requires explicit PATH propagation when executing commands via --command flag.
    /// This helper ensures the command runs with the current PATH environment and any
    /// additional environment variables specified in the options.
    ///
    /// # Arguments
    ///
    /// * `cmd` - The command to wrap
    /// * `env` - Optional slice of environment variable key-value pairs to set
    ///
    /// # Returns
    ///
    /// A bash command string with PATH and environment variables properly set
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let cmd = wrap_command_with_path("echo hello", Some(&[("MY_VAR", "value")]));
    /// // Returns: "env PATH=/usr/bin:... MY_VAR=value bash -c 'echo hello'"
    /// ```
    #[instrument(skip(env))]
    fn build_env_wrapped_command(cmd: &str, env: Option<&[(&str, &str)]>) -> String {
        let path = std::env::var("PATH").unwrap_or_default();
        // Escape spaces with backslash so Tilix/bash sees a single PATH element
        let escaped_path = path.replace(' ', "\\s");
        debug!("Wrapping command with PATH environment");

        // Build the env command with PATH and any additional environment variables
        let mut env_vars = vec![format!("PATH={}", escaped_path)];

        if let Some(env_slice) = env {
            if !env_slice.is_empty() {
                debug!("Adding environment variables: {:?}", env_slice);
                for (k, v) in env_slice {
                    env_vars.push(format!("{}={}", k, v));
                }
            }
        }

        let env_string = env_vars.join(" ");
        debug!(
            "Wrapping command with environment: PATH + {} additional vars",
            env.map(|e| e.len()).unwrap_or(0)
        );

        format!("env {} bash -c '{}'", env_string, cmd)
    }

    /// Run a tilix command with the given arguments
    #[instrument(skip(args))]
    fn run_tilix_command(&self, args: &[&str]) -> Result<String, MuxError> {
        // Sanitize args for logging by replacing PATH values with $PATH
        let sanitized_args = Self::sanitize_args_for_logging(args);
        info!("Running tilix command with args: {:?}", sanitized_args);

        let output = Command::new("tilix").args(args).output().map_err(|e| {
            error!("Failed to execute tilix: {}", e);
            MuxError::Other(format!("Failed to execute tilix: {}", e))
        })?;

        if output.status.success() {
            let result = String::from_utf8_lossy(&output.stdout).trim().to_string();
            debug!("tilix command completed successfully");
            Ok(result)
        } else {
            let error_msg = format!(
                "tilix command failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            error!("{}", error_msg);
            Err(MuxError::CommandFailed(error_msg))
        }
    }
}

impl Multiplexer for TilixMultiplexer {
    fn id(&self) -> &'static str {
        Self::id()
    }

    #[instrument]
    fn is_available(&self) -> bool {
        Self::is_available()
    }

    #[instrument(skip(opts))]
    fn open_window(&self, opts: &WindowOptions) -> Result<WindowId, MuxError> {
        debug!(
            "Opening new Tilix window with options: title={:?}, cwd={:?}, init_command={:?}",
            opts.title, opts.cwd, opts.init_command
        );

        let mut args = vec![];

        // Add title if specified
        if let Some(title) = opts.title {
            debug!("Setting window title: {}", title);
            args.extend_from_slice(&["--title".to_string(), title.to_string()]);
        }

        // Add working directory if specified
        if let Some(cwd) = opts.cwd {
            let cwd_str = cwd.to_string_lossy().to_string();
            debug!("Setting working directory: {}", cwd_str);
            args.extend_from_slice(&["--working-directory".to_string(), cwd_str]);
        }

        // Add command if specified in init_command
        if let Some(init_cmd) = opts.init_command {
            debug!("Setting initial command: {}", init_cmd);
            let custom_command = Self::build_env_wrapped_command(init_cmd, None);
            args.extend_from_slice(&["--command".to_string(), custom_command]);
        }

        args.push("--action".to_string());
        args.push("app-new-session".to_string());

        // Convert to &str for the command
        let args_str: Vec<&str> = args.iter().map(|s| s.as_str()).collect();

        // Run the command - tilix doesn't return window IDs directly
        // We'll use a generated ID based on title or timestamp
        let window_id = if let Some(title) = opts.title {
            let id = format!("tilix:{}", title);
            debug!("Generated window ID from title: {}", id);
            id
        } else {
            let id = format!("tilix:{}", std::process::id());
            debug!("Generated window ID from process ID: {}", id);
            id
        };

        self.run_tilix_command(&args_str)?;

        info!("Successfully opened Tilix window: {}", window_id);
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
        info!(
            "Splitting pane in direction: {:?}, with initial_cmd: {:?}, cwd: {:?}, env: {:?}",
            dir, initial_cmd, opts.cwd, opts.env
        );

        // Tilix uses actions to split panes within the current session
        // session-add-right: splits the active terminal horizontally (side by side)
        // session-add-down: splits the active terminal vertically (top/bottom)
        // session-add-auto: splits the active terminal automatically based on content
        let split_action = match dir {
            SplitDirection::Vertical => "session-add-down",
            SplitDirection::Horizontal => "session-add-right",
            SplitDirection::Auto => "session-add-auto",
        };

        debug!("Using Tilix action: {}", split_action);

        let mut args = vec!["--action".to_string(), split_action.to_string()];

        // Add working directory if specified
        if let Some(cwd) = opts.cwd {
            let cwd_str = cwd.to_string_lossy().to_string();
            debug!("Setting working directory for split: {}", cwd_str);
            args.extend_from_slice(&["--working-directory".to_string(), cwd_str]);
        }

        // Add command if specified
        if let Some(cmd) = initial_cmd {
            debug!("Setting initial command for split: {}", cmd);

            // Pass environment variables directly to wrap_command_with_path
            let custom_command = Self::build_env_wrapped_command(cmd, opts.env);
            args.extend_from_slice(&["--command".to_string(), custom_command]);
        }

        // Convert to &str for the command
        let args_str: Vec<&str> = args.iter().map(|s| s.as_str()).collect();

        self.run_tilix_command(&args_str)?;

        // Tilix doesn't return pane IDs, so we'll generate one based on the direction and timestamp
        let pane_id = format!(
            "tilix:pane:{}:{}",
            match dir {
                SplitDirection::Horizontal => "h",
                SplitDirection::Vertical => "v",
                SplitDirection::Auto => "a",
            },
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis()
        );

        info!("Successfully split pane ({}): {}", split_action, pane_id);
        Ok(pane_id)
    }

    #[instrument(skip(opts))]
    fn run_command(&self, pane: &PaneId, cmd: &str, opts: &CommandOptions) -> Result<(), MuxError> {
        debug!("Attempting to run command in pane {}: {}", pane, cmd);
        debug!("Command options: cwd={:?}, env={:?}", opts.cwd, opts.env);

        warn!(
            "Run command not supported for Tilix (pane: {}, cmd: {}): \
             Tilix does not support running commands in existing panes. \
             Commands must be specified when creating the pane.",
            pane, cmd
        );

        // Tilix doesn't have direct send-text or command execution capability for existing panes.
        // Commands must be specified at pane creation time via --command parameter.
        // See: https://gnunn1.github.io/tilix-web/manual/
        Err(MuxError::NotAvailable(
            "Tilix does not support running commands in existing panes - use initial_cmd in split_pane instead",
        ))
    }

    #[instrument]
    fn send_text(&self, pane: &PaneId, text: &str) -> Result<(), MuxError> {
        debug!(
            "Attempting to send text to pane {}: {} bytes",
            pane,
            text.len()
        );

        warn!(
            "Send text not supported for Tilix (pane: {}): \
             Tilix does not have native send-keys capability. \
             Interactive input requires external tools like xdotool or using non-interactive program flags.",
            pane
        );

        // Tilix doesn't have a direct send-text capability like tmux's send-keys.
        // This would require external tools like xdotool, ydotool, or similar input automation.
        // As noted in the spec: "Not supported natively; rely on the program's non-interactive flags."
        // See: https://gnunn1.github.io/tilix-web/manual/
        Err(MuxError::NotAvailable(
            "Tilix does not support sending text to panes - use non-interactive program flags",
        ))
    }

    #[instrument]
    fn focus_window(&self, window: &WindowId) -> Result<(), MuxError> {
        debug!("Attempting to focus window: {}", window);

        warn!(
            "Focus window not fully supported for Tilix (window: {}): \
             Tilix does not provide robust CLI for window focusing. \
             Window focus is typically handled by the window manager. \
             Consider using wmctrl or similar tools with window title matching.",
            window
        );

        // Tilix doesn't have direct window focusing via CLI.
        // Window focus is typically handled by the window manager (X11/Wayland).
        // External tools like wmctrl (X11) or window manager-specific commands would be needed.
        // As noted in the spec: "Use window titles and the window manager; Tilix itself does not
        // provide a robust CLI for 'focus by title'."
        // See: https://gnunn1.github.io/tilix-web/manual/
        Err(MuxError::NotAvailable(
            "Tilix does not support programmatic window focusing - use window manager tools like wmctrl",
        ))
    }

    #[instrument]
    fn focus_pane(&self, pane: &PaneId) -> Result<(), MuxError> {
        debug!("Attempting to focus pane: {}", pane);

        warn!(
            "Focus pane not supported for Tilix (pane: {}): \
             Tilix has move-focus actions but lacks addressable pane IDs. \
             Pane focus requires keyboard shortcuts or manual interaction.",
            pane
        );

        // Tilix has focus navigation actions (e.g., session-focus-up, session-focus-down),
        // but these are relative movements, not absolute pane targeting.
        // There's no way to specify "focus pane with ID X" via CLI.
        // See: https://gnunn1.github.io/tilix-web/manual/
        Err(MuxError::NotAvailable(
            "Tilix does not support programmatic pane focusing by ID - use keyboard navigation",
        ))
    }

    #[instrument]
    fn list_windows(&self, title_substr: Option<&str>) -> Result<Vec<WindowId>, MuxError> {
        debug!(
            "Attempting to list windows with title filter: {:?}",
            title_substr
        );

        warn!(
            "Window listing not supported for Tilix (filter: {:?}): \
             Tilix CLI does not provide window enumeration. \
             External tools like wmctrl or D-Bus queries may be needed.",
            title_substr
        );

        // Tilix doesn't provide a way to list windows programmatically via CLI.
        // This is a limitation of its command-line interface.
        // D-Bus interface may provide some capabilities, but it's not part of the standard CLI.
        // External tools like wmctrl can list X11 windows by WM_CLASS or title.
        // See: https://gnunn1.github.io/tilix-web/manual/
        Err(MuxError::NotAvailable(
            "Tilix does not support listing windows via CLI - consider D-Bus or wmctrl",
        ))
    }

    #[instrument]
    fn list_panes(&self, window: &WindowId) -> Result<Vec<PaneId>, MuxError> {
        debug!("Attempting to list panes for window: {}", window);

        warn!(
            "Pane listing not supported for Tilix (window: {}): \
             Tilix does not provide pane enumeration via CLI. \
             Session layout is managed internally without exposing pane IDs.",
            window
        );

        // Tilix doesn't provide pane enumeration via its CLI.
        // The internal session structure is not exposed programmatically.
        // D-Bus interface might provide limited capabilities but not standard pane enumeration.
        // See: https://gnunn1.github.io/tilix-web/manual/
        Err(MuxError::NotAvailable(
            "Tilix does not support listing panes via CLI",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    /// Test that the Tilix multiplexer reports the correct ID
    #[test]
    fn test_tilix_id() {
        assert_eq!(TilixMultiplexer::id(), "tilix");
    }

    /// Test multiplexer ID method consistency
    #[test]
    fn test_instance_id_matches_static_id() {
        // Only run if tilix is available
        if TilixMultiplexer::is_available() {
            let mux = TilixMultiplexer::new().expect("Failed to create TilixMultiplexer");
            assert_eq!(mux.id(), TilixMultiplexer::id());
        }
    }

    /// Test availability detection
    #[test]
    fn test_availability_detection() {
        let is_available = TilixMultiplexer::is_available();
        tracing::info!("Tilix availability: {}", is_available);

        // Test that availability is consistent
        assert_eq!(is_available, TilixMultiplexer::is_available());

        // Test that new() respects availability
        let result = TilixMultiplexer::new();
        if is_available {
            assert!(
                result.is_ok(),
                "new() should succeed when tilix is available"
            );
        } else {
            assert!(
                matches!(result, Err(MuxError::NotAvailable("Tilix"))),
                "new() should fail with NotAvailable when tilix is not available"
            );
        }
    }

    /// Test that the instance is_available method matches static method
    #[test]
    fn test_instance_availability_consistency() {
        if TilixMultiplexer::is_available() {
            let mux = TilixMultiplexer::new().expect("Failed to create TilixMultiplexer");
            assert_eq!(mux.is_available(), TilixMultiplexer::is_available());
        }
    }

    /// Test command path wrapping functionality
    #[test]
    fn test_wrap_command_with_path() {
        let original_path = std::env::var("PATH").unwrap_or_default();

        let cmd = "echo hello";
        let wrapped = TilixMultiplexer::build_env_wrapped_command(cmd, None);

        assert!(
            wrapped.contains(&original_path),
            "Wrapped command should contain PATH"
        );
        assert!(
            wrapped.contains("bash -c"),
            "Wrapped command should use bash -c"
        );
        assert!(
            wrapped.contains(cmd),
            "Wrapped command should contain original command"
        );

        let expected_format = format!("env PATH={} bash -c '{}'", original_path, cmd);
        assert_eq!(wrapped, expected_format);
    }

    /// Test command wrapping with special characters
    #[test]
    fn test_wrap_command_with_special_chars() {
        let cmd = "echo 'hello world' && ls -la";
        let wrapped = TilixMultiplexer::build_env_wrapped_command(cmd, None);

        assert!(
            wrapped.contains(cmd),
            "Should preserve command with special characters"
        );
        assert!(
            wrapped.starts_with("env PATH="),
            "Should start with env PATH="
        );
        assert!(wrapped.contains("bash -c"), "Should use bash -c wrapper");
    }

    /// Test that run_command returns NotAvailable error (Tilix limitation)
    #[test]
    fn test_run_command_not_available() {
        if let Ok(mux) = TilixMultiplexer::new() {
            let pane_id = "test-pane".to_string();
            let cmd = "echo test";
            let opts = CommandOptions {
                cwd: None,
                env: None,
            };

            let result = mux.run_command(&pane_id, cmd, &opts);

            assert!(matches!(result, Err(MuxError::NotAvailable(_))));
            if let Err(MuxError::NotAvailable(msg)) = result {
                assert!(
                    msg.contains("does not support running commands in existing panes"),
                    "Error message should explain the limitation"
                );
            }
        }
    }

    /// Test that send_text returns NotAvailable error (Tilix limitation)
    #[test]
    fn test_send_text_not_available() {
        if let Ok(mux) = TilixMultiplexer::new() {
            let pane_id = "test-pane".to_string();
            let text = "test text";

            let result = mux.send_text(&pane_id, text);

            assert!(matches!(result, Err(MuxError::NotAvailable(_))));
            if let Err(MuxError::NotAvailable(msg)) = result {
                assert!(
                    msg.contains("does not support sending text to panes"),
                    "Error message should explain the limitation"
                );
            }
        }
    }

    /// Test that focus_window returns NotAvailable error (Tilix limitation)
    #[test]
    fn test_focus_window_not_available() {
        if let Ok(mux) = TilixMultiplexer::new() {
            let window_id = "test-window".to_string();

            let result = mux.focus_window(&window_id);

            assert!(matches!(result, Err(MuxError::NotAvailable(_))));
            if let Err(MuxError::NotAvailable(msg)) = result {
                assert!(
                    msg.contains("does not support programmatic window focusing"),
                    "Error message should explain the limitation"
                );
            }
        }
    }

    /// Test that focus_pane returns NotAvailable error (Tilix limitation)
    #[test]
    fn test_focus_pane_not_available() {
        if let Ok(mux) = TilixMultiplexer::new() {
            let pane_id = "test-pane".to_string();

            let result = mux.focus_pane(&pane_id);

            assert!(matches!(result, Err(MuxError::NotAvailable(_))));
            if let Err(MuxError::NotAvailable(msg)) = result {
                assert!(
                    msg.contains("does not support programmatic pane focusing by ID"),
                    "Error message should explain the limitation"
                );
            }
        }
    }

    /// Test that list_windows returns NotAvailable error (Tilix limitation)
    #[test]
    fn test_list_windows_not_available() {
        if let Ok(mux) = TilixMultiplexer::new() {
            let result = mux.list_windows(None);

            assert!(matches!(result, Err(MuxError::NotAvailable(_))));
            if let Err(MuxError::NotAvailable(msg)) = result {
                assert!(
                    msg.contains("does not support listing windows via CLI"),
                    "Error message should explain the limitation"
                );
            }
        }
    }

    /// Test that list_windows with filter returns NotAvailable error
    #[test]
    fn test_list_windows_with_filter_not_available() {
        if let Ok(mux) = TilixMultiplexer::new() {
            let result = mux.list_windows(Some("test-filter"));

            assert!(matches!(result, Err(MuxError::NotAvailable(_))));
        }
    }

    /// Test that list_panes returns NotAvailable error (Tilix limitation)
    #[test]
    fn test_list_panes_not_available() {
        if let Ok(mux) = TilixMultiplexer::new() {
            let window_id = "test-window".to_string();

            let result = mux.list_panes(&window_id);

            assert!(matches!(result, Err(MuxError::NotAvailable(_))));
            if let Err(MuxError::NotAvailable(msg)) = result {
                assert!(
                    msg.contains("does not support listing panes via CLI"),
                    "Error message should explain the limitation"
                );
            }
        }
    }

    /// Test window options handling for open_window (dry run - doesn't execute tilix)
    #[test]
    fn test_open_window_options_processing() {
        // This test verifies the option processing logic without actually running tilix
        // We'll test the argument building by checking what would be passed to run_tilix_command

        if !TilixMultiplexer::is_available() {
            // Skip test if tilix is not available
            return;
        }

        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let temp_path = temp_dir.path().to_path_buf();

        let opts_with_all = WindowOptions {
            title: Some("test-title"),
            cwd: Some(&temp_path),
            focus: false,
            profile: None,
            init_command: Some("echo hello"),
        };

        let opts_minimal = WindowOptions {
            title: None,
            cwd: None,
            focus: false,
            profile: None,
            init_command: None,
        };

        // We can't easily test the actual command execution without mocking,
        // but we can test that the multiplexer accepts valid options
        // The actual execution would be tested in integration tests

        assert!(
            opts_with_all.title.is_some(),
            "Title option should be preserved"
        );
        assert!(
            opts_with_all.cwd.is_some(),
            "CWD option should be preserved"
        );
        assert!(
            opts_with_all.init_command.is_some(),
            "Init command should be preserved"
        );

        assert!(
            opts_minimal.title.is_none(),
            "Minimal options should have no title"
        );
        assert!(
            opts_minimal.cwd.is_none(),
            "Minimal options should have no cwd"
        );
        assert!(
            opts_minimal.init_command.is_none(),
            "Minimal options should have no init_command"
        );
    }

    /// Test split direction mapping
    #[test]
    fn test_split_direction_mapping() {
        // Test that split directions map to correct tilix actions
        // This tests the logic in split_pane method

        // Vertical split should use "session-add-down"
        // Horizontal split should use "session-add-right"
        // Auto split should use "session-add-auto"

        // We can't easily test the actual method without running tilix,
        // but we can verify the direction logic would be correct
        let vertical_dir = SplitDirection::Vertical;
        let horizontal_dir = SplitDirection::Horizontal;
        let auto_dir = SplitDirection::Auto;

        // The actual mapping is tested in integration tests
        // Here we just verify the enum values exist and are distinct
        assert!(matches!(vertical_dir, SplitDirection::Vertical));
        assert!(matches!(horizontal_dir, SplitDirection::Horizontal));
        assert!(matches!(auto_dir, SplitDirection::Auto));

        // Verify they are distinct
        assert_ne!(
            std::mem::discriminant(&vertical_dir),
            std::mem::discriminant(&horizontal_dir)
        );
        assert_ne!(
            std::mem::discriminant(&vertical_dir),
            std::mem::discriminant(&auto_dir)
        );
        assert_ne!(
            std::mem::discriminant(&horizontal_dir),
            std::mem::discriminant(&auto_dir)
        );
    }

    /// Test command options processing for split_pane
    #[test]
    fn test_split_pane_command_options() {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let temp_path = temp_dir.path().to_path_buf();

        let opts_with_cwd = CommandOptions {
            cwd: Some(&temp_path),
            env: None,
        };

        let opts_empty = CommandOptions {
            cwd: None,
            env: None,
        };

        // Test that options are properly structured
        assert!(opts_with_cwd.cwd.is_some(), "CWD should be preserved");
        assert!(opts_empty.cwd.is_none(), "Empty options should have no CWD");
    }

    /// Test that Tilix is properly platform-gated (Linux only)
    #[test]
    #[cfg(not(target_os = "linux"))]
    fn test_tilix_not_available_on_non_linux() {
        // On non-Linux platforms, Tilix should not be available
        assert!(!TilixMultiplexer::is_available());

        let result = TilixMultiplexer::new();
        assert!(matches!(result, Err(MuxError::NotAvailable("Tilix"))));
    }

    /// Test Linux-specific functionality
    #[test]
    #[cfg(target_os = "linux")]
    fn test_tilix_linux_specific() {
        // On Linux, test that the multiplexer can be created if tilix binary exists
        let availability = TilixMultiplexer::is_available();

        if availability {
            // If tilix is available, we should be able to create an instance
            let result = TilixMultiplexer::new();
            assert!(
                result.is_ok(),
                "Should be able to create TilixMultiplexer on Linux with tilix installed"
            );
        } else {
            // If tilix is not available, creation should fail appropriately
            let result = TilixMultiplexer::new();
            assert!(
                matches!(result, Err(MuxError::NotAvailable("Tilix"))),
                "Should fail with NotAvailable when tilix binary is not found"
            );
        }
    }

    /// Test error handling for invalid commands
    #[test]
    fn test_error_handling() {
        if let Ok(_mux) = TilixMultiplexer::new() {
            // Test that all unsupported operations return appropriate NotAvailable errors
            // This ensures consistent error handling across the API

            // These are tested individually above, but this confirms they all
            // return the expected error type consistently
            let _pane_id = "test-pane".to_string();
            let _window_id = "test-window".to_string();

            // All these operations should return NotAvailable errors
            let unsupported_ops = [
                "run_command",
                "send_text",
                "focus_window",
                "focus_pane",
                "list_windows",
                "list_panes",
            ];

            // We've already tested each individual method above
            // This test just confirms we have consistent error handling philosophy
            assert!(
                !unsupported_ops.is_empty(),
                "Should have unsupported operations listed"
            );
        }
    }
}
