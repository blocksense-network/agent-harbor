// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Agent execution engine for spawning and managing agent processes
//!
//! This module provides the core functionality for spawning agent processes.
//! It is used by both REST server and local task managers for basic agent execution.

use ah_local_db::Database;
use anyhow::Result;
use chrono::{Datelike, Utc};
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;
use tokio::task::JoinHandle;
use tracing::{error, info};

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

    /// Get the recordings directory path based on AH_HOME or platform defaults.
    ///
    /// This follows the same logic as the database path (see State-Persistence.md):
    /// Uses the same base directory as the database and appends "recordings/".
    /// - AH_HOME environment variable (custom)
    /// - Platform-specific defaults:
    ///   - Linux: `${XDG_STATE_HOME:-~/.local/state}/agent-harbor/recordings/`
    ///   - macOS: `~/Library/Application Support/agent-harbor/recordings/`
    ///   - Windows: `%LOCALAPPDATA%\agent-harbor\recordings\`
    fn get_recordings_dir() -> Result<PathBuf> {
        Ok(Database::default_base_dir()?.join("recordings"))
    }

    /// Get the command line for executing the agent
    ///
    /// Returns the command arguments that should be executed to run the agent.
    /// This follows the pattern where core traits provide commands to be executed,
    /// and thin wrappers handle the execution in different contexts (direct spawn,
    /// multiplexer, SSH, etc.).
    ///
    /// # Recording Parameters
    ///
    /// - `with_recording`: Whether to wrap the agent command with `ah agent record`
    ///   for capturing output and providing session viewer UI
    /// - `persist_recording`: Whether to save the recording to a file on disk
    ///   (only effective when `with_recording` is true)
    #[allow(clippy::too_many_arguments)]
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
        persist_recording: bool,
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
            let _socket_name = task_manager_socket_path.ok_or_else(|| {
                "task_manager_socket_path is required when with_recording is true".to_string()
            })?;

            // Get the full path to the current executable
            // Construct the full command: <exe_path> agent record --session-id <id> --task-manager-socket <path> --out-file <path> -- <agent_args...>
            let mut cmd_parts = vec![
                exe_path,
                "agent".to_string(),
                "record".to_string(),
                "--session-id".to_string(),
                session_id.to_string(),
                // TODO(zah): Restore this option once it works properly
                // The reason for commenting this out is that when using zellij,
                // we experience issues with the rendering of the initial TUI dashboard
                // "--task-manager-socket".to_string(),
                // socket_name.to_string(),
            ];

            // Add --out-file parameter with the default recordings path only when persistence is requested
            if persist_recording {
                let recordings_dir = Self::get_recordings_dir().map_err(|e| e.to_string())?;
                let now = Utc::now();
                let year_month = format!("{:04}/{:02}", now.year(), now.month());
                let recording_path =
                    recordings_dir.join(year_month).join(format!("{}.ahr", session_id));
                cmd_parts.push("--out-file".to_string());
                cmd_parts.push(recording_path.to_string_lossy().to_string());
            }

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
    #[allow(clippy::too_many_arguments)]
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
        persist_recording: bool,
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
            persist_recording,
            task_manager_socket_path,
        )?;

        Ok(cmd_args
            .into_iter()
            .map(|arg| Self::shell_escape(&arg))
            .collect::<Vec<_>>()
            .join(" "))
    }

    /// Get the full agent command with advanced launch options
    pub fn get_agent_command_with_options(
        &self,
        params: &crate::task_manager::TaskLaunchParams,
        session_id: &str,
        cwd: Option<&Path>,
        snapshot_id: Option<String>,
        task_manager_socket_path: Option<&str>,
    ) -> Result<Vec<String>, String> {
        let model = &params.models()[0]; // We validated there's at least one model
        let mut agent_args = self.get_agent_command(
            session_id,
            model.agent.software.cli_arg(),
            &model.model,
            params.description(),
            *params.working_copy_mode(),
            cwd,
            snapshot_id,
            params.record(),
            params.record() && params.record_output().unwrap_or(false),
            task_manager_socket_path,
        )?;

        // Add advanced launch options that are actually supported by the CLI

        // Web search capability
        if let Some(allow_web_search) = params.allow_web_search() {
            if allow_web_search {
                agent_args.push("--allow-web-search".to_string());
            }
        }

        // Interactive mode (inverse of non-interactive)
        if let Some(interactive_mode) = params.interactive_mode() {
            if !interactive_mode {
                agent_args.push("--non-interactive".to_string());
            }
        }

        // Output format (maps to --output flag)
        if let Some(output_format) = params.output_format() {
            if output_format == "text-normalized" {
                agent_args.push("--output".to_string());
                agent_args.push("text-normalized".to_string());
            } else if output_format == "json" {
                agent_args.push("--output".to_string());
                agent_args.push("json".to_string());
            } else if output_format == "json-normalized" {
                agent_args.push("--output".to_string());
                agent_args.push("json-normalized".to_string());
            }
            // "text" is the default, so no flag needed
        }

        // LLM provider/API settings
        if let Some(llm_provider) = params.llm_provider() {
            if !llm_provider.is_empty() {
                agent_args.push("--llm-api".to_string());
                agent_args.push(llm_provider.to_string());
            }
        }

        // Environment variables (passed via --agent-flags as KEY=VALUE)
        if let Some(env_vars) = params.environment_variables() {
            if !env_vars.is_empty() {
                // Add environment variables as agent flags
                for (key, value) in env_vars {
                    // Use --agent-flags to pass environment variables
                    // This is a bit of a hack, but it's how the CLI currently handles env vars
                    agent_args.push("--agent-flags".to_string());
                    agent_args.push(format!("{}={}", key, value));
                }
            }
        }

        // Sandbox settings (only basic sandbox support exists)
        if let Some(sandbox_profile) = params.sandbox_profile() {
            if sandbox_profile == "disabled" {
                // No sandbox - don't add any sandbox flags
            } else if !sandbox_profile.is_empty() && sandbox_profile != "local" {
                // Enable sandbox with custom type
                agent_args.push("--sandbox".to_string());
                agent_args.push("--sandbox-type".to_string());
                agent_args.push(sandbox_profile.to_string());
            } else {
                // Enable basic sandbox
                agent_args.push("--sandbox".to_string());
            }
        }

        // Container and KVM permissions (map to existing flags)
        if let Some(allow_containers) = params.allow_containers() {
            if allow_containers {
                agent_args.push("--allow-containers".to_string());
            }
        }

        if let Some(allow_vms) = params.allow_vms() {
            if allow_vms {
                agent_args.push("--allow-kvm".to_string());
            }
        }

        // Network access (maps to allow-network)
        if let Some(allow_egress) = params.allow_egress() {
            if allow_egress {
                agent_args.push("--allow-network".to_string());
            }
        }

        // TODO: Add support for the following options when CLI flags are implemented:
        // - devcontainer_path: --devcontainer-path (devcontainer integration)
        // - fs_snapshots: --fs-snapshots (filesystem snapshot provider selection)
        // - record_output: --no-record-output (disable output recording)
        // - timeout: --timeout (execution timeout setting)
        // - delivery_method: --delivery-method (PR/branch/patch delivery)
        // - target_branch: --target-branch (target branch for delivery)
        // - create_task_files: --no-create-task-files (disable task file creation)
        // - create_metadata_commits: --no-create-metadata-commits (disable metadata commits)
        // - notifications: --notifications (enable notifications)
        // - labels: --label KEY=VALUE (task labeling)
        // - fleet: --fleet (fleet selection for distributed execution)

        Ok(agent_args)
    }

    /// Escape a string for safe use in shell commands
    pub fn shell_escape(s: &str) -> String {
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
    #[allow(clippy::too_many_arguments)]
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
                    true, // spawn_agent_process always persists recording
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
