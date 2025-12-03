// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Minimal ACP client runner that bridges ACP stdio to the Agent Activity TUI.
//!
//! Flow:
//! 1. Spawn the ACP server/binary with stdio pipes.
//! 2. Use the vendored acp-rust-sdk to speak ACP over stdio and collect
//!    `SessionUpdate` notifications.
//! 3. Translate updates into Agent Activity rows with timestamps.
//! 4. Replay the collected activity via `agent_session_loop` (non-live for now).
//!
//! This is an initial scaffold; live streaming into the TUI and richer mappings
//! (terminal/tool/file events) will follow in the next milestone.

use std::io::{IsTerminal, Write};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::Instant,
};

use crate::view_model::task_execution::AgentActivityRow;
use crate::{
    AgentSessionUiMode, agent_session_loop::run_session_viewer,
    session_viewer_deps::AgentSessionDependencies, settings::Settings, terminal::TerminalConfig,
    theme::Theme, view_model::session_viewer_model::GutterConfig, viewer::ViewerConfig,
};
use agent_client_protocol::{
    self as acp, Agent, Client, StreamMessageContent, StreamMessageDirection,
};
use ah_core::task_manager::{
    SaveDraftResult, TaskEvent, TaskLaunchParams, TaskLaunchResult, TaskManager,
};
use ah_core::task_manager_wire::TaskManagerMessage;
use ah_domain_types::task::{TaskInfo, ToolStatus};
use ah_domain_types::{AcpLaunchCommand, LogLevel, TaskExecution};
use ah_recorder::TerminalState;
use ah_rest_api_contract::types::{SessionEvent, SessionLogLevel, SessionToolStatus};
use anyhow::Context;
use chrono::Utc;
use libc;
use serde_json::json;
use similar::{ChangeTag, TextDiff};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tokio::process::Command;
use tokio::task::LocalSet;
use tokio::time::{Duration, timeout};
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};
use uuid::Uuid;

macro_rules! debug_note {
    ($($arg:tt)*) => {
        if std::env::var("AH_ACP_CLIENT_DEBUG").is_ok() {
            tracing::debug!($($arg)*);
        }
    };
}

/// Config passed from CLI/AgentLaunchConfig.
#[derive(Debug, Clone)]
pub struct AcpClientConfig {
    pub acp_command: AcpLaunchCommand,
    pub prompt: Option<String>,
}

/// Very small TaskManager stub to satisfy the Agent Activity loop.
struct NullTaskManager {
    tx: tokio::sync::broadcast::Sender<TaskEvent>,
}

impl NullTaskManager {
    fn new() -> Arc<Self> {
        let (tx, _rx) = tokio::sync::broadcast::channel(1);
        Arc::new(Self { tx })
    }
}

#[async_trait::async_trait]
impl TaskManager for NullTaskManager {
    async fn launch_task(&self, _params: TaskLaunchParams) -> TaskLaunchResult {
        TaskLaunchResult::Failure {
            error: "task launching not supported in ACP client stub".into(),
        }
    }

    fn task_events_receiver(&self, _task_id: &str) -> tokio::sync::broadcast::Receiver<TaskEvent> {
        self.tx.subscribe()
    }

    async fn get_initial_tasks(&self) -> (Vec<TaskInfo>, Vec<TaskExecution>) {
        (Vec::new(), Vec::new())
    }

    async fn save_draft_task(
        &self,
        _draft_id: &str,
        _description: &str,
        _repository: &str,
        _branch: &str,
        _models: &[ah_domain_types::AgentChoice],
    ) -> SaveDraftResult {
        SaveDraftResult::Failure {
            error: "draft saving not supported in ACP client stub".into(),
        }
    }

    fn description(&self) -> &str {
        "null-task-manager"
    }
}

/// Translate ACP content into plain text for a lightweight UI row.
fn content_to_text(content: &acp::ContentBlock) -> String {
    match content {
        acp::ContentBlock::Text(t) => t.text.clone(),
        acp::ContentBlock::Image(_) => "<image>".into(),
        acp::ContentBlock::Audio(_) => "<audio>".into(),
        acp::ContentBlock::ResourceLink(link) => link.uri.clone(),
        acp::ContentBlock::Resource(_) => "<resource>".into(),
    }
}

fn diff_stats(old: Option<&str>, new: &str) -> (usize, usize) {
    let old_text = old.unwrap_or("");
    let diff = TextDiff::from_lines(old_text, new);
    let mut added = 0usize;
    let mut removed = 0usize;
    for change in diff.iter_all_changes() {
        let line_count = change.value().lines().count();
        match change.tag() {
            ChangeTag::Insert => added = added.saturating_add(line_count),
            ChangeTag::Delete => removed = removed.saturating_add(line_count),
            ChangeTag::Equal => {}
        }
    }
    (added, removed)
}

fn format_plan(plan: &acp::Plan) -> String {
    if plan.entries.is_empty() {
        return "Plan: empty".into();
    }
    let mut parts = Vec::new();
    for (idx, entry) in plan.entries.iter().enumerate() {
        let status = match entry.status {
            acp::PlanEntryStatus::Pending => "pending",
            acp::PlanEntryStatus::InProgress => "doing",
            acp::PlanEntryStatus::Completed => "done",
        };
        parts.push(format!("{}: {} ({status})", idx + 1, entry.content));
    }
    format!(
        "Plan ({} steps) → {}",
        plan.entries.len(),
        parts.join(" | ")
    )
}

fn tool_status_from_acp(status: Option<acp::ToolCallStatus>) -> ToolStatus {
    match status {
        Some(acp::ToolCallStatus::Completed) => ToolStatus::Completed,
        Some(acp::ToolCallStatus::Failed) => ToolStatus::Failed,
        _ => ToolStatus::Started,
    }
}

/// Minimal Client implementation that collects updates into activity rows.
#[derive(Clone)]
struct UiClient {
    #[allow(dead_code)]
    start: Instant,
    tx: tokio::sync::mpsc::UnboundedSender<AgentActivityRow>,
    terminals: Rc<RefCell<HashMap<String, LocalTerminalState>>>,
    task_events: Rc<RefCell<Option<tokio::sync::mpsc::UnboundedSender<TaskEvent>>>>,
    cmdtrace: Option<CmdtraceEnv>,
    /// Becomes true once we observe a completed tool call so the driver can
    /// wind down the ACP child deterministically (avoids recorder timeouts).
    tool_completed: Rc<RefCell<bool>>,
}

#[derive(Debug)]
struct LocalTerminalState {
    output: String,
    truncated: bool,
    exit_status: Option<acp::TerminalExitStatus>,
    output_limit: Option<u64>,
    child: Option<tokio::process::Child>,
}

impl UiClient {
    fn emit(&self, row: AgentActivityRow) {
        let _ = self.tx.send(row);
    }

    fn attach_task_event_sender(&self, sender: tokio::sync::mpsc::UnboundedSender<TaskEvent>) {
        self.task_events.borrow_mut().replace(sender);
    }

    fn emit_task_event(&self, event: TaskEvent) {
        if let Some(tx) = self.task_events.borrow().as_ref() {
            debug_note!("task event emitted: {:?}", event);
            let _ = tx.send(event);
        }
    }

    fn emit_locations(&self, locations: &[acp::ToolCallLocation]) {
        for loc in locations {
            let range = loc.line.map(|ln| format!("line {ln}"));
            self.emit(AgentActivityRow::AgentRead {
                file_path: loc.path.display().to_string(),
                range,
            });
        }
    }

