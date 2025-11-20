// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Zellij multiplexer implementation
//!
//! Zellij is a terminal workspace that uses CLI commands for session
//! management and can optionally consume KDL layout files for defining
//! complex pane arrangements.

use std::process::Command;

use ah_mux_core::*;
use tracing::{debug, error, info, instrument, warn};

use crate::MuxError;

/// Helper to derive a Zellij session name from an opaque pane identifier.
///
/// Agent Harbor encodes pane identifiers as:
/// - `<session>.0`, `<session>.1`, ... for role-based panes, or
/// - `zellij-pane-<session>` as a generic placeholder.
///
/// This function normalizes those forms back down to the underlying
/// Zellij session name for CLI targeting.
fn pane_to_session_name(pane: &PaneId) -> &str {
    if let Some(dot) = pane.find('.') {
        &pane[..dot]
    } else if let Some(stripped) = pane.strip_prefix("zellij-pane-") {
        stripped
    } else {
        pane.as_str()
    }
}

/// Zellij multiplexer implementation
#[derive(Debug)]
pub struct ZellijMultiplexer;

impl ZellijMultiplexer {
    /// Create a new Zellij multiplexer instance by verifying that the `zellij`
    /// binary is available and responsive.
    #[instrument(fields(component = "ah-mux", operation = "zellij_new"))]
    pub fn new() -> Result<Self, MuxError> {
        debug!("Initializing Zellij multiplexer");

        let output = Command::new("zellij").arg("--version").output().map_err(|e| {
            error!(
                component = "ah-mux",
                operation = "zellij_new",
                error = %e,
                "Failed to run zellij --version"
            );
            MuxError::Other(format!("Failed to run zellij --version: {}", e))
        })?;

        if !output.status.success() {
            warn!("Zellij is not available");
            return Err(MuxError::NotAvailable("zellij"));
        }

        info!("Zellij multiplexer initialized successfully");
        Ok(Self)
    }
}

