// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Shared ACP translation helpers used by the REST gateway, CLI access-point
//! bridge, and TUI clients.
//!
//! The goal is to keep a single, well-tested mapping between Harbor task/session
//! events and ACP `SessionUpdate` notifications so transports (WS/UDS/stdio/REST)
//! stay in sync. All translators attach the original event payload in `_meta`
//! to preserve fidelity for downstream consumers.

use agent_client_protocol::{
    ContentBlock, SessionId, SessionNotification, SessionUpdate, TextContent, ToolCall, ToolCallId,
    ToolCallStatus, ToolCallUpdate, ToolCallUpdateFields, ToolKind,
};
use ah_core::task_manager::TaskEvent;
use ah_domain_types::task::ToolStatus as DomainToolStatus;
use ah_rest_api_contract::{
    Session, SessionEvent, SessionFileEditEvent, SessionLogEvent, SessionStatus,
    SessionStatusEvent, SessionThoughtEvent, SessionToolResultEvent, SessionToolStatus,
    SessionToolUseEvent,
};
use serde_json::{Value, json};
use std::{
    fs, io,
    path::{Path, PathBuf},
};

/// Convert a REST `SessionEvent` into an ACP `SessionNotification`.
pub fn session_event_to_notification(
    session_id: &str,
    event: &SessionEvent,
) -> SessionNotification {
    let meta = Some(session_event_meta(event));
    let update = match event {
        SessionEvent::Status(evt) => status_update(evt),
        SessionEvent::Log(evt) => log_update(evt),
        SessionEvent::Error(evt) => {
            agent_chunk(&format!("ERROR: {}", bytes_to_string(&evt.message)))
        }
        SessionEvent::Thought(evt) => thought_update(evt),
        SessionEvent::ToolUse(evt) => tool_use_update(evt),
        SessionEvent::ToolResult(evt) => tool_result_update(evt),
        SessionEvent::FileEdit(evt) => file_edit_update(evt),
    };

    SessionNotification {
        session_id: SessionId(session_id.into()),
        update,
        meta,
    }
}

/// Convert a TaskManager `TaskEvent` (used by the TUI and mock task managers)
/// into an ACP notification. The mapping mirrors `session_event_to_notification`
/// but operates on higher-level string-based events.
pub fn task_event_to_notification(session_id: &str, event: &TaskEvent) -> SessionNotification {
    let (update, meta) = match event {
        TaskEvent::Status { status, ts } => (
            agent_chunk(&status.to_string()),
            json!({
                "type": "status",
                "status": status,
                "timestamp": ts.timestamp_millis(),
            }),
        ),
        TaskEvent::Log {
            level,
            message,
            tool_execution_id,
            ts,
        } => (
            agent_chunk(message),
            json!({
                "type": "log",
                "level": level,
                "message": message,
                "toolExecutionId": tool_execution_id,
                "timestamp": ts.timestamp_millis(),
            }),
        ),
        TaskEvent::Thought {
            thought,
            reasoning,
            ts,
        } => (
            SessionUpdate::AgentThoughtChunk {
                content: ContentBlock::Text(TextContent {
                    annotations: None,
                    text: reasoning
                        .as_ref()
                        .map(|r| format!("{thought}\n\n{r}"))
                        .unwrap_or_else(|| thought.clone()),
                    meta: None,
                }),
            },
            json!({
                "type": "thought",
                "text": thought,
                "reasoning": reasoning,
                "timestamp": ts.timestamp_millis(),
            }),
        ),
        TaskEvent::ToolUse {
            tool_name,
            tool_args,
            tool_execution_id,
            status,
            ts,
        } => (
            SessionUpdate::ToolCall(ToolCall {
                id: ToolCallId(tool_execution_id.clone().into()),
                title: tool_name.clone(),
                kind: ToolKind::Execute,
                status: tool_status_from_task(status),
                content: Vec::new(),
                locations: Vec::new(),
                raw_input: Some(tool_args.clone()),
                raw_output: None,
                meta: None,
            }),
            json!({
                "type": "tool_use",
                "toolName": tool_name,
                "executionId": tool_execution_id,
                "status": status_string_from_task(status),
                "args": tool_args,
                "timestamp": ts.timestamp_millis(),
            }),
        ),
        TaskEvent::ToolResult {
            tool_name: _,
            tool_output,
            tool_execution_id,
            status,
            ts,
        } => (
            SessionUpdate::ToolCallUpdate(ToolCallUpdate {
                id: ToolCallId(tool_execution_id.clone().into()),
                fields: ToolCallUpdateFields {
                    status: Some(tool_status_from_task(status)),
                    content: None,
                    locations: None,
                    raw_input: None,
                    raw_output: Some(parse_json_or_string(tool_output)),
                    title: None,
                    kind: None,
                },
                meta: None,
            }),
            json!({
                "type": "tool_result",
                "executionId": tool_execution_id,
                "status": status_string_from_task(status),
                "output": tool_output,
                "timestamp": ts.timestamp_millis(),
            }),
        ),
        TaskEvent::FileEdit {
            file_path,
            lines_added,
            lines_removed,
            description,
            ts,
        } => (
            agent_chunk(&format!(
                "[file_edit] {} (+{} -{})",
                file_path, lines_added, lines_removed
            )),
            json!({
                "type": "file_edit",
                "path": file_path,
                "added": lines_added,
                "removed": lines_removed,
                "description": description,
                "timestamp": ts.timestamp_millis(),
            }),
        ),
        TaskEvent::UserInput {
            author,
            content,
            ts,
        } => (
            agent_chunk(&format!("{author}: {content}")),
            json!({
                "type": "user_input",
                "author": author,
                "content": content,
                "timestamp": ts.timestamp_millis(),
            }),
        ),
    };

    SessionNotification {
        session_id: SessionId(session_id.into()),
        update,
        meta: Some(meta),
    }
}