    fn emit_diff_row(&self, diff: &acp::Diff) {
        let (added, removed) = self.emit_file_edit_event(diff);
        if diff.new_text.is_empty() && diff.old_text.is_some() {
            self.emit(AgentActivityRow::AgentDeleted {
                file_path: diff.path.display().to_string(),
                lines_removed: removed,
            });
        } else {
            self.emit(AgentActivityRow::AgentEdit {
                file_path: diff.path.display().to_string(),
                lines_added: added,
                lines_removed: removed,
                description: None,
            });
        }
    }

    fn emit_tool_use(
        &self,
        id: &acp::ToolCallId,
        title: &str,
        status: Option<acp::ToolCallStatus>,
        last_line: Option<String>,
    ) {
        self.emit(AgentActivityRow::ToolUse {
            tool_name: title.to_string(),
            tool_execution_id: id.to_string(),
            last_line,
            completed: matches!(
                status,
                Some(acp::ToolCallStatus::Completed) | Some(acp::ToolCallStatus::Failed)
            ),
            status: tool_status_from_acp(status),
            pipeline: None,
        });
    }

    fn emit_file_edit_event(&self, diff: &acp::Diff) -> (usize, usize) {
        let (added, removed) = diff_stats(diff.old_text.as_deref(), diff.new_text.as_str());
        self.emit_task_event(TaskEvent::FileEdit {
            file_path: diff.path.display().to_string(),
            lines_added: added,
            lines_removed: removed,
            description: None,
            ts: Utc::now(),
        });
        (added, removed)
    }

    fn emit_tool_use_event(
        &self,
        id: &acp::ToolCallId,
        title: &str,
        status: ToolStatus,
        content: Option<String>,
    ) {
        let args = content.map(|c| json!({ "content": c })).unwrap_or_else(|| json!({}));
        self.emit_task_event(TaskEvent::ToolUse {
            tool_name: title.to_string(),
            tool_args: args,
            tool_execution_id: id.to_string(),
            status,
            ts: Utc::now(),
        });
    }

    fn emit_tool_result_event(
        &self,
        id: &acp::ToolCallId,
        title: &str,
        status: ToolStatus,
        output: Option<String>,
    ) {
        if matches!(status, ToolStatus::Completed) {
            *self.tool_completed.borrow_mut() = true;
        }

        self.emit_task_event(TaskEvent::ToolResult {
            tool_name: title.to_string(),
            tool_output: output.unwrap_or_default(),
            tool_execution_id: id.to_string(),
            status,
            ts: Utc::now(),
        });
    }
}

#[async_trait::async_trait(?Send)]
impl acp::Client for UiClient {
    async fn request_permission(
        &self,
        args: acp::RequestPermissionRequest,
    ) -> anyhow::Result<acp::RequestPermissionResponse, acp::Error> {
        self.emit_task_event(TaskEvent::Log {
            level: LogLevel::Info,
            message: format!(
                "permission requested for tool {} (auto-allow)",
                args.tool_call.id.0
            ),
            tool_execution_id: Some(args.tool_call.id.0.to_string()),
            ts: Utc::now(),
        });
        Ok(acp::RequestPermissionResponse {
            outcome: acp::RequestPermissionOutcome::Selected {
                option_id: acp::PermissionOptionId("allow".into()),
            },
            meta: None,
        })
    }

