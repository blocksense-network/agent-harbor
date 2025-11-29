#![allow(clippy::collapsible_match)]
// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

use std::sync::Arc;

use agent_client_protocol::Agent;
use agent_client_protocol_schema::{
    ClientCapabilities, ContentBlock, CreateTerminalRequest, InitializeRequest, NewSessionRequest,
    PromptRequest, ReadTextFileRequest, ReadTextFileResponse, RequestPermissionOutcome,
    RequestPermissionRequest, RequestPermissionResponse, SessionId, TerminalOutputRequest,
    TerminalOutputResponse, WaitForTerminalExitRequest, WaitForTerminalExitResponse,
};
use ah_scenario_format::{InputContent, Scenario, TimelineEvent, UserInputEntry};
use mock_agent::executor::{ScenarioAgent, ScenarioExecutor};
use portable_pty::{CommandBuilder, PtySize};
use tokio::sync::mpsc;

/// Minimal client that records session/update notifications and fails on unexpected requests.
#[derive(Clone, Default)]
struct TestClient {
    notifications: Arc<tokio::sync::Mutex<Vec<agent_client_protocol_schema::SessionNotification>>>,
}

#[async_trait::async_trait(?Send)]
impl agent_client_protocol::Client for TestClient {
    async fn request_permission(
        &self,
        _args: agent_client_protocol_schema::RequestPermissionRequest,
    ) -> Result<
        agent_client_protocol_schema::RequestPermissionResponse,
        agent_client_protocol_schema::Error,
    > {
        Err(agent_client_protocol_schema::Error::method_not_found())
    }

    async fn session_notification(
        &self,
        args: agent_client_protocol_schema::SessionNotification,
    ) -> Result<(), agent_client_protocol_schema::Error> {
        self.notifications.lock().await.push(args);
        Ok(())
    }
}

#[tokio::test]
async fn acp_round_trip_over_stdio() {
    let scenario = Arc::new(Scenario {
        name: "acp_round_trip".into(),
        tags: vec![],
        terminal_ref: None,
        initial_prompt: None,
        repo: None,
        ah: None,
        server: None,
        acp: None,
        rules: None,
        timeline: vec![
            TimelineEvent::UserInputs {
                user_inputs: vec![UserInputEntry {
                    relative_time: 0,
                    input: InputContent::Text("hello".into()),
                    target: None,
                    meta: None,
                    expected_response: None,
                }],
            },
            TimelineEvent::LlmResponse {
                meta: None,
                llm_response: vec![ah_scenario_format::ResponseElement::Assistant {
                    assistant: vec![ah_scenario_format::AssistantStep {
                        relative_time: 0,
                        content: ah_scenario_format::ContentBlock::Text("hi".into()),
                    }],
                }],
            },
        ],
        expect: None,
    });

    // Build transcript and ScenarioAgent
    let executor = ScenarioExecutor::new(scenario.clone());
    let transcript = executor.to_acp_transcript(None, None, None, None);
    let agent = ScenarioAgent::new(transcript);

    // Wire in-memory stdio via piper, mirroring vendor/acp-rust-sdk tests
    let (client_to_agent_rx, client_to_agent_tx) = piper::pipe(1024);
    let (agent_to_client_rx, agent_to_client_tx) = piper::pipe(1024);

    let local = tokio::task::LocalSet::new();
    local
        .run_until(async move {
            let test_client = TestClient::default();

            let (agent_conn, agent_io) = agent_client_protocol::AgentSideConnection::new(
                agent.clone(),
                client_to_agent_tx,
                agent_to_client_rx,
                |fut| {
                    tokio::task::spawn_local(fut);
                },
            );

            let (client_conn, client_io) = agent_client_protocol::ClientSideConnection::new(
                test_client.clone(),
                agent_to_client_tx,
                client_to_agent_rx,
                |fut| {
                    tokio::task::spawn_local(fut);
                },
            );

            // Forward ScenarioAgent notifications into the connection
            let (tx, mut rx) = mpsc::unbounded_channel();
            agent.set_notifier(tx).await;
            tokio::task::spawn_local(async move {
                while let Some(note) = rx.recv().await {
                    let _ = agent_conn.notify(
                        agent_client_protocol::CLIENT_METHOD_NAMES.session_update,
                        Some(
                            agent_client_protocol_schema::AgentNotification::SessionNotification(
                                note,
                            ),
                        ),
                    );
                }
            });

            // Spawn IO tasks
            tokio::task::spawn_local(agent_io);
            tokio::task::spawn_local(client_io);

            // Initialize and start session
            client_conn
                .initialize(InitializeRequest {
                    protocol_version: agent_client_protocol_schema::VERSION,
                    client_capabilities: ClientCapabilities::default(),
                    meta: None,
                })
                .await
                .expect("initialize failed");

            client_conn
                .new_session(NewSessionRequest {
                    mcp_servers: vec![],
                    cwd: std::path::PathBuf::from("/tmp"),
                    meta: None,
                })
                .await
                .expect("new_session failed");

            // Prompt to trigger live updates
            client_conn
                .prompt(PromptRequest {
                    session_id: SessionId(Arc::from("mock-session")),
                    prompt: vec![ContentBlock::Text(
                        agent_client_protocol_schema::TextContent {
                            annotations: None,
                            text: "hello".into(),
                            meta: None,
                        },
                    )],
                    meta: None,
                })
                .await
                .expect("prompt failed");

            tokio::task::yield_now().await;

            let notes = test_client.notifications.lock().await;
            assert!(
                notes.iter().any(|n| matches!(
                    n.update,
                    agent_client_protocol_schema::SessionUpdate::UserMessageChunk { .. }
                )),
                "expected user message chunk"
            );
            assert!(
                notes.iter().any(|n| matches!(
                    n.update,
                    agent_client_protocol_schema::SessionUpdate::AgentMessageChunk { .. }
                )),
                "expected agent message chunk"
            );
        })
        .await;
}

