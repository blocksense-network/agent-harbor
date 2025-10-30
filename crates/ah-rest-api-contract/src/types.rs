// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! API contract types for the agent-harbor REST service

use ah_domain_types::{LogLevel, TaskState, ToolStatus};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use ssz::DecodeError;
use std::collections::HashMap;
use url::Url;
use validator::Validate;

/// Session status - SSZ-compatible version of TaskExecutionStatus for IPC communication
#[derive(
    Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ssz_derive::Encode, ssz_derive::Decode,
)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[serde(rename_all = "lowercase")]
#[ssz(enum_behaviour = "tag")]
pub enum SessionStatus {
    Queued,
    Provisioning,
    Running,
    Pausing,
    Paused,
    Resuming,
    Stopping,
    Stopped,
    Completed,
    Failed,
    Cancelled,
}

impl std::fmt::Display for SessionStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let status_str = match self {
            SessionStatus::Queued => "queued",
            SessionStatus::Provisioning => "provisioning",
            SessionStatus::Running => "running",
            SessionStatus::Pausing => "pausing",
            SessionStatus::Paused => "paused",
            SessionStatus::Resuming => "resuming",
            SessionStatus::Stopping => "stopping",
            SessionStatus::Stopped => "stopped",
            SessionStatus::Completed => "completed",
            SessionStatus::Failed => "failed",
            SessionStatus::Cancelled => "cancelled",
        };
        write!(f, "{}", status_str)
    }
}

impl From<ah_domain_types::TaskState> for SessionStatus {
    fn from(status: ah_domain_types::TaskState) -> Self {
        match status {
            ah_domain_types::TaskState::Queued => SessionStatus::Queued,
            ah_domain_types::TaskState::Provisioning => SessionStatus::Provisioning,
            ah_domain_types::TaskState::Running => SessionStatus::Running,
            ah_domain_types::TaskState::Pausing => SessionStatus::Pausing,
            ah_domain_types::TaskState::Paused => SessionStatus::Paused,
            ah_domain_types::TaskState::Resuming => SessionStatus::Resuming,
            ah_domain_types::TaskState::Stopping => SessionStatus::Stopping,
            ah_domain_types::TaskState::Stopped => SessionStatus::Stopped,
            ah_domain_types::TaskState::Completed => SessionStatus::Completed,
            ah_domain_types::TaskState::Failed => SessionStatus::Failed,
            ah_domain_types::TaskState::Cancelled => SessionStatus::Cancelled,
            // Note: Draft and Merged states don't have SessionStatus equivalents
            ah_domain_types::TaskState::Draft | ah_domain_types::TaskState::Merged => {
                // These shouldn't be converted to SessionStatus, but for completeness
                SessionStatus::Queued // Default fallback
            }
        }
    }
}

/// Session log level - SSZ-compatible version of LogLevel for IPC communication
#[derive(
    Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ssz_derive::Encode, ssz_derive::Decode,
)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[serde(rename_all = "lowercase")]
#[ssz(enum_behaviour = "tag")]
pub enum SessionLogLevel {
    Debug,
    Info,
    Warn,
    Error,
}

/// Session tool status - SSZ-compatible version of ToolStatus for IPC communication
#[derive(
    Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ssz_derive::Encode, ssz_derive::Decode,
)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[serde(rename_all = "lowercase")]
#[ssz(enum_behaviour = "tag")]
pub enum SessionToolStatus {
    Started,
    Completed,
    Failed,
}

/// Repository mode for task creation
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[serde(rename_all = "lowercase")]
pub enum RepoMode {
    Git,
    Upload,
    None,
}

/// Runtime type for task execution
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[serde(rename_all = "lowercase")]
pub enum RuntimeType {
    Devcontainer,
    Local,
    Disabled,
}

/// Delivery mode for task results
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[serde(rename_all = "lowercase")]
pub enum DeliveryMode {
    Pr,
    Branch,
    Patch,
}

