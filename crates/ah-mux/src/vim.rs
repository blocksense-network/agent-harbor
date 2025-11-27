// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Vim/Neovim multiplexer implementation
//!
//! This implementation supports both Vim and Neovim, with Neovim being preferred
//! due to its better RPC interface for automation.

use std::fs;
use std::process::Command;
use tempfile;
use tracing::{debug, error, info, instrument, warn};

use ah_mux_core::{
    CommandOptions, Multiplexer, MuxError, PaneId, SplitDirection, WindowId, WindowOptions,
};

/// Vim multiplexer implementation supporting both Vim and Neovim
#[derive(Debug)]
pub struct VimMultiplexer {
    /// Whether to use Neovim (preferred) or Vim
    use_neovim: bool,
}

impl VimMultiplexer {
    /// Create a new Vim multiplexer instance, preferring Neovim if available
    #[instrument]
    pub fn new() -> Result<Self, MuxError> {
        debug!("Creating new Vim multiplexer");
        let use_neovim = Command::new("nvim")
            .arg("--version")
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false);

        if !Self::is_available() {
            error!("Neither Vim nor Neovim are available");
            return Err(MuxError::NotAvailable("Vim/Neovim"));
        }

        let editor = if use_neovim { "Neovim" } else { "Vim" };
        info!("Vim multiplexer created successfully using {}", editor);
        Ok(Self { use_neovim })
    }

    /// Check if vim or nvim is available
    #[instrument]
    pub fn is_available() -> bool {
        debug!("Checking Vim/Neovim availability");
        let nvim_available = Command::new("nvim")
            .arg("--version")
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false);
        let vim_available = Command::new("vim")
            .arg("--version")
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false);

        let available = nvim_available || vim_available;
        debug!(
            "Vim availability: {} (nvim: {}, vim: {})",
            available, nvim_available, vim_available
        );
        available
    }

    /// Get multiplexer identifier
    pub fn id() -> &'static str {
        "vim"
    }

    /// Get the vim command to use
    fn vim_command(&self) -> &str {
        if self.use_neovim { "nvim" } else { "vim" }
    }

    /// Run a vim command with the given arguments
    #[instrument(skip(args))]
    fn run_vim_command(&self, args: &[&str]) -> Result<String, MuxError> {
        debug!(
            "Running {} command with args: {:?}",
            self.vim_command(),
            args
        );
        let output = Command::new(self.vim_command()).args(args).output().map_err(|e| {
            error!("Failed to execute {}: {}", self.vim_command(), e);
            MuxError::Other(format!("Failed to execute {}: {}", self.vim_command(), e))
        })?;

        if output.status.success() {
            let result = String::from_utf8_lossy(&output.stdout).trim().to_string();
            debug!("{} command completed successfully", self.vim_command());
            Ok(result)
        } else {
            let error_msg = format!(
                "{} command failed: {}",
                self.vim_command(),
                String::from_utf8_lossy(&output.stderr)
            );
            error!("{}", error_msg);
            Err(MuxError::CommandFailed(error_msg))
        }
    }

    /// Create a temporary vimscript file and execute it
    #[instrument(skip(script))]
    fn execute_vimscript(&self, script: &str) -> Result<(), MuxError> {
        debug!("Executing vimscript");
        let temp_file = tempfile::NamedTempFile::new().map_err(|e| {
            error!("Failed to create temporary file: {}", e);
            MuxError::Other(e.to_string())
        })?;
        let temp_path = temp_file.path();

        fs::write(temp_path, script).map_err(|e| {
            error!("Failed to write vimscript to file: {}", e);
            MuxError::Other(e.to_string())
        })?;

        let temp_path_str = temp_path.to_string_lossy().to_string();
        if self.use_neovim {
            let args = ["--headless", "-S", &temp_path_str, "-c", "quit"];
            self.run_vim_command(&args)?;
        } else {
            let args = ["-S", &temp_path_str, "-c", "quit"];
            self.run_vim_command(&args)?;
        }
        info!("Vimscript executed successfully");
        Ok(())
    }
}