#[derive(Clone, Default)]
struct ExpectingClient {
    permissions: Arc<tokio::sync::Mutex<Vec<RequestPermissionRequest>>>,
    reads: Arc<tokio::sync::Mutex<Vec<ReadTextFileRequest>>>,
}

#[async_trait::async_trait(?Send)]
impl agent_client_protocol::Client for ExpectingClient {
    async fn request_permission(
        &self,
        args: RequestPermissionRequest,
    ) -> Result<RequestPermissionResponse, agent_client_protocol_schema::Error> {
        self.permissions.lock().await.push(args.clone());
        let choice = args.options.first().map(|o| o.id.clone()).unwrap_or_else(|| {
            agent_client_protocol_schema::PermissionOptionId(Arc::from("allow"))
        });
        Ok(RequestPermissionResponse {
            outcome: RequestPermissionOutcome::Selected { option_id: choice },
            meta: None,
        })
    }

    async fn read_text_file(
        &self,
        args: ReadTextFileRequest,
    ) -> Result<ReadTextFileResponse, agent_client_protocol_schema::Error> {
        self.reads.lock().await.push(args.clone());
        Ok(ReadTextFileResponse {
            content: "expected content".into(),
            meta: None,
        })
    }

    async fn session_notification(
        &self,
        _args: agent_client_protocol_schema::SessionNotification,
    ) -> Result<(), agent_client_protocol_schema::Error> {
        Ok(())
    }
}

/// PTY harness for future end-to-end terminal streaming tests.
///
/// Design goal: run the follower command's real payload in a PTY, stream its stdout via
/// `terminal/output` and surface its exit via `terminal/wait_for_exit`. Today this is only
/// used by the placeholder `run_cmd_triggers_follower_terminal` test; the test is failing
/// because we still need robust parsing of the follower command and better synchronization.
/// When another developer picks this up, please wire this harness to parse the payload from
/// `ah show-sandbox-execution "<cmd>" --id <tool_id>` reliably and assert that stdout is
/// reflected in ACP updates.
struct PtyHarness {
    child: Box<dyn portable_pty::Child + Send + Sync>,
    reader: Box<dyn std::io::Read + Send>,
    exit_status: Option<agent_client_protocol_schema::TerminalExitStatus>,
    buf: Vec<u8>,
}