/// Session event types for SSE streaming
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub enum EventType {
    Status,
    Log,
    Moment,
    Delivery,
    FenceStarted,
    FenceResult,
    HostStarted,
    HostLog,
    HostExited,
    Summary,
    FollowersCatalog,
    Note,
    Thought,
    ToolUse,
    ToolResult,
    FileEdit,
}

/// Repository configuration for task creation
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Validate)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct RepoConfig {
    pub mode: RepoMode,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<Url>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commit: Option<String>,
}

/// Runtime configuration for task execution
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Validate)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct RuntimeConfig {
    #[serde(rename = "type")]
    pub runtime_type: RuntimeType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub devcontainer_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resources: Option<ResourceLimits>,
}

/// Resource limits for runtime execution
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct ResourceLimits {
    pub cpu: u32,
    #[serde(rename = "memoryMiB")]
    pub memory_mib: u32,
}

/// Workspace configuration
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct WorkspaceConfig {
    #[serde(rename = "snapshotPreference")]
    pub snapshot_preference: Vec<String>,
    #[serde(rename = "executionHostId", skip_serializing_if = "Option::is_none")]
    pub execution_host_id: Option<String>,
}

/// Agent configuration
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct AgentConfig {
    #[serde(rename = "type")]
    pub agent_type: String,
    #[serde(default = "default_version")]
    pub version: String,
    #[serde(skip_serializing_if = "HashMap::is_empty", default)]
    pub settings: HashMap<String, serde_json::Value>,
}

fn default_version() -> String {
    "latest".to_string()
}

/// Delivery configuration
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct DeliveryConfig {
    pub mode: DeliveryMode,
    #[serde(rename = "targetBranch", skip_serializing_if = "Option::is_none")]
    pub target_branch: Option<String>,
}

/// Task creation request
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct CreateTaskRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tenant_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_id: Option<String>,
    #[validate(length(min = 1, message = "Prompt cannot be empty"))]
    pub prompt: String,
    #[validate(nested)]
    pub repo: RepoConfig,
    #[validate(nested)]
    pub runtime: RuntimeConfig,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace: Option<WorkspaceConfig>,
    #[validate(nested)]
    pub agent: AgentConfig,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delivery: Option<DeliveryConfig>,
    #[serde(skip_serializing_if = "HashMap::is_empty", default)]
    pub labels: HashMap<String, String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub webhooks: Vec<WebhookConfig>,
}

/// Webhook configuration for task events
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct WebhookConfig {
    pub event: String,
    pub url: Url,
}

/// Task creation response
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct CreateTaskResponse {
    #[serde(rename = "session_ids")]
    pub session_ids: Vec<String>,
    pub status: SessionStatus,
    pub links: TaskLinks,
}

/// Links for task/session resources
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct TaskLinks {
    #[serde(rename = "self")]
    pub self_link: String,
    pub events: String,
    pub logs: String,
}

/// Session information
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct Session {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tenant_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_id: Option<String>,
    pub task: TaskInfo,
    pub agent: AgentConfig,
    pub runtime: RuntimeConfig,
    pub workspace: WorkspaceInfo,
    pub vcs: VcsInfo,
    pub status: SessionStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ended_at: Option<DateTime<Utc>>,
    pub links: SessionLinks,
}

/// Task information within a session
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct TaskInfo {
    pub prompt: String,
    #[serde(skip_serializing_if = "HashMap::is_empty", default)]
    pub attachments: HashMap<String, String>,
    #[serde(skip_serializing_if = "HashMap::is_empty", default)]
    pub labels: HashMap<String, String>,
}

/// Workspace information
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct WorkspaceInfo {
    #[serde(rename = "snapshotProvider")]
    pub snapshot_provider: String,
    #[serde(rename = "mountPath")]
    pub mount_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,
    #[serde(
        rename = "devcontainerDetails",
        skip_serializing_if = "Option::is_none"
    )]
    pub devcontainer_details: Option<DevcontainerInfo>,
}

/// Devcontainer information
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct DevcontainerInfo {
    pub image: String,
    #[serde(rename = "containerId")]
    pub container_id: String,
    pub workspace_folder: String,
}

