//! Vim/Neovim multiplexer implementation
//!
//! This implementation supports both Vim and Neovim, with Neovim being preferred
//! due to its better RPC interface for automation.

use std::process::Command;
use std::fs;
use tempfile;

use ah_mux_core::{Multiplexer, WindowId, PaneId, WindowOptions, CommandOptions, SplitDirection, MuxError};

/// Vim multiplexer implementation supporting both Vim and Neovim
pub struct VimMultiplexer {
    /// Whether to use Neovim (preferred) or Vim
    use_neovim: bool,
}

impl VimMultiplexer {
    /// Create a new Vim multiplexer instance, preferring Neovim if available
    pub fn new() -> Result<Self, MuxError> {
        let use_neovim = Command::new("nvim")
            .arg("--version")
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false);

        Ok(Self { use_neovim })
    }

    /// Check if vim or nvim is available
    pub fn is_available() -> bool {
        Command::new("nvim")
            .arg("--version")
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
            || Command::new("vim")
                .arg("--version")
                .output()
                .map(|output| output.status.success())
                .unwrap_or(false)
    }

    /// Get multiplexer identifier
    pub fn id() -> &'static str {
        "vim"
    }

    /// Get the vim command to use
    fn vim_command(&self) -> &str {
        if self.use_neovim {
            "nvim"
        } else {
            "vim"
        }
    }

    /// Run a vim command with the given arguments
    fn run_vim_command(&self, args: &[&str]) -> Result<String, MuxError> {
        let output = Command::new(self.vim_command())
            .args(args)
            .output()
            .map_err(|e| MuxError::Other(format!("Failed to execute {}: {}", self.vim_command(), e)))?;

        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
        } else {
            let error_msg = format!(
                "{} command failed: {}",
                self.vim_command(),
                String::from_utf8_lossy(&output.stderr)
            );
            Err(MuxError::CommandFailed(error_msg))
        }
    }


    /// Create a temporary vimscript file and execute it
    fn execute_vimscript(&self, script: &str) -> Result<(), MuxError> {
        let temp_file = tempfile::NamedTempFile::new()
            .map_err(|e| MuxError::Other(e.to_string()))?;
        let temp_path = temp_file.path();

        fs::write(temp_path, script)
            .map_err(|e| MuxError::Other(e.to_string()))?;

        let temp_path_str = temp_path.to_string_lossy().to_string();
        let args = if self.use_neovim {
            vec!["--headless", "-S", &temp_path_str, "-c", "quit"]
        } else {
            vec!["-S", &temp_path_str, "-c", "quit"]
        };

        let args_str: Vec<&str> = args.iter().map(|s| *s).collect();
        self.run_vim_command(&args_str)?;
        Ok(())
    }
}

impl Multiplexer for VimMultiplexer {
    fn id(&self) -> &'static str {
        Self::id()
    }

    fn is_available(&self) -> bool {
        Self::is_available()
    }

    fn open_window(&self, opts: &WindowOptions) -> Result<WindowId, MuxError> {
        // For Vim/Neovim, "opening a window" means starting vim with a specific layout
        // We'll create a script that sets up the initial layout

        let default_title = format!("ah-task-{}", std::process::id());
        let title = opts.title.unwrap_or(&default_title);
        let window_id = format!("vim:{}", title);

        let mut script = format!(r#"
tabnew
file {}
"#, title);

        if let Some(cwd) = opts.cwd {
            script.push_str(&format!("cd {}\n", cwd.to_string_lossy()));
        }

        self.execute_vimscript(&script)?;

        Ok(window_id)
    }

    fn split_pane(&self, _window: &WindowId, _target: Option<&PaneId>, dir: SplitDirection, _percent: Option<u8>, _opts: &CommandOptions, _initial_cmd: Option<&str>) -> Result<PaneId, MuxError> {
        let cmd = match dir {
            SplitDirection::Vertical => "vsplit",
            SplitDirection::Horizontal => "split",
        };

        let script = format!("{}\n", cmd);
        self.execute_vimscript(&script)?;
        Ok("vim:pane:1".to_string())
    }

    fn run_command(&self, _pane: &PaneId, cmd: &str, _opts: &CommandOptions) -> Result<(), MuxError> {
        let script = format!("terminal {}\n", cmd);
        self.execute_vimscript(&script)
    }

    fn send_text(&self, _pane: &PaneId, text: &str) -> Result<(), MuxError> {
        // For Vim/Neovim, we can use chansend to send text to terminal buffers
        // This is a simplified version - in practice, we'd need to track terminal buffer IDs
        if self.use_neovim {
            let script = format!("call chansend(b:terminal_job_id, \"{}\")\n", text.escape_default());
            self.execute_vimscript(&script)?;
            Ok(())
        } else {
            // Vim has term_sendkeys but it's more complex
            Err(MuxError::NotAvailable("Vim text sending requires buffer-specific implementation"))
        }
    }

    fn focus_window(&self, _window: &WindowId) -> Result<(), MuxError> {
        // Window focus in Vim/Neovim context means switching tabs
        // This would require more complex tab management
        Err(MuxError::NotAvailable("Vim tab focusing requires session-specific implementation"))
    }

    fn focus_pane(&self, _pane: &PaneId) -> Result<(), MuxError> {
        // Vim pane focusing requires knowing which window to focus
        Err(MuxError::NotAvailable("Vim pane focusing requires window-specific implementation"))
    }

    fn list_windows(&self, _title_substr: Option<&str>) -> Result<Vec<WindowId>, MuxError> {
        // Listing windows would require parsing Vim's internal state
        // This is complex and not implemented in the basic version
        Err(MuxError::NotAvailable("Vim window listing requires advanced RPC integration"))
    }

    fn list_panes(&self, _window: &WindowId) -> Result<Vec<PaneId>, MuxError> {
        // Similar to windows, pane listing requires parsing Vim's window state
        Err(MuxError::NotAvailable("Vim pane listing requires advanced RPC integration"))
    }
}