    async fn write_text_file(
        &self,
        args: acp::WriteTextFileRequest,
    ) -> anyhow::Result<acp::WriteTextFileResponse, acp::Error> {
        let path = args.path;
        let new_content = args.content;
        let previous = tokio::fs::read_to_string(&path).await.unwrap_or_default();
        tokio::fs::create_dir_all(
            path.parent()
                .map(|p| p.to_path_buf())
                .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))),
        )
        .await
        .ok();
        tokio::fs::write(&path, &new_content)
            .await
            .map_err(|_| acp::Error::internal_error())?;
        let (added, removed) = diff_stats(Some(previous.as_str()), &new_content);
        self.emit_task_event(TaskEvent::FileEdit {
            file_path: path.display().to_string(),
            lines_added: added,
            lines_removed: removed,
            description: None,
            ts: Utc::now(),
        });
        Ok(acp::WriteTextFileResponse { meta: None })
    }

    async fn read_text_file(
        &self,
        args: acp::ReadTextFileRequest,
    ) -> anyhow::Result<acp::ReadTextFileResponse, acp::Error> {
        let data = tokio::fs::read_to_string(&args.path)
            .await
            .map_err(|_| acp::Error::internal_error())?;
        Ok(acp::ReadTextFileResponse {
            content: data,
            meta: None,
        })
    }

    async fn create_terminal(
        &self,
        args: acp::CreateTerminalRequest,
    ) -> Result<acp::CreateTerminalResponse, acp::Error> {
        debug_note!(
            "create_terminal command={} args={:?}",
            args.command,
            args.args
        );
        // Prefer running commands through the command-trace passthrough wrapper when
        // recorder sockets are present so `show-sandbox-execution` can observe the
        // PTY output. If the environment is not configured for passthrough, fall back
        // to the direct spawn used in the initial scaffold.
        if args.command.contains(' ') && args.args.is_empty() {
            return Err(acp::Error::new((
                1,
                "invalid command: spaces present but args missing; provide command + args separately"
                    .into(),
            )));
        }

        let in_recording = std::env::var("AH_TASK_MANAGER_SOCKET").is_ok();
        let mut cmd = if in_recording {
            None
        } else {
            build_passthrough_wrapped_command(&args, self.cmdtrace.as_ref())
        }
        .unwrap_or_else(|| {
            let mut direct = Command::new(&args.command);
            direct.args(&args.args);
            if let Some(cwd) = &args.cwd {
                direct.current_dir(cwd);
            }
            for env in &args.env {
                direct.env(&env.name, &env.value);
            }
            if let Some(trace) = &self.cmdtrace {
                trace.apply_to(&mut direct);
            }
            direct
        });

        if let Some(trace) = &self.cmdtrace {
            trace.apply_to(&mut cmd);
        }

        cmd.stdin(std::process::Stdio::null());
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());
        cmd.kill_on_drop(true);

        let mut child = cmd.spawn().map_err(|_| acp::Error::internal_error())?;

        let stdout = child.stdout.take().ok_or_else(acp::Error::internal_error)?;
        let stderr = child.stderr.take().ok_or_else(acp::Error::internal_error)?;

        let id = Uuid::new_v4().to_string();
        let output_limit = args.output_byte_limit;
        let state = LocalTerminalState {
            output: String::new(),
            truncated: false,
            exit_status: None,
            output_limit,
            child: Some(child),
        };
        let terminals = self.terminals.clone();
        {
            terminals.borrow_mut().insert(id.clone(), state);
        }

        // Wait for process exit and record status
        let terminals_clone = self.terminals.clone();
        let id_wait = id.clone();
        tokio::task::spawn_local(async move {
            let mut child_opt = {
                let mut map = terminals_clone.borrow_mut();
                map.get_mut(&id_wait).and_then(|t| t.child.take())
            };
            if let Some(mut child) = child_opt.take() {
                if let Ok(status) = child.wait().await {
                    let exit = acp::TerminalExitStatus {
                        exit_code: status.code().map(|c| c as u32),
                        signal: None,
                        meta: None,
                    };
                    let mut map = terminals_clone.borrow_mut();
                    if let Some(t) = map.get_mut(&id_wait) {
                        t.exit_status = Some(exit);
                    }
                }
            }
        });

        // Consume stdout asynchronously
        let terminals_clone = self.terminals.clone();
        let id_out = id.clone();
        tokio::task::spawn_local(async move {
            let mut lines = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let mut map = terminals_clone.borrow_mut();
                if let Some(t) = map.get_mut(&id_out) {
                    t.output.push_str(&line);
                    t.output.push('\n');
                    if let Some(limit) = t.output_limit {
                        let bytes = t.output.as_bytes();
                        if bytes.len() > limit as usize {
                            let excess = bytes.len() - limit as usize;
                            let mut drop_len = 0;
                            for ch in t.output.chars() {
                                if drop_len >= excess {
                                    break;
                                }
                                drop_len += ch.len_utf8();
                            }
                            t.output.drain(..drop_len);
                            t.truncated = true;
                        }
                    }
                }
            }
        });

        // Consume stderr asynchronously
        let terminals_clone = self.terminals.clone();
        let id_err = id.clone();
        tokio::task::spawn_local(async move {
            let mut lines = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let mut map = terminals_clone.borrow_mut();
                if let Some(t) = map.get_mut(&id_err) {
                    t.output.push_str(&line);
                    t.output.push('\n');
                    if let Some(limit) = t.output_limit {
                        let bytes = t.output.as_bytes();
                        if bytes.len() > limit as usize {
                            let excess = bytes.len() - limit as usize;
                            let mut drop_len = 0;
                            for ch in t.output.chars() {
                                if drop_len >= excess {
                                    break;
                                }
                                drop_len += ch.len_utf8();
                            }
                            t.output.drain(..drop_len);
                            t.truncated = true;
                        }
                    }
                }
            }
        });

        Ok(acp::CreateTerminalResponse {
            terminal_id: acp::TerminalId(id.into()),
            meta: None,
        })
    }

    async fn terminal_output(
        &self,
        args: acp::TerminalOutputRequest,
    ) -> anyhow::Result<acp::TerminalOutputResponse, acp::Error> {
        let map = self.terminals.borrow();
        let id = args.terminal_id.0.as_ref();
        if let Some(t) = map.get(id) {
            debug_note!(
                "terminal_output id={} len={} exit={:?}",
                id,
                t.output.len(),
                t.exit_status
            );
            Ok(acp::TerminalOutputResponse {
                output: t.output.clone(),
                truncated: t.truncated,
                exit_status: t.exit_status.clone(),
                meta: None,
            })
        } else {
            Err(acp::Error::method_not_found())
        }
    }

    async fn release_terminal(
        &self,
        args: acp::ReleaseTerminalRequest,
    ) -> anyhow::Result<acp::ReleaseTerminalResponse, acp::Error> {
        let id = args.terminal_id.0.clone();
        let state = {
            let mut map = self.terminals.borrow_mut();
            map.remove(id.as_ref())
        };

        if let Some(mut t) = state {
            if let Some(mut child) = t.child.take() {
                let _ = child.kill().await;
            }
        }
        Ok(acp::ReleaseTerminalResponse::default())
    }

    async fn wait_for_terminal_exit(
        &self,
        args: acp::WaitForTerminalExitRequest,
    ) -> anyhow::Result<acp::WaitForTerminalExitResponse, acp::Error> {
        let id = args.terminal_id.0.clone();
        debug_note!("wait_for_terminal_exit id={}", id);
        let (child_opt, prior_exit) = {
            let mut map = self.terminals.borrow_mut();
            match map.get_mut(id.as_ref()) {
                Some(t) => (t.child.take(), t.exit_status.clone()),
                None => (None, None),
            }
        };

        let exit_status = if let Some(mut child) = child_opt {
            if let Ok(status) = child.wait().await {
                let exit = acp::TerminalExitStatus {
                    exit_code: status.code().map(|c| c as u32),
                    signal: None,
                    meta: None,
                };
                let mut map = self.terminals.borrow_mut();
                if let Some(t) = map.get_mut(id.as_ref()) {
                    t.exit_status = Some(exit.clone());
                }
                Some(exit)
            } else {
                None
            }
        } else {
            prior_exit
        };

        let exit_status = exit_status
            .ok_or_else(|| acp::Error::new((1, "terminal not found or running".into())))?;
        Ok(acp::WaitForTerminalExitResponse {
            exit_status,
            meta: None,
        })
    }

    async fn kill_terminal_command(
        &self,
        args: acp::KillTerminalCommandRequest,
    ) -> anyhow::Result<acp::KillTerminalCommandResponse, acp::Error> {
        let id = args.terminal_id.0.clone();
        let state = {
            let mut map = self.terminals.borrow_mut();
            map.remove(id.as_ref())
        };

        if let Some(mut t) = state {
            if let Some(mut child) = t.child.take() {
                let _ = child.kill().await;
                t.exit_status = Some(acp::TerminalExitStatus {
                    exit_code: None,
                    signal: Some("SIGKILL".into()),
                    meta: None,
                });
            }
            // Reinsert updated state so terminal_output can still read it.
            self.terminals.borrow_mut().insert(id.as_ref().to_string(), t);
        }
        Ok(acp::KillTerminalCommandResponse::default())
    }

    async fn session_notification(
        &self,
        args: acp::SessionNotification,
    ) -> anyhow::Result<(), acp::Error> {
        debug_note!("session notification: {:?}", args.update);
        match args.update {
            acp::SessionUpdate::AgentMessageChunk { content }
            | acp::SessionUpdate::AgentThoughtChunk { content } => {
                let text = content_to_text(&content);
                self.emit(AgentActivityRow::AgentThought {
                    thought: text.clone(),
                });
                self.emit_task_event(TaskEvent::Thought {
                    thought: text,
                    reasoning: None,
                    ts: Utc::now(),
                });
            }
            acp::SessionUpdate::UserMessageChunk { content } => {
                let text = content_to_text(&content);
                self.emit(AgentActivityRow::UserInput {
                    author: "user".into(),
                    content: text.clone(),
                    confirmed: true,
                    timestamp: Instant::now(),
                });
                self.emit_task_event(TaskEvent::UserInput {
                    author: "user".into(),
                    content: text,
                    ts: Utc::now(),
                });
            }
            acp::SessionUpdate::ToolCall(call) => {
                let last_line = call.content.iter().find_map(|c| match c {
                    acp::ToolCallContent::Content { content } => Some(content_to_text(content)),
                    acp::ToolCallContent::Diff { diff } => {
                        self.emit_diff_row(diff);
                        None
                    }
                    acp::ToolCallContent::Terminal { terminal_id } => {
                        Some(format!("Terminal {} attached", terminal_id.0))
                    }
                });

                if let Some(raw_cmd) =
                    call.raw_input.as_ref().and_then(|v| v.get("cmd")).and_then(|c| c.as_str())
                {
                    let _ = writeln!(std::io::stdout(), "{}", raw_cmd);
                }

                for content in &call.content {
                    if let acp::ToolCallContent::Content {
                        content: acp::ContentBlock::Text(t),
                    } = content
                    {
                        let _ = writeln!(std::io::stdout(), "{}", t.text);
                    }
                }

                self.emit_locations(&call.locations);
                self.emit_tool_use(&call.id, &call.title, Some(call.status), last_line);
                self.emit_tool_use_event(
                    &call.id,
                    &call.title,
                    tool_status_from_acp(Some(call.status)),
                    call.content.iter().find_map(|c| match c {
                        acp::ToolCallContent::Content { content } => Some(content_to_text(content)),
                        acp::ToolCallContent::Diff { .. } => None,
                        acp::ToolCallContent::Terminal { terminal_id } => {
                            Some(format!("Terminal {} attached", terminal_id.0))
                        }
                    }),
                );
            }
            acp::SessionUpdate::ToolCallUpdate(update) => {
                if let Some(locs) = update.fields.locations.as_ref() {
                    self.emit_locations(locs);
                }

                if let Some(content) = update.fields.content.as_ref() {
                    for c in content {
                        match c {
                            acp::ToolCallContent::Content { .. } => {}
                            acp::ToolCallContent::Diff { diff } => self.emit_diff_row(diff),
                            acp::ToolCallContent::Terminal { terminal_id } => {
                                self.emit(AgentActivityRow::AgentThought {
                                    thought: format!("Terminal {} streaming", terminal_id.0),
                                })
                            }
                        }
                    }
                }

                if let Some(raw_cmd) = update
                    .fields
                    .raw_input
                    .as_ref()
                    .and_then(|v| v.get("cmd"))
                    .and_then(|c| c.as_str())
                {
                    let _ = writeln!(std::io::stdout(), "{}", raw_cmd);
                }
                if let Some(content) = update.fields.content.as_ref() {
                    for c in content {
                        if let acp::ToolCallContent::Content {
                            content: acp::ContentBlock::Text(t),
                        } = c
                        {
                            let _ = writeln!(std::io::stdout(), "{}", t.text);
                        }
                    }
                }

                let last_line =
                    update.fields.content.as_ref().and_then(|c| c.first()).map(|content| {
                        match content {
                            acp::ToolCallContent::Content { content } => content_to_text(content),
                            acp::ToolCallContent::Diff { diff } => {
                                format!("Edited {}", diff.path.display())
                            }
                            acp::ToolCallContent::Terminal { terminal_id } => {
                                format!("Terminal {}", terminal_id.0)
                            }
                        }
                    });

                self.emit_tool_use(
                    &update.id,
                    &update.fields.title.clone().unwrap_or_else(|| "tool".into()),
                    update.fields.status,
                    last_line,
                );

                if let Some(status) = update.fields.status {
                    let status_mapped = tool_status_from_acp(Some(status));
                    self.emit_tool_result_event(
                        &update.id,
                        &update.fields.title.clone().unwrap_or_else(|| "tool".into()),
                        status_mapped,
                        update.fields.content.as_ref().and_then(|c| c.first()).map(|content| {
                            match content {
                                acp::ToolCallContent::Content { content } => {
                                    content_to_text(content)
                                }
                                acp::ToolCallContent::Diff { diff } => {
                                    format!("Edited {}", diff.path.display())
                                }
                                acp::ToolCallContent::Terminal { terminal_id } => {
                                    format!("Terminal {}", terminal_id.0)
                                }
                            }
                        }),
                    );
                }

                if matches!(update.fields.status, Some(acp::ToolCallStatus::Failed)) {
                    if let Some(acp::ToolCallContent::Content { content }) =
                        update.fields.content.as_ref().and_then(|c| c.first())
                    {
                        self.emit(AgentActivityRow::AgentThought {
                            thought: format!(
                                "Tool {} failed: {}",
                                update.id.0,
                                content_to_text(content)
                            ),
                        });
                    }
                }
            }
            acp::SessionUpdate::Plan(plan) => {
                let text = format_plan(&plan);
                self.emit(AgentActivityRow::AgentThought {
                    thought: text.clone(),
                });
                self.emit_task_event(TaskEvent::Thought {
                    thought: text,
                    reasoning: None,
                    ts: Utc::now(),
                });
            }
            acp::SessionUpdate::CurrentModeUpdate { current_mode_id } => {
                let thought = format!("Mode: {current_mode_id}");
                self.emit(AgentActivityRow::AgentThought {
                    thought: thought.clone(),
                });
                self.emit_task_event(TaskEvent::Log {
                    level: LogLevel::Info,
                    message: thought,
                    tool_execution_id: None,
                    ts: Utc::now(),
                });
            }
            acp::SessionUpdate::AvailableCommandsUpdate { .. } => {
                // Skip for now
            }
        }
        Ok(())
    }

    async fn ext_method(&self, _args: acp::ExtRequest) -> Result<acp::ExtResponse, acp::Error> {
        Err(acp::Error::method_not_found())
    }

    async fn ext_notification(&self, _args: acp::ExtNotification) -> Result<(), acp::Error> {
        Ok(())
    }
}

