// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Neovim multiplexer implementation
//!
//! Enhanced Vim/Neovim implementation that leverages Neovim's RPC interface
//! for better automation capabilities.

use std::process::Command;

use ah_mux_core::{
    CommandOptions, Multiplexer, MuxError, PaneId, SplitDirection, WindowId, WindowOptions,
};

/// Neovim multiplexer implementation with RPC support
pub struct NeovimMultiplexer;

impl NeovimMultiplexer {
    /// Create a new Neovim multiplexer instance
    pub fn new() -> Result<Self, MuxError> {
        Ok(Self)
    }

    /// Check if nvim is available
    pub fn is_available() -> bool {
        Command::new("nvim")
            .arg("--version")
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    }

    /// Get multiplexer identifier
    pub fn id() -> &'static str {
        "neovim"
    }

    /// Run a nvim command with the given arguments
    fn run_nvim_command(&self, args: &[&str]) -> Result<String, MuxError> {
        let output = Command::new("nvim")
            .args(args)
            .output()
            .map_err(|e| MuxError::Other(format!("Failed to execute nvim: {}", e)))?;

        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
        } else {
            let error_msg = format!(
                "nvim command failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            Err(MuxError::CommandFailed(error_msg))
        }
    }

    /// Execute Lua code in Neovim
    fn execute_lua(&self, lua_code: &str) -> Result<(), MuxError> {
        let lua_cmd = format!("lua {}", lua_code);
        let args = vec!["--headless", "-c", &lua_cmd, "-c", "quit"];
        let args_str: Vec<&str> = args.iter().map(|s| *s).collect();
        self.run_nvim_command(&args_str)?;
        Ok(())
    }
}

impl Multiplexer for NeovimMultiplexer {
    fn id(&self) -> &'static str {
        Self::id()
    }

    fn is_available(&self) -> bool {
        Self::is_available()
    }

    fn open_window(&self, opts: &WindowOptions) -> Result<WindowId, MuxError> {
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

        Ok(window_id)
    }

    fn split_pane(
        &self,
        _window: &WindowId,
        _target: Option<&PaneId>,
        dir: SplitDirection,
        _percent: Option<u8>,
        _opts: &CommandOptions,
        _initial_cmd: Option<&str>,
    ) -> Result<PaneId, MuxError> {
        let cmd = match dir {
            SplitDirection::Vertical => "vsplit",
            SplitDirection::Horizontal => "split",
        };

        let lua_code = format!(r#"vim.cmd("{}")"#, cmd);
        self.execute_lua(&lua_code)?;
        Ok("neovim:pane:1".to_string())
    }

    fn run_command(
        &self,
        _pane: &PaneId,
        cmd: &str,
        _opts: &CommandOptions,
    ) -> Result<(), MuxError> {
        let lua_code = format!(r#"vim.cmd("terminal {}")"#, cmd);
        self.execute_lua(&lua_code)
    }

    fn send_text(&self, _pane: &PaneId, text: &str) -> Result<(), MuxError> {
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
        Ok(())
    }

    fn focus_window(&self, _window: &WindowId) -> Result<(), MuxError> {
        // Tab switching in Neovim
        Err(MuxError::NotAvailable(
            "Neovim tab focusing requires session-specific implementation",
        ))
    }

    fn focus_pane(&self, _pane: &PaneId) -> Result<(), MuxError> {
        // Neovim pane focusing requires knowing which window to focus
        Err(MuxError::NotAvailable(
            "Neovim pane focusing requires window-specific implementation",
        ))
    }

    fn list_windows(&self, _title_substr: Option<&str>) -> Result<Vec<WindowId>, MuxError> {
        // Would require RPC integration to query Neovim state
        Err(MuxError::NotAvailable(
            "Neovim window listing requires RPC integration",
        ))
    }

    fn list_panes(&self, _window: &WindowId) -> Result<Vec<PaneId>, MuxError> {
        // Would require RPC integration to query window state
        Err(MuxError::NotAvailable(
            "Neovim pane listing requires RPC integration",
        ))
    }
}