/// Attempt to convert the metadata attached to a `SessionNotification` back
/// into a structured REST `SessionEvent`. This is primarily used by test
/// harnesses and TUI bridges that need to fan TaskEvents into components that
/// still consume `SessionEvent` types.
pub fn notification_to_session_event(notification: &SessionNotification) -> Option<SessionEvent> {
    notification
        .meta
        .as_ref()
        .and_then(|meta| serde_json::from_value::<SessionEvent>(meta.clone()).ok())
}

/// Build an ACP notification that communicates the current session status and
/// workspace metadata (used when loading an existing session).
pub fn session_snapshot_to_notification(session: &Session) -> SessionNotification {
    let meta = json!({
        "status": session.status,
        "workspace": {
            "mountPath": session.workspace.mount_path,
            "snapshotProvider": session.workspace.snapshot_provider,
            "readOnly": matches!(session.status, SessionStatus::Paused),
        },
        "_links": session.links,
    });

    SessionNotification {
        session_id: SessionId(session.id.clone().into()),
        update: agent_chunk(&session.status.to_string()),
        meta: Some(meta),
    }
}

/// Parse a raw JSON session/update payload (e.g., from legacy transports) into
/// a typed ACP notification. Returns `None` if no `sessionId` can be found.
pub fn value_to_session_notification(params: &Value) -> Option<SessionNotification> {
    let session_id =
        params.get("sessionId").and_then(|v| v.as_str()).unwrap_or_default().to_string();
    if session_id.is_empty() {
        return None;
    }

    if let Ok(event) = serde_json::from_value::<SessionEvent>(
        params.get("event").cloned().unwrap_or_else(|| params.clone()),
    ) {
        return Some(session_event_to_notification(&session_id, &event));
    }

    // Fallback: attempt to reuse the legacy stringly typed mapping.
    let meta = params.get("event").cloned().or_else(|| Some(params.clone()));
    let update = match params.get("event").and_then(|e| e.get("type")).and_then(|t| t.as_str()) {
        Some("thought") => {
            let text = params
                .get("event")
                .and_then(|e| e.get("text"))
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            SessionUpdate::AgentThoughtChunk {
                content: ContentBlock::Text(TextContent {
                    annotations: None,
                    text,
                    meta: None,
                }),
            }
        }
        Some("status") => {
            let status = params
                .get("event")
                .and_then(|e| e.get("status"))
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            agent_chunk(&status)
        }
        Some("log") => {
            let message = params
                .get("event")
                .and_then(|e| e.get("message"))
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            agent_chunk(&message)
        }
        Some("tool_use") => {
            let event = params.get("event").cloned().unwrap_or_default();
            let exec = event.get("executionId").and_then(|s| s.as_str()).unwrap_or_default();
            SessionUpdate::ToolCall(ToolCall {
                id: ToolCallId(exec.into()),
                title: event
                    .get("followerCommand")
                    .or_else(|| event.get("toolName"))
                    .and_then(|s| s.as_str())
                    .unwrap_or_default()
                    .to_string(),
                kind: ToolKind::Execute,
                status: ToolCallStatus::InProgress,
                content: Vec::new(),
                locations: Vec::new(),
                raw_input: event.get("args").cloned(),
                raw_output: None,
                meta: None,
            })
        }
        Some("tool_result") => {
            let event = params.get("event").cloned().unwrap_or_default();
            let exec = event.get("executionId").and_then(|s| s.as_str()).unwrap_or_default();
            let status = match event.get("status").and_then(|v| v.as_str()).unwrap_or_default() {
                "completed" => ToolCallStatus::Completed,
                "failed" => ToolCallStatus::Failed,
                _ => ToolCallStatus::InProgress,
            };
            SessionUpdate::ToolCallUpdate(ToolCallUpdate {
                id: ToolCallId(exec.into()),
                fields: ToolCallUpdateFields {
                    status: Some(status),
                    content: None,
                    locations: None,
                    raw_input: None,
                    raw_output: event.get("output").cloned(),
                    title: None,
                    kind: None,
                },
                meta: None,
            })
        }
        _ => {
            agent_chunk(&params.get("event").cloned().unwrap_or_else(|| params.clone()).to_string())
        }
    };

    Some(SessionNotification {
        session_id: SessionId(session_id.into()),
        update,
        meta,
    })
}