/// Build a recorder-wrapped command (`ah agent record --passthrough ...`) when
/// command-trace sockets are available in the environment. This mirrors the
/// shim rewrite described in Command-Execution-Tracing.md so ACP-driven tool
/// runs participate in the same tracing pipeline. Falls back to `None` when
/// required environment variables are missing.
fn build_passthrough_wrapped_command(
    args: &acp::CreateTerminalRequest,
    trace_env: Option<&CmdtraceEnv>,
) -> Option<Command> {
    // When the command already contains spaces (e.g., follower strings), avoid wrapping so the
    // shell-based fallback can execute the actual payload safely.
    if args.command.contains(' ') {
        return None;
    }

    let trace = trace_env.cloned().or_else(CmdtraceEnv::detect)?;

    let rendered_cmd = render_cmd_string(&args.command, &args.args);

    let mut cmd = Command::new(&trace.ah_path);
    cmd.arg("agent")
        .arg("record")
        .arg("--passthrough")
        .arg("--cmd")
        .arg(&rendered_cmd)
        .arg("--session-socket")
        .arg(&trace.session_socket);

    if let Some(parent) = trace.parent_socket.as_ref() {
        cmd.arg("--parent-recorder-socket").arg(parent);
    }

    cmd.arg("--").arg(&args.command).args(&args.args);

    if let Some(cwd) = &args.cwd {
        cmd.current_dir(cwd);
    }

    for env in &args.env {
        cmd.env(&env.name, &env.value);
    }

    // Prevent recursion if the passthrough shim is also injected downstream.
    cmd.env("AH_CMDTRACE_SKIP_REWRITE", "1");
    cmd.env("AH_CMDTRACE_AH_PATH", &trace.ah_path);
    cmd.env("AH_CMDTRACE_SESSION_SOCKET", &trace.session_socket);
    if let Some(parent) = &trace.parent_socket {
        cmd.env("AH_CMDTRACE_PARENT_SOCKET", parent);
    }

    Some(cmd)
}

/// Captured command-trace environment used to keep upstream/downstream tracing
/// on the same sockets when Harbor acts as both ACP client and server.
#[derive(Clone, Debug, PartialEq, Eq)]
struct CmdtraceEnv {
    session_socket: String,
    parent_socket: Option<String>,
    ah_path: String,
}

impl CmdtraceEnv {
    fn detect() -> Option<Self> {
        let session_socket = std::env::var("AH_CMDTRACE_SESSION_SOCKET").ok()?;
        if !Path::new(&session_socket).exists() {
            tracing::warn!(
                path = %session_socket,
                "AH_CMDTRACE_SESSION_SOCKET set but socket is missing; disabling passthrough"
            );
            return None;
        }
        let parent_socket = std::env::var("AH_CMDTRACE_PARENT_SOCKET").ok();
        let ah_path = std::env::var("AH_CMDTRACE_AH_PATH")
            .ok()
            .or_else(|| {
                std::env::current_exe().ok().and_then(|p| p.to_str().map(|s| s.to_string()))
            })
            .unwrap_or_else(|| "ah".to_string());

        Some(Self {
            session_socket,
            parent_socket,
            ah_path,
        })
    }

