/// Core traits and types for agent abstraction layer
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tokio::process::Child;

/// Agent launch configuration
#[derive(Debug, Clone)]
pub struct AgentLaunchConfig {
    /// Initial prompt for the agent
    pub prompt: String,

    /// Custom HOME directory for the agent (for environment isolation)
    pub home_dir: PathBuf,

    /// Whether to run in interactive mode
    pub interactive: bool,

    /// Whether to request JSON-formatted output (if agent supports it)
    pub json_output: bool,

    /// Optional custom API server URL for LLM requests
    pub api_server: Option<String>,

    /// Optional API key for authentication with the LLM service
    pub api_key: Option<String>,

    /// List of MCP (Model Context Protocol) servers to configure
    pub mcp_servers: Vec<String>,

    /// Additional environment variables to set
    pub env_vars: Vec<(String, String)>,

    /// Working directory for the agent
    pub working_dir: PathBuf,

    /// Whether to automatically copy credentials from system HOME to custom home_dir
    /// Only applies when home_dir differs from system HOME
    pub copy_credentials: bool,
}

/// Agent version information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentVersion {
    /// Version string (e.g., "2025.09.18-39624ef")
    pub version: String,

    /// Optional commit hash
    pub commit: Option<String>,

    /// Optional release date
    pub release_date: Option<String>,
}

/// Normalized API events from agent output
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AgentEvent {
    /// Agent is thinking/reasoning
    Thinking { content: String },

    /// Agent is using a tool
    ToolUse {
        tool_name: String,
        arguments: serde_json::Value,
    },

    /// Agent produced a log message
    Log { level: String, message: String },

    /// Agent produced output text
    Output { content: String },

    /// Agent encountered an error
    Error { message: String },

    /// Agent completed successfully
    Complete { summary: Option<String> },
}

/// Result type for agent operations
pub type AgentResult<T> = Result<T, AgentError>;

/// Errors that can occur during agent operations
#[derive(Debug, thiserror::Error)]
pub enum AgentError {
    #[error("Failed to spawn agent process: {0}")]
    ProcessSpawnFailed(#[from] std::io::Error),

    #[error("Agent not found in PATH: {0}")]
    AgentNotFound(String),

    #[error("Version detection failed: {0}")]
    VersionDetectionFailed(String),

    #[error("Unsupported agent version: {version} (expected {expected})")]
    UnsupportedVersion { version: String, expected: String },

    #[error("Credential copy failed: {0}")]
    CredentialCopyFailed(String),

    #[error("Session export failed: {0}")]
    SessionExportFailed(String),

    #[error("Session import failed: {0}")]
    SessionImportFailed(String),

    #[error("Output parsing failed: {0}")]
    OutputParsingFailed(String),

    #[error("Configuration error: {0}")]
    ConfigurationError(String),

    #[error("Config creation failed: {0}")]
    ConfigCreationFailed(String),

    #[error("Other error: {0}")]
    Other(#[from] anyhow::Error),
}

/// Core trait for agent execution and management
#[async_trait]
pub trait AgentExecutor: Send + Sync {
    /// Get the name of this agent (e.g., "claude", "codex")
    fn name(&self) -> &'static str;

    /// Detect the installed version of this agent
    async fn detect_version(&self) -> AgentResult<AgentVersion>;

    /// Prepare an agent launch by setting up the environment and returning a configured Command
    ///
    /// This method sets up the temporary HOME directory, copies credentials if needed,
    /// and configures all environment variables and command arguments, but does not spawn the process.
    /// The returned Command can then be used with `ah agent record` for session recording.
    ///
    /// Returns a configured Command ready to be spawned
    async fn prepare_launch(
        &self,
        config: AgentLaunchConfig,
    ) -> AgentResult<tokio::process::Command>;

    /// Launch the agent with the given configuration
    ///
    /// Returns a process handle that can be monitored for output
    async fn launch(&self, config: AgentLaunchConfig) -> AgentResult<Child> {
        let mut cmd = self.prepare_launch(config).await?;
        let child = cmd.spawn().map_err(AgentError::ProcessSpawnFailed)?;
        Ok(child)
    }

    /// Execute the agent by replacing the current process with the agent
    ///
    /// This replaces the current process image with the agent process using execve.
    /// Unlike launch(), this does not return and the current process is replaced.
    async fn exec(&self, config: AgentLaunchConfig) -> AgentResult<()> {
        let mut cmd = self.prepare_launch(config).await?;
        // Convert tokio command to std command and exec
        use std::os::unix::process::CommandExt;
        let err = cmd.as_std_mut().exec();
        // This should never return on success, but if it does, it's an error
        Err(AgentError::ProcessSpawnFailed(err))
    }

    /// Copy credentials from source HOME to destination HOME
    ///
    /// This allows setting up a custom HOME directory with authentication
    /// credentials copied from the user's actual home directory.
    async fn copy_credentials(&self, src_home: &Path, dst_home: &Path) -> AgentResult<()>;

    /// Export agent session from HOME directory to compressed archive
    ///
    /// Creates a tar.gz archive containing all agent state, config, and session files
    async fn export_session(&self, home_dir: &Path) -> AgentResult<PathBuf>;

    /// Import agent session from compressed archive to HOME directory
    ///
    /// Extracts archive contents to populate the agent's HOME directory
    async fn import_session(&self, session_archive: &Path, home_dir: &Path) -> AgentResult<()>;

    /// Parse raw agent output into normalized API events
    ///
    /// This is a streaming parser that processes chunks of output as they arrive
    fn parse_output(&self, raw_output: &[u8]) -> AgentResult<Vec<AgentEvent>>;

    /// Get the expected configuration directory path for this agent
    ///
    /// For example: ~/.cursor, ~/.config/crush, ~/.copilot
    fn config_dir(&self, home: &Path) -> PathBuf;

    /// Get the expected state/data directory path for this agent
    ///
    /// Some agents separate config and state; this returns the state directory
    fn state_dir(&self, home: &Path) -> PathBuf {
        // Default: same as config directory
        self.config_dir(home)
    }
}

/// Builder for launch configuration
impl AgentLaunchConfig {
    pub fn new(prompt: impl Into<String>, home_dir: impl Into<PathBuf>) -> Self {
        Self {
            prompt: prompt.into(),
            home_dir: home_dir.into(),
            interactive: false,
            json_output: false,
            api_server: None,
            api_key: None,
            mcp_servers: Vec::new(),
            env_vars: Vec::new(),
            working_dir: std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/")),
            copy_credentials: true, // Default to true for convenience
        }
    }

    pub fn interactive(mut self, interactive: bool) -> Self {
        self.interactive = interactive;
        self
    }

    pub fn json_output(mut self, json: bool) -> Self {
        self.json_output = json;
        self
    }

    pub fn api_server(mut self, url: impl Into<String>) -> Self {
        self.api_server = Some(url.into());
        self
    }

    pub fn api_key(mut self, key: impl Into<String>) -> Self {
        self.api_key = Some(key.into());
        self
    }

    pub fn mcp_server(mut self, server: impl Into<String>) -> Self {
        self.mcp_servers.push(server.into());
        self
    }

    pub fn env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.env_vars.push((key.into(), value.into()));
        self
    }

    pub fn working_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.working_dir = dir.into();
        self
    }

    pub fn copy_credentials(mut self, copy: bool) -> Self {
        self.copy_credentials = copy;
        self
    }
}
