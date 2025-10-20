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

    /// Get the command line for executing the agent
    ///
    /// Returns the command arguments that should be executed to run the agent.
    /// This follows the pattern where core traits provide commands to be executed,
    /// and thin wrappers handle the execution in different contexts (direct spawn,
    /// multiplexer, SSH, etc.).
    pub fn get_agent_command(
        &self,
        session_id: &str,
        agent_type: &str,
        prompt: &str,
        working_copy_mode: WorkingCopyMode,
        cwd: Option<&Path>,
        snapshot_id: Option<String>,
    ) -> Vec<String> {
        let mut agent_args = vec![
            "agent".to_string(),
            "start".to_string(),
            "--agent".to_string(),
            agent_type.to_string(),
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
            agent_args.push(prompt.to_string());
        }

        // Add config file to start command if specified
        if let Some(ref config_file) = self.config.config_file {
            agent_args.push("--config".to_string());
            agent_args.push(config_file.clone());
        }

        // Add workspace path if available
        if let Some(cwd) = cwd {
            agent_args.push("--cwd".to_string());
            agent_args.push(cwd.to_string_lossy().to_string());
        }

        // Construct the full command: ah agent record --session-id <id> -- <agent_args...>
        let mut cmd_parts = vec![
            "ah".to_string(),
            "agent".to_string(),
            "record".to_string(),
            "--session-id".to_string(),
            session_id.to_string(),
        ];

        // Add config file to record command if specified
        if let Some(ref config_file) = self.config.config_file {
            cmd_parts.push("--config".to_string());
            cmd_parts.push(config_file.clone());
        }

        // Add the agent start command as the command to record
        cmd_parts.push("--".to_string());
        cmd_parts.extend(agent_args);

        cmd_parts
    }

    /// Get the command line as a string for executing the agent
    ///
    /// Returns the full command line string that should be executed to run the agent.
    /// This is a convenience method that wraps get_agent_command and joins the arguments.
    pub fn get_agent_command_string(
        &self,
        session_id: &str,
        agent_type: &str,
        prompt: &str,
        working_copy_mode: WorkingCopyMode,
        cwd: Option<&Path>,
        snapshot_id: Option<String>,
    ) -> String {
        self.get_agent_command(
            session_id,
            agent_type,
            prompt,
            working_copy_mode,
            cwd,
            snapshot_id,
        )
        .join(" ")
    }

    /// Spawn the agent process using `ah agent record` wrapping `ah agent start`
    ///
    /// This method is kept for backward compatibility but is deprecated in favor
    /// of the pattern where core traits provide command lines and thin wrappers
    /// handle execution in different contexts.
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

            // Get the command arguments from the shared method
            let command_args = {
                // Create a temporary executor to get the command (since we can't call async methods in spawn)
                let temp_config = AgentExecutionConfig {
                    config_file: config_file.clone(),
                };
                let temp_executor = AgentExecutor::new(temp_config);
                temp_executor.get_agent_command(
                    &session_id,
                    &agent_type,
                    &prompt,
                    working_copy_mode,
                    cwd.as_deref(),
                    snapshot_id,
                )
            };

            info!("Executing command: {}", command_args.join(" "));

            if command_args.is_empty() {
                error!("Empty command arguments for session {}", session_id);
                return;
            }

            let mut cmd = Command::new(&command_args[0]);
            cmd.args(&command_args[1..]);

            match cmd.status() {
                Ok(status) => {
                    if status.success() {
                        info!(
                            "Agent process completed successfully for session {}",
                            session_id
                        );
                    } else {
                        error!(
                            "Agent process failed with exit code {:?} for session {}",
                            status.code(),
                            session_id
                        );
                    }
                }
                Err(e) => {
                    error!(
                        "Failed to spawn agent process for session {}: {}",
                        session_id, e
                    );
                }
            }
        });

        Ok(handle)
    }
}