impl PtyHarness {
    fn spawn(cmdline: &str) -> anyhow::Result<Self> {
        let pty_system = portable_pty::native_pty_system();
        let pair = pty_system.openpty(PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        })?;
        // NOTE: shell_words::split is a best-effort tokenizer; if the follower command
        // contains nested quoting, this may break. Consider a more robust parser.
        let parts = shell_words::split(cmdline)?;
        let prog = parts.first().cloned().unwrap_or_else(|| "sh".to_string());
        let mut cmd = CommandBuilder::new(prog);
        if parts.len() > 1 {
            cmd.args(&parts[1..]);
        }
        let child = pair.slave.spawn_command(cmd)?;
        let reader = pair.master.try_clone_reader()?;
        Ok(Self {
            child,
            reader,
            exit_status: None,
            buf: Vec::new(),
        })
    }

    fn read_once(&mut self) -> (String, bool) {
        let mut buffer = [0u8; 1024];
        if let Ok(n) = self.reader.read(&mut buffer) {
            if n > 0 {
                self.buf.extend_from_slice(&buffer[..n]);
                let s = String::from_utf8_lossy(&buffer[..n]).to_string();
                return (s, false);
            }
        }
        if let Ok(Some(status)) = self.child.try_wait() {
            self.exit_status = Some(agent_client_protocol_schema::TerminalExitStatus {
                exit_code: Some(status.exit_code()),
                signal: None,
                meta: None,
            });
            return (String::new(), true);
        }
        (String::new(), false)
    }

    fn wait(&mut self) {
        if let Ok(status) = self.child.wait() {
            self.exit_status.get_or_insert_with(|| {
                agent_client_protocol_schema::TerminalExitStatus {
                    exit_code: Some(status.exit_code()),
                    signal: None,
                    meta: None,
                }
            });
        }
    }

    fn exit_status(&self) -> agent_client_protocol_schema::TerminalExitStatus {
        self.exit_status
            .clone()
            .unwrap_or(agent_client_protocol_schema::TerminalExitStatus {
                exit_code: Some(0),
                signal: None,
                meta: None,
            })
    }

    fn output_seen(&self) -> bool {
        !self.buf.is_empty()
    }

    fn collected_output(&self) -> String {
        String::from_utf8_lossy(&self.buf).to_string()
    }
}

#[tokio::test]
async fn permission_and_file_reads_are_forwarded() {
    use ah_scenario_format::{
        AgentFileReadsData, AgentPermissionRequestData, FileReadSpec, PermissionOption,
    };
    let scenario = Arc::new(Scenario {
        name: "reqs".into(),
        tags: vec![],
        terminal_ref: None,
        initial_prompt: None,
        repo: None,
        ah: None,
        server: None,
        acp: None,
        rules: None,
        timeline: vec![
            TimelineEvent::AgentPermissionRequest {
                agent_permission_request: AgentPermissionRequestData {
                    session_id: None,
                    tool_call: None,
                    options: Some(vec![PermissionOption {
                        id: "allow".into(),
                        label: "Allow".into(),
                        kind: "allow_once".into(),
                    }]),
                    decision: None,
                    granted: Some(true),
                },
                meta: None,
            },
            TimelineEvent::AgentFileReads {
                agent_file_reads: AgentFileReadsData {
                    files: vec![FileReadSpec {
                        path: "/tmp/file".into(),
                        expected_content: Some(serde_yaml::Value::String(
                            "expected content".into(),
                        )),
                    }],
                },
                meta: None,
            },
        ],
        expect: None,
    });

    let executor = ScenarioExecutor::new(scenario.clone());
    let transcript = executor.to_acp_transcript(None, None, None, None);
    let agent = ScenarioAgent::new(transcript);

    let (client_to_agent_rx, client_to_agent_tx) = piper::pipe(1024);
    let (agent_to_client_rx, agent_to_client_tx) = piper::pipe(1024);

    let local = tokio::task::LocalSet::new();
    local
        .run_until(async move {
            let test_client = ExpectingClient::default();
            let (agent_conn, agent_io) = agent_client_protocol::AgentSideConnection::new(
                agent.clone(),
                client_to_agent_tx,
                agent_to_client_rx,
                |fut| {
                    tokio::task::spawn_local(fut);
                },
            );
            let agent_conn = Arc::new(agent_conn);
            agent.attach_client_connection(agent_conn.clone()).await;

            let (client_conn, client_io) = agent_client_protocol::ClientSideConnection::new(
                test_client.clone(),
                agent_to_client_tx,
                client_to_agent_rx,
                |fut| {
                    tokio::task::spawn_local(fut);
                },
            );

            tokio::task::spawn_local(agent_io);
            tokio::task::spawn_local(client_io);

            client_conn
                .initialize(InitializeRequest {
                    protocol_version: agent_client_protocol_schema::VERSION,
                    client_capabilities: ClientCapabilities::default(),
                    meta: None,
                })
                .await
                .expect("initialize failed");

            client_conn
                .new_session(NewSessionRequest {
                    mcp_servers: vec![],
                    cwd: std::path::PathBuf::from("/tmp"),
                    meta: None,
                })
                .await
                .expect("new_session failed");

            tokio::task::yield_now().await;
            assert_eq!(test_client.permissions.lock().await.len(), 1);
            assert_eq!(test_client.reads.lock().await.len(), 1);
        })
        .await;
}

