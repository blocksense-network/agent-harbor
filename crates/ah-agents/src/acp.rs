// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! ACP client agent implementation (stdio transport)
//!
//! Milestone 1 provides a lightweight scaffold that wires the ACP Rust SDK
//! into the Agent Harbor agent abstraction. The implementation is intentionally
//! minimal: it resolves the external ACP agent binary, prepares a configured
//! command (including subcommand-style binaries such as `opencode acp`), and
//! offers helpers for building SDK connections. Subsequent milestones will
//! flesh out capability negotiation, filesystem/terminal method handling, and
//! rich event translation.

use crate::session::{export_directory, import_directory};
use crate::traits::*;
use ah_core::task_manager::TaskEvent;
use ah_domain_types::AcpLaunchCommand;
use async_trait::async_trait;
#[cfg(test)]
use chrono::Utc;
use regex::Regex;
use serde_json::Value;
use std::env;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use tokio::process::Command;
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};
use tracing::warn;

/// ACP client executor that launches an external ACP-compliant agent binary
pub struct AcpAgent {
    /// Default launch command (binary + args) when `AgentLaunchConfig` does not override
    default_command: AcpLaunchCommand,
}

impl Default for AcpAgent {
    fn default() -> Self {
        Self::new()
    }
}

impl AcpAgent {
    /// Create a new ACP agent executor using the best-effort binary resolution
    pub fn new() -> Self {
        // First, honor unified command string if provided.
        if let Some(cmdline) =
            std::env::var("AH_ACP_AGENT_CMD").ok().filter(|v| !v.trim().is_empty())
        {
            match AcpLaunchCommand::from_command_string(&cmdline) {
                Ok(cmd) => {
                    return Self {
                        default_command: cmd,
                    };
                }
                Err(err) => {
                    warn!(
                        %err,
                        "ignoring invalid AH_ACP_AGENT_CMD; falling back to legacy ACP defaults"
                    );
                }
            }
        }

        // Prefer explicit legacy overrides, otherwise look for a reasonable default
        Self {
            default_command: Self::legacy_default_command(),
        }
    }

    /// Create an agent executor with an explicit binary path (used by tests)
    pub fn with_binary(path: impl Into<PathBuf>) -> Self {
        Self {
            default_command: AcpLaunchCommand {
                binary: path.into(),
                args: Vec::new(),
            },
        }
    }

    /// Create an agent executor with an explicit stdio launch command (binary + args)
    /// Useful when testing subcommand-based binaries.
    pub fn with_stdio_command(command: AcpLaunchCommand) -> Self {
        Self {
            default_command: command,
        }
    }

    fn legacy_default_command() -> AcpLaunchCommand {
        let default_binary = std::env::var("AH_ACP_BINARY")
            .ok()
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
            .or_else(|| which::which("acp-agent").ok())
            .or_else(|| which::which("mock-agent").ok())
            .unwrap_or_else(|| PathBuf::from("acp-agent"));

        // Optional extra args for subcommand-style binaries (e.g., `opencode acp`)
        let default_args: Vec<String> = std::env::var("AH_ACP_ARGS")
            .ok()
            .filter(|v| !v.trim().is_empty())
            .map(|v| v.split_whitespace().map(|s| s.to_string()).collect())
            .unwrap_or_default();

        AcpLaunchCommand {
            binary: default_binary,
            args: default_args,
        }
    }

    fn resolve_command(&self, config: &AgentLaunchConfig) -> AgentResult<AcpLaunchCommand> {
        if let Some(cmd) = &config.acp_stdio_command {
            return Ok(cmd.clone());
        }

        if let Some(binary) = &config.acp_binary {
            return Ok(AcpLaunchCommand {
                binary: binary.clone(),
                args: Vec::new(),
            });
        }

        Ok(self.default_command.clone())
    }