impl Multiplexer for ZellijMultiplexer {
    fn id(&self) -> &'static str {
        "zellij"
    }

    #[instrument(
        skip(self),
        fields(component = "ah-mux", operation = "zellij_is_available")
    )]
    fn is_available(&self) -> bool {
        debug!("Checking if Zellij is available");

        let available = Command::new("zellij")
            .arg("--version")
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false);

        debug!(available = %available, "Zellij availability check completed");
        available
    }

    #[instrument(
        skip(self, opts),
        fields(
            component = "ah-mux",
            operation = "zellij_open_window",
            title = ?opts.title,
            cwd = ?opts.cwd,
            focus = %opts.focus
        )
    )]
    fn open_window(&self, opts: &WindowOptions) -> Result<WindowId, MuxError> {
        // Determine target session name:
        // 1. Use ZELLIJ_SESSION_NAME if set (we are inside Zellij).
        // 2. Use provided title or default "ah-session".
        let env_session = std::env::var("ZELLIJ_SESSION_NAME").ok();
        let session_name = if let Some(ref env) = env_session {
            env.as_str()
        } else {
            opts.title.unwrap_or("ah-session")
        };

        debug!(
            component = "ah-mux",
            operation = "zellij_open_window",
            session_name,
            in_zellij = env_session.is_some(),
            "Opening window (tab) in Zellij session"
        );

        // Check if the session exists (if we're not already in it)
        let session_exists = if env_session.is_some() {
            true
        } else {
            self.list_windows(Some(session_name))
                .map(|windows| windows.contains(&session_name.to_string()))
                .unwrap_or(false)
        };

        if session_exists {
            // Session exists. Create a new tab in it.
            let mut cmd = Command::new("zellij");

            // If running from outside, target the session explicitly.
            if env_session.is_none() {
                cmd.arg("--session").arg(session_name);
            }

            cmd.arg("action").arg("new-tab");

            if let Some(cwd) = opts.cwd {
                cmd.arg("--cwd").arg(cwd);
            }

            if let Some(title) = opts.title {
                cmd.arg("--name").arg(title);
            }

            let output = cmd.output().map_err(|e| {
                error!(
                    component = "ah-mux",
                    operation = "zellij_open_window",
                    error = %e,
                    "Failed to run zellij action new-tab"
                );
                MuxError::Other(format!("Failed to run zellij action new-tab: {}", e))
            })?;

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                error!(
                    component = "ah-mux",
                    operation = "zellij_open_window",
                    %stderr,
                    "zellij action new-tab failed"
                );
                return Err(MuxError::CommandFailed(format!(
                    "zellij action new-tab failed: {}",
                    stderr
                )));
            }

            debug!(
                component = "ah-mux",
                operation = "zellij_open_window",
                session_name,
                "Created new tab in existing Zellij session"
            );

            return Ok(session_name.to_string());
        }

        // Fallback: not currently inside a Zellij session and session doesn't exist.
        // Create a new session with a minimal layout.
        let mut cmd = Command::new("zellij");
        cmd.arg("--session").arg(session_name);

        if let Some(cwd) = opts.cwd {
            debug!("Setting working directory to: {}", cwd.display());
            cmd.arg("--cwd").arg(cwd);
        }

        let output = cmd.output().map_err(|e| {
            error!(
                component = "ah-mux",
                operation = "zellij_open_window",
                error = %e,
                "Failed to start zellij session"
            );
            MuxError::Other(format!("Failed to start zellij session: {}", e))
        })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(MuxError::CommandFailed(format!(
                "zellij session creation failed: {}",
                stderr
            )));
        }

        Ok(session_name.to_string())
    }

    #[instrument(
        skip(self, opts, initial_cmd),
        fields(
            component = "ah-mux",
            operation = "zellij_split_pane",
            window = ?window,
            direction = ?dir,
            percent = ?_percent,
            has_initial_cmd = %initial_cmd.is_some(),
            has_env = %opts.env.is_some(),
            has_cwd = %opts.cwd.is_some()
        )
    )]
    fn split_pane(
        &self,
        window: Option<&WindowId>,
        _target: Option<&PaneId>,
        dir: SplitDirection,
        _percent: Option<u8>,
        opts: &CommandOptions,
        initial_cmd: Option<&str>,
    ) -> Result<PaneId, MuxError> {
        // Use `zellij run` to create a new pane within the appropriate session.
        let session_name_env = std::env::var("ZELLIJ_SESSION_NAME").ok();
        let mut cmd = Command::new("zellij");
        if let Some(session) = window {
            cmd.arg("--session").arg(session);
        } else if let Some(name) = session_name_env.as_deref() {
            cmd.arg("--session").arg(name);
        }
        cmd.arg("run");

        // Honor the requested split direction using Zellij's `--direction`
        // flag. We map our abstract directions to concrete Zellij directions
        // that match the desired layout semantics (side-by-side vs
        // top/bottom).
        let direction = match dir {
            SplitDirection::Horizontal => "down",
            SplitDirection::Vertical => "right",
        };
        cmd.arg("--direction").arg(direction);

        if let Some(env) = opts.env {
            for (key, value) in env {
                cmd.env(key, value);
            }
        }

        if let Some(cwd) = opts.cwd {
            cmd.arg("--cwd").arg(cwd);
        }

        // Zellij `run` expects a command and args separately. Since
        // `initial_cmd` is a full shell command line from agent-harbor, we
        // execute it via `sh -lc` so pipelines and complex commands work.
        if let Some(cmd_str) = initial_cmd {
            cmd.arg("--").arg("sh").arg("-lc").arg(cmd_str);
        } else {
            cmd.arg("--").arg("sh");
        }

        let output = cmd.output().map_err(|e| {
            error!(
                component = "ah-mux",
                operation = "zellij_split_pane",
                error = %e,
                "Failed to run zellij command"
            );
            MuxError::Other(format!("Failed to run zellij command: {}", e))
        })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(MuxError::CommandFailed(format!(
                "zellij run failed: {}",
                stderr
            )));
        }

        // Zellij doesn't currently expose stable pane IDs via the CLI. Return
        // a placeholder that is meaningful only for logging/debugging.
        let window_str = window.map(|w| w.as_str()).unwrap_or("default");
        Ok(format!("zellij-pane-{}", window_str))
    }

    #[instrument(
        skip(self, opts, cmd),
        fields(
            component = "ah-mux",
            operation = "zellij_run_command",
            pane = %pane,
            has_env = %opts.env.is_some(),
            has_cwd = %opts.cwd.is_some()
        )
    )]
    fn run_command(&self, pane: &PaneId, cmd: &str, opts: &CommandOptions) -> Result<(), MuxError> {
        // Launch a new pane in the appropriate Zellij session:
        // - inside Zellij: use `ZELLIJ_SESSION_NAME`
        // - outside Zellij: derive the session name from the pane identifier.
        let session_name_env = std::env::var("ZELLIJ_SESSION_NAME").ok();
        let session_name =
            session_name_env.as_deref().unwrap_or_else(|| pane_to_session_name(pane));

        let mut zellij_cmd = Command::new("zellij");
        zellij_cmd.arg("--session").arg(session_name).arg("run");

        if let Some(env) = opts.env {
            for (key, value) in env {
                zellij_cmd.env(key, value);
            }
        }

        if let Some(cwd) = opts.cwd {
            zellij_cmd.arg("--cwd").arg(cwd);
        }

        // Treat `cmd` as a full shell command line for parity with other
        // multiplexers. Invoke it via `sh -lc` so complex commands work.
        zellij_cmd.arg("--").arg("sh").arg("-lc").arg(cmd);

        let output = zellij_cmd.output().map_err(|e| {
            error!(
                component = "ah-mux",
                operation = "zellij_run_command",
                error = %e,
                "Failed to execute zellij run command"
            );
            if e.kind() == std::io::ErrorKind::NotFound {
                MuxError::NotAvailable("zellij")
            } else {
                MuxError::Io(e)
            }
        })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(MuxError::CommandFailed(format!(
                "zellij run failed: {}",
                stderr
            )));
        }

        Ok(())
    }

    #[instrument(
        skip(self, text),
        fields(component = "ah-mux", operation = "zellij_send_text", pane = %pane, text_len = %text.len())
    )]
    fn send_text(&self, pane: &PaneId, text: &str) -> Result<(), MuxError> {
        // Send literal text to the focused pane via `zellij action write-chars`.
        // The pane identifier is used only to derive the target session.
        let session_name_env = std::env::var("ZELLIJ_SESSION_NAME").ok();
        let session_name =
            session_name_env.as_deref().unwrap_or_else(|| pane_to_session_name(pane));

        let output = Command::new("zellij")
            .arg("--session")
            .arg(session_name)
            .arg("action")
            .arg("write-chars")
            .arg(text)
            .output()
            .map_err(|e| {
                error!(
                    component = "ah-mux",
                    operation = "zellij_send_text",
                    error = %e,
                    "Failed to send text to zellij"
                );
                if e.kind() == std::io::ErrorKind::NotFound {
                    MuxError::NotAvailable("zellij")
                } else {
                    MuxError::Io(e)
                }
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(MuxError::CommandFailed(format!(
                "zellij action write-chars failed: {}",
                stderr
            )));
        }

        Ok(())
    }

    #[instrument(
        skip(self),
        fields(component = "ah-mux", operation = "zellij_focus_window", window = %window)
    )]
    fn focus_window(&self, window: &WindowId) -> Result<(), MuxError> {
        // Attach to the given session and bring it to the foreground.
        let output = Command::new("zellij").arg("attach").arg(window).output().map_err(|e| {
            error!(
                component = "ah-mux",
                operation = "zellij_focus_window",
                error = %e,
                "Failed to run zellij attach"
            );
            if e.kind() == std::io::ErrorKind::NotFound {
                MuxError::NotAvailable("zellij")
            } else {
                MuxError::Io(e)
            }
        })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(MuxError::CommandFailed(format!(
                "zellij attach failed: {}",
                stderr
            )));
        }

        Ok(())
    }

    #[instrument(
        skip(self),
        fields(component = "ah-mux", operation = "zellij_focus_pane", pane = %_pane)
    )]
    fn focus_pane(&self, _pane: &PaneId) -> Result<(), MuxError> {
        // Zellij doesn't expose direct pane focusing via CLI; rely on user or
        // layout-driven focus instead.
        Err(MuxError::NotAvailable("zellij"))
    }

    #[instrument(
        skip(self),
        fields(component = "ah-mux", operation = "zellij_list_windows", title_substr = ?title_substr)
    )]
    fn list_windows(&self, title_substr: Option<&str>) -> Result<Vec<WindowId>, MuxError> {
        let output = Command::new("zellij").arg("list-sessions").output().map_err(|e| {
            error!(
                component = "ah-mux",
                operation = "zellij_list_windows",
                error = %e,
                "Failed to run zellij list-sessions"
            );
            if e.kind() == std::io::ErrorKind::NotFound {
                MuxError::NotAvailable("zellij")
            } else {
                MuxError::Io(e)
            }
        })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(MuxError::CommandFailed(format!(
                "zellij list-sessions failed: {}",
                stderr
            )));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut sessions = Vec::new();

        for line in stdout.lines() {
            // Output lines typically look like: "session_name EXITED|ATTACHED|DETACHED".
            let parts: Vec<&str> = line.split_whitespace().collect();
            if let Some(session_name) = parts.first() {
                if let Some(substr) = title_substr {
                    if session_name.contains(substr) {
                        sessions.push(session_name.to_string());
                    }
                } else {
                    sessions.push(session_name.to_string());
                }
            }
        }

        Ok(sessions)
    }

    #[instrument(
        skip(self),
        fields(component = "ah-mux", operation = "zellij_list_panes", window = %window)
    )]
    fn list_panes(&self, window: &WindowId) -> Result<Vec<PaneId>, MuxError> {
        // Zellij does not currently expose a stable pane-listing interface in
        // its CLI suitable for programmatic use. AH integrations are expected
        // to rely on known layouts instead of dynamically discovering panes.
        Err(MuxError::NotAvailable("zellij"))
    }

    #[instrument(
        skip(self),
        fields(component = "ah-mux", operation = "zellij_current_pane")
    )]
    fn current_pane(&self) -> Result<Option<PaneId>, MuxError> {
        // When running inside Zellij, each process receives `ZELLIJ_PANE_ID`
        // identifying the pane it was launched from. Outside Zellij, this
        // environment variable is absent and we report `None`.
        Ok(std::env::var("ZELLIJ_PANE_ID").ok())
    }

    #[instrument(
        skip(self),
        fields(component = "ah-mux", operation = "zellij_current_window")
    )]
    fn current_window(&self) -> Result<Option<WindowId>, MuxError> {
        // We treat the Zellij session name as our WindowId. When running
        // inside Zellij, `ZELLIJ_SESSION_NAME` is set; otherwise, we report
        // `None` and higher-level code should avoid split-mode layouts.
        Ok(std::env::var("ZELLIJ_SESSION_NAME").ok())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pane_to_session_name_with_role_suffix() {
        assert_eq!(
            pane_to_session_name(&"ah-session.0".to_string()),
            "ah-session"
        );
        assert_eq!(pane_to_session_name(&"proj.42".to_string()), "proj");
    }

    #[test]
    fn test_pane_to_session_name_with_zellij_prefix() {
        assert_eq!(
            pane_to_session_name(&"zellij-pane-ah-session".to_string()),
            "ah-session"
        );
    }

    #[test]
    fn test_pane_to_session_name_passthrough() {
        assert_eq!(
            pane_to_session_name(&"plain-session".to_string()),
            "plain-session"
        );
    }

    #[test]
    fn test_zellij_id() {
        let mux = ZellijMultiplexer::new().unwrap();
        assert_eq!(mux.id(), "zellij");
    }

    #[test]
    fn test_zellij_focus_pane_not_available() {
        let mux = ZellijMultiplexer::new().unwrap();
        let result = mux.focus_pane(&"dummy".to_string());
        assert!(matches!(result, Err(MuxError::NotAvailable("zellij"))));
    }
}