#[derive(Clone, Default)]
struct TerminalClient {
    creates: Arc<tokio::sync::Mutex<Vec<CreateTerminalRequest>>>,
    /// PTY harness for follower command execution (spawned on terminal/create).
    /// None means follower parsing failed or not yet implemented.
    pty: Arc<tokio::sync::Mutex<Option<PtyHarness>>>,
    notifications: Arc<tokio::sync::Mutex<Vec<agent_client_protocol_schema::SessionNotification>>>,
    spawn_failures: Arc<tokio::sync::Mutex<Vec<String>>>,
}

impl TerminalClient {
    /// Extract the payload command from a follower invocation of the form:
    /// `ah show-sandbox-execution "<cmd>" --id <tool_id>`.
    /// This is intentionally defensive; uses token parsing with a regex fallback.
    fn extract_follower_payload(command: &str) -> Option<String> {
        // Preferred path: tokenize and find the first non-flag argument after
        // `show-sandbox-execution`, skipping paired flag values.
        if let Ok(parts) = shell_words::split(command) {
            if let Some(payload) = Self::extract_from_parts(&parts) {
                return Some(payload);
            }
        }

        // Fallback: slice the raw string between the follower prefix and the first
        // `--id`/`--session` flag, then trim surrounding quotes.
        if let Some(idx) = command.find("show-sandbox-execution") {
            let after = &command[idx + "show-sandbox-execution".len()..];
            let before_flag = after
                .split_once("--id")
                .map(|(head, _)| head)
                .or_else(|| after.split_once("--session").map(|(head, _)| head))
                .unwrap_or(after);
            let candidate = before_flag.trim().trim_matches('"');
            if !candidate.is_empty() {
                return Some(candidate.to_string());
            }
        }

        // Regex fallback disabled for compilation - would require regex crate dependency
        // If needed, the simpler string-based fallback above should handle most cases
        None
    }

    fn extract_from_parts(parts: &[String]) -> Option<String> {
        let pos = parts.iter().position(|p| p.contains("show-sandbox-execution"))?;
        let mut iter = parts.iter().skip(pos + 1).peekable();
        while let Some(part) = iter.next() {
            if part.starts_with("--") {
                if Self::flag_takes_value(part) {
                    iter.next();
                }
                continue;
            }
            return Some(part.to_string());
        }
        None
    }

    fn flag_takes_value(flag: &str) -> bool {
        matches!(
            flag,
            "--id" | "--session" | "--cwd" | "--output" | "--tool" | "--step" | "--exec" | "--env"
        ) || flag.contains('=')
    }