    fn resolve_binary_path(&self, binary: &Path) -> AgentResult<PathBuf> {
        // If the caller supplied a path (absolute or containing separators), verify it directly.
        if binary.is_absolute() || binary.components().count() > 1 {
            if !binary.exists() {
                return Err(AgentError::VersionDetectionFailed(format!(
                    "ACP binary '{}' not found; set --acp-agent-cmd or AH_ACP_AGENT_CMD",
                    binary.display()
                )));
            }

            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let meta = std::fs::metadata(binary).map_err(|e| {
                    AgentError::VersionDetectionFailed(format!(
                        "Unable to read ACP binary metadata ({}): {}",
                        binary.display(),
                        e
                    ))
                })?;

                if meta.permissions().mode() & 0o111 == 0 {
                    return Err(AgentError::VersionDetectionFailed(format!(
                        "ACP binary '{}' is not executable; chmod +x or provide an executable path",
                        binary.display()
                    )));
                }
            }

            return Ok(binary.to_path_buf());
        }

        // Otherwise search PATH
        which::which(binary).map_err(|_| {
            AgentError::VersionDetectionFailed(format!(
                "ACP binary '{}' not found in PATH; set --acp-agent-cmd/AH_ACP_AGENT_CMD or install an ACP agent",
                binary.display()
            ))
        })
    }

    fn extract_semver(text: &str) -> Option<String> {
        // Accepts versions like 0.1.0, 1.2.3-beta.1, 2.0.0+meta
        Regex::new(r"(?m)(\d+\.\d+\.\d+(?:[-+][0-9A-Za-z\.-]+)?)")
            .ok()
            .and_then(|re| re.captures(text))
            .and_then(|caps| caps.get(1).map(|m| m.as_str().to_string()))
    }

    /// Detect version for a specific launch command (binary + args)
    async fn detect_version_for_launch(
        &self,
        launch: &AcpLaunchCommand,
    ) -> AgentResult<AgentVersion> {
        let resolved_binary = self.resolve_binary_path(&launch.binary)?;

        let output = Command::new(&resolved_binary)
            .args(&launch.args)
            .arg("--version")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(|e| AgentError::VersionDetectionFailed(format!("{}", e)))?;

        if !output.status.success() {
            return Err(AgentError::VersionDetectionFailed(format!(
                "{} --version exited with status {:?}",
                resolved_binary.display(),
                output.status.code()
            )));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let combined = format!("{}\n{}", stdout, stderr);

        let version = Self::extract_semver(&combined).ok_or_else(|| {
            AgentError::VersionDetectionFailed(
                "could not parse ACP binary version from --version output".to_string(),
            )
        })?;

        Ok(AgentVersion {
            version,
            commit: None,
            release_date: None,
        })
    }

    /// Detect version using a specific AgentLaunchConfig (honors --acp-agent-cmd overrides)
    pub async fn detect_version_for_config(
        &self,
        config: &AgentLaunchConfig,
    ) -> AgentResult<AgentVersion> {
        let launch = self.resolve_command(config)?;
        self.detect_version_for_launch(&launch).await
    }

    /// Build a JSON-RPC client connection to the spawned ACP binary over stdio using the SDK.
    ///
    /// This is a convenience for callers that need to speak ACP instead of parsing stdout.
    pub async fn attach_stdio_client<H>(
        &self,
        child: &mut tokio::process::Child,
        handler: H,
    ) -> AgentResult<(
        agent_client_protocol::ClientSideConnection,
        impl std::future::Future<Output = anyhow::Result<()>>,
    )>
    where
        H: agent_client_protocol::MessageHandler<agent_client_protocol::ClientSide>
            + Clone
            + 'static,
    {
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| AgentError::ConfigurationError("failed to take child stdin".into()))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| AgentError::ConfigurationError("failed to take child stdout".into()))?;

        let (client_conn, client_io) = agent_client_protocol::ClientSideConnection::new(
            handler,
            stdin.compat_write(),
            stdout.compat(),
            |task| {
                tokio::task::spawn_local(task);
            },
        );

        Ok((client_conn, client_io))
    }
}

