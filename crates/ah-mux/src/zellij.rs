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
#[allow(clippy::disallowed_methods)]
mod tests {
    use std::{env, thread, time::Duration};

    use super::*;

    /// Initialize tracing for tests if not already initialized
    fn init_tracing() {
        use std::sync::Once;
        static INIT: Once = Once::new();

        INIT.call_once(|| {
            // Only initialize if RUST_LOG is set (user wants logging)
            if std::env::var("RUST_LOG").is_ok() {
                tracing_subscriber::fmt()
                    .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
                    .with_test_writer()
                    .try_init()
                    .ok(); // Ignore errors if already initialized
            }
        });
    }

    /// Helper to start a detached Zellij session and return its session name
    /// Returns None if Zellij is not available or fails to start
    fn start_zellij_session(name: &str) -> Option<String> {
        use std::process::{Command, Stdio};

        // 1) Check if zellij is runnable at all
        let version_output = Command::new("zellij").arg("--version").output().ok()?;
        debug!(
            "zellij --version: status={:?}, stdout={}, stderr={}",
            version_output.status,
            String::from_utf8_lossy(&version_output.stdout),
            String::from_utf8_lossy(&version_output.stderr),
        );
        if !version_output.status.success() {
            error!("zellij --version failed, treating zellij as unavailable");
            return None;
        }

        // 2) Start a detached Zellij session by redirecting stdin/stdout/stderr
        // This prevents Zellij from trying to interact with a terminal
        let start_output = Command::new("zellij")
            .args(["--session", name])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();

        match start_output {
            Ok(mut child) => {
                // Give the session a moment to initialize
                thread::sleep(Duration::from_millis(1000));

                // Check if the process is still running (it should be detached)
                match child.try_wait() {
                    Ok(Some(status)) => {
                        debug!("zellij process exited with status: {:?}", status);
                        // Process exited, but session might still exist
                    }
                    Ok(None) => {
                        debug!("zellij process is still running");
                        // Process is still running, good
                    }
                    Err(e) => {
                        error!("Error checking zellij process: {}", e);
                    }
                }
            }
            Err(e) => {
                error!("Failed to start zellij session '{}': {}", name, e);
                return None;
            }
        }

        // 3) Verify the session exists by listing sessions
        // Try multiple times with delays as session creation might take time
        for attempt in 1..=5 {
            thread::sleep(Duration::from_millis(300));

            let ls_output = Command::new("zellij").arg("list-sessions").output().ok()?;
            debug!(
                "zellij list-sessions (attempt {}): status={:?}, stdout={}, stderr={}",
                attempt,
                ls_output.status,
                String::from_utf8_lossy(&ls_output.stdout),
                String::from_utf8_lossy(&ls_output.stderr),
            );

            if !ls_output.status.success() {
                error!("`zellij list-sessions` failed, treating zellij as unavailable");
                return None;
            }

            let stdout = String::from_utf8_lossy(&ls_output.stdout);
            for line in stdout.lines() {
                if line.contains(name) {
                    debug!("Found session '{}' in list-sessions output", name);
                    return Some(name.to_string());
                }
            }

            if attempt < 5 {
                debug!(
                    "Session '{}' not found yet (attempt {}), retrying...",
                    name, attempt
                );
            }
        }

        error!(
            "zellij session '{}' not found in `zellij list-sessions` output after 5 attempts, treating as unavailable",
            name
        );
        None
    }