    #[cfg(test)]
    fn assert_extract_payloads() {
        let cases = vec![
            (
                r#"ah show-sandbox-execution "python script.py" --id abc"#,
                "python script.py",
            ),
            (
                r#"ah show-sandbox-execution python.sh --session s1 --id abc"#,
                "python.sh",
            ),
            (
                r#"ah show-sandbox-execution "echo spaced arg" --cwd /tmp --id t1"#,
                "echo spaced arg",
            ),
        ];
        for (cmd, expected) in cases {
            let got = Self::extract_follower_payload(cmd);
            assert_eq!(got.as_deref(), Some(expected), "cmd: {}", cmd);
        }
    }

    #[cfg(test)]
    fn assert_progress_expectation() {
        let step = ah_scenario_format::ProgressStep {
            relative_time: 0,
            message: "out".into(),
            expect_output: Some("out".into()),
        };
        assert_eq!(step.expect_output.as_deref(), Some("out"));
    }
}

#[async_trait::async_trait(?Send)]
impl agent_client_protocol::Client for TerminalClient {
    async fn create_terminal(
        &self,
        args: CreateTerminalRequest,
    ) -> Result<
        agent_client_protocol_schema::CreateTerminalResponse,
        agent_client_protocol_schema::Error,
    > {
        self.creates.lock().await.push(args.clone());
        if let Some(cmd) = Self::extract_follower_payload(&args.command) {
            match PtyHarness::spawn(&cmd) {
                Ok(h) => {
                    *self.pty.lock().await = Some(h);
                }
                Err(err) => {
                    self.spawn_failures.lock().await.push(err.to_string());
                    tracing::warn!(?err, follower=?cmd, "failed to spawn follower PTY");
                }
            }
        } else {
            tracing::warn!(command = %args.command, "could not parse follower payload");
        }
        Ok(agent_client_protocol_schema::CreateTerminalResponse {
            terminal_id: "term-1".into(),
            meta: None,
        })
    }

    async fn terminal_output(
        &self,
        _args: TerminalOutputRequest,
    ) -> Result<TerminalOutputResponse, agent_client_protocol_schema::Error> {
        if let Some(h) = self.pty.lock().await.as_mut() {
            let (out, done) = h.read_once();
            return Ok(TerminalOutputResponse {
                output: out,
                truncated: false,
                exit_status: if done { Some(h.exit_status()) } else { None },
                meta: None,
            });
        }
        Ok(TerminalOutputResponse {
            output: String::new(),
            truncated: false,
            exit_status: None,
            meta: None,
        })
    }

    async fn wait_for_terminal_exit(
        &self,
        _args: WaitForTerminalExitRequest,
    ) -> Result<WaitForTerminalExitResponse, agent_client_protocol_schema::Error> {
        let mut guard = self.pty.lock().await;
        if let Some(h) = guard.as_mut() {
            h.wait();
            return Ok(WaitForTerminalExitResponse {
                exit_status: h.exit_status(),
                meta: None,
            });
        }
        Ok(WaitForTerminalExitResponse {
            exit_status: agent_client_protocol_schema::TerminalExitStatus {
                exit_code: None,
                signal: None,
                meta: None,
            },
            meta: None,
        })
    }

    async fn kill_terminal_command(
        &self,
        _args: agent_client_protocol_schema::KillTerminalCommandRequest,
    ) -> Result<
        agent_client_protocol_schema::KillTerminalCommandResponse,
        agent_client_protocol_schema::Error,
    > {
        Ok(agent_client_protocol_schema::KillTerminalCommandResponse { meta: None })
    }

    async fn release_terminal(
        &self,
        _args: agent_client_protocol_schema::ReleaseTerminalRequest,
    ) -> Result<
        agent_client_protocol_schema::ReleaseTerminalResponse,
        agent_client_protocol_schema::Error,
    > {
        Ok(agent_client_protocol_schema::ReleaseTerminalResponse { meta: None })
    }