#[async_trait]
impl AgentExecutor for AcpAgent {
    fn name(&self) -> &'static str {
        "acp"
    }

    async fn detect_version(&self) -> AgentResult<AgentVersion> {
        self.detect_version_for_launch(&self.default_command).await
    }

    async fn prepare_launch(&self, config: AgentLaunchConfig) -> AgentResult<Command> {
        let launch = self.resolve_command(&config)?;

        // We exec the Harbor-owned entrypoint (`ah tui acp-client`) so sandboxing
        // can wrap the ACP session while keeping Harbor in control of IO.
        let current_exe = env::current_exe().unwrap_or_else(|_| PathBuf::from("ah"));
        let mut cmd = Command::new(current_exe);
        cmd.arg("tui").arg("acp-client");
        cmd.arg("--acp-agent-cmd").arg(launch.to_command_string());
        if let Some(prompt) = &config.prompt {
            cmd.arg("--prompt").arg(prompt);
        }
        cmd.current_dir(&config.working_dir);
        cmd.env("HOME", &config.home_dir);

        for (k, v) in &config.env_vars {
            cmd.env(k, v);
        }

        if let Some(api) = &config.api_server {
            cmd.env("ACP_LLM_API", api);
        }

        if let Some(key) = &config.api_key {
            cmd.env("ACP_LLM_API_KEY", key);
        }

        if let Some(prompt) = &config.prompt {
            cmd.env("ACP_INITIAL_PROMPT", prompt);
        }

        if config.json_output {
            cmd.env("ACP_OUTPUT", "json");
        }

        if let Some(snapshot_cmd) = &config.snapshot_cmd {
            cmd.env("ACP_SNAPSHOT_CMD", snapshot_cmd);
        }

        // Keep IO attached for interactive usage; higher layers can still pipe
        // the stdio when recording sessions.
        cmd.stdin(Stdio::inherit());
        cmd.stdout(Stdio::inherit());
        cmd.stderr(Stdio::inherit());

        Ok(cmd)
    }

    async fn copy_credentials(&self, _src_home: &Path, _dst_home: &Path) -> AgentResult<()> {
        // ACP clients delegate authentication to the external agent binary; no
        // credential copy is required at this layer.
        Ok(())
    }

    async fn get_user_api_key(&self) -> AgentResult<Option<String>> {
        Ok(None)
    }

    async fn export_session(&self, home_dir: &Path) -> AgentResult<PathBuf> {
        let archive = home_dir.join("acp-session.tar.gz");
        export_directory(home_dir, &archive).await?;
        Ok(archive)
    }

    async fn import_session(&self, session_archive: &Path, home_dir: &Path) -> AgentResult<()> {
        import_directory(session_archive, home_dir).await
    }

    fn parse_output(&self, raw_output: &[u8]) -> AgentResult<Vec<AgentEvent>> {
        let text = String::from_utf8_lossy(raw_output);
        let mut events = Vec::new();

        for line in text.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }

            // Prefer structured TaskEvent JSON emitted by ACP-aware clients.
            if let Ok(task_event) = serde_json::from_str::<TaskEvent>(trimmed) {
                events.extend(task_event_to_agent_events(task_event));
                continue;
            }

            if trimmed.to_ascii_lowercase().contains("error") {
                events.push(AgentEvent::Error {
                    message: trimmed.to_string(),
                });
            } else if trimmed.to_ascii_lowercase().starts_with("log:") {
                events.push(AgentEvent::Log {
                    level: "info".to_string(),
                    message: trimmed.trim_start_matches("log:").trim().to_string(),
                });
            } else {
                events.push(AgentEvent::Output {
                    content: trimmed.to_string(),
                });
            }
        }

        Ok(events)
    }

    fn config_dir(&self, home: &Path) -> PathBuf {
        home.join(".acp")
    }

    fn credential_paths(&self) -> Vec<PathBuf> {
        Vec::new()
    }
}