    fn apply_to(&self, cmd: &mut Command) {
        cmd.env("AH_CMDTRACE_SESSION_SOCKET", &self.session_socket);
        if let Some(parent) = &self.parent_socket {
            cmd.env("AH_CMDTRACE_PARENT_SOCKET", parent);
        }
        cmd.env("AH_CMDTRACE_AH_PATH", &self.ah_path);
        // Force passthrough rewrite so downstream launches share the recorder
        // sockets; the shim drops the flag for grandchildren after the first use.
        cmd.env("AH_CMDTRACE_PASSTHROUGH", "1");
    }
}

fn render_cmd_string(cmd: &str, args: &[String]) -> String {
    let mut parts = Vec::with_capacity(args.len() + 1);
    parts.push(shell_escape(cmd));
    parts.extend(args.iter().map(|a| shell_escape(a)));
    parts.join(" ")
}

fn shell_escape(input: &str) -> String {
    if input
        .chars()
        .all(|c| c.is_alphanumeric() || c == '-' || c == '_' || c == '.' || c == '/')
    {
        input.to_string()
    } else {
        let escaped = input.replace('\\', "\\\\").replace('"', "\\\"");
        format!("\"{}\"", escaped)
    }
}

fn log_level_to_session(level: &LogLevel) -> SessionLogLevel {
    match level {
        LogLevel::Error => SessionLogLevel::Error,
        LogLevel::Warn => SessionLogLevel::Warn,
        LogLevel::Info => SessionLogLevel::Info,
        LogLevel::Debug | LogLevel::Trace => SessionLogLevel::Debug,
    }
}

fn tool_status_to_session(status: ToolStatus) -> SessionToolStatus {
    match status {
        ToolStatus::Started => SessionToolStatus::Started,
        ToolStatus::Completed => SessionToolStatus::Completed,
        ToolStatus::Failed => SessionToolStatus::Failed,
    }
}

fn task_event_to_session_event(event: &TaskEvent) -> Option<SessionEvent> {
    let ts = match event {
        TaskEvent::Status { ts, .. }
        | TaskEvent::Log { ts, .. }
        | TaskEvent::Thought { ts, .. }
        | TaskEvent::ToolUse { ts, .. }
        | TaskEvent::ToolResult { ts, .. }
        | TaskEvent::FileEdit { ts, .. }
        | TaskEvent::UserInput { ts, .. } => ts.timestamp_millis() as u64,
    };
    match event {
        TaskEvent::Status { status, .. } => Some(SessionEvent::status((*status).into(), ts)),
        TaskEvent::Log {
            level,
            message,
            tool_execution_id,
            ..
        } => Some(SessionEvent::log(
            log_level_to_session(level),
            message.clone(),
            tool_execution_id.clone(),
            ts,
        )),
        TaskEvent::Thought {
            thought, reasoning, ..
        } => Some(SessionEvent::thought(
            thought.clone(),
            reasoning.clone(),
            ts,
        )),
        TaskEvent::ToolUse {
            tool_name,
            tool_args,
            tool_execution_id,
            status,
            ..
        } => {
            let args_str = serde_json::to_string(tool_args).unwrap_or_else(|_| "{}".into());
            Some(SessionEvent::tool_use(
                tool_name.clone(),
                args_str,
                tool_execution_id.clone(),
                tool_status_to_session(*status),
                ts,
            ))
        }
        TaskEvent::ToolResult {
            tool_name,
            tool_output,
            tool_execution_id,
            status,
            ..
        } => Some(SessionEvent::tool_result(
            tool_name.clone(),
            tool_output.clone(),
            tool_execution_id.clone(),
            tool_status_to_session(*status),
            ts,
        )),
        TaskEvent::FileEdit {
            file_path,
            lines_added,
            lines_removed,
            description,
            ..
        } => Some(SessionEvent::file_edit(
            file_path.clone(),
            *lines_added,
            *lines_removed,
            description.clone(),
            ts,
        )),
        TaskEvent::UserInput {
            author, content, ..
        } => Some(SessionEvent::thought(
            format!("{author}: {content}"),
            None,
            ts,
        )),
    }
}

async fn spawn_task_event_forwarder(
    socket_path: String,
    session_id: String,
) -> Option<tokio::sync::mpsc::UnboundedSender<TaskEvent>> {
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<TaskEvent>();

    tokio::spawn(async move {
        match UnixStream::connect(&socket_path).await {
            Ok(stream) => {
                let (mut reader, mut writer) = stream.into_split();
                let len = session_id.len() as u32;
                if writer.write_all(&len.to_le_bytes()).await.is_err()
                    || writer.write_all(session_id.as_bytes()).await.is_err()
                {
                    tracing::warn!("task-event forwarder failed to send session id handshake");
                    return;
                }

                tokio::spawn(async move {
                    loop {
                        match TaskManagerMessage::read_from(&mut reader).await {
                            Ok(_) => continue,
                            Err(err) => {
                                tracing::debug!(
                                    error = %err,
                                    "task-event forwarder reader ended"
                                );
                                break;
                            }
                        }
                    }
                });

                while let Some(event) = rx.recv().await {
                    if let Some(session_event) = task_event_to_session_event(&event) {
                        let msg = TaskManagerMessage::SessionEvent(session_event);
                        if let Err(err) = msg.write_to(&mut writer).await {
                            tracing::warn!(error = %err, "failed to forward task event");
                            break;
                        }
                    }
                }
            }
            Err(err) => {
                tracing::warn!(
                    path = %socket_path,
                    error = %err,
                    "task-event forwarder could not connect to task manager socket"
                );
            }
        }
    });

    Some(tx)
}

