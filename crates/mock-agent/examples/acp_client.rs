#![allow(
    clippy::disallowed_methods,
    clippy::default_constructed_unit_structs,
    clippy::get_first
)]
// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Minimal ACP SDK client for exercising the `mock-agent` ACP mode over stdio.
//!
//! Usage:
//! ```bash
//! cargo run -p mock-agent --example acp_client -- \\
//!   --agent target/debug/mock-agent \\
//!   --scenario tests/tools/mock-agent-acp/scenarios/acp_echo.yaml
//! ```

use agent_client_protocol as acp;
use agent_client_protocol::Agent;
use anyhow::{Context, Result};
use base64::Engine;
use clap::Parser;
use std::path::Path;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};

#[derive(Parser, Debug)]
struct Args {
    /// Path to the ACP agent binary (defaults to cargo-built mock-agent)
    #[arg(long, default_value = "target/debug/mock-agent")]
    agent: String,

    /// Scenario file passed to mock-agent (required for deterministic playback)
    #[arg(long)]
    scenario: Option<String>,

    /// Optional session id to force when selecting scenarios
    #[arg(long)]
    session_id: Option<String>,

    /// Image file to include in the initial prompt (base64-encoded)
    #[arg(long)]
    image_file: Option<String>,

    /// Audio file to include in the initial prompt (base64-encoded)
    #[arg(long)]
    audio_file: Option<String>,

    /// Initial prompt sent to the agent
    #[arg(long, default_value = "Hello from acp_client")]
    prompt: String,

    /// Additional args forwarded to the agent binary
    #[arg(last = true, value_name = "AGENT_ARG")]
    agent_args: Vec<String>,
}

#[derive(Clone, Default)]
struct LoggingClient;

#[async_trait::async_trait(?Send)]
impl acp::Client for LoggingClient {
    async fn request_permission(
        &self,
        args: acp::RequestPermissionRequest,
    ) -> Result<acp::RequestPermissionResponse, acp::Error> {
        // Auto-select the first option to keep flows moving.
        let choice = args.options.get(0).cloned().unwrap_or_else(|| acp::PermissionOption {
            id: acp::PermissionOptionId("allow".into()),
            name: "Allow".into(),
            kind: acp::PermissionOptionKind::AllowOnce,
            meta: None,
        });
        Ok(acp::RequestPermissionResponse {
            outcome: acp::RequestPermissionOutcome::Selected {
                option_id: choice.id,
            },
            meta: None,
        })
    }

    async fn write_text_file(
        &self,
        args: acp::WriteTextFileRequest,
    ) -> Result<acp::WriteTextFileResponse, acp::Error> {
        println!("[client] write_text_file {}", args.path.display());
        Ok(acp::WriteTextFileResponse { meta: None })
    }

    async fn read_text_file(
        &self,
        args: acp::ReadTextFileRequest,
    ) -> Result<acp::ReadTextFileResponse, acp::Error> {
        println!("[client] read_text_file {}", args.path.display());
        Ok(acp::ReadTextFileResponse {
            content: String::new(),
            meta: None,
        })
    }

    async fn create_terminal(
        &self,
        args: acp::CreateTerminalRequest,
    ) -> Result<acp::CreateTerminalResponse, acp::Error> {
        println!("[client] create_terminal {}", args.command);
        Ok(acp::CreateTerminalResponse {
            terminal_id: acp::TerminalId("term-1".into()),
            meta: None,
        })
    }

    async fn terminal_output(
        &self,
        _args: acp::TerminalOutputRequest,
    ) -> Result<acp::TerminalOutputResponse, acp::Error> {
        Ok(acp::TerminalOutputResponse {
            output: String::new(),
            truncated: false,
            exit_status: Some(acp::TerminalExitStatus {
                exit_code: Some(0),
                signal: None,
                meta: None,
            }),
            meta: None,
        })
    }

    async fn release_terminal(
        &self,
        _args: acp::ReleaseTerminalRequest,
    ) -> Result<acp::ReleaseTerminalResponse, acp::Error> {
        Ok(acp::ReleaseTerminalResponse { meta: None })
    }

    async fn wait_for_terminal_exit(
        &self,
        _args: acp::WaitForTerminalExitRequest,
    ) -> Result<acp::WaitForTerminalExitResponse, acp::Error> {
        Ok(acp::WaitForTerminalExitResponse {
            exit_status: acp::TerminalExitStatus {
                exit_code: Some(0),
                signal: None,
                meta: None,
            },
            meta: None,
        })
    }

    async fn kill_terminal_command(
        &self,
        _args: acp::KillTerminalCommandRequest,
    ) -> Result<acp::KillTerminalCommandResponse, acp::Error> {
        Ok(acp::KillTerminalCommandResponse { meta: None })
    }