fn task_event_to_agent_events(task_event: TaskEvent) -> Vec<AgentEvent> {
    match task_event {
        TaskEvent::Thought { thought, .. } => {
            vec![AgentEvent::Thinking { content: thought }]
        }
        TaskEvent::Log { level, message, .. } => vec![AgentEvent::Log {
            level: level.to_string(),
            message,
        }],
        TaskEvent::ToolUse {
            tool_name,
            tool_args,
            ..
        } => {
            vec![AgentEvent::ToolUse {
                tool_name,
                arguments: tool_args,
            }]
        }
        TaskEvent::ToolResult {
            tool_name,
            tool_output,
            ..
        } => vec![
            AgentEvent::ToolUse {
                tool_name: tool_name.clone(),
                arguments: Value::String(tool_output.clone()),
            },
            AgentEvent::Output {
                content: tool_output,
            },
        ],
        TaskEvent::FileEdit { file_path, .. } => vec![AgentEvent::Output {
            content: format!("edited {}", file_path),
        }],
        TaskEvent::UserInput {
            author, content, ..
        } => vec![AgentEvent::Output {
            content: format!("{author}: {content}"),
        }],
        TaskEvent::Status { status, .. } => vec![AgentEvent::Log {
            level: "info".into(),
            message: format!("status: {}", status),
        }],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agent_client_protocol::{
        AgentNotification, AgentRequest, AgentResponse, AgentSideConnection, Client,
        ClientNotification, ClientRequest, ClientResponse, ClientSideConnection, Error, ExtRequest,
        ExtResponse,
    };
    use serde_json::value::RawValue;
    use std::future;
    use tempfile::TempDir;
    use tokio::io::{duplex, split};
    use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};

    #[tokio::test(flavor = "current_thread")]
    async fn acp_client_initialization() {
        let temp = TempDir::new().expect("tempdir");
        let binary_path = temp.path().join("fake-acp-agent");

        // Lightweight script returning version string
        std::fs::write(&binary_path, "#!/bin/sh\necho 0.1.0\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&binary_path).unwrap().permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&binary_path, perms).unwrap();
        }

        let agent = AcpAgent::with_binary(&binary_path);
        let config = AgentLaunchConfig::new(temp.path()).acp_binary(binary_path.clone());

        let cmd = agent.prepare_launch(config).await.expect("prepare_launch");
        let args: Vec<String> =
            cmd.as_std().get_args().map(|a| a.to_string_lossy().to_string()).collect();
        let bin_str = binary_path.to_string_lossy().to_string();
        assert!(args.windows(2).any(|w| w == ["--acp-agent-cmd", &bin_str]));

        let version = agent.detect_version().await.expect("detect_version");
        assert_eq!(version.version, "0.1.0");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn acp_version_parses_noisy_output_with_args() {
        let temp = TempDir::new().expect("tempdir");
        let binary_path = temp.path().join("fake-acp-agent-args");

        // Script accepts optional subcommand before --version
        std::fs::write(
            &binary_path,
            "#!/bin/sh\nif [ \"$1\" = \"acp\" ]; then shift; fi\nif [ \"$1\" = \"--version\" ]; then echo \"mock-agent version v0.2.3\"; exit 0; fi\necho \"missing args\"; exit 1\n",
        )
        .unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&binary_path).unwrap().permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&binary_path, perms).unwrap();
        }

        let agent = AcpAgent::with_stdio_command(AcpLaunchCommand {
            binary: binary_path.clone(),
            args: vec!["acp".to_string()],
        });

        let version = agent.detect_version().await.expect("detect_version");
        assert_eq!(version.version, "0.2.3");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn acp_version_honors_launch_config_command() {
        let temp = TempDir::new().expect("tempdir");
        let binary_path = temp.path().join("fake-acp-from-config");

        std::fs::write(&binary_path, "#!/bin/sh\necho acp client v0.3.4\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&binary_path).unwrap().permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&binary_path, perms).unwrap();
        }

        // Default command would fail (acp-agent likely absent), so this exercises config override.
        let agent = AcpAgent::new();
        let config =
            AgentLaunchConfig::new(temp.path()).acp_stdio_command_with_args(&binary_path, ["acp"]);

        let version = agent
            .detect_version_for_config(&config)
            .await
            .expect("detect_version_for_config");
        assert_eq!(version.version, "0.3.4");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn acp_version_missing_binary_errors_cleanly() {
        let agent = AcpAgent::with_binary("/no/such/acp-agent");
        let err = agent.detect_version().await.unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("not found"),
            "expected helpful not-found error, got {msg}"
        );
    }

    #[test]
    fn parse_output_prefers_structured_task_events() {
        let agent = AcpAgent::new();
        let task_event = TaskEvent::Thought {
            thought: "structured event".into(),
            reasoning: None,
            ts: Utc::now(),
        };
        let raw = serde_json::to_string(&task_event).expect("serialize");
        let parsed = agent.parse_output(raw.as_bytes()).expect("parse");
        assert!(matches!(
            parsed.first(),
            Some(AgentEvent::Thinking { content }) if content == "structured event"
        ));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn acp_client_basic_dispatch() {
        let local = tokio::task::LocalSet::new();

        local
            .run_until(async {
                #[derive(Clone)]
                struct StubClient;

                impl agent_client_protocol::MessageHandler<agent_client_protocol::ClientSide>
                    for StubClient
                {
                    fn handle_request(
                        &self,
                        request: AgentRequest,
                    ) -> impl std::future::Future<Output = Result<ClientResponse, Error>> {
                        match request {
                            AgentRequest::ExtMethodRequest(_req) => {
                                let response: ExtResponse =
                                    RawValue::from_string("true".into()).expect("raw value").into();
                                future::ready(Ok(ClientResponse::ExtMethodResponse(response)))
                            }
                            other => {
                                let _ = other;
                                future::ready(Err(Error::method_not_found()))
                            }
                        }
                    }

                    fn handle_notification(
                        &self,
                        _notification: AgentNotification,
                    ) -> impl std::future::Future<Output = Result<(), Error>> {
                        future::ready(Ok(()))
                    }
                }

                #[derive(Clone)]
                struct StubAgent;

                impl agent_client_protocol::MessageHandler<agent_client_protocol::AgentSide>
                    for StubAgent
                {
                    fn handle_request(
                        &self,
                        _request: ClientRequest,
                    ) -> impl std::future::Future<Output = Result<AgentResponse, Error>> {
                        future::ready(Err(Error::method_not_found()))
                    }

                    fn handle_notification(
                        &self,
                        _notification: ClientNotification,
                    ) -> impl std::future::Future<Output = Result<(), Error>> {
                        future::ready(Ok(()))
                    }
                }

                let (client_stream, agent_stream) = duplex(8 * 1024);
                let (client_reader, client_writer) = split(client_stream);
                let (agent_reader, agent_writer) = split(agent_stream);

                let (_client_conn, client_io) = ClientSideConnection::new(
                    StubClient,
                    client_writer.compat_write(),
                    client_reader.compat(),
                    |task| {
                        tokio::task::spawn_local(task);
                    },
                );
                let (agent_conn, agent_io) = AgentSideConnection::new(
                    StubAgent,
                    agent_writer.compat_write(),
                    agent_reader.compat(),
                    |task| {
                        tokio::task::spawn_local(task);
                    },
                );

                tokio::task::spawn_local(client_io);
                tokio::task::spawn_local(agent_io);

                let req = ExtRequest {
                    method: "ping".into(),
                    params: RawValue::from_string("{}".into()).unwrap().into(),
                };

                let response = agent_conn.ext_method(req).await.expect("ext method ok");
                assert_eq!(response.get(), "true");
            })
            .await;
    }
}
