// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Agent execution engine for spawning and managing agent processes
//!
//! This module provides the core functionality for spawning agent processes.
//! It is used by both REST server and local task managers for basic agent execution.

use anyhow::Result;
use std::path::Path;
use std::process::Command;
use tokio::task::JoinHandle;
use tracing::{debug, error, info};

/// Working copy mode for agent execution
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
    /// Whether recording is disabled (e.g., when fs-snapshots is set to disable)
    pub recording_disabled: bool,
}

/// Agent executor for spawning and managing agent processes
#[derive(Debug)]
pub struct AgentExecutor {
    config: AgentExecutionConfig,
}

pub fn ah_full_path() -> Result<String, String> {
    Ok(std::env::current_exe()
        .map_err(|e| format!("Failed to get current executable path: {}", e))?
        .canonicalize()
        .map_err(|e| format!("Failed to canonicalize executable path: {}", e))?
        .to_string_lossy()
        .to_string())
}

impl AgentExecutor {
    /// Create a new agent executor
    pub fn new(config: AgentExecutionConfig) -> Self {
        Self { config }
    }

    /// Get the configuration
    pub fn config(&self) -> &AgentExecutionConfig {
        &self.config
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
        model: &str,
        prompt: &str,
        working_copy_mode: WorkingCopyMode,
        cwd: Option<&Path>,
        snapshot_id: Option<String>,
        with_recording: bool,
        task_manager_socket_path: Option<&str>,
    ) -> Result<Vec<String>, String> {
        let exe_path = ah_full_path()?;

        let mut agent_args = vec![
            exe_path.clone(),
            "agent".to_string(),
            "start".to_string(),
            "--agent".to_string(),
            agent_type.to_string(),
            // "--non-interactive".to_string(),
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

        // Add model argument
        agent_args.push("--model".to_string());
        agent_args.push(model.to_string());

        // Add workspace path if available
        if let Some(cwd) = cwd {
            agent_args.push("--cwd".to_string());
            agent_args.push(cwd.to_string_lossy().to_string());
        }

        if with_recording {
            let socket_name = task_manager_socket_path.ok_or_else(|| {
                "task_manager_socket_path is required when with_recording is true".to_string()
            })?;

            // Get the full path to the current executable
            // Construct the full command: <exe_path> agent record --session-id <id> --task-manager-socket <path> -- <agent_args...>
            let mut cmd_parts = vec![
                exe_path,
                "agent".to_string(),
                "record".to_string(),
                "--session-id".to_string(),
                session_id.to_string(),
                "--task-manager-socket".to_string(),
                socket_name.to_string(),
            ];

            // Add config file to record command if specified
            if let Some(ref config_file) = self.config.config_file {
                cmd_parts.push("--config".to_string());
                cmd_parts.push(config_file.clone());
            }

            // Add the agent start command as the command to record
            cmd_parts.push("--".to_string());
            cmd_parts.extend(agent_args);

            Ok(cmd_parts)
        } else {
            Ok(agent_args)
        }
    }

    /// Get the command line as a string for executing the agent
    ///
    /// Returns the full command line string that should be executed to run the agent.
    /// This is a convenience method that wraps get_agent_command and joins the arguments.
    pub fn get_agent_command_string(
        &self,
        session_id: &str,
        agent_type: &str,
        model: &str,
        prompt: &str,
        working_copy_mode: WorkingCopyMode,
        cwd: Option<&Path>,
        snapshot_id: Option<String>,
        with_recording: bool,
        task_manager_socket_path: Option<&str>,
    ) -> Result<String, String> {
        let cmd_args = self.get_agent_command(
            session_id,
            agent_type,
            model,
            prompt,
            working_copy_mode,
            cwd,
            snapshot_id,
            with_recording,
            task_manager_socket_path,
        )?;

        Ok(cmd_args
            .into_iter()
            .map(|arg| Self::shell_escape(&arg))
            .collect::<Vec<_>>()
            .join(" "))
    }

    /// Escape a string for safe use in shell commands
    fn shell_escape(s: &str) -> String {
        // If the string contains no special characters, return as-is
        if s.chars()
            .all(|c| c.is_alphanumeric() || c == '-' || c == '_' || c == '.' || c == '/')
        {
            s.to_string()
        } else {
            // Escape double quotes and backslashes, then wrap in double quotes
            let escaped = s.replace('\\', "\\\\").replace('"', "\\\"");
            format!("\"{}\"", escaped)
        }
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
        model: &str,
        prompt: &str,
        working_copy_mode: WorkingCopyMode,
        cwd: Option<&Path>,
        snapshot_id: Option<String>,
    ) -> Result<JoinHandle<()>> {
        let session_id = session_id.to_string();
        let agent_type = agent_type.to_string();
        let model = model.to_string();
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
                    recording_disabled: false, // Not relevant for temp executor
                };
                let temp_executor = AgentExecutor::new(temp_config);
                match temp_executor.get_agent_command(
                    &session_id,
                    &agent_type,
                    &model,
                    &prompt,
                    working_copy_mode,
                    cwd.as_deref(),
                    snapshot_id,
                    true, // spawn_agent_process always uses recording
                    None, // spawn_agent_process doesn't need task manager socket
                ) {
                    Ok(args) => args,
                    Err(e) => {
                        error!("Failed to get agent command: {}", e);
                        return;
                    }
                }
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