/// Wrap a `SessionNotification` into a JSON-RPC `session/update` envelope.
pub fn notification_envelope(notification: &SessionNotification) -> Value {
    let mut params = serde_json::to_value(notification).unwrap_or_else(|_| json!({}));

    // Preserve legacy `event` key for callers/tests that expect the raw event payload.
    if params.get("event").is_none() {
        if let Some(meta) = &notification.meta {
            if let Some(obj) = params.as_object_mut() {
                obj.insert("event".to_string(), meta.clone());
            }
        }
    }

    json!({
        "jsonrpc": "2.0",
        "method": "session/update",
        "params": params,
    })
}

fn status_update(evt: &SessionStatusEvent) -> SessionUpdate {
    agent_chunk(&evt.status.to_string())
}

fn log_update(evt: &SessionLogEvent) -> SessionUpdate {
    let level_prefix = format!("{:?}", evt.level).to_uppercase();
    let text = bytes_to_string(&evt.message);
    agent_chunk(&format!("[{level_prefix}] {text}"))
}

fn thought_update(evt: &SessionThoughtEvent) -> SessionUpdate {
    let combined = match evt.reasoning.as_ref() {
        Some(reasoning) => format!(
            "{}\n\n{}",
            bytes_to_string(&evt.thought),
            bytes_to_string(reasoning)
        ),
        None => bytes_to_string(&evt.thought),
    };
    SessionUpdate::AgentThoughtChunk {
        content: ContentBlock::Text(TextContent {
            annotations: None,
            text: combined,
            meta: None,
        }),
    }
}

