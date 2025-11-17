// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Tilix multiplexer implementation
//!
//! Tilix is a Linux tiling terminal emulator that supports tabs and splits
//! through session files (declarative) and CLI actions (imperative).
//!
//! **Important Notes**:
//! - CLI actions (`--action=session-add-right/down`) are unreliable for programmatic
//!   layout creation because they depend on window focus. For production use, prefer
//!   session JSON files which declaratively define the entire layout.
//! - Commands passed via `--command` bypass shell initialization (no `.bashrc`, `.profile`, etc.)
//!   This implementation automatically wraps commands in a login shell (`$SHELL -l -c '...'`)
//!   to ensure environment variables, PATH, and aliases are properly loaded.
//!
//! See specs/Public/Terminal-Multiplexers/Tilix.md for details.

use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use ah_mux_core::{
    CommandOptions, Multiplexer, MuxError, PaneId, SplitDirection, WindowId, WindowOptions,
};

/// Tilix multiplexer implementation
///
/// Uses session JSON files for reliable layout creation.
pub struct TilixMultiplexer {
    window_counter: AtomicU32,
    pane_counter: AtomicU32,
}

impl Default for TilixMultiplexer {
    fn default() -> Self {
        Self {
            window_counter: AtomicU32::new(0),
            pane_counter: AtomicU32::new(0),
        }
    }
}

impl TilixMultiplexer {
    /// Create a new Tilix multiplexer instance
    pub fn new() -> Result<Self, MuxError> {
        let mux = Self::default();
        if !mux.is_available() {
            return Err(MuxError::NotAvailable("tilix"));
        }
        Ok(mux)
    }

    /// Generate a unique window ID
    fn next_window_id(&self) -> WindowId {
        let id = self.window_counter.fetch_add(1, Ordering::SeqCst);
        format!("tilix-window-{}", id)
    }

    /// Generate a unique pane ID
    fn next_pane_id(&self) -> PaneId {
        let id = self.pane_counter.fetch_add(1, Ordering::SeqCst);
        format!("tilix-pane-{}", id)
    }

    /// Get multiplexer identifier
    pub fn id() -> &'static str {
        "tilix"
    }

    /// Check if tilix is available
    pub fn is_available() -> bool {
        Command::new("tilix")
            .arg("--version")
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    }

    /// Run a tilix command with the given arguments
    fn run_tilix_command(&self, args: &[&str]) -> Result<String, MuxError> {
        let output = Command::new("tilix")
            .args(args)
            .output()
            .map_err(|e| MuxError::Other(format!("Failed to execute tilix: {}", e)))?;

        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
        } else {
            let error_msg = format!(
                "tilix command failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            Err(MuxError::CommandFailed(error_msg))
        }
    }

    /// Create a session JSON file for declarative layout creation
    ///
    /// This is the recommended approach for programmatic Tilix layouts as it's
    /// deterministic and doesn't depend on window focus state.
    #[allow(dead_code)]
    fn create_session_file(
        &self,
        title: &str,
        cwd: &std::path::Path,
        commands: &[(&str, bool)], // (command, read_only)
        layout: SessionLayout,
    ) -> Result<PathBuf, MuxError> {
        use serde_json::json;

        let session_file = std::env::temp_dir().join(format!("tilix-session-{}.json", title));

        let session_json = match layout {
            // Two-pane horizontal split
            SessionLayout::TwoPaneHorizontal => {
                let (cmd1, ro1) = commands.first().unwrap_or(&("bash", false));
                let (cmd2, ro2) = commands.get(1).unwrap_or(&("bash", false));

                json!({
                    "name": title,
                    "synchronizedInput": false,
                    "child": {
                        "type": "Paned",
                        "orientation": "horizontal",
                        "position": 50,
                        "children": [
                            {
                                "type": "Terminal",
                                "command": cmd1,
                                "directory": cwd.display().to_string(),
                                "profile": "Default",
                                "readOnly": *ro1
                            },
                            {
                                "type": "Terminal",
                                "command": cmd2,
                                "directory": cwd.display().to_string(),
                                "profile": "Default",
                                "readOnly": *ro2
                            }
                        ]
                    }
                })
            }
            // Three-pane: left vertical split + right pane
            SessionLayout::ThreePaneLeftVerticalRightSingle => {
                let (cmd1, ro1) = commands.first().unwrap_or(&("bash", false));
                let (cmd2, ro2) = commands.get(1).unwrap_or(&("bash", false));
                let (cmd3, ro3) = commands.get(2).unwrap_or(&("bash", false));

                json!({
                    "name": title,
                    "synchronizedInput": false,
                    "child": {
                        "type": "Paned",
                        "orientation": "horizontal",
                        "position": 50,
                        "children": [
                            {
                                "type": "Paned",
                                "orientation": "vertical",
                                "position": 70,
                                "children": [
                                    {
                                        "type": "Terminal",
                                        "command": cmd1,
                                        "directory": cwd.display().to_string(),
                                        "profile": "Default",
                                        "readOnly": *ro1
                                    },
                                    {
                                        "type": "Terminal",
                                        "command": cmd2,
                                        "directory": cwd.display().to_string(),
                                        "profile": "Default",
                                        "readOnly": *ro2
                                    }
                                ]
                            },
                            {
                                "type": "Terminal",
                                "command": cmd3,
                                "directory": cwd.display().to_string(),
                                "profile": "Default",
                                "readOnly": *ro3
                            }
                        ]
                    }
                })
            }
        };

        fs::write(&session_file, serde_json::to_string_pretty(&session_json).unwrap())
            .map_err(|e| MuxError::Other(format!("Failed to write session file: {}", e)))?;

        Ok(session_file)
    }
}