    async fn session_notification(&self, args: acp::SessionNotification) -> Result<(), acp::Error> {
        let rendered = match &args.update {
            acp::SessionUpdate::AgentMessageChunk { content }
            | acp::SessionUpdate::UserMessageChunk { content } => {
                format!("{content:?}")
            }
            acp::SessionUpdate::ToolCall(call) => {
                format!("tool call: {} {:?}", call.title, call.status)
            }
            acp::SessionUpdate::ToolCallUpdate(update) => {
                format!("tool update: {:?}", update.fields.status)
            }
            acp::SessionUpdate::Plan(plan) => format!("plan entries: {}", plan.entries.len()),
            acp::SessionUpdate::AgentThoughtChunk { .. } => "thought chunk".into(),
            acp::SessionUpdate::CurrentModeUpdate { current_mode_id } => {
                format!("mode -> {}", current_mode_id.0.as_ref())
            }
            acp::SessionUpdate::AvailableCommandsUpdate { .. } => "commands update".into(),
        };
        println!("[session/update] {}", rendered);
        Ok(())
    }

    async fn ext_method(&self, _args: acp::ExtRequest) -> Result<acp::ExtResponse, acp::Error> {
        Err(acp::Error::method_not_found())
    }

    async fn ext_notification(&self, _args: acp::ExtNotification) -> Result<(), acp::Error> {
        Ok(())
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    let args = Args::parse();

    let mut cmd = tokio::process::Command::new(&args.agent);
    if let Some(scenario) = &args.scenario {
        cmd.arg("--scenario").arg(scenario);
    }
    if let Some(session_id) = &args.session_id {
        cmd.arg("--session-id").arg(session_id);
    }
    cmd.args(&args.agent_args);
    cmd.stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .kill_on_drop(true);

    let mut child = cmd.spawn().context("failed to spawn agent process")?;
    let outgoing = child.stdin.take().context("agent stdin unavailable")?.compat_write();
    let incoming = child.stdout.take().context("agent stdout unavailable")?.compat();

    let local = tokio::task::LocalSet::new();
    local
        .run_until(async move {
            let (conn, io) = acp::ClientSideConnection::new(
                LoggingClient::default(),
                outgoing,
                incoming,
                |fut| {
                    tokio::task::spawn_local(fut);
                },
            );
            tokio::task::spawn_local(io);

            let init = conn
                .initialize(acp::InitializeRequest {
                    protocol_version: acp::V1,
                    client_capabilities: acp::ClientCapabilities::default(),
                    meta: None,
                })
                .await?;
            println!(
                "Connected (agent loadSession: {})",
                init.agent_capabilities.load_session
            );

            let session = conn
                .new_session(acp::NewSessionRequest {
                    mcp_servers: Vec::new(),
                    cwd: std::env::current_dir()?,
                    meta: None,
                })
                .await?;
            println!("Session id: {}", session.session_id.0.as_ref());

            // Build initial prompt with optional image/audio blocks.
            let mut initial_prompt: Vec<acp::ContentBlock> = Vec::new();
            if let Some(path) = &args.image_file {
                initial_prompt.push(load_image_block(path)?);
            }
            if let Some(path) = &args.audio_file {
                initial_prompt.push(load_audio_block(path)?);
            }
            initial_prompt.push(acp::ContentBlock::from(args.prompt.clone()));

            conn.prompt(acp::PromptRequest {
                session_id: session.session_id.clone(),
                prompt: initial_prompt,
                meta: None,
            })
            .await?;

            // Interactive loop: one prompt per stdin line until EOF.
            let mut reader = BufReader::new(tokio::io::stdin());
            let mut line = String::new();
            loop {
                line.clear();
                let n = reader.read_line(&mut line).await?;
                if n == 0 {
                    break;
                }
                let trimmed = line.trim_end_matches(&['\n', '\r'][..]);
                if trimmed.is_empty() {
                    continue;
                }
                conn.prompt(acp::PromptRequest {
                    session_id: session.session_id.clone(),
                    prompt: vec![acp::ContentBlock::from(trimmed.to_string())],
                    meta: None,
                })
                .await?;
            }

            // Give the agent time to stream updates then exit.
            tokio::time::sleep(std::time::Duration::from_millis(300)).await;
            Ok::<(), anyhow::Error>(())
        })
        .await?;

    Ok(())
}

fn load_image_block(path: &str) -> Result<acp::ContentBlock> {
    let data = std::fs::read(path).with_context(|| format!("read image {}", path))?;
    let mime = guess_mime(path, "image/png");
    Ok(acp::ContentBlock::Image(acp::ImageContent {
        annotations: None,
        data: base64::engine::general_purpose::STANDARD.encode(data),
        mime_type: mime,
        uri: Some(path.to_string()),
        meta: None,
    }))
}

fn load_audio_block(path: &str) -> Result<acp::ContentBlock> {
    let data = std::fs::read(path).with_context(|| format!("read audio {}", path))?;
    let mime = guess_mime(path, "audio/mpeg");
    Ok(acp::ContentBlock::Audio(acp::AudioContent {
        annotations: None,
        data: base64::engine::general_purpose::STANDARD.encode(data),
        mime_type: mime,
        meta: None,
    }))
}

fn guess_mime(path: &str, default: &str) -> String {
    match Path::new(path)
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
        .as_str()
    {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "bmp" => "image/bmp",
        "webp" => "image/webp",
        "mp3" => "audio/mpeg",
        "wav" => "audio/wav",
        "flac" => "audio/flac",
        "ogg" => "audio/ogg",
        _ => default,
    }
    .to_string()
}