/// VCS information
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct VcsInfo {
    pub repo_url: Option<String>,
    pub branch: Option<String>,
    pub commit: Option<String>,
}

/// Links for session resources
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct SessionLinks {
    #[serde(rename = "self")]
    pub self_link: String,
    pub events: String,
    pub logs: String,
}

/// Session list response
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct SessionListResponse {
    pub items: Vec<Session>,
    #[serde(rename = "nextPage", skip_serializing_if = "Option::is_none")]
    pub next_page: Option<String>,
    pub total: Option<u32>,
}

// SSZ Union-based session events for IPC communication
// Using Vec<u8> for strings, u64 for timestamps, and proper enums as SSZ supports these types
// Each variant contains a single tuple/struct of SSZ-compatible types

/// Status change event
#[derive(
    Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ssz_derive::Encode, ssz_derive::Decode,
)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct SessionStatusEvent {
    /// Task execution status
    pub status: SessionStatus,
    /// Unix timestamp
    pub timestamp: u64,
}

/// Log message event
#[derive(
    Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ssz_derive::Encode, ssz_derive::Decode,
)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct SessionLogEvent {
    /// Log level
    pub level: SessionLogLevel,
    /// Log message
    pub message: Vec<u8>,
    /// Optional tool execution ID
    pub tool_execution_id: Option<Vec<u8>>,
    /// Unix timestamp
    pub timestamp: u64,
}

/// Agent error event
#[derive(
    Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ssz_derive::Encode, ssz_derive::Decode,
)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct SessionErrorEvent {
    /// Error message
    pub message: Vec<u8>,
    /// Unix timestamp
    pub timestamp: u64,
}

/// Agent thought/reasoning event
#[derive(
    Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ssz_derive::Encode, ssz_derive::Decode,
)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct SessionThoughtEvent {
    /// Thought content
    pub thought: Vec<u8>,
    /// Optional reasoning
    pub reasoning: Option<Vec<u8>>,
    /// Unix timestamp
    pub timestamp: u64,
}

/// Tool usage started event
#[derive(
    Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ssz_derive::Encode, ssz_derive::Decode,
)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct SessionToolUseEvent {
    /// Tool name
    pub tool_name: Vec<u8>,
    /// Tool arguments as JSON string
    pub tool_args: Vec<u8>,
    /// Tool execution ID
    pub tool_execution_id: Vec<u8>,
    /// Tool status
    pub status: SessionToolStatus,
    /// Unix timestamp
    pub timestamp: u64,
}

/// Tool execution completed event
#[derive(
    Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ssz_derive::Encode, ssz_derive::Decode,
)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct SessionToolResultEvent {
    /// Tool name
    pub tool_name: Vec<u8>,
    /// Tool output
    pub tool_output: Vec<u8>,
    /// Tool execution ID
    pub tool_execution_id: Vec<u8>,
    /// Tool status
    pub status: SessionToolStatus,
    /// Unix timestamp
    pub timestamp: u64,
}

/// File modification event
#[derive(
    Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ssz_derive::Encode, ssz_derive::Decode,
)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct SessionFileEditEvent {
    /// File path
    pub file_path: Vec<u8>,
    /// Lines added
    pub lines_added: usize,
    /// Lines removed
    pub lines_removed: usize,
    /// Optional description
    pub description: Option<Vec<u8>>,
    /// Unix timestamp
    pub timestamp: u64,
}

#[derive(
    Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ssz_derive::Encode, ssz_derive::Decode,
)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[ssz(enum_behaviour = "union")]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SessionEvent {
    /// Status change event
    Status(SessionStatusEvent),
    /// Log message event
    Log(SessionLogEvent),
    /// Agent error event
    Error(SessionErrorEvent),
    /// Agent thought/reasoning event
    Thought(SessionThoughtEvent),
    /// Tool usage started event
    ToolUse(SessionToolUseEvent),
    /// Tool execution completed event
    ToolResult(SessionToolResultEvent),
    /// File modification event
    FileEdit(SessionFileEditEvent),
}