fn tool_use_update(evt: &SessionToolUseEvent) -> SessionUpdate {
    SessionUpdate::ToolCall(ToolCall {
        id: ToolCallId(bytes_to_string(&evt.tool_execution_id).into()),
        title: bytes_to_string(&evt.tool_name),
        kind: ToolKind::Execute,
        status: tool_status(&evt.status),
        content: Vec::new(),
        locations: Vec::new(),
        raw_input: Some(parse_json_or_string(&bytes_to_string(&evt.tool_args))),
        raw_output: None,
        meta: None,
    })
}

fn tool_result_update(evt: &SessionToolResultEvent) -> SessionUpdate {
    SessionUpdate::ToolCallUpdate(ToolCallUpdate {
        id: ToolCallId(bytes_to_string(&evt.tool_execution_id).into()),
        fields: ToolCallUpdateFields {
            status: Some(tool_status(&evt.status)),
            content: None,
            locations: None,
            raw_input: None,
            raw_output: Some(parse_json_or_string(&bytes_to_string(&evt.tool_output))),
            title: None,
            kind: None,
        },
        meta: None,
    })
}

fn file_edit_update(evt: &SessionFileEditEvent) -> SessionUpdate {
    let path = bytes_to_string(&evt.file_path);
    let text = format!(
        "[file_edit] {} (+{} -{})",
        path, evt.lines_added, evt.lines_removed
    );
    agent_chunk(&text)
}

fn agent_chunk(text: &str) -> SessionUpdate {
    SessionUpdate::AgentMessageChunk {
        content: ContentBlock::Text(TextContent {
            annotations: None,
            text: text.to_string(),
            meta: None,
        }),
    }
}

fn session_event_meta(event: &SessionEvent) -> Value {
    match event {
        SessionEvent::Status(evt) => json!({
            "type": "status",
            "status": evt.status,
            "timestamp": evt.timestamp,
        }),
        SessionEvent::Log(evt) => json!({
            "type": "log",
            "level": evt.level,
            "message": bytes_to_string(&evt.message),
            "toolExecutionId": evt.tool_execution_id.as_ref().map(|id| bytes_to_string(id)),
            "timestamp": evt.timestamp,
        }),
        SessionEvent::Error(evt) => json!({
            "type": "error",
            "message": bytes_to_string(&evt.message),
            "timestamp": evt.timestamp,
        }),
        SessionEvent::Thought(evt) => json!({
            "type": "thought",
            "text": bytes_to_string(&evt.thought),
            "reasoning": evt.reasoning.as_ref().map(|r| bytes_to_string(r)),
            "timestamp": evt.timestamp,
        }),
        SessionEvent::ToolUse(evt) => json!({
            "type": "tool_use",
            "toolName": bytes_to_string(&evt.tool_name),
            "args": parse_json_or_string(&bytes_to_string(&evt.tool_args)),
            "executionId": bytes_to_string(&evt.tool_execution_id),
            "status": status_string(&evt.status),
            "timestamp": evt.timestamp,
        }),
        SessionEvent::ToolResult(evt) => json!({
            "type": "tool_result",
            "executionId": bytes_to_string(&evt.tool_execution_id),
            "status": status_string(&evt.status),
            "output": parse_json_or_string(&bytes_to_string(&evt.tool_output)),
            "timestamp": evt.timestamp,
        }),
        SessionEvent::FileEdit(evt) => json!({
            "type": "file_edit",
            "path": bytes_to_string(&evt.file_path),
            "added": evt.lines_added,
            "removed": evt.lines_removed,
            "description": evt.description.as_ref().map(|d| bytes_to_string(d)),
            "timestamp": evt.timestamp,
        }),
    }
}

fn bytes_to_string(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).to_string()
}

fn parse_json_or_string(text: &str) -> Value {
    serde_json::from_str(text).unwrap_or_else(|_| json!(text))
}

fn tool_status(status: &SessionToolStatus) -> ToolCallStatus {
    match status {
        SessionToolStatus::Started => ToolCallStatus::InProgress,
        SessionToolStatus::Completed => ToolCallStatus::Completed,
        SessionToolStatus::Failed => ToolCallStatus::Failed,
    }
}