    async fn write_text_file(
        &self,
        _args: agent_client_protocol_schema::WriteTextFileRequest,
    ) -> Result<
        agent_client_protocol_schema::WriteTextFileResponse,
        agent_client_protocol_schema::Error,
    > {
        Ok(agent_client_protocol_schema::WriteTextFileResponse { meta: None })
    }

    async fn ext_method(
        &self,
        _args: agent_client_protocol_schema::ExtRequest,
    ) -> Result<agent_client_protocol_schema::ExtResponse, agent_client_protocol_schema::Error>
    {
        Ok(serde_json::value::RawValue::NULL.to_owned().into())
    }

    async fn ext_notification(
        &self,
        _args: agent_client_protocol_schema::ExtNotification,
    ) -> Result<(), agent_client_protocol_schema::Error> {
        Ok(())
    }

    async fn request_permission(
        &self,
        _args: RequestPermissionRequest,
    ) -> Result<RequestPermissionResponse, agent_client_protocol_schema::Error> {
        Err(agent_client_protocol_schema::Error::method_not_found())
    }

    async fn read_text_file(
        &self,
        _args: ReadTextFileRequest,
    ) -> Result<ReadTextFileResponse, agent_client_protocol_schema::Error> {
        Err(agent_client_protocol_schema::Error::method_not_found())
    }

    async fn session_notification(
        &self,
        args: agent_client_protocol_schema::SessionNotification,
    ) -> Result<(), agent_client_protocol_schema::Error> {
        self.notifications.lock().await.push(args);
        Ok(())
    }
}