impl Multiplexer for VimMultiplexer {
    fn id(&self) -> &'static str {
        Self::id()
    }

    #[instrument]
    fn is_available(&self) -> bool {
        Self::is_available()
    }

    #[instrument(skip(opts))]
    fn open_window(&self, opts: &WindowOptions) -> Result<WindowId, MuxError> {
        debug!("Opening new Vim window");
        // For Vim/Neovim, "opening a window" means starting vim with a specific layout
        // We'll create a script that sets up the initial layout

        let default_title = format!("ah-task-{}", std::process::id());
        let title = opts.title.unwrap_or(&default_title);
        let window_id = format!("vim:{}", title);

        let mut script = format!(
            r#"
tabnew
file {}
"#,
            title
        );

        if let Some(cwd) = opts.cwd {
            debug!("Setting working directory to: {}", cwd.display());
            script.push_str(&format!("cd {}\n", cwd.to_string_lossy()));
        }

        self.execute_vimscript(&script)?;
        info!("Opened Vim window: {}", window_id);
        Ok(window_id)
    }

    #[instrument(skip(_opts, _initial_cmd))]
    fn split_pane(
        &self,
        _window: Option<&WindowId>,
        _target: Option<&PaneId>,
        dir: SplitDirection,
        _percent: Option<u8>,
        _opts: &CommandOptions,
        _initial_cmd: Option<&str>,
    ) -> Result<PaneId, MuxError> {
        debug!("Splitting pane in direction: {:?}", dir);
        let cmd = match dir {
            SplitDirection::Vertical => "vsplit",
            SplitDirection::Horizontal => "split",
            SplitDirection::Auto => "split", // Fall back to horizontal split for now
        };

        let script = format!("{}\n", cmd);
        self.execute_vimscript(&script)?;
        let pane_id = "vim:pane:1".to_string();
        info!("Split pane created: {}", pane_id);
        Ok(pane_id)
    }

    #[instrument(skip(_opts))]
    fn run_command(
        &self,
        _pane: &PaneId,
        cmd: &str,
        _opts: &CommandOptions,
    ) -> Result<(), MuxError> {
        debug!("Running command in Vim pane {}: {}", _pane, cmd);
        let script = format!("terminal {}\n", cmd);
        self.execute_vimscript(&script)?;
        info!("Command executed in pane {}", _pane);
        Ok(())
    }

    #[instrument]
    fn send_text(&self, _pane: &PaneId, text: &str) -> Result<(), MuxError> {
        debug!("Sending text to Vim pane {}: {}", _pane, text);
        // For Vim/Neovim, we can use chansend to send text to terminal buffers
        // This is a simplified version - in practice, we'd need to track terminal buffer IDs
        if self.use_neovim {
            let script = format!(
                "call chansend(b:terminal_job_id, \"{}\")\n",
                text.escape_default()
            );
            self.execute_vimscript(&script)?;
            info!("Text sent to pane {}", _pane);
            Ok(())
        } else {
            warn!("Text sending not available in Vim: requires buffer-specific implementation");
            // Vim has term_sendkeys but it's more complex
            Err(MuxError::NotAvailable(
                "Vim text sending requires buffer-specific implementation",
            ))
        }
    }

    #[instrument]
    fn focus_window(&self, _window: &WindowId) -> Result<(), MuxError> {
        warn!("Focus window not available for Vim: requires session-specific implementation");
        // Window focus in Vim/Neovim context means switching tabs
        // This would require more complex tab management
        Err(MuxError::NotAvailable(
            "Vim tab focusing requires session-specific implementation",
        ))
    }

    #[instrument]
    fn focus_pane(&self, _pane: &PaneId) -> Result<(), MuxError> {
        warn!("Focus pane not available for Vim: requires window-specific implementation");
        // Vim pane focusing requires knowing which window to focus
        Err(MuxError::NotAvailable(
            "Vim pane focusing requires window-specific implementation",
        ))
    }

    #[instrument]
    fn list_windows(&self, _title_substr: Option<&str>) -> Result<Vec<WindowId>, MuxError> {
        warn!("Window listing not available for Vim: requires advanced RPC integration");
        // Listing windows would require parsing Vim's internal state
        // This is complex and not implemented in the basic version
        Err(MuxError::NotAvailable(
            "Vim window listing requires advanced RPC integration",
        ))
    }

    #[instrument]
    fn list_panes(&self, _window: &WindowId) -> Result<Vec<PaneId>, MuxError> {
        warn!("Pane listing not available for Vim: requires advanced RPC integration");
        // Similar to windows, pane listing requires parsing Vim's window state
        Err(MuxError::NotAvailable(
            "Vim pane listing requires advanced RPC integration",
        ))
    }
}