fn status_string(status: &SessionToolStatus) -> &'static str {
    match status {
        SessionToolStatus::Started => "started",
        SessionToolStatus::Completed => "completed",
        SessionToolStatus::Failed => "failed",
    }
}

fn tool_status_from_task(status: &DomainToolStatus) -> ToolCallStatus {
    match status {
        DomainToolStatus::Started => ToolCallStatus::InProgress,
        DomainToolStatus::Completed => ToolCallStatus::Completed,
        DomainToolStatus::Failed => ToolCallStatus::Failed,
    }
}

fn status_string_from_task(status: &DomainToolStatus) -> &'static str {
    match status {
        DomainToolStatus::Started => "started",
        DomainToolStatus::Completed => "completed",
        DomainToolStatus::Failed => "failed",
    }
}

/// Platform-specific default path for the ACP access-point Unix-domain socket.
///
/// The path mirrors the discovery locations used by the CLI bridge so both
/// server and client agree on where to find the socket without extra
/// configuration. On non-Unix platforms the value is still computed but
/// callers should gate UDS usage behind `cfg(unix)`.
pub fn default_uds_path() -> PathBuf {
    #[cfg(target_os = "macos")]
    {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("Library")
            .join("Caches")
            .join("io.agentharbor")
            .join("acp.sock")
    }
    #[cfg(target_os = "windows")]
    {
        dirs::data_local_dir()
            .unwrap_or_else(|| PathBuf::from(r"C:\\tmp"))
            .join("AgentHarbor")
            .join("acp.sock")
    }
    #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
    {
        dirs::runtime_dir()
            .or_else(dirs::data_local_dir)
            .unwrap_or_else(|| PathBuf::from("/tmp"))
            .join("agentharbor")
            .join("acp.sock")
    }
}

/// Ensure the parent directory for a Unix socket exists with conservative
/// permissions. Callers can ignore errors when the directory already exists
/// with stricter permissions than requested.
pub fn ensure_uds_parent(path: &Path) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(parent, fs::Permissions::from_mode(0o700));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use ah_domain_types::task::TaskState;

    use super::*;

    #[test]
    fn tool_use_translation_includes_meta() {
        let event = SessionEvent::tool_use(
            "ls".into(),
            "--color=auto".into(),
            "exec-1".into(),
            SessionToolStatus::Started,
            1_700_000_000_000,
        );
        let notif = session_event_to_notification("sess-1", &event);
        assert_eq!(notif.session_id.0.as_ref(), "sess-1");
        assert!(matches!(notif.update, SessionUpdate::ToolCall(_)));
        let meta = notif.meta.unwrap();
        assert_eq!(
            meta.get("executionId").and_then(|v| v.as_str()),
            Some("exec-1")
        );
    }

    #[test]
    fn notification_envelope_wraps_jsonrpc() {
        let event = SessionEvent::status(SessionStatus::Running, 0);
        let notif = session_event_to_notification("abc", &event);
        let envelope = notification_envelope(&notif);
        assert_eq!(
            envelope.get("method").and_then(|m| m.as_str()),
            Some("session/update")
        );
    }

    #[test]
    fn notification_meta_roundtrips_to_session_event() {
        let task_evt = TaskEvent::Status {
            status: TaskState::Running,
            ts: chrono::Utc::now(),
        };
        let notif = task_event_to_notification("sess-1", &task_evt);
        let meta = notif.meta.clone().expect("meta expected");
        let parsed: SessionEvent = serde_json::from_value(meta.clone())
            .unwrap_or_else(|_| panic!("meta did not parse: {meta}"));
        assert!(notification_to_session_event(&notif).is_some());
        match parsed {
            SessionEvent::Status(ev) => {
                assert_eq!(ev.status, SessionStatus::Running);
            }
            _ => panic!("expected status event"),
        }
    }

    #[test]
    fn default_uds_path_uses_expected_filename() {
        let path = default_uds_path();
        assert_eq!(path.file_name().and_then(|s| s.to_str()), Some("acp.sock"));
    }
}
