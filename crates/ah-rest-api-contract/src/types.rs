//! API contract types for the agent-harbor REST service

use ah_domain_types::{LogLevel, TaskExecutionStatus};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use url::Url;
use validator::Validate;

/// Session lifecycle states - alias for TaskExecutionStatus
pub type SessionStatus = TaskExecutionStatus;

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
    pub id: String,
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

/// Session event for SSE streaming
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct SessionEvent {
    #[serde(rename = "type")]
    pub event_type: EventType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<SessionStatus>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub level: Option<LogLevel>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snapshot_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hosts: Option<HashMap<String, HostResult>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub passed: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failed: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delivery: Option<DeliveryInfo>,
    // Agent activity event fields
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thought: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_args: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_output: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_execution_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lines_added: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lines_removed: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub ts: DateTime<Utc>,
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