// Constructors for SSZ union variants (convert high-level types to SSZ-compatible Vec<u8>/u64)
impl SessionEvent {
    /// Extract the timestamp from any variant
    pub fn timestamp(&self) -> u64 {
        match self {
            SessionEvent::Status(event) => event.timestamp,
            SessionEvent::Log(event) => event.timestamp,
            SessionEvent::Error(event) => event.timestamp,
            SessionEvent::Thought(event) => event.timestamp,
            SessionEvent::ToolUse(event) => event.timestamp,
            SessionEvent::ToolResult(event) => event.timestamp,
            SessionEvent::FileEdit(event) => event.timestamp,
        }
    }

    /// Create a status event
    pub fn status(status: SessionStatus, timestamp: u64) -> Self {
        Self::Status(SessionStatusEvent { status, timestamp })
    }

    /// Create a log event
    pub fn log(
        level: SessionLogLevel,
        message: String,
        tool_execution_id: Option<String>,
        timestamp: u64,
    ) -> Self {
        Self::Log(SessionLogEvent {
            level,
            message: message.into_bytes(),
            tool_execution_id: tool_execution_id.map(|s| s.into_bytes()),
            timestamp,
        })
    }

    /// Create an error event
    pub fn error(message: String, timestamp: u64) -> Self {
        Self::Error(SessionErrorEvent {
            message: message.into_bytes(),
            timestamp,
        })
    }

    /// Create a thought event
    pub fn thought(thought: String, reasoning: Option<String>, timestamp: u64) -> Self {
        Self::Thought(SessionThoughtEvent {
            thought: thought.into_bytes(),
            reasoning: reasoning.map(|s| s.into_bytes()),
            timestamp,
        })
    }

    /// Create a tool use event
    pub fn tool_use(
        tool_name: String,
        tool_args: String,
        tool_execution_id: String,
        status: SessionToolStatus,
        timestamp: u64,
    ) -> Self {
        Self::ToolUse(SessionToolUseEvent {
            tool_name: tool_name.into_bytes(),
            tool_args: tool_args.into_bytes(),
            tool_execution_id: tool_execution_id.into_bytes(),
            status,
            timestamp,
        })
    }

    /// Create a tool result event
    pub fn tool_result(
        tool_name: String,
        tool_output: String,
        tool_execution_id: String,
        status: SessionToolStatus,
        timestamp: u64,
    ) -> Self {
        Self::ToolResult(SessionToolResultEvent {
            tool_name: tool_name.into_bytes(),
            tool_output: tool_output.into_bytes(),
            tool_execution_id: tool_execution_id.into_bytes(),
            status,
            timestamp,
        })
    }

    /// Create a file edit event
    pub fn file_edit(
        file_path: String,
        lines_added: usize,
        lines_removed: usize,
        description: Option<String>,
        timestamp: u64,
    ) -> Self {
        Self::FileEdit(SessionFileEditEvent {
            file_path: file_path.into_bytes(),
            lines_added,
            lines_removed,
            description: description.map(|s| s.into_bytes()),
            timestamp,
        })
    }
}

/// Host result for fence operations
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct HostResult {
    pub state: String,
    #[serde(rename = "tookMs")]
    pub took_ms: u64,
}

/// Delivery information for session events
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct DeliveryInfo {
    pub mode: String,
    pub url: String,
}

/// Log entry for session logs
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct LogEntry {
    pub level: LogLevel,
    pub message: String,
    pub ts: DateTime<Utc>,
}

/// Session logs response
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct SessionLogsResponse {
    pub items: Vec<LogEntry>,
    #[serde(rename = "nextPage", skip_serializing_if = "Option::is_none")]
    pub next_page: Option<String>,
}

/// Agent capability information
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct AgentCapability {
    #[serde(rename = "type")]
    pub agent_type: String,
    pub versions: Vec<String>,
    #[serde(rename = "settingsSchemaRef", skip_serializing_if = "Option::is_none")]
    pub settings_schema_ref: Option<String>,
}

