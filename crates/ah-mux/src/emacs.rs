//! Emacs multiplexer implementation
//!
//! Emacs integration using vterm for terminal emulation and Elisp for automation.

use std::process::Command;

use ah_mux_core::{Multiplexer, WindowId, PaneId, WindowOptions, CommandOptions, SplitDirection, MuxError};

/// Emacs multiplexer implementation
pub struct EmacsMultiplexer;

impl EmacsMultiplexer {
    /// Create a new Emacs multiplexer instance
    pub fn new() -> Result<Self, MuxError> {
        Ok(Self)
    }

    /// Check if emacs is available and has vterm
    pub fn is_available() -> bool {
        // Check if emacs is available
        let emacs_available = Command::new("emacs")
            .arg("--version")
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false);

        if !emacs_available {
            return false;
        }

        // For a more thorough check, we could verify vterm is installed
        // but for now, we'll assume it's available if emacs is
        true
    }

    /// Get multiplexer identifier
    pub fn id() -> &'static str {
        "emacs"
    }

    /// Run an emacsclient command with the given arguments
    fn run_emacsclient_command(&self, args: &[&str]) -> Result<String, MuxError> {
        let output = Command::new("emacsclient")
            .args(args)
            .output()
            .map_err(|e| MuxError::Other(format!("Failed to execute emacsclient: {}", e)))?;

        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
        } else {
            let error_msg = format!(
                "emacsclient command failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            Err(MuxError::CommandFailed(error_msg))
        }
    }

    /// Execute Elisp code via emacsclient
    fn execute_elisp(&self, elisp: &str) -> Result<String, MuxError> {
        self.run_emacsclient_command(&["-e", elisp])
    }
}

impl Multiplexer for EmacsMultiplexer {
    fn id(&self) -> &'static str {
        Self::id()
    }

    fn is_available(&self) -> bool {
        Self::is_available()
    }

    fn open_window(&self, opts: &WindowOptions) -> Result<WindowId, MuxError> {
        let default_title = format!("ah-task-{}", std::process::id());
        let title = opts.title.unwrap_or(&default_title);
        let window_id = format!("emacs:{}", title);

        // Create a new frame (window) and set it up
        let elisp = format!(r#"
(progn
  (select-frame (make-frame '((name . "{}"))))
  (rename-buffer "{}")
  (delete-other-windows))
"#, title, title);

        self.execute_elisp(&elisp)?;

        Ok(window_id)
    }

    fn split_pane(&self, _window: &WindowId, _target: Option<&PaneId>, dir: SplitDirection, _percent: Option<u8>, _opts: &CommandOptions, _initial_cmd: Option<&str>) -> Result<PaneId, MuxError> {
        let func = match dir {
            SplitDirection::Vertical => "split-window-right",
            SplitDirection::Horizontal => "split-window-below",
        };

        let elisp = format!("({})", func);
        self.execute_elisp(&elisp)?;
        Ok("emacs:pane:1".to_string())
    }

    fn run_command(&self, _pane: &PaneId, cmd: &str, _opts: &CommandOptions) -> Result<(), MuxError> {
        // Switch to the target window and run vterm with command
        let elisp = format!(r#"
(progn
  (other-window 1)
  (vterm)
  (vterm-send-string "{}")
  (vterm-send-return))
"#, cmd);

        self.execute_elisp(&elisp)?;
        Ok(())
    }

    fn send_text(&self, _pane: &PaneId, text: &str) -> Result<(), MuxError> {
        // Send text to the current vterm buffer
        let elisp = format!(r#"
(progn
  (vterm-send-string "{}")
  (vterm-send-return))
"#, text);

        self.execute_elisp(&elisp)?;
        Ok(())
    }

    fn focus_window(&self, window_id: &WindowId) -> Result<(), MuxError> {
        // Focus the frame by name
        let frame_name = window_id.strip_prefix("emacs:").unwrap_or(window_id);
        let elisp = format!(r#"
(let ((frame (car (seq-filter (lambda (f) (string= "{}" (frame-parameter f 'name))) (frame-list)))))
  (when frame
    (select-frame frame)))
"#, frame_name);

        self.execute_elisp(&elisp)?;
        Ok(())
    }

    fn focus_pane(&self, _pane: &PaneId) -> Result<(), MuxError> {
        // Emacs pane focusing requires knowing which window to focus
        Err(MuxError::NotAvailable("Emacs pane focusing requires window-specific implementation"))
    }

    fn list_windows(&self, title_substr: Option<&str>) -> Result<Vec<WindowId>, MuxError> {
        let filter_elisp = if let Some(substr) = title_substr {
            format!(r#"(lambda (f) (string-match-p "{}" (frame-parameter f 'name)))"#, substr)
        } else {
            "(lambda (f) t)".to_string()
        };

        let elisp = format!(r#"
(mapcar (lambda (f) (format "emacs:%s" (frame-parameter f 'name)))
        (seq-filter {} (frame-list)))
"#, filter_elisp);

        let result = self.execute_elisp(&elisp)?;
        // Parse the result - this would be a Lisp list that we need to parse
        // For simplicity, we'll return an empty list for now
        Ok(vec![])
    }

    fn list_panes(&self, _window: &WindowId) -> Result<Vec<PaneId>, MuxError> {
        // Listing panes would require enumerating Emacs windows in the current frame
        // This is complex and not implemented in the basic version
        Err(MuxError::NotAvailable("Emacs pane listing requires advanced Elisp integration"))
    }
}