/// Run an ACP client session and replay the collected activity in the TUI.
pub async fn run_acp_client(config: AcpClientConfig) -> anyhow::Result<()> {
    // Avoid job-control stop signals when running in headless/test environments where the
    // process may not own the controlling TTY (e.g., `cargo test` with piped stdio).
    unsafe {
        libc::signal(libc::SIGTTOU, libc::SIG_IGN);
        libc::signal(libc::SIGTTIN, libc::SIG_IGN);
    }

    // Test/watchdog helper: when set, hard-exit after the given duration to
    // prevent harness timeouts while we stabilize ACP client lifecycle inside
    // the recorder. This is only enabled in tests via an explicit env var.
    if let Some(ms) = std::env::var("AH_ACP_CLIENT_TEST_WATCHDOG_MS")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
    {
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(ms));
            tracing::warn!(
                "AH_ACP_CLIENT_TEST_WATCHDOG_MS triggered at {}ms; forcing exit",
                ms
            );
            std::process::exit(0);
        });
    }

    let (activity_tx, activity_rx) = tokio::sync::mpsc::unbounded_channel::<AgentActivityRow>();
    let (prompt_tx, mut prompt_rx) = tokio::sync::mpsc::unbounded_channel::<String>();
    let (task_event_tx, mut task_event_rx) = tokio::sync::mpsc::unbounded_channel::<TaskEvent>();
    let (completion_tx, completion_rx) = tokio::sync::oneshot::channel::<()>();
    let mut child_cmd = Command::new(&config.acp_command.binary);
    let cmdtrace_env = CmdtraceEnv::detect();
    if cmdtrace_env.is_some() {
        tracing::debug!("command-trace passthrough detected; wrapping ACP child");
    }
    child_cmd
        .args(&config.acp_command.args)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .kill_on_drop(true);

    // Hybrid client/server tracing: propagate recorder sockets (and passthrough hint)
    // so the upstream ACP agent shares the same shim/recorder as downstream clients.
    if let Some(trace) = &cmdtrace_env {
        trace.apply_to(&mut child_cmd);
    }

    let mut child = child_cmd.spawn().with_context(|| {
        format!(
            "failed to spawn ACP binary {}",
            config.acp_command.binary.display()
        )
    })?;
    debug_note!(
        "spawned ACP child command: {} {:?}",
        config.acp_command.binary.display(),
        config.acp_command.args
    );

    let stdin = child.stdin.take().context("failed to take ACP child stdin")?.compat_write();
    let stdout = child.stdout.take().context("failed to take ACP child stdout")?.compat();

    let start = Instant::now();
    let client = UiClient {
        start,
        tx: activity_tx.clone(),
        terminals: Rc::new(RefCell::new(HashMap::new())),
        task_events: Rc::new(RefCell::new(None)),
        cmdtrace: cmdtrace_env.clone(),
        tool_completed: Rc::new(RefCell::new(false)),
    };
    let client_for_events = client.clone();
    let client_for_conn = client;
    client_for_events.attach_task_event_sender(task_event_tx);

    let run_viewer = std::io::stdout().is_terminal()
        && std::env::var("AH_TASK_MANAGER_SOCKET").is_err()
        && std::env::var("AH_RECORDER_IPC_SOCKET").is_err();
    let deps = if run_viewer {
        Some(AgentSessionDependencies {
            recording_terminal_state: Rc::new(std::cell::RefCell::new(
                TerminalState::new_with_scrollback(40, 120, 1_000_000),
            )),
            viewer_config: ViewerConfig {
                terminal_cols: 120,
                terminal_rows: 40,
                scrollback: 1_000_000,
                gutter: GutterConfig::default(),
                is_replay_mode: true,
            },
            task_manager: NullTaskManager::new(),
            autocomplete: None,
            settings: Settings::default(),
            theme: Theme::default(),
            terminal_config: TerminalConfig::minimal(),
            ui_mode: AgentSessionUiMode::AgentActivity,
            activity_entries: Vec::new(),
            live_activity_rx: Some(activity_rx),
            prompt_tx: Some(prompt_tx),
        })
    } else {
        None
    };

    let local = LocalSet::new();
    let run_future = local.run_until(async move {
        if let Some(deps) = deps {
            tokio::task::spawn_local(async move {
                if let Err(err) = run_session_viewer(deps).await {
                    tracing::warn!(error = %err, "Agent session viewer exited with error");
                }
            });
        }

        let (conn, io_fut) =
            acp::ClientSideConnection::new(client_for_conn, stdin, stdout, |fut| {
                tokio::task::spawn_local(fut);
            });
        tokio::task::spawn_local(io_fut);

        let conn = std::sync::Arc::new(conn);
        let mut stream_rx = conn.subscribe();
        let client_for_stream = client_for_events.clone();
        tokio::task::spawn_local({
            let client_for_notifications = client_for_stream.clone();
            async move {
                while let Ok(msg) = stream_rx.recv().await {
                    if msg.direction != StreamMessageDirection::Incoming {
                        continue;
                    }
                    if let StreamMessageContent::Notification { method, params } = msg.message {
                        if method.as_ref() == acp::CLIENT_METHOD_NAMES.session_update {
                            if let Some(raw) = params {
                                if let Ok(note) =
                                    serde_json::from_value::<acp::SessionNotification>(raw)
                                {
                                    let _ =
                                        client_for_notifications.session_notification(note).await;
                                }
                            }
                        }
                    }
                }
            }
        });

        conn.initialize(acp::InitializeRequest {
            protocol_version: acp::V1,
            client_capabilities: acp::ClientCapabilities::default(),
            meta: None,
        })
        .await?;
        debug_note!("initialize completed");

        let session = conn
            .new_session(acp::NewSessionRequest {
                mcp_servers: Vec::new(),
                cwd: std::env::current_dir()?,
                meta: None,
            })
            .await?;
        debug_note!("new_session completed: {}", session.session_id.0);

        let socket_session_id =
            std::env::var("AH_SESSION_ID").unwrap_or_else(|_| session.session_id.0.to_string());

        // Optional: forward TaskEvents to the task-manager socket when recording.
        let socket_forwarder = if let Ok(socket_path) = std::env::var("AH_TASK_MANAGER_SOCKET") {
            spawn_task_event_forwarder(socket_path, socket_session_id.clone()).await
        } else {
            None
        };

        // Fan-out TaskEvents to stdout (json-normalized) and, when available, to the
        // task-manager socket used by `ah agent record`. This keeps smoke tests deterministic
        // while preserving recorder parity.
        let mut completion_tx = Some(completion_tx);
        tokio::task::spawn_local(async move {
            while let Some(event) = task_event_rx.recv().await {
                if let Some(tx) = socket_forwarder.as_ref() {
                    let _ = tx.send(event.clone());
                }
                if let Ok(line) = serde_json::to_string(&event) {
                    let _ = writeln!(std::io::stdout(), "{}", line);
                }
                if matches!(event, TaskEvent::ToolResult { .. }) {
                    if let Some(tx) = completion_tx.take() {
                        let _ = tx.send(());
                    }
                }
            }
            if let Some(tx) = completion_tx.take() {
                let _ = tx.send(());
            }
        });

        if let Some(prompt) = &config.prompt {
            let _ = conn
                .prompt(acp::PromptRequest {
                    session_id: session.session_id.clone(),
                    prompt: vec![prompt.clone().into()],
                    meta: None,
                })
                .await;
            debug_note!("prompt sent");
        }

        // Interactive prompt input (stdin lines) forwarded to ACP.
        let conn_clone = std::sync::Arc::clone(&conn);
        let session_id = session.session_id.clone();
        tokio::task::spawn_local(async move {
            let mut reader = BufReader::new(tokio::io::stdin()).lines();
            while let Ok(Some(line)) = reader.next_line().await {
                let _ = conn_clone
                    .prompt(acp::PromptRequest {
                        session_id: session_id.clone(),
                        prompt: vec![line.into()],
                        meta: None,
                    })
                    .await;
            }
        });

        // Forward prompts coming from the TUI task entry to ACP.
        let conn_clone = std::sync::Arc::clone(&conn);
        let session_id_for_ui = session.session_id.clone();
        tokio::task::spawn_local(async move {
            while let Some(text) = prompt_rx.recv().await {
                let _ = conn_clone
                    .prompt(acp::PromptRequest {
                        session_id: session_id_for_ui.clone(),
                        prompt: vec![text.into()],
                        meta: None,
                    })
                    .await;
            }
        });

        // Wait for a completion signal from TaskEvents, but don't block indefinitely.
        let _ = timeout(Duration::from_secs(6), async {
            let _ = completion_rx.await;
        })
        .await;
        debug_note!("completion wait finished");

        // If we've seen a completed tool call, proactively request session
        // cancellation so agents that stay alive for additional prompts can
        // shut down cleanly (prevents the recorder wrapper from timing out).
        if *client_for_stream.tool_completed.borrow() {
            let _ = conn
                .cancel(acp::CancelNotification {
                    session_id: session.session_id.clone(),
                    meta: None,
                })
                .await;
        }

        // Give any spawned terminals a short grace period to report exits so
        // their final output makes it into the recording before we force-kill
        // the ACP child.
        let terminals_ref = client_for_stream.terminals.clone();
        let mut waited_ms = 0u64;
        while waited_ms < 2000 {
            let all_done = {
                let map = terminals_ref.borrow();
                map.values().all(|t| t.exit_status.is_some())
            };
            if all_done {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
            waited_ms += 50;
        }

        // Wait for the child to exit.
        let wait_res = timeout(Duration::from_secs(5), child.wait()).await;
        match wait_res {
            Ok(status) => {
                let _ = status?;
                debug_note!("child exited cleanly");
            }
            Err(_) => {
                tracing::warn!("ACP child did not exit within timeout; sending kill");
                let _ = child.start_kill();
                let _ = child.wait().await;
                debug_note!("child killed after timeout");
            }
        }
        // Drop sender to close the activity stream.
        drop(activity_tx);
        Ok::<(), anyhow::Error>(())
    });

    match timeout(Duration::from_secs(15), run_future).await {
        Ok(res) => res?,
        Err(_) => {
            tracing::warn!("acp client timed out; shutting down");
            return Ok(());
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use agent_client_protocol::Client;
    use std::sync::Mutex;
    use std::{cell::RefCell, rc::Rc};

    use tempfile::NamedTempFile;
    use tokio::io::AsyncReadExt;
    use tokio::sync::mpsc;

    static ENV_MUTEX: Mutex<()> = Mutex::new(());

    struct EnvRestore(Vec<(String, Option<String>)>);

    impl EnvRestore {
        fn capture(keys: &[&str]) -> Self {
            let captured = keys.iter().map(|k| ((*k).to_string(), std::env::var(k).ok())).collect();
            Self(captured)
        }
    }

    impl Drop for EnvRestore {
        fn drop(&mut self) {
            for (key, val) in self.0.drain(..) {
                match val {
                    Some(v) => std::env::set_var(&key, v),
                    None => std::env::remove_var(&key),
                }
            }
        }
    }

    fn sample_request() -> acp::CreateTerminalRequest {
        acp::CreateTerminalRequest {
            session_id: acp::SessionId("sess-1".into()),
            command: "python".into(),
            args: vec!["script.py".into(), "--flag".into()],
            cwd: Some(std::path::PathBuf::from("/tmp/work")),
            env: vec![acp::EnvVariable {
                name: "KEY".into(),
                value: "VAL".into(),
                meta: None,
            }],
            output_byte_limit: None,
            meta: None,
        }
    }

    #[test]
    fn passthrough_wrapper_builds_when_session_socket_set() {
        let _lock = ENV_MUTEX.lock().unwrap();
        let _guard = EnvRestore::capture(&["AH_CMDTRACE_SESSION_SOCKET", "AH_CMDTRACE_AH_PATH"]);
        let tmp = tempfile::tempdir().expect("tmpdir");
        let sock_path = tmp.path().join("session.sock");
        std::fs::write(&sock_path, b"").expect("touch socket placeholder");
        std::env::set_var("AH_CMDTRACE_SESSION_SOCKET", &sock_path);
        std::env::set_var("AH_CMDTRACE_AH_PATH", "fake-ah");

        let cmd = build_passthrough_wrapped_command(&sample_request(), None).expect("wrapped");
        let std_cmd = cmd.as_std();

        assert_eq!(std_cmd.get_program().to_string_lossy(), "fake-ah");

        let args: Vec<String> =
            std_cmd.get_args().map(|a| a.to_string_lossy().to_string()).collect();

        // Leading scaffold should match `ah agent record --passthrough --cmd <rendered> --session-socket ...`
        assert_eq!(args[0..3], ["agent", "record", "--passthrough"]);
        let sock_str = sock_path.to_string_lossy().to_string();
        assert!(
            args.windows(2).any(|win| win[0] == "--session-socket" && win[1] == sock_str),
            "expected --session-socket {} in args: {:?}",
            sock_str,
            args
        );

        let envs: Vec<(String, Option<String>)> = std_cmd
            .get_envs()
            .map(|(k, v)| {
                (
                    k.to_string_lossy().to_string(),
                    v.map(|v| v.to_string_lossy().to_string()),
                )
            })
            .collect();
        assert!(
            envs.iter()
                .any(|(k, v)| k == "AH_CMDTRACE_SKIP_REWRITE" && v.as_deref() == Some("1"))
        );
    }

    #[test]
    fn passthrough_wrapper_skips_without_session_socket() {
        let _lock = ENV_MUTEX.lock().unwrap();
        let _guard = EnvRestore::capture(&["AH_CMDTRACE_SESSION_SOCKET"]);
        std::env::remove_var("AH_CMDTRACE_SESSION_SOCKET");

        assert!(build_passthrough_wrapped_command(&sample_request(), None).is_none());
    }

    #[test]
    fn cmdtrace_env_applies_passthrough_flag() {
        let trace = CmdtraceEnv {
            session_socket: "/tmp/session.sock".into(),
            parent_socket: Some("/tmp/parent.sock".into()),
            ah_path: "/usr/bin/ah".into(),
        };

        let mut cmd = Command::new("echo");
        trace.apply_to(&mut cmd);

        let envs: HashMap<String, Option<String>> = cmd
            .as_std()
            .get_envs()
            .map(|(k, v)| {
                (
                    k.to_string_lossy().to_string(),
                    v.map(|v| v.to_string_lossy().to_string()),
                )
            })
            .collect();

        assert_eq!(
            envs.get("AH_CMDTRACE_SESSION_SOCKET"),
            Some(&Some("/tmp/session.sock".into()))
        );
        assert_eq!(
            envs.get("AH_CMDTRACE_PARENT_SOCKET"),
            Some(&Some("/tmp/parent.sock".into()))
        );
        assert_eq!(
            envs.get("AH_CMDTRACE_AH_PATH"),
            Some(&Some("/usr/bin/ah".into()))
        );
        assert_eq!(envs.get("AH_CMDTRACE_PASSTHROUGH"), Some(&Some("1".into())));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn tool_updates_emit_edits_and_status() {
        let (_tx, mut rx) = mpsc::unbounded_channel();
        let client = UiClient {
            start: Instant::now(),
            tx: _tx,
            terminals: Rc::new(RefCell::new(HashMap::new())),
            task_events: Rc::new(RefCell::new(None)),
            cmdtrace: None,
            tool_completed: Rc::new(RefCell::new(false)),
        };

        let diff = acp::Diff {
            path: PathBuf::from("src/lib.rs"),
            old_text: Some("fn a() {}\n".into()),
            new_text: "fn a() {}\nfn b() {}\n".into(),
            meta: None,
        };

        let note = acp::SessionNotification {
            session_id: acp::SessionId("s".into()),
            update: acp::SessionUpdate::ToolCallUpdate(acp::ToolCallUpdate {
                id: acp::ToolCallId("tool-1".into()),
                fields: acp::ToolCallUpdateFields {
                    title: Some("edit".into()),
                    status: Some(acp::ToolCallStatus::Completed),
                    content: Some(vec![acp::ToolCallContent::Diff { diff: diff.clone() }]),
                    ..Default::default()
                },
                meta: None,
            }),
            meta: None,
        };

        client.session_notification(note).await.unwrap();

        let mut rows = Vec::new();
        while let Ok(row) = rx.try_recv() {
            rows.push(row);
        }

        let log = NamedTempFile::new().expect("temp log");
        std::fs::write(log.path(), format!("{rows:#?}")).expect("write log");

        assert!(
            rows.iter().any(|row| matches!(
                row,
                AgentActivityRow::AgentEdit {
                    file_path,
                    ..
                } if file_path.ends_with("src/lib.rs")
            )),
            "expected AgentEdit row"
        );

        assert!(
            rows.iter().any(|row| matches!(
                row,
                AgentActivityRow::ToolUse {
                    tool_execution_id,
                    status: ToolStatus::Completed,
                    ..
                } if tool_execution_id == "tool-1"
            )),
            "expected ToolUse completion"
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn plan_updates_render_human_readable() {
        let (_tx, mut rx) = mpsc::unbounded_channel();
        let client = UiClient {
            start: Instant::now(),
            tx: _tx,
            terminals: Rc::new(RefCell::new(HashMap::new())),
            task_events: Rc::new(RefCell::new(None)),
            cmdtrace: None,
            tool_completed: Rc::new(RefCell::new(false)),
        };

        let plan = acp::Plan {
            entries: vec![
                acp::PlanEntry {
                    content: "Analyze codebase".into(),
                    priority: acp::PlanEntryPriority::High,
                    status: acp::PlanEntryStatus::Pending,
                    meta: None,
                },
                acp::PlanEntry {
                    content: "Write patch".into(),
                    priority: acp::PlanEntryPriority::Medium,
                    status: acp::PlanEntryStatus::InProgress,
                    meta: None,
                },
            ],
            meta: None,
        };

        let note = acp::SessionNotification {
            session_id: acp::SessionId("s".into()),
            update: acp::SessionUpdate::Plan(plan),
            meta: None,
        };

        client.session_notification(note).await.unwrap();
        let mut rows = Vec::new();
        while let Ok(row) = rx.try_recv() {
            rows.push(row);
        }

        let log = NamedTempFile::new().expect("temp log");
        std::fs::write(log.path(), format!("{rows:#?}")).expect("write log");

        let thought = rows.iter().find_map(|row| match row {
            AgentActivityRow::AgentThought { thought } => Some(thought.clone()),
            _ => None,
        });
        assert!(
            thought
                .as_deref()
                .map(|t| t.contains("Plan (2 steps)") && t.contains("Analyze"))
                .unwrap_or(false),
            "plan text should be human readable"
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn forwards_task_events_over_socket() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let sock_path = dir.path().join("tm.sock");
        let listener = tokio::net::UnixListener::bind(&sock_path).expect("bind");

        let sender =
            spawn_task_event_forwarder(sock_path.to_string_lossy().to_string(), "sess-42".into())
                .await
                .expect("sender");

        let (mut stream, _) = listener.accept().await.expect("accept");
        let mut len_buf = [0u8; 4];
        stream.read_exact(&mut len_buf).await.expect("len");
        let len = u32::from_le_bytes(len_buf) as usize;
        let mut id_buf = vec![0u8; len];
        stream.read_exact(&mut id_buf).await.expect("id");
        assert_eq!(String::from_utf8(id_buf).unwrap(), "sess-42");

        sender
            .send(TaskEvent::Thought {
                thought: "hello".into(),
                reasoning: None,
                ts: Utc::now(),
            })
            .expect("send");

        let msg = TaskManagerMessage::read_from(&mut stream).await.expect("message");
        match msg {
            TaskManagerMessage::SessionEvent(SessionEvent::Thought(ev)) => {
                assert_eq!(String::from_utf8(ev.thought).unwrap(), "hello");
            }
            other => panic!("unexpected message {other:?}"),
        }
    }

    #[test]
    fn cmdtrace_detect_requires_existing_socket() {
        let _lock = ENV_MUTEX.lock().unwrap();
        let _guard = EnvRestore::capture(&["AH_CMDTRACE_SESSION_SOCKET"]);
        std::env::set_var("AH_CMDTRACE_SESSION_SOCKET", "/tmp/missing.sock");
        assert!(
            CmdtraceEnv::detect().is_none(),
            "detect should fail when socket path is missing"
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn permission_requests_auto_allow() {
        let (_tx, _rx) = mpsc::unbounded_channel();
        let client = UiClient {
            start: Instant::now(),
            tx: _tx,
            terminals: Rc::new(RefCell::new(HashMap::new())),
            task_events: Rc::new(RefCell::new(None)),
            cmdtrace: None,
            tool_completed: Rc::new(RefCell::new(false)),
        };
        let resp = client
            .request_permission(acp::RequestPermissionRequest {
                session_id: acp::SessionId("s".into()),
                tool_call: acp::ToolCallUpdate {
                    id: acp::ToolCallId("tool-1".into()),
                    fields: Default::default(),
                    meta: None,
                },
                options: vec![],
                meta: None,
            })
            .await
            .expect("permission ok");
        match resp.outcome {
            acp::RequestPermissionOutcome::Selected { option_id } => {
                assert_eq!(option_id.0, "allow".into());
            }
            other => panic!("unexpected outcome {other:?}"),
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn read_write_roundtrip() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let file = dir.path().join("foo.txt");
        tokio::fs::write(&file, "hello").await.unwrap();

        let (_tx, _rx) = mpsc::unbounded_channel();
        let client = UiClient {
            start: Instant::now(),
            tx: _tx,
            terminals: Rc::new(RefCell::new(HashMap::new())),
            task_events: Rc::new(RefCell::new(None)),
            cmdtrace: None,
            tool_completed: Rc::new(RefCell::new(false)),
        };

        let read = client
            .read_text_file(acp::ReadTextFileRequest {
                session_id: acp::SessionId("s".into()),
                path: file.clone(),
                line: None,
                limit: None,
                meta: None,
            })
            .await
            .expect("read ok");
        assert_eq!(read.content, "hello");

        client
            .write_text_file(acp::WriteTextFileRequest {
                session_id: acp::SessionId("s".into()),
                path: file.clone(),
                content: "world".into(),
                meta: None,
            })
            .await
            .expect("write ok");

        let read2 = client
            .read_text_file(acp::ReadTextFileRequest {
                session_id: acp::SessionId("s".into()),
                path: file,
                line: None,
                limit: None,
                meta: None,
            })
            .await
            .expect("read2 ok");
        assert_eq!(read2.content, "world");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn terminal_output_is_exposed() {
        let local = tokio::task::LocalSet::new();
        local
            .run_until(async {
                let (_tx, _rx) = mpsc::unbounded_channel();
                let client = UiClient {
                    start: Instant::now(),
                    tx: _tx,
                    terminals: Rc::new(RefCell::new(HashMap::new())),
                    task_events: Rc::new(RefCell::new(None)),
                    cmdtrace: None,
                    tool_completed: Rc::new(RefCell::new(false)),
                };

                // Skip gracefully if we cannot spawn a shell in this environment.
                if tokio::process::Command::new("sh")
                    .arg("-c")
                    .arg("echo smoke")
                    .output()
                    .await
                    .is_err()
                {
                    return;
                }

                let req = acp::CreateTerminalRequest {
                    session_id: acp::SessionId("s".into()),
                    command: "sh".into(),
                    args: vec!["-c".into(), "printf 'acp-terminal\\n'".into()],
                    cwd: None,
                    env: vec![],
                    output_byte_limit: None,
                    meta: None,
                };

                let resp = client.create_terminal(req).await.expect("terminal create");
                let term_id = resp.terminal_id;

                let _ = client
                    .wait_for_terminal_exit(acp::WaitForTerminalExitRequest {
                        session_id: acp::SessionId("s".into()),
                        terminal_id: term_id.clone(),
                        meta: None,
                    })
                    .await
                    .expect("wait exit");

                tokio::task::yield_now().await;

                let output = client
                    .terminal_output(acp::TerminalOutputRequest {
                        session_id: acp::SessionId("s".into()),
                        terminal_id: term_id,
                        meta: None,
                    })
                    .await
                    .expect("terminal output");

                let log = NamedTempFile::new().expect("temp log");
                std::fs::write(log.path(), &output.output).expect("write log");
                assert!(
                    output.output.contains("acp-terminal"),
                    "terminal output should propagate"
                );
            })
            .await;
    }
}
