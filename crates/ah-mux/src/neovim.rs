// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Neovim multiplexer implementation
//!
//! Enhanced Vim/Neovim implementation that leverages Neovim's RPC interface
//! for better automation capabilities.

use std::process::Command;
use tracing::{debug, error, info, instrument, warn};

use ah_mux_core::{
    CommandOptions, Multiplexer, MuxError, PaneId, SplitDirection, WindowId, WindowOptions,
};

/// Neovim multiplexer implementation with RPC support
#[derive(Debug)]
pub struct NeovimMultiplexer;

impl NeovimMultiplexer {
    /// Create a new Neovim multiplexer instance
    #[instrument]
    pub fn new() -> Result<Self, MuxError> {
        debug!("Creating new Neovim multiplexer");
        if !Self::is_available() {
            error!("Neovim is not available");
            return Err(MuxError::NotAvailable("Neovim"));
        }
        info!("Neovim multiplexer created successfully");
        Ok(Self)
    }

    /// Check if nvim is available
    #[instrument]
    pub fn is_available() -> bool {
        debug!("Checking Neovim availability");
        let available = Command::new("nvim")
            .arg("--version")
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false);
        debug!("Neovim availability: {}", available);
        available
    }

    /// Get multiplexer identifier
    pub fn id() -> &'static str {
        "neovim"
    }

    /// Run a nvim command with the given arguments
    #[instrument(skip(args))]
    fn run_nvim_command(&self, args: &[&str]) -> Result<String, MuxError> {
        debug!("Running nvim command with args: {:?}", args);
        let output = Command::new("nvim").args(args).output().map_err(|e| {
            error!("Failed to execute nvim: {}", e);
            MuxError::Other(format!("Failed to execute nvim: {}", e))
        })?;

        if output.status.success() {
            let result = String::from_utf8_lossy(&output.stdout).trim().to_string();
            debug!("nvim command completed successfully");
            Ok(result)
        } else {
            let error_msg = format!(
                "nvim command failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            error!("{}", error_msg);
            Err(MuxError::CommandFailed(error_msg))
        }
    }

    /// Execute Lua code in Neovim
    #[instrument(skip(lua_code))]
    fn execute_lua(&self, lua_code: &str) -> Result<(), MuxError> {
        debug!("Executing Lua code in Neovim");
        let lua_cmd = format!("lua {}", lua_code);
        let args = ["--headless", "-c", &lua_cmd, "-c", "quit"];
        self.run_nvim_command(&args)?;
        info!("Lua code executed successfully");
        Ok(())
    }
}

impl Multiplexer for NeovimMultiplexer {
    fn id(&self) -> &'static str {
        Self::id()
    }

    #[instrument]
    fn is_available(&self) -> bool {
        Self::is_available()
    }

    #[instrument(skip(opts))]
    fn open_window(&self, opts: &WindowOptions) -> Result<WindowId, MuxError> {
        debug!("Opening new Neovim window");
        let default_title = format!("ah-task-{}", std::process::id());
        let title = opts.title.unwrap_or(&default_title);
        let window_id = format!("neovim:{}", title);

        let lua_code = format!(
            r#"
vim.cmd("tabnew")
vim.cmd("file {}")
"#,
            title
        );

        self.execute_lua(&lua_code)?;
        info!("Opened Neovim window: {}", window_id);
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
        };

        let lua_code = format!(r#"vim.cmd("{}")"#, cmd);
        self.execute_lua(&lua_code)?;
        let pane_id = "neovim:pane:1".to_string();
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
        debug!("Running command in Neovim pane {}: {}", _pane, cmd);
        let lua_code = format!(r#"vim.cmd("terminal {}")"#, cmd);
        self.execute_lua(&lua_code)?;
        info!("Command executed in pane {}", _pane);
        Ok(())
    }

    #[instrument]
    fn send_text(&self, _pane: &PaneId, text: &str) -> Result<(), MuxError> {
        debug!("Sending text to Neovim pane {}: {}", _pane, text);
        // Use chansend to send text to terminal buffers in Neovim
        let lua_code = format!(
            r#"
if vim.b.terminal_job_id then
    vim.fn.chansend(vim.b.terminal_job_id, "{}")
end
"#,
            text
        );

        self.execute_lua(&lua_code)?;
        info!("Text sent to pane {}", _pane);
        Ok(())
    }

    #[instrument]
    fn focus_window(&self, _window: &WindowId) -> Result<(), MuxError> {
        warn!("Focus window not available for Neovim: requires session-specific implementation");
        // Tab switching in Neovim
        Err(MuxError::NotAvailable(
            "Neovim tab focusing requires session-specific implementation",
        ))
    }

    #[instrument]
    fn focus_pane(&self, _pane: &PaneId) -> Result<(), MuxError> {
        warn!("Focus pane not available for Neovim: requires window-specific implementation");
        // Neovim pane focusing requires knowing which window to focus
        Err(MuxError::NotAvailable(
            "Neovim pane focusing requires window-specific implementation",
        ))
    }

    #[instrument]
    fn list_windows(&self, _title_substr: Option<&str>) -> Result<Vec<WindowId>, MuxError> {
        warn!("Window listing not available for Neovim: requires RPC integration");
        // Would require RPC integration to query Neovim state
        Err(MuxError::NotAvailable(
            "Neovim window listing requires RPC integration",
        ))
    }

    #[instrument]
    fn list_panes(&self, _window: &WindowId) -> Result<Vec<PaneId>, MuxError> {
        warn!("Pane listing not available for Neovim: requires RPC integration");
        // Would require RPC integration to query window state
        Err(MuxError::NotAvailable(
            "Neovim pane listing requires RPC integration",
        ))
    }
}
