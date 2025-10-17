//! Agent execution engine for spawning and managing agent processes
//!
//! This module provides the core functionality for spawning agent processes.
//! It is used by both REST server and local task managers for basic agent execution.

use anyhow::Result;
use std::path::Path;
use std::process::Command;
use tokio::task::JoinHandle;
use tracing::{error, info};

/// Working copy mode for agent execution
#[derive(Debug, Clone, Copy)]
pub enum WorkingCopyMode {
    /// Use snapshots for efficient workspace management
    Snapshots,
    /// Work directly in place without snapshots
    InPlace,
}

/// Configuration for agent execution
#[derive(Debug, Clone)]
pub struct AgentExecutionConfig {
    /// Optional config file to pass to agent commands
    pub config_file: Option<String>,
}

/// Agent executor for spawning and managing agent processes
#[derive(Debug)]
pub struct AgentExecutor {
    config: AgentExecutionConfig,
}

impl AgentExecutor {
    /// Create a new agent executor
    pub fn new(config: AgentExecutionConfig) -> Self {
        Self { config }
    }

    /// Spawn the agent process using `ah agent record` wrapping `ah agent start`
    pub async fn spawn_agent_process(
        &self,
        session_id: &str,
        agent_type: &str,
        prompt: &str,
        working_copy_mode: WorkingCopyMode,
        cwd: Option<&Path>,
        snapshot_id: Option<String>,
    ) -> Result<JoinHandle<()>> {
        let session_id = session_id.to_string();
        let agent_type = agent_type.to_string();
        let prompt = prompt.to_string();
        let cwd = cwd.map(|p| p.to_path_buf());
        let config_file = self.config.config_file.clone();

        // Build the command: ah agent record --session-id <id> -- <agent_command>
        let handle = tokio::spawn(async move {
            info!("Spawning agent process for session {}", session_id);

            // Construct the agent start command with appropriate arguments
            let mut agent_args = vec![
                "agent".to_string(),
                "start".to_string(),
                "--agent".to_string(),
                agent_type,
                "--non-interactive".to_string(),
            ];

            // Add working copy mode
            match working_copy_mode {
                WorkingCopyMode::Snapshots => {
                    agent_args.push("--working-copy".to_string());
                    agent_args.push("snapshots".to_string());
                }
                WorkingCopyMode::InPlace => {
                    agent_args.push("--working-copy".to_string());
                    agent_args.push("in-place".to_string());
                }
            }

            // Add either --from-snapshot or --prompt based on whether we have a cached snapshot
            if let Some(snapshot_id) = &snapshot_id {
                agent_args.push("--from-snapshot".to_string());
                agent_args.push(snapshot_id.clone());
            } else {
                agent_args.push("--prompt".to_string());
                agent_args.push(prompt);
            }

            // Add config file to start command if specified
            if let Some(ref config_file) = config_file {
                agent_args.push("--config".to_string());
                agent_args.push(config_file.clone());
            }

            // Add workspace path if available
            if let Some(cwd) = cwd {
                agent_args.push("--cwd".to_string());
                agent_args.push(cwd.to_string_lossy().to_string());
            }

            // Construct the full command: ah agent record --session-id <id> -- <agent_args...>
            let mut cmd = Command::new("ah");
            cmd.arg("agent")
                .arg("record")
                .arg("--session-id")
                .arg(&session_id);

            // Add config file to record command if specified
            if let Some(ref config_file) = config_file {
                cmd.arg("--config").arg(config_file);
            }

            // Add the agent start command as the command to record
            cmd.arg("--").args(&agent_args);

            info!("Executing command: ah agent record --session-id {} -- {:?}", session_id, agent_args);

            match cmd.status() {
                Ok(status) => {
                    if status.success() {
                        info!("Agent process completed successfully for session {}", session_id);
                    } else {
                        error!("Agent process failed with exit code {:?} for session {}", status.code(), session_id);
                    }
                }
                Err(e) => {
                    error!("Failed to spawn agent process for session {}: {}", session_id, e);
                }
            }
        });

        Ok(handle)
    }
}