/// Runtime capability information
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct RuntimeCapability {
    #[serde(rename = "type")]
    pub runtime_type: RuntimeType,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub images: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub paths: Vec<String>,
    #[serde(
        rename = "sandboxProfiles",
        skip_serializing_if = "Vec::is_empty",
        default
    )]
    pub sandbox_profiles: Vec<String>,
}

/// Executor information
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct Executor {
    pub id: String,
    pub os: String,
    pub arch: String,
    #[serde(rename = "snapshotCapabilities")]
    pub snapshot_capabilities: Vec<String>,
    pub health: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub overlay: Option<OverlayInfo>,
}

/// Overlay information for executors
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct OverlayInfo {
    pub provider: String,
    pub address: String,
    #[serde(rename = "magicName")]
    pub magic_name: String,
    pub state: String,
}

/// Project information
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct Project {
    pub id: String,
    #[serde(rename = "displayName")]
    pub display_name: String,
    #[serde(rename = "lastUsedAt", skip_serializing_if = "Option::is_none")]
    pub last_used_at: Option<DateTime<Utc>>,
}

/// Repository information
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct Repository {
    pub id: String,
    #[serde(rename = "displayName")]
    pub display_name: String,
    #[serde(rename = "scmProvider")]
    pub scm_provider: String,
    #[serde(rename = "remoteUrl")]
    pub remote_url: Url,
    #[serde(rename = "defaultBranch")]
    pub default_branch: String,
    #[serde(rename = "lastUsedAt", skip_serializing_if = "Option::is_none")]
    pub last_used_at: Option<DateTime<Utc>>,
}

/// Branch information for repository branch listing
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct BranchInfo {
    /// Branch name
    pub name: String,
    /// Whether this is the default branch
    #[serde(rename = "isDefault")]
    pub is_default: bool,
    /// Last commit hash (optional)
    #[serde(rename = "lastCommit", skip_serializing_if = "Option::is_none")]
    pub last_commit: Option<String>,
}

/// Repository branches response
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct RepositoryBranchesResponse {
    /// Repository ID
    #[serde(rename = "repositoryId")]
    pub repository_id: String,
    /// List of branches
    pub branches: Vec<BranchInfo>,
}

/// File information for repository file listing
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct RepositoryFile {
    /// File path
    pub path: String,
    /// Additional file details (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

/// Repository files response
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct RepositoryFilesResponse {
    /// Repository ID
    #[serde(rename = "repositoryId")]
    pub repository_id: String,
    /// List of files
    pub files: Vec<RepositoryFile>,
}

/// Workspace summary
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct Workspace {
    pub id: String,
    pub status: String,
    #[serde(rename = "executorId")]
    pub executor_id: String,
    pub age: String,
    #[serde(rename = "lastActivity")]
    pub last_activity: DateTime<Utc>,
    #[serde(rename = "storageUsed")]
    pub storage_used: Option<String>,
    #[serde(rename = "taskHistory")]
    pub task_history: Vec<String>,
}

/// Session info response
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct SessionInfoResponse {
    pub id: String,
    pub status: SessionStatus,
    pub fleet: FleetInfo,
    pub endpoints: SessionEndpoints,
}

/// Fleet information for session
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct FleetInfo {
    pub leader: String,
    pub followers: Vec<FollowerInfo>,
}

/// Follower information
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct FollowerInfo {
    pub name: String,
    pub os: String,
    pub health: String,
}

/// Session endpoints
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct SessionEndpoints {
    pub events: String,
}

/// Control commands for sessions
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct SessionControlRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// Pagination query parameters
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct PaginationQuery {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page: Option<u32>,
    #[serde(rename = "perPage", skip_serializing_if = "Option::is_none")]
    pub per_page: Option<u32>,
}

/// Filtering query parameters
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct FilterQuery {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tenant_id: Option<String>,
}

/// Query parameters for session logs
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct LogQuery {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tail: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub level: Option<LogLevel>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub since: Option<DateTime<Utc>>,
}

/// Idempotency key for POST requests (ULID format)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct IdempotencyKey(pub String);