    /// Helper to kill the Zellij session
    fn kill_zellij_session(name: &str) {
        let _ = Command::new("zellij").args(["delete-session", name, "--force"]).output();
    }

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
        if let Ok(mux) = ZellijMultiplexer::new() {
            assert_eq!(mux.id(), "zellij");
        }
    }

    #[test]
    fn test_zellij_focus_pane_not_available() {
        if let Ok(mux) = ZellijMultiplexer::new() {
            let result = mux.focus_pane(&"dummy".to_string());
            assert!(matches!(result, Err(MuxError::NotAvailable("zellij"))));
        }
    }

    #[test]
    fn test_zellij_list_panes_not_available() {
        if let Ok(mux) = ZellijMultiplexer::new() {
            let result = mux.list_panes(&"dummy-window".to_string());
            assert!(matches!(result, Err(MuxError::NotAvailable("zellij"))));
        }
    }

    #[test]
    #[serial_test::file_serial]
    fn test_current_window_with_env() {
        let session_name = "test-session";
        env::set_var("ZELLIJ_SESSION_NAME", session_name);
        if let Ok(mux) = ZellijMultiplexer::new() {
            let result = mux.current_window().unwrap();
            assert_eq!(result, Some(session_name.to_string()));
        }
        env::remove_var("ZELLIJ_SESSION_NAME");
    }

    #[test]
    #[serial_test::file_serial]
    fn test_current_window_without_env() {
        env::remove_var("ZELLIJ_SESSION_NAME");
        if let Ok(mux) = ZellijMultiplexer::new() {
            let result = mux.current_window().unwrap();
            assert_eq!(result, None);
        }
    }

    #[test]
    #[serial_test::file_serial]
    fn test_current_window_empty_env() {
        env::set_var("ZELLIJ_SESSION_NAME", "");
        if let Ok(mux) = ZellijMultiplexer::new() {
            let result = mux.current_window().unwrap();
            assert_eq!(result, Some("".to_string()));
        }
        env::remove_var("ZELLIJ_SESSION_NAME");
    }

    #[test]
    #[serial_test::file_serial]
    fn test_current_pane_with_env() {
        env::set_var("ZELLIJ_PANE_ID", "5");
        if let Ok(mux) = ZellijMultiplexer::new() {
            let result = mux.current_pane().unwrap();
            assert_eq!(result, Some("5".to_string()));
        }
        env::remove_var("ZELLIJ_PANE_ID");
    }

    #[test]
    #[serial_test::file_serial]
    fn test_current_pane_without_env() {
        env::remove_var("ZELLIJ_PANE_ID");
        if let Ok(mux) = ZellijMultiplexer::new() {
            let result = mux.current_pane().unwrap();
            assert_eq!(result, None);
        }
    }

    #[test]
    #[serial_test::file_serial]
    fn test_current_pane_empty_env() {
        env::set_var("ZELLIJ_PANE_ID", "");
        if let Ok(mux) = ZellijMultiplexer::new() {
            let result = mux.current_pane().unwrap();
            assert_eq!(result, Some("".to_string()));
        }
        env::remove_var("ZELLIJ_PANE_ID");
    }

    #[test]
    #[serial_test::file_serial]
    fn test_open_window_in_zellij() {
        init_tracing();

        // Skip this test in CI: zellij behaviour have limitations in our CI
        if std::env::var("CI").is_ok() {
            eprintln!("Skipping test_open_window_in_zellij in CI");
            return;
        }

        let session_name = format!("test-open-in-{}", std::process::id());

        let session = start_zellij_session(&session_name).expect(
            "Zellij is not available or failed to start. Please install Zellij to run this test.",
        );

        env::set_var("ZELLIJ_SESSION_NAME", &session);

        let mux = ZellijMultiplexer::new().expect("Failed to create ZellijMultiplexer");

        let window_opts = WindowOptions {
            title: Some("test-window"),
            cwd: None,
            focus: false,
            profile: None,
            init_command: None,
        };

        let result = mux.open_window(&window_opts);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), session);

        // Verify the session is in the list
        let sessions = mux.list_windows(None).expect("Failed to list sessions");
        assert!(
            sessions.iter().any(|s| s.contains(&session_name)),
            "Session '{}' not found in list: {:?}",
            session_name,
            sessions
        );

        env::remove_var("ZELLIJ_SESSION_NAME");
        kill_zellij_session(&session);
    }

    #[test]
    #[serial_test::file_serial]
    fn test_open_window_with_cwd() {
        init_tracing();

        // Skip this test in CI: zellij behaviour have limitations in our CI
        if std::env::var("CI").is_ok() {
            eprintln!("Skipping test_open_window_with_cwd in CI");
            return;
        }

        let session_name = format!("test-open-cwd-{}", std::process::id());

        let session = start_zellij_session(&session_name).expect(
            "Zellij is not available or failed to start. Please install Zellij to run this test.",
        );

        env::set_var("ZELLIJ_SESSION_NAME", &session);

        let mux = ZellijMultiplexer::new().expect("Failed to create ZellijMultiplexer");

        let tmp_dir = std::path::Path::new("/tmp");
        let window_opts = WindowOptions {
            title: Some("cwd-test"),
            cwd: Some(tmp_dir),
            focus: false,
            profile: None,
            init_command: None,
        };

        let result = mux.open_window(&window_opts);
        assert!(result.is_ok());

        env::remove_var("ZELLIJ_SESSION_NAME");
        kill_zellij_session(&session);
    }

    #[test]
    #[serial_test::file_serial]
    fn test_open_window_existing_session() {
        init_tracing();

        // Skip this test in CI: zellij behaviour have limitations in our CI
        if std::env::var("CI").is_ok() {
            eprintln!("Skipping test_open_window_existing_session in CI");
            return;
        }

        let session_name = format!("test-open-existing-{}", std::process::id());

        let session = start_zellij_session(&session_name).expect(
            "Zellij is not available or failed to start. Please install Zellij to run this test.",
        );

        env::set_var("ZELLIJ_SESSION_NAME", &session);

        let mux = ZellijMultiplexer::new().expect("Failed to create ZellijMultiplexer");

        let window_opts = WindowOptions {
            title: Some("existing-test"),
            cwd: None,
            focus: false,
            profile: None,
            init_command: None,
        };

        // Open first time
        let result1 = mux.open_window(&window_opts);
        assert!(result1.is_ok());

        // Open second time (should reuse session)
        let result2 = mux.open_window(&window_opts);
        assert!(result2.is_ok());

        env::remove_var("ZELLIJ_SESSION_NAME");
        kill_zellij_session(&session);
    }

    #[test]
    #[serial_test::file_serial]
    fn test_split_pane_vertical() {
        init_tracing();

        // Skip this test in CI: zellij behaviour have limitations in our CI
        if std::env::var("CI").is_ok() {
            eprintln!("Skipping test_split_pane_vertical in CI");
            return;
        }

        let session_name = format!("test-split-v-{}", std::process::id());

        let session = match start_zellij_session(&session_name) {
            Some(s) => s,
            None => panic!(
                "Zellij is not available or failed to start. Please install Zellij to run this test."
            ),
        };

        env::set_var("ZELLIJ_SESSION_NAME", &session);

        let mux = ZellijMultiplexer::new().expect("Failed to create ZellijMultiplexer");

        let cmd_opts = CommandOptions {
            cwd: None,
            env: None,
        };

        let result = mux.split_pane(
            Some(&session),
            None,
            SplitDirection::Vertical,
            None,
            &cmd_opts,
            None,
        );
        assert!(result.is_ok());

        env::remove_var("ZELLIJ_SESSION_NAME");
        kill_zellij_session(&session);
    }

    #[test]
    #[serial_test::file_serial]
    fn test_split_pane_horizontal() {
        init_tracing();

        // Skip this test in CI: zellij behaviour have limitations in our CI
        if std::env::var("CI").is_ok() {
            eprintln!("Skipping test_split_pane_horizontal in CI");
            return;
        }

        let session_name = format!("test-split-h-{}", std::process::id());

        let session = match start_zellij_session(&session_name) {
            Some(s) => s,
            None => panic!(
                "Zellij is not available or failed to start. Please install Zellij to run this test."
            ),
        };

        env::set_var("ZELLIJ_SESSION_NAME", &session);

        let mux = ZellijMultiplexer::new().expect("Failed to create ZellijMultiplexer");

        let cmd_opts = CommandOptions {
            cwd: None,
            env: None,
        };

        let result = mux.split_pane(
            Some(&session),
            None,
            SplitDirection::Horizontal,
            None,
            &cmd_opts,
            None,
        );
        assert!(result.is_ok());

        env::remove_var("ZELLIJ_SESSION_NAME");
        kill_zellij_session(&session);
    }

    #[test]
    #[serial_test::file_serial]
    fn test_split_pane_with_cwd() {
        init_tracing();

        // Skip this test in CI: zellij behaviour have limitations in our CI
        if std::env::var("CI").is_ok() {
            eprintln!("Skipping test_split_pane_with_cwd in CI");
            return;
        }

        let session_name = format!("test-split-cwd-{}", std::process::id());

        let session = match start_zellij_session(&session_name) {
            Some(s) => s,
            None => panic!(
                "Zellij is not available or failed to start. Please install Zellij to run this test."
            ),
        };

        env::set_var("ZELLIJ_SESSION_NAME", &session);

        let mux = ZellijMultiplexer::new().expect("Failed to create ZellijMultiplexer");

        let tmp_dir = std::path::Path::new("/tmp");
        let cmd_opts = CommandOptions {
            cwd: Some(tmp_dir),
            env: None,
        };

        let result = mux.split_pane(
            Some(&session),
            None,
            SplitDirection::Vertical,
            None,
            &cmd_opts,
            None,
        );
        assert!(result.is_ok());

        env::remove_var("ZELLIJ_SESSION_NAME");
        kill_zellij_session(&session);
    }

    #[test]
    #[serial_test::file_serial]
    fn test_split_pane_with_initial_cmd() {
        init_tracing();

        // Skip this test in CI: zellij behaviour have limitations in our CI
        if std::env::var("CI").is_ok() {
            eprintln!("Skipping test_split_pane_with_initial_cmd in CI");
            return;
        }

        let session_name = format!("test-split-cmd-{}", std::process::id());

        let session = match start_zellij_session(&session_name) {
            Some(s) => s,
            None => {
                panic!(
                    "Zellij is not available or failed to start. Please install Zellij to run this test."
                );
            }
        };

        env::set_var("ZELLIJ_SESSION_NAME", &session);

        let mux = ZellijMultiplexer::new().expect("Failed to create ZellijMultiplexer");

        let cmd_opts = CommandOptions {
            cwd: None,
            env: None,
        };

        let result = mux.split_pane(
            Some(&session),
            None,
            SplitDirection::Vertical,
            None,
            &cmd_opts,
            Some("echo 'test command'"),
        );
        assert!(result.is_ok());

        env::remove_var("ZELLIJ_SESSION_NAME");
        kill_zellij_session(&session);
    }

    #[test]
    #[serial_test::file_serial]
    fn test_split_pane_with_env() {
        init_tracing();

        // Skip this test in CI: zellij behaviour have limitations in our CI
        if std::env::var("CI").is_ok() {
            eprintln!("Skipping test_split_pane_with_env in CI");
            return;
        }

        let session_name = format!("test-split-env-{}", std::process::id());

        let session = match start_zellij_session(&session_name) {
            Some(s) => s,
            None => {
                panic!(
                    "Zellij is not available or failed to start. Please install Zellij to run this test."
                );
            }
        };

        env::set_var("ZELLIJ_SESSION_NAME", &session);

        let mux = ZellijMultiplexer::new().expect("Failed to create ZellijMultiplexer");

        let env_vars = [("TEST_VAR", "test_value")];
        let cmd_opts = CommandOptions {
            cwd: None,
            env: Some(&env_vars),
        };

        let result = mux.split_pane(
            Some(&session),
            None,
            SplitDirection::Vertical,
            None,
            &cmd_opts,
            Some("echo $TEST_VAR"),
        );
        assert!(result.is_ok());

        env::remove_var("ZELLIJ_SESSION_NAME");
        kill_zellij_session(&session);
    }

    #[test]
    #[serial_test::file_serial]
    fn test_split_pane_complex_command() {
        init_tracing();

        // Skip this test in CI: zellij behaviour have limitations in our CI
        if std::env::var("CI").is_ok() {
            eprintln!("Skipping test_split_pane_complex_command in CI");
            return;
        }

        let session_name = format!("test-split-complex-{}", std::process::id());

        let session = match start_zellij_session(&session_name) {
            Some(s) => s,
            None => panic!(
                "Zellij is not available or failed to start. Please install Zellij to run this test."
            ),
        };

        env::set_var("ZELLIJ_SESSION_NAME", &session);

        let mux = ZellijMultiplexer::new().expect("Failed to create ZellijMultiplexer");

        let cmd_opts = CommandOptions {
            cwd: None,
            env: None,
        };

        // Test complex command with pipes and redirects
        let result = mux.split_pane(
            Some(&session),
            None,
            SplitDirection::Vertical,
            None,
            &cmd_opts,
            Some("echo 'test' | cat > /dev/null"),
        );
        assert!(result.is_ok());

        env::remove_var("ZELLIJ_SESSION_NAME");
        kill_zellij_session(&session);
    }

    #[test]
    #[serial_test::file_serial]
    fn test_run_command_basic() {
        init_tracing();

        // Skip this test in CI: zellij behaviour have limitations in our CI
        if std::env::var("CI").is_ok() {
            eprintln!("Skipping test_run_command_basic in CI");
            return;
        }

        let session_name = format!("test-run-basic-{}", std::process::id());

        let session = match start_zellij_session(&session_name) {
            Some(s) => s,
            None => panic!(
                "Zellij is not available or failed to start. Please install Zellij to run this test."
            ),
        };

        env::set_var("ZELLIJ_SESSION_NAME", &session);

        let mux = ZellijMultiplexer::new().expect("Failed to create ZellijMultiplexer");

        let cmd_opts = CommandOptions {
            cwd: None,
            env: None,
        };

        let pane_id = format!("zellij-pane-{}", session);
        let result = mux.run_command(&pane_id, "echo 'test'", &cmd_opts);
        assert!(result.is_ok());

        env::remove_var("ZELLIJ_SESSION_NAME");
        kill_zellij_session(&session);
    }

    #[test]
    #[serial_test::file_serial]
    fn test_run_command_with_cwd() {
        init_tracing();

        // Skip this test in CI: zellij behaviour have limitations in our CI
        if std::env::var("CI").is_ok() {
            eprintln!("Skipping test_run_command_with_cwd in CI");
            return;
        }

        let session_name = format!("test-run-cwd-{}", std::process::id());

        let session = match start_zellij_session(&session_name) {
            Some(s) => s,
            None => panic!(
                "Zellij is not available or failed to start. Please install Zellij to run this test."
            ),
        };

        env::set_var("ZELLIJ_SESSION_NAME", &session);

        let mux = ZellijMultiplexer::new().expect("Failed to create ZellijMultiplexer");

        let tmp_dir = std::path::Path::new("/tmp");
        let cmd_opts = CommandOptions {
            cwd: Some(tmp_dir),
            env: None,
        };

        let pane_id = format!("zellij-pane-{}", session);
        let result = mux.run_command(&pane_id, "pwd", &cmd_opts);
        assert!(result.is_ok());

        env::remove_var("ZELLIJ_SESSION_NAME");
        kill_zellij_session(&session);
    }

    #[test]
    #[serial_test::file_serial]
    fn test_run_command_with_env() {
        init_tracing();

        // Skip this test in CI: zellij behaviour have limitations in our CI
        if std::env::var("CI").is_ok() {
            eprintln!("Skipping test_run_command_with_env in CI");
            return;
        }

        let session_name = format!("test-run-env-{}", std::process::id());

        let session = match start_zellij_session(&session_name) {
            Some(s) => s,
            None => panic!(
                "Zellij is not available or failed to start. Please install Zellij to run this test."
            ),
        };

        env::set_var("ZELLIJ_SESSION_NAME", &session);

        let mux = ZellijMultiplexer::new().expect("Failed to create ZellijMultiplexer");

        let env_vars = [("TEST_VAR", "test_value")];
        let cmd_opts = CommandOptions {
            cwd: None,
            env: Some(&env_vars),
        };

        let pane_id = format!("zellij-pane-{}", session);
        let result = mux.run_command(&pane_id, "echo $TEST_VAR", &cmd_opts);
        assert!(result.is_ok());

        env::remove_var("ZELLIJ_SESSION_NAME");
        kill_zellij_session(&session);
    }

    #[test]
    #[serial_test::file_serial]
    fn test_run_command_invalid_session() {
        init_tracing();

        // Skip this test in CI: zellij behaviour have limitations in our CI
        if std::env::var("CI").is_ok() {
            eprintln!("Skipping test_run_command_invalid_session in CI");
            return;
        }

        let session_name = "test-invalid-run-99999";

        env::set_var("ZELLIJ_SESSION_NAME", session_name);

        if let Ok(mux) = ZellijMultiplexer::new() {
            let cmd_opts = CommandOptions {
                cwd: None,
                env: None,
            };

            let result = mux.run_command(&"dummy-pane".to_string(), "echo 'test'", &cmd_opts);
            assert!(result.is_err());

            // Clean up the auto-created session
            kill_zellij_session(session_name);
        }

        env::remove_var("ZELLIJ_SESSION_NAME");
    }

    #[test]
    #[serial_test::file_serial]
    fn test_send_text_basic() {
        init_tracing();

        // Skip this test in CI: zellij behaviour have limitations in our CI
        if std::env::var("CI").is_ok() {
            eprintln!("Skipping test_send_text_basic in CI");
            return;
        }

        let session_name = format!("test-send-basic-{}", std::process::id());

        let session = match start_zellij_session(&session_name) {
            Some(s) => s,
            None => panic!(
                "Zellij is not available or failed to start. Please install Zellij to run this test."
            ),
        };

        env::set_var("ZELLIJ_SESSION_NAME", &session);

        let mux = ZellijMultiplexer::new().expect("Failed to create ZellijMultiplexer");

        let pane_id = format!("zellij-pane-{}", session);
        let result = mux.send_text(&pane_id, "echo 'test'");
        assert!(result.is_ok());

        env::remove_var("ZELLIJ_SESSION_NAME");
        kill_zellij_session(&session);
    }

    #[test]
    #[serial_test::file_serial]
    fn test_send_text_with_newline() {
        init_tracing();

        // Skip this test in CI: zellij behaviour have limitations in our CI
        if std::env::var("CI").is_ok() {
            eprintln!("Skipping test_send_text_with_newline in CI");
            return;
        }

        let session_name = format!("test-send-newline-{}", std::process::id());

        let session = match start_zellij_session(&session_name) {
            Some(s) => s,
            None => panic!(
                "Zellij is not available or failed to start. Please install Zellij to run this test."
            ),
        };

        env::set_var("ZELLIJ_SESSION_NAME", &session);

        let mux = ZellijMultiplexer::new().expect("Failed to create ZellijMultiplexer");

        let pane_id = format!("zellij-pane-{}", session);
        let result = mux.send_text(&pane_id, "ls\n");
        assert!(result.is_ok());

        env::remove_var("ZELLIJ_SESSION_NAME");
        kill_zellij_session(&session);
    }

    #[test]
    #[serial_test::file_serial]
    fn test_send_text_special_chars() {
        init_tracing();

        // Skip this test in CI: zellij behaviour have limitations in our CI
        if std::env::var("CI").is_ok() {
            eprintln!("Skipping test_send_text_special_chars in CI");
            return;
        }

        let session_name = format!("test-send-special-{}", std::process::id());

        let session = match start_zellij_session(&session_name) {
            Some(s) => s,
            None => panic!(
                "Zellij is not available or failed to start. Please install Zellij to run this test."
            ),
        };

        env::set_var("ZELLIJ_SESSION_NAME", &session);

        let mux = ZellijMultiplexer::new().expect("Failed to create ZellijMultiplexer");

        let pane_id = format!("zellij-pane-{}", session);

        // Test with dollar sign
        let result = mux.send_text(&pane_id, "echo '$HOME'");
        assert!(result.is_ok());

        // Test with quotes
        let result = mux.send_text(&pane_id, "echo \"test\"");
        assert!(result.is_ok());

        // Test with backslash
        let result = mux.send_text(&pane_id, "echo \\test");
        assert!(result.is_ok());

        env::remove_var("ZELLIJ_SESSION_NAME");
        kill_zellij_session(&session);
    }

    #[test]
    #[serial_test::file_serial]
    fn test_send_text_invalid_session() {
        init_tracing();

        // Skip this test in CI: zellij behaviour have limitations in our CI
        if std::env::var("CI").is_ok() {
            eprintln!("Skipping test_send_text_invalid_session in CI");
            return;
        }

        let session_name = "test-invalid-send-99999";

        env::set_var("ZELLIJ_SESSION_NAME", session_name);

        if let Ok(mux) = ZellijMultiplexer::new() {
            let result = mux.send_text(&"dummy-pane".to_string(), "echo 'test'");
            assert!(result.is_err());

            // Clean up the auto-created session
            kill_zellij_session(session_name);
        }

        env::remove_var("ZELLIJ_SESSION_NAME");
    }

    #[test]
    #[serial_test::file_serial]
    fn test_list_windows_all() {
        init_tracing();

        // Skip this test in CI: zellij behaviour have limitations in our CI
        if std::env::var("CI").is_ok() {
            eprintln!("Skipping test_list_windows_all in CI");
            return;
        }

        let session_name = format!("test-list-all-{}", std::process::id());

        let session = match start_zellij_session(&session_name) {
            Some(s) => s,
            None => panic!(
                "Zellij is not available or failed to start. Please install Zellij to run this test."
            ),
        };

        let mux = ZellijMultiplexer::new().expect("Failed to create ZellijMultiplexer");

        let result = mux.list_windows(None);
        assert!(result.is_ok());
        let windows = result.unwrap();
        // Should contain at least our test session
        assert!(windows.iter().any(|w| w.contains(&session_name)));

        kill_zellij_session(&session);
    }

    #[test]
    #[serial_test::file_serial]
    fn test_list_windows_with_filter() {
        init_tracing();

        // Skip this test in CI: zellij behaviour have limitations in our CI
        if std::env::var("CI").is_ok() {
            eprintln!("Skipping test_list_windows_with_filter in CI");
            return;
        }

        let session_name = format!("test-list-filter-{}", std::process::id());

        let session = match start_zellij_session(&session_name) {
            Some(s) => s,
            None => panic!(
                "Zellij is not available or failed to start. Please install Zellij to run this test."
            ),
        };

        let mux = ZellijMultiplexer::new().expect("Failed to create ZellijMultiplexer");

        let result = mux.list_windows(Some("test-list-filter"));
        assert!(result.is_ok());
        let windows = result.unwrap();
        // Should contain only filtered sessions
        for window in windows {
            assert!(window.contains("test-list-filter"));
        }

        kill_zellij_session(&session);
    }

    #[test]
    #[serial_test::file_serial]
    fn test_list_windows_no_sessions() {
        init_tracing();

        // Skip this test in CI: zellij behaviour have limitations in our CI
        if std::env::var("CI").is_ok() {
            eprintln!("Skipping test_list_windows_no_sessions in CI");
            return;
        }

        if let Ok(mux) = ZellijMultiplexer::new() {
            // Try to list with a filter that won't match anything
            let result = mux.list_windows(Some("nonexistent-session-xyz-123"));
            assert!(result.is_ok());
            let windows = result.unwrap();
            assert!(windows.is_empty());
        }
    }

    #[test]
    #[serial_test::file_serial]
    fn test_focus_window() {
        init_tracing();

        // Skip this test in CI: zellij behaviour have limitations in our CI
        if std::env::var("CI").is_ok() {
            eprintln!("Skipping test_focus_window in CI");
            return;
        }

        let session_name = format!("test-focus-{}", std::process::id());

        let session = match start_zellij_session(&session_name) {
            Some(s) => s,
            None => panic!(
                "Zellij is not available or failed to start. Please install Zellij to run this test."
            ),
        };

        if let Ok(mux) = ZellijMultiplexer::new() {
            // Focus might fail if running headless, which is expected
            let result = mux.focus_window(&session);
            // We don't assert success here because attach might require a terminal
            let _ = result;
        }

        kill_zellij_session(&session);
    }

    #[test]
    #[serial_test::file_serial]
    fn test_focus_window_invalid_session() {
        init_tracing();

        if let Ok(mux) = ZellijMultiplexer::new() {
            let result = mux.focus_window(&"99999-nonexistent".to_string());
            // This should fail because the session doesn't exist
            assert!(result.is_err());
        }
    }

    #[test]
    #[serial_test::file_serial]
    fn test_command_escaping_single_quotes() {
        init_tracing();

        // Skip this test in CI: zellij behaviour have limitations in our CI
        if std::env::var("CI").is_ok() {
            eprintln!("Skipping test_command_escaping_single_quotes in CI");
            return;
        }

        let session_name = format!("test-escape-sq-{}", std::process::id());

        let session = match start_zellij_session(&session_name) {
            Some(s) => s,
            None => panic!(
                "Zellij is not available or failed to start. Please install Zellij to run this test."
            ),
        };

        env::set_var("ZELLIJ_SESSION_NAME", &session);

        let mux = ZellijMultiplexer::new().expect("Failed to create ZellijMultiplexer");

        let cmd_opts = CommandOptions {
            cwd: None,
            env: None,
        };

        let pane_id = format!("zellij-pane-{}", session);
        let result = mux.run_command(&pane_id, "echo 'hello world'", &cmd_opts);
        assert!(result.is_ok());

        env::remove_var("ZELLIJ_SESSION_NAME");
        kill_zellij_session(&session);
    }

    #[test]
    #[serial_test::file_serial]
    fn test_command_escaping_double_quotes() {
        init_tracing();

        // Skip this test in CI: zellij behaviour have limitations in our CI
        if std::env::var("CI").is_ok() {
            eprintln!("Skipping test_command_escaping_double_quotes in CI");
            return;
        }

        let session_name = format!("test-escape-dq-{}", std::process::id());

        let session = match start_zellij_session(&session_name) {
            Some(s) => s,
            None => panic!(
                "Zellij is not available or failed to start. Please install Zellij to run this test."
            ),
        };

        env::set_var("ZELLIJ_SESSION_NAME", &session);

        let mux = ZellijMultiplexer::new().expect("Failed to create ZellijMultiplexer");

        let cmd_opts = CommandOptions {
            cwd: None,
            env: None,
        };

        let pane_id = format!("zellij-pane-{}", session);
        let result = mux.run_command(&pane_id, "echo \"hello world\"", &cmd_opts);
        assert!(result.is_ok());

        env::remove_var("ZELLIJ_SESSION_NAME");
        kill_zellij_session(&session);
    }

    #[test]
    #[serial_test::file_serial]
    fn test_command_escaping_dollar_signs() {
        init_tracing();

        // Skip this test in CI: zellij behaviour have limitations in our CI
        if std::env::var("CI").is_ok() {
            eprintln!("Skipping test_command_escaping_dollar_signs in CI");
            return;
        }

        let session_name = format!("test-escape-dollar-{}", std::process::id());

        let session = match start_zellij_session(&session_name) {
            Some(s) => s,
            None => panic!(
                "Zellij is not available or failed to start. Please install Zellij to run this test."
            ),
        };

        env::set_var("ZELLIJ_SESSION_NAME", &session);

        let mux = ZellijMultiplexer::new().expect("Failed to create ZellijMultiplexer");

        let cmd_opts = CommandOptions {
            cwd: None,
            env: None,
        };

        let pane_id = format!("zellij-pane-{}", session);
        let result = mux.run_command(&pane_id, "echo $HOME", &cmd_opts);
        assert!(result.is_ok());

        env::remove_var("ZELLIJ_SESSION_NAME");
        kill_zellij_session(&session);
    }

    #[test]
    #[serial_test::file_serial]
    fn test_command_escaping_backslashes() {
        init_tracing();

        // Skip this test in CI: zellij behaviour have limitations in our CI
        if std::env::var("CI").is_ok() {
            eprintln!("Skipping test_command_escaping_backslashes in CI");
            return;
        }

        let session_name = format!("test-escape-bs-{}", std::process::id());

        let session = match start_zellij_session(&session_name) {
            Some(s) => s,
            None => panic!(
                "Zellij is not available or failed to start. Please install Zellij to run this test."
            ),
        };

        env::set_var("ZELLIJ_SESSION_NAME", &session);

        let mux = ZellijMultiplexer::new().expect("Failed to create ZellijMultiplexer");

        let cmd_opts = CommandOptions {
            cwd: None,
            env: None,
        };

        let pane_id = format!("zellij-pane-{}", session);
        let result = mux.run_command(&pane_id, "echo \\test\\path", &cmd_opts);
        assert!(result.is_ok());

        env::remove_var("ZELLIJ_SESSION_NAME");
        kill_zellij_session(&session);
    }

    #[test]
    #[serial_test::file_serial]
    fn test_command_escaping_mixed_special_chars() {
        init_tracing();

        // Skip this test in CI: zellij behaviour have limitations in our CI
        if std::env::var("CI").is_ok() {
            eprintln!("Skipping test_command_escaping_mixed_special_chars in CI");
            return;
        }

        let session_name = format!("test-escape-mixed-{}", std::process::id());

        let session = match start_zellij_session(&session_name) {
            Some(s) => s,
            None => panic!(
                "Zellij is not available or failed to start. Please install Zellij to run this test."
            ),
        };

        env::set_var("ZELLIJ_SESSION_NAME", &session);

        let mux = ZellijMultiplexer::new().expect("Failed to create ZellijMultiplexer");

        let cmd_opts = CommandOptions {
            cwd: None,
            env: None,
        };

        let pane_id = format!("zellij-pane-{}", session);
        let result = mux.run_command(&pane_id, "echo '$HOME' \\test \"quoted\"", &cmd_opts);
        assert!(result.is_ok());

        env::remove_var("ZELLIJ_SESSION_NAME");
        kill_zellij_session(&session);
    }

    #[test]
    #[serial_test::file_serial]
    fn test_zellij_integration_lifecycle() {
        init_tracing();

        // Skip this test in CI: zellij behaviour have limitations in our CI
        if std::env::var("CI").is_ok() {
            eprintln!("Skipping test_zellij_integration_lifecycle in CI");
            return;
        }

        let session_name = format!("test-lifecycle-{}", std::process::id());

        // 1. Start Zellij session
        let session = match start_zellij_session(&session_name) {
            Some(s) => s,
            None => panic!(
                "Zellij is not available or failed to start. Please install Zellij to run this test."
            ),
        };

        env::set_var("ZELLIJ_SESSION_NAME", &session);

        // 2. Create ZellijMultiplexer instance
        let mux = ZellijMultiplexer::new().expect("Failed to create ZellijMultiplexer");

        // 3. Test is_available()
        assert!(mux.is_available());

        // 4. Open multiple windows/tabs
        let window_opts1 = WindowOptions {
            title: Some("lifecycle-window-1"),
            cwd: None,
            focus: false,
            profile: None,
            init_command: None,
        };
        let window_id1 = mux.open_window(&window_opts1).expect("Failed to open first window");
        assert_eq!(window_id1, session);

        let window_opts2 = WindowOptions {
            title: Some("lifecycle-window-2"),
            cwd: None,
            focus: false,
            profile: None,
            init_command: None,
        };
        let window_id2 = mux.open_window(&window_opts2).expect("Failed to open second window");
        assert_eq!(window_id2, session);

        // 5. List sessions
        let windows = mux.list_windows(None).expect("Failed to list windows");
        assert!(windows.iter().any(|w| w.contains(&session_name)));

        // 6. Split panes (vertical and horizontal)
        let cmd_opts = CommandOptions {
            cwd: None,
            env: None,
        };

        let _pane1 = mux
            .split_pane(
                Some(&session),
                None,
                SplitDirection::Vertical,
                None,
                &cmd_opts,
                Some("echo 'vertical split'"),
            )
            .expect("Failed to split vertically");

        let _pane2 = mux
            .split_pane(
                Some(&session),
                None,
                SplitDirection::Horizontal,
                None,
                &cmd_opts,
                Some("echo 'horizontal split'"),
            )
            .expect("Failed to split horizontally");

        // 7. Run commands in panes
        let pane_id = format!("zellij-pane-{}", session);
        mux.run_command(&pane_id, "echo 'test command'", &cmd_opts)
            .expect("Failed to run command");

        // 8. Send text to panes
        mux.send_text(&pane_id, "echo 'sent text'\n").expect("Failed to send text");

        // 9. Cleanup
        env::remove_var("ZELLIJ_SESSION_NAME");
        kill_zellij_session(&session);

        // 10. Verify session deleted
        thread::sleep(Duration::from_millis(500));
        let windows_after = mux.list_windows(Some(&session_name)).unwrap_or_default();
        assert!(windows_after.is_empty() || !windows_after.iter().any(|w| w == &session_name));
    }
}