/// Session layout patterns supported by Tilix
#[allow(dead_code)]
#[derive(Debug, Clone, Copy)]
enum SessionLayout {
    TwoPaneHorizontal,
    ThreePaneLeftVerticalRightSingle,
}

impl Multiplexer for TilixMultiplexer {
    fn id(&self) -> &'static str {
        Self::id()
    }

    fn is_available(&self) -> bool {
        Self::is_available()
    }

    fn open_window(&self, opts: &WindowOptions) -> Result<WindowId, MuxError> {
        let window_id = self.next_window_id();
        
        // Build args as owned strings for proper lifetime management
        let mut args: Vec<String> = Vec::new();
        
        // Tilix doesn't have --new-window, it creates a new window by default
        // or use -w flag (but that's for "new instance mode")
        // When called without being in an existing Tilix instance, it creates a new window
        
        if let Some(title) = opts.title {
            args.push("--title".to_string());
            args.push(title.to_string());
        }

        if let Some(cwd) = opts.cwd {
            args.push("--working-directory".to_string());
            args.push(cwd.display().to_string());
        }

        // Convert to &str for the command
        let args_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        self.run_tilix_command(&args_refs)?;

        // Give Tilix a moment to create the window
        std::thread::sleep(Duration::from_millis(300));

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

        // Use CLI actions for splitting (note: this is fragile and depends on focus)
        // The correct action names are session-add-right and session-add-down
        //
        // IMPORTANT: Tilix --command bypasses shell initialization (.bashrc, .profile, etc.)
        // This means environment variables, PATH, and aliases are not loaded.
        // Solution: Wrap commands in a login shell (bash -l -c '...') to ensure full environment.
        let action = match dir {
            SplitDirection::Horizontal => "session-add-down",  // Horizontal split = down
            SplitDirection::Vertical => "session-add-right",   // Vertical split = right
        };

        // Build args as owned strings
        let mut args: Vec<String> = vec![
            "--action".to_string(),
            action.to_string(),
        ];

        if let Some(cwd) = opts.cwd {
            args.push("--working-directory".to_string());
            args.push(cwd.display().to_string());
        }

        if let Some(cmd) = initial_cmd {
            // Wrap command in a login shell to ensure environment is loaded
            // Tilix --command bypasses shell initialization, so we need to explicitly use a login shell
            // This ensures PATH, environment variables, and shell aliases are available
            //
            // Detect the user's shell from SHELL environment variable, fallback to bash
            let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string());
            
            // Escape single quotes in the command for safe shell interpolation
            let escaped_cmd = cmd.replace('\'', "'\\''");
            
            args.push("--command".to_string());
            args.push(format!("{} -l -c '{}'", shell, escaped_cmd));
        }

        // Convert to &str for the command
        let args_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        self.run_tilix_command(&args_refs)?;

        // Give Tilix a moment to create the pane
        std::thread::sleep(Duration::from_millis(200));

        Ok(pane_id)
    }

    fn run_command(
        &self,
        _pane: &PaneId,
        _cmd: &str,
        _opts: &CommandOptions,
    ) -> Result<(), MuxError> {
        // Tilix doesn't have direct send-text capability for existing panes
        // Commands must be specified when creating the pane with --command or session JSON
        Err(MuxError::NotAvailable(
            "Tilix does not support running commands in existing panes; use initial_cmd in split_pane or session JSON",
        ))
    }

    fn send_text(&self, _pane: &PaneId, _text: &str) -> Result<(), MuxError> {
        // Tilix doesn't have a direct send-text capability
        // This would require external tools like xdotool or D-Bus interface
        Err(MuxError::NotAvailable(
            "Tilix does not support sending text to panes; use xdotool or D-Bus as workaround",
        ))
    }

    fn focus_window(&self, _window: &WindowId) -> Result<(), MuxError> {
        // Tilix doesn't have direct window focusing via CLI
        // Window focus is typically handled by the window manager using wmctrl or xdotool
        Err(MuxError::Other(
            "Tilix does not support programmatic window focusing; use wmctrl/xdotool with window title".to_string()
        ))
    }

    fn focus_pane(&self, _pane: &PaneId) -> Result<(), MuxError> {
        // Tilix has navigation actions (session-switch-to-terminal-*) but they're
        // internal keyboard shortcuts, not available via --action
        Err(MuxError::NotAvailable(
            "Tilix does not support programmatic pane focusing via CLI",
        ))
    }

    fn list_windows(&self, _title_substr: Option<&str>) -> Result<Vec<WindowId>, MuxError> {
        // Tilix doesn't provide a way to list windows programmatically via CLI
        // This could potentially be done via D-Bus interface
        Err(MuxError::NotAvailable(
            "Tilix does not support listing windows via CLI; use D-Bus interface if needed",
        ))
    }

    fn list_panes(&self, _window: &WindowId) -> Result<Vec<PaneId>, MuxError> {
        // Tilix doesn't provide pane enumeration via CLI
        Err(MuxError::NotAvailable(
            "Tilix does not support listing panes via CLI",
        ))
    }
}