/// Intended PTY-backed end-to-end follower test. Currently ignored because the follower
/// command parsing is brittle; a future implementer should:
/// 1) Parse `args.command` robustly to extract the payload after `ah show-sandbox-execution`.
/// 2) Ensure `TerminalClient` spawns `PtyHarness` and streams stdout into terminal/output.
/// 3) Assert tool updates reflect real PTY output and exit status.
#[tokio::test]
async fn run_cmd_triggers_follower_terminal() {
    use ah_scenario_format::{ProgressStep, ToolUseData};
    // This is intended to be a true PTY-backed integration test:
    // - follower command: ah show-sandbox-execution "<cmd>" --id <tool_id>
    // - client should parse <cmd>, spawn it under a PTY (see TerminalClient/PtyHarness)
    // - terminal/output returns real stdout so tool updates reflect actual command output
    //
    // CURRENT STATUS: failing ("pty not spawned") because follower command parsing is
    // brittle. A future implementer should improve parsing of args.command to extract
    // the payload reliably (consider parsing by prefix/suffix instead of positional).
    let script = r#"import sys, time
print("hello from pty")
sys.stdout.flush()
time.sleep(0.05)
print("goodbye")
sys.stdout.flush()
sys.exit(0)
"#;
    let tmp = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(tmp.path(), script).unwrap();
    let script_cmd = format!("python {}", tmp.path().display());
    let scenario = Arc::new(Scenario {
        name: "run_cmd".into(),
        tags: vec![],
        terminal_ref: None,
        initial_prompt: None,
        repo: None,
        ah: None,
        server: None,
        acp: None,
        rules: None,
        timeline: vec![TimelineEvent::AgentToolUse {
            agent_tool_use: ToolUseData {
                tool_name: "runCmd".into(),
                args: [("cmd".into(), serde_yaml::Value::from(script_cmd.clone()))].into(),
                tool_call_id: None,
                progress: Some(vec![ProgressStep {
                    relative_time: 0,
                    message: "starting".into(),
                    expect_output: None,
                }]),
                result: None,
                status: None,
                tool_execution: None,
                meta: None,
            },
            meta: None,
        }],
        expect: None,
    });

    let executor = ScenarioExecutor::new(scenario.clone());
    let transcript = executor.to_acp_transcript(None, None, None, None);
    let agent = ScenarioAgent::new(transcript);

    let (client_to_agent_rx, client_to_agent_tx) = piper::pipe(1024);
    let (agent_to_client_rx, agent_to_client_tx) = piper::pipe(1024);

    let local = tokio::task::LocalSet::new();
    local
        .run_until(async move {
            let client = TerminalClient::default();
            let (agent_conn, agent_io) = agent_client_protocol::AgentSideConnection::new(
                agent.clone(),
                client_to_agent_tx,
                agent_to_client_rx,
                |fut| {
                    tokio::task::spawn_local(fut);
                },
            );
            let agent_conn = Arc::new(agent_conn);
            agent.attach_client_connection(agent_conn.clone()).await;

            let (client_conn, client_io) = agent_client_protocol::ClientSideConnection::new(
                client.clone(),
                agent_to_client_tx,
                client_to_agent_rx,
                |fut| {
                    tokio::task::spawn_local(fut);
                },
            );

            tokio::task::spawn_local(agent_io);
            tokio::task::spawn_local(client_io);

            client_conn
                .initialize(InitializeRequest {
                    protocol_version: agent_client_protocol_schema::VERSION,
                    client_capabilities: ClientCapabilities::default(),
                    meta: None,
                })
                .await
                .expect("initialize failed");

            client_conn
                .new_session(NewSessionRequest {
                    mcp_servers: vec![],
                    cwd: std::path::PathBuf::from("/tmp"),
                    meta: None,
                })
                .await
                .expect("new_session failed");

            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
            let created = client.creates.lock().await;
            assert_eq!(created.len(), 1);
            let cmd = created[0].command.clone();
            assert!(
                cmd.contains("show-sandbox-execution"),
                "unexpected follower command: {}",
                cmd
            );
            let extracted = TerminalClient::extract_follower_payload(&cmd)
                .expect("failed to extract follower payload");
            assert_eq!(extracted, script_cmd, "follower payload mismatch");

            // Wait for streaming to surface real PTY output and completion status.
            let mut aggregated = String::new();
            let mut statuses = Vec::new();
            for _ in 0..20 {
                (aggregated, statuses) = {
                    let notes = client.notifications.lock().await.clone();
                    let mut out = String::new();
                    let mut status_list = Vec::new();
                    for note in notes {
                        if let agent_client_protocol_schema::SessionUpdate::ToolCallUpdate(update) = note.update {
                            if let Some(content) = update.fields.content {
                                for block in content {
                                    if let agent_client_protocol_schema::ToolCallContent::Content { content } = block {
                                        if let ContentBlock::Text(t) = content {
                                            out.push_str(&t.text);
                                        }
                                    }
                                }
                            }
                            if let Some(status) = update.fields.status {
                                status_list.push(status);
                            }
                        }
                    }
                    (out, status_list)
                };

                let saw_output = aggregated.contains("hello from pty") && aggregated.contains("goodbye");
                let saw_completion = statuses
                    .iter()
                    .any(|s| matches!(s, agent_client_protocol_schema::ToolCallStatus::Completed));
                if saw_output && saw_completion {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            }
            let spawn_failures = client.spawn_failures.lock().await.clone();
            let (has_pty, harness_output) = {
                let guard = client.pty.lock().await;
                (
                    guard.is_some(),
                    guard
                        .as_ref()
                        .map(|h| h.collected_output())
                        .unwrap_or_default(),
                )
            };

            assert!(
                aggregated.contains("hello from pty") && aggregated.contains("goodbye"),
                "expected PTY output in tool updates, got: {:?} (harness buffered: {:?}, pty present: {}, spawn failures: {:?})",
                aggregated,
                harness_output,
                has_pty,
                spawn_failures
            );
            assert!(
                statuses
                    .iter()
                    .any(|s| matches!(s, agent_client_protocol_schema::ToolCallStatus::Completed)),
                "expected completion status in tool updates, got: {:?}",
                statuses
            );

            // Ensure PTY output was observed in the harness as well.
            let guard = client.pty.lock().await;
            if let Some(h) = guard.as_ref() {
                assert!(h.output_seen(), "pty harness never captured output");
            } else {
                panic!("pty not spawned");
            }
        })
        .await;
}

#[test]
fn follower_payload_parsing_handles_quotes() {
    TerminalClient::assert_extract_payloads();
}

#[test]
fn progress_expectation_field_exists() {
    TerminalClient::assert_progress_expectation();
}
