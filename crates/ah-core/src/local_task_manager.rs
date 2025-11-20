// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Local Task Manager - Direct Task Execution
//!
//! This module provides the LocalTaskManager implementation that executes tasks
//! directly on the local machine without snapshot caching. It's designed for
//! local development and testing scenarios where users want immediate agent execution.
//!
//! Tasks are executed through the configured multiplexer (tmux, kitty, etc.) to provide
//! proper terminal window management and session isolation.

use crate::db::DatabaseManager;
use crate::task_manager::{
    SaveDraftResult, TaskEvent, TaskLaunchParams, TaskLaunchResult, TaskManager,
};
use ah_domain_types::{AgentChoice, LogLevel, TaskExecution, TaskInfo, TaskState, ToolStatus};
use ah_local_db::models::DraftRecord;
use ah_mux_core::Multiplexer;
use ah_tui_multiplexer::{AwMultiplexer, LayoutConfig};
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::io::AsyncReadExt;
use tokio::net::UnixListener;
use tokio::net::UnixStream;
use tokio::sync::broadcast;

/// Generic Local task manager implementation that executes tasks through a multiplexer
///
/// This implementation runs agents directly from the current filesystem state,
/// without the snapshot caching complexity used by the server. Tasks are executed
/// through the configured multiplexer (tmux, kitty, etc.) to provide proper terminal
/// window management and session isolation.
pub struct GenericLocalTaskManager<M: Multiplexer + Send + Sync + 'static> {
    agent_executor: std::sync::Arc<crate::AgentExecutor>,
    db_manager: DatabaseManager,
    multiplexer: AwMultiplexer<M>,
    /// Single shared socket listener for all recording tasks
    shared_listener: Arc<Mutex<Option<Arc<UnixListener>>>>,
    /// Broadcast channels for distributing events to task subscribers
    event_senders: Arc<Mutex<HashMap<String, broadcast::Sender<TaskEvent>>>>,
    /// Flag to track if the accept loop is running
    accept_loop_running: Arc<Mutex<bool>>,
    /// Handle to the accept loop task
    accept_loop_handle: Arc<Mutex<Option<tokio::task::JoinHandle<()>>>>,
}

impl<M> GenericLocalTaskManager<M>
where
    M: Multiplexer + Send + Sync + 'static,
{
    /// Create a new generic local task manager with the specified multiplexer
    pub fn new(config: crate::AgentExecutionConfig, multiplexer: M) -> anyhow::Result<Self> {
        let agent_executor = std::sync::Arc::new(crate::AgentExecutor::new(config));
        let db_manager = DatabaseManager::new()?;
        let multiplexer = AwMultiplexer::new(multiplexer);
        let shared_listener = Arc::new(Mutex::new(None));
        let event_senders = Arc::new(Mutex::new(HashMap::new()));
        let accept_loop_running = Arc::new(Mutex::new(false));
        let accept_loop_handle = Arc::new(Mutex::new(None));
        Ok(Self {
            agent_executor,
            db_manager,
            multiplexer,
            shared_listener,
            event_senders,
            accept_loop_running,
            accept_loop_handle,
        })
    }

    /// Get a clone of the database manager
    pub fn db_manager(&self) -> DatabaseManager {
        self.db_manager.clone()
    }

    /// Ensure the shared listener is created and the accept loop is running
    fn ensure_shared_listener(&self) -> anyhow::Result<()> {
        let mut accept_loop_running = self.accept_loop_running.lock().unwrap();

        if *accept_loop_running {
            // Already running, nothing to do
            return Ok(());
        }

        let socket_path = task_manager_socket_path();
        tracing::debug!(
            "Creating task manager socket listener: {}",
            socket_path.display()
        );

        // Clean up any stale socket file
        let _ = std::fs::remove_file(&socket_path);

        // Create the shared Unix socket listener
        let listener = UnixListener::bind(&socket_path).map_err(|e| {
            anyhow::anyhow!(
                "Failed to bind task manager socket {}: {}",
                socket_path.display(),
                e
            )
        })?;

        // Store the listener
        {
            let mut shared_listener = self.shared_listener.lock().unwrap();
            *shared_listener = Some(Arc::new(listener));
        }

        // Mark as running and start the accept loop
        *accept_loop_running = true;

        // Clone the necessary Arcs for the accept loop
        let shared_listener = Arc::clone(&self.shared_listener);
        let event_senders = Arc::clone(&self.event_senders);
        let accept_loop_running = Arc::clone(&self.accept_loop_running);

        // Start the accept loop
        let handle = tokio::spawn(async move {
            Self::run_accept_loop(shared_listener, event_senders, accept_loop_running).await;
        });

        // Store the handle
        {
            let mut accept_loop_handle = self.accept_loop_handle.lock().unwrap();
            *accept_loop_handle = Some(handle);
        }

        Ok(())
    }

    /// Run the accept loop that handles all incoming recorder connections
    async fn run_accept_loop(
        shared_listener: Arc<Mutex<Option<Arc<UnixListener>>>>,
        event_senders: Arc<Mutex<HashMap<String, broadcast::Sender<TaskEvent>>>>,
        accept_loop_running: Arc<Mutex<bool>>,
    ) {
        // For simplicity, accept connections sequentially (not concurrently)
        // This can be optimized later if concurrent connection handling becomes necessary

        while *accept_loop_running.lock().unwrap() {
            // Clone the listener Arc so we can use it without holding the lock
            let listener_opt = {
                let guard = shared_listener.lock().unwrap();
                guard.as_ref().map(Arc::clone)
            };

            let stream_result = if let Some(listener) = listener_opt {
                Some(listener.accept().await)
            } else {
                None
            };

            match stream_result {
                Some(Ok((stream, _addr))) => {
                    tracing::debug!("Accepted recorder connection");

                    // Clone the event_senders for the connection handler
                    let event_senders = Arc::clone(&event_senders);

                    // Handle this connection (sequentially for now)
                    Self::handle_recorder_connection(stream, event_senders).await;
                }
                Some(Err(e)) => {
                    tracing::warn!("Failed to accept connection: {}", e);
                    // Continue the loop unless it's a permanent error
                    if e.kind() == std::io::ErrorKind::NotConnected {
                        break;
                    }
                    // Add a small delay to prevent busy looping on errors
                    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                }
                None => {
                    tracing::error!("Shared listener is None");
                    break;
                }
            }

            // Check if we should still be running after each connection
            if !*accept_loop_running.lock().unwrap() {
                break;
            }
        }

        tracing::debug!("Accept loop ended");
    }

    /// Handle a single recorder connection
    async fn handle_recorder_connection(
        mut stream: UnixStream,
        event_senders: Arc<Mutex<HashMap<String, broadcast::Sender<TaskEvent>>>>,
    ) {
        // First, read the session ID (length-prefixed)
        let session_id = match Self::read_session_id(&mut stream).await {
            Ok(id) => id,
            Err(e) => {
                tracing::warn!("Failed to read session ID from recorder: {}", e);
                return;
            }
        };

        tracing::debug!("Recorder connected for session: {}", session_id);

        // Get the broadcast sender for this session
        let sender = {
            let event_senders_guard = event_senders.lock().unwrap();
            event_senders_guard.get(&session_id).cloned()
        };

        if sender.is_none() {
            tracing::warn!("No broadcast sender found for session: {}", session_id);
            return;
        }

        let sender = sender.unwrap();

        // Now read events from the stream and broadcast them
        loop {
            // Read length-prefixed SSZ-encoded event from socket
            let mut len_buf = [0u8; 4];
            match stream.read_exact(&mut len_buf).await {
                Ok(_) => {
                    let len = u32::from_le_bytes(len_buf) as usize;
                    let mut event_buf = vec![0u8; len];
                    match stream.read_exact(&mut event_buf).await {
                        Ok(_) => {
                            // Decode SSZ event
                            use ah_rest_api_contract::types::SessionEvent;
                            use ssz::Decode;
                            match <SessionEvent as Decode>::from_ssz_bytes(&event_buf) {
                                Ok(session_event) => {
                                    // Create a readable version of the session event for logging
                                    let event_description = match &session_event {
                                        ah_rest_api_contract::types::SessionEvent::Error(e) => {
                                            format!(
                                                "Error({})",
                                                String::from_utf8_lossy(&e.message)
                                            )
                                        }
                                        ah_rest_api_contract::types::SessionEvent::Status(s) => {
                                            format!("Status({:?})", s.status)
                                        }
                                        ah_rest_api_contract::types::SessionEvent::FileEdit(_) => {
                                            "FileEdit".to_string()
                                        }
                                        ah_rest_api_contract::types::SessionEvent::ToolUse(_) => {
                                            "ToolUse".to_string()
                                        }
                                        ah_rest_api_contract::types::SessionEvent::ToolResult(
                                            _,
                                        ) => "ToolResult".to_string(),
                                        ah_rest_api_contract::types::SessionEvent::Log(_) => {
                                            "Log".to_string()
                                        }
                                        ah_rest_api_contract::types::SessionEvent::Thought(_) => {
                                            "Thought".to_string()
                                        }
                                    };
                                    tracing::debug!(
                                        "Received session event from recorder for session {}: {}",
                                        session_id,
                                        event_description
                                    );
                                    // Convert SessionEvent to TaskEvent
                                    if let Some(task_event) =
                                        session_event_to_task_event(session_event)
                                    {
                                        tracing::debug!(
                                            "Converted to task event for session {}: {:?}",
                                            session_id,
                                            task_event
                                        );
                                        // Send to all subscribers (ignore send errors - subscribers may have dropped)
                                        let send_result = sender.send(task_event);
                                        tracing::debug!(
                                            "Broadcast task event to subscribers for session {} (receivers: {})",
                                            session_id,
                                            send_result.unwrap_or(0)
                                        );
                                    } else {
                                        tracing::debug!(
                                            "Session event converted to None for session {}",
                                            session_id
                                        );
                                    }
                                }
                                Err(e) => {
                                    tracing::warn!(
                                        "Failed to decode SSZ session event for session {}: {:?}",
                                        session_id,
                                        e
                                    );
                                    break;
                                }
                            }
                        }
                        Err(e) => {
                            tracing::warn!(
                                "Failed to read event data from socket for session {}: {}",
                                session_id,
                                e
                            );
                            break;
                        }
                    }
                }
                Err(e) => {
                    // Socket closed or error
                    tracing::debug!("Socket read error for session {}: {}", session_id, e);
                    break;
                }
            }
        }

        tracing::debug!("Recorder connection closed for session: {}", session_id);
    }

    /// Read the session ID from the recorder connection
    async fn read_session_id(stream: &mut UnixStream) -> anyhow::Result<String> {
        // Read length-prefixed session ID
        let mut len_buf = [0u8; 4];
        stream.read_exact(&mut len_buf).await?;
        let len = u32::from_le_bytes(len_buf) as usize;

        let mut id_buf = vec![0u8; len];
        stream.read_exact(&mut id_buf).await?;

        String::from_utf8(id_buf).map_err(|e| anyhow::anyhow!("Invalid UTF-8 in session ID: {}", e))
    }
}

/// Get the base directory for AH sockets following OS conventions
fn socket_dir() -> std::path::PathBuf {
    use std::os::unix::fs::PermissionsExt;

    // Choose base directory based on OS
    let base_dir = if cfg!(target_os = "linux") {
        // Linux: prefer XDG_RUNTIME_DIR, fallback to /tmp
        std::env::var("XDG_RUNTIME_DIR")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|_| std::path::PathBuf::from("/tmp"))
            .join("ah")
    } else if cfg!(target_os = "macos") {
        // macOS: use TMPDIR
        std::env::var("TMPDIR")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|_| std::path::PathBuf::from("/tmp"))
            .join("ah")
    } else {
        // Other Unix-like systems: /tmp
        std::path::PathBuf::from("/tmp").join("ah")
    };

    // Create directory with proper permissions if it doesn't exist
    if !base_dir.exists() {
        if let Err(e) = std::fs::create_dir_all(&base_dir) {
            tracing::warn!(
                "Failed to create socket directory {}: {}",
                base_dir.display(),
                e
            );
        } else {
            // Set permissions to 0700 (owner read/write/execute only)
            if let Ok(metadata) = std::fs::metadata(&base_dir) {
                let mut perms = metadata.permissions();
                perms.set_mode(0o700);
                let _ = std::fs::set_permissions(&base_dir, perms);
            }
        }
    }

    base_dir
}

/// Get the task manager socket path for event streaming from recorders
fn task_manager_socket_path() -> std::path::PathBuf {
    socket_dir().join("task-manager.sock")
}

#[async_trait]
impl<M> TaskManager for GenericLocalTaskManager<M>
where
    M: Multiplexer + Send + Sync + 'static,
{
    async fn launch_task(&self, params: TaskLaunchParams) -> TaskLaunchResult {
        tracing::info!(
            "Starting task launch for description: {} with {} model(s)",
            params.description(),
            params.models().len()
        );

        // For local mode, we run agents directly from the current filesystem state
        // without snapshot caching. Tasks are executed through the configured multiplexer
        // to provide proper terminal window management and session isolation.

        // Disable recording if configured globally (e.g., when fs-snapshots is set to disable)
        if self.agent_executor.config().recording_disabled {
            // Note: record field is now immutable, we can't modify it
            tracing::info!("Recording disabled by configuration");
        }

        let base_task_id = params.task_id().to_string();
        let current_dir = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
        tracing::info!(
            "Using base task ID: {}, working directory: {}",
            base_task_id,
            current_dir.display()
        );

        // Extract working copy mode and snapshot from starting point
        let (working_copy_mode, snapshot_id) = match params.starting_point() {
            crate::task_manager::StartingPoint::RepositoryBranch { .. } => {
                // For repository branches, use the specified working copy mode
                (*params.working_copy_mode(), None)
            }
            crate::task_manager::StartingPoint::RepositoryCommit { .. } => {
                // For specific commits, we would need to checkout that commit
                // For now, fall back to in-place mode
                tracing::warn!(
                    "RepositoryCommit starting point not fully implemented, using in-place mode"
                );
                (crate::WorkingCopyMode::InPlace, None)
            }
            crate::task_manager::StartingPoint::FilesystemSnapshot { snapshot_id } => {
                // For filesystem snapshots, always use snapshots mode
                (crate::WorkingCopyMode::Snapshots, Some(snapshot_id.clone()))
            }
        };

        // Launch a task for each model instance (respecting count)
        let mut launched_task_ids = Vec::new();
        let mut errors = Vec::new();
        let mut global_instance_index = 0;

        for (model_index, model) in params.models().iter().enumerate() {
            for instance_index in 0..model.count {
                // Generate unique session ID for this model instance
                let session_id = if params.models().len() == 1 && model.count == 1 {
                    // Single model, single instance - use base task ID
                    base_task_id.clone()
                } else if model.count == 1 {
                    // Single instance per model - use model index
                    format!("{}-{}", base_task_id, model_index)
                } else {
                    // Multiple instances - use global instance index
                    format!("{}-{}", base_task_id, global_instance_index)
                };

                tracing::info!(
                    "Launching task for agent '{:?}' with version '{}' and model '{}' (instance {}/{}) with session ID: {}",
                    model.agent.software,
                    model.agent.version,
                    model.model,
                    instance_index + 1,
                    model.count,
                    session_id
                );

                global_instance_index += 1;

                // For recording, ensure the shared socket listener is set up
                let task_manager_socket_path = if params.record() {
                    let name = task_manager_socket_path().to_string_lossy().to_string();
                    tracing::debug!("Using task manager socket: {}", name);

                    // Ensure the shared listener is created and accept loop is running
                    if let Err(e) = self.ensure_shared_listener() {
                        errors.push(format!(
                            "Failed to setup shared listener for {}: {}",
                            session_id, e
                        ));
                        continue;
                    }

                    // Create broadcast channel for this task's events
                    {
                        let mut event_senders = self.event_senders.lock().unwrap();
                        let (tx, _rx) = broadcast::channel(100); // Buffer size of 100 events
                        event_senders.insert(session_id.clone(), tx);
                    }

                    Some(name)
                } else {
                    None
                };

                // Get the agent command line from the executor with advanced options
                let agent_cmd_args = match self.agent_executor.get_agent_command_with_options(
                    &params,
                    &session_id,
                    Some(&current_dir),
                    snapshot_id.clone(),
                    task_manager_socket_path.as_deref(),
                ) {
                    Ok(cmd) => cmd,
                    Err(e) => {
                        tracing::error!(
                            "Failed to generate agent command for {}: {}",
                            session_id,
                            e
                        );
                        // Clean up broadcast channel if it was created
                        if params.record() {
                            let mut event_senders = self.event_senders.lock().unwrap();
                            event_senders.remove(&session_id);
                        }
                        errors.push(format!(
                            "Failed to generate agent command for {}: {}",
                            session_id, e
                        ));
                        continue;
                    }
                };

                let agent_cmd_inner = agent_cmd_args
                    .iter()
                    .map(|arg| crate::agent_executor::AgentExecutor::shell_escape(arg))
                    .collect::<Vec<_>>()
                    .join(" ");

                tracing::info!(
                    "Generated agent command for {}: {}",
                    session_id,
                    agent_cmd_inner
                );

                // Create a multiplexer layout for the task
                let layout_config = LayoutConfig {
                    task_id: &session_id,
                    working_dir: &current_dir,
                    editor_cmd: Some("lazygit"), // Launch lazygit in the editor pane
                    agent_cmd: &agent_cmd_inner,
                    log_cmd: None, // No separate log command for now
                    split_mode: *params.split_mode(),
                    focus: params.focus(),
                };

                match self.multiplexer.create_task_layout(&layout_config) {
                    Ok(_layout_handle) => {
                        // Task layout created successfully in multiplexer
                        tracing::info!("Task launched successfully with ID: {}", session_id);
                        launched_task_ids.push(session_id);
                    }
                    Err(e) => {
                        // Clean up broadcast channel if it was created
                        if params.record() {
                            let mut event_senders = self.event_senders.lock().unwrap();
                            event_senders.remove(&session_id);
                        }
                        tracing::error!("Task launch failed for {}: {}", session_id, e);
                        errors.push(format!(
                            "Failed to create task layout for {}: {}",
                            session_id, e
                        ));
                    }
                }
            }
        }

        // Return result based on what was launched
        if launched_task_ids.is_empty() {
            TaskLaunchResult::Failure {
                error: format!("All task launches failed: {}", errors.join("; ")),
            }
        } else {
            if !errors.is_empty() {
                tracing::warn!("Some tasks failed to launch: {}", errors.join("; "));
            }
            TaskLaunchResult::Success {
                session_ids: launched_task_ids,
            }
        }
    }

    fn task_events_receiver(&self, task_id: &str) -> broadcast::Receiver<TaskEvent> {
        let event_senders = Arc::clone(&self.event_senders);
        let task_id = task_id.to_string();

        tracing::debug!("Getting event receiver for task: {}", task_id);

        // Get the broadcast receiver for this task
        let event_senders_guard = event_senders.lock().unwrap();
        if let Some(sender) = event_senders_guard.get(&task_id) {
            tracing::debug!("Broadcast sender found for task: {}", task_id);
            sender.subscribe()
        } else {
            tracing::debug!("No broadcast sender found for task {}", task_id);
            // Return a receiver that will never receive anything
            // This is a bit of a hack, but broadcast channels don't have a good way to create a "dead" receiver
            let (tx, rx) = broadcast::channel(1);
            drop(tx); // Close the sender immediately
            rx
        }
    }

    async fn get_initial_tasks(&self) -> (Vec<TaskInfo>, Vec<TaskExecution>) {
        // Get draft tasks from the database
        match self.db_manager.list_drafts() {
            Ok(draft_records) => {
                let draft_tasks: Vec<TaskInfo> = draft_records
                    .into_iter()
                    .map(|draft| {
                        // Parse models JSON
                        let models = serde_json::from_str::<Vec<ah_domain_types::AgentChoice>>(
                            &draft.models,
                        )
                        .unwrap_or_default();

                        TaskInfo {
                            id: draft.id,
                            title: draft.description,
                            status: "draft".to_string(),
                            repository: draft.repository,
                            branch: draft.branch.unwrap_or_default(),
                            created_at: draft.created_at,
                            models,
                        }
                    })
                    .collect();

                // For completed tasks, return empty for now (could be extended to get from database)
                (draft_tasks, Vec::<TaskExecution>::new())
            }
            Err(e) => {
                tracing::warn!("Failed to list drafts: {}", e);
                (Vec::new(), Vec::new())
            }
        }
    }

    async fn save_draft_task(
        &self,
        draft_id: &str,
        description: &str,
        repository: &str,
        branch: &str,
        models: &[AgentChoice],
    ) -> SaveDraftResult {
        // Convert models to JSON string
        let models_json = match serde_json::to_string(models) {
            Ok(json) => json,
            Err(e) => {
                return SaveDraftResult::Failure {
                    error: format!("Failed to serialize models: {}", e),
                };
            }
        };

        let now = chrono::Utc::now().to_rfc3339();
        let draft_record = DraftRecord {
            id: draft_id.to_string(),
            description: description.to_string(),
            repository: repository.to_string(),
            branch: Some(branch.to_string()),
            models: models_json,
            created_at: now.clone(),
            updated_at: now,
        };

        match self.db_manager.save_draft(&draft_record) {
            Ok(()) => SaveDraftResult::Success,
            Err(e) => SaveDraftResult::Failure {
                error: format!("Failed to save draft: {}", e),
            },
        }
    }

    fn description(&self) -> &str {
        "Local Task Manager - executes tasks directly on this machine"
    }
}

/// Convert a SessionEvent from the REST API contract to a TaskEvent
fn session_event_to_task_event(
    session_event: ah_rest_api_contract::types::SessionEvent,
) -> Option<TaskEvent> {
    use ah_rest_api_contract::types::SessionEvent;

    // Convert u64 timestamp to DateTime<Utc)
    let datetime_ts = chrono::DateTime::from_timestamp(session_event.timestamp() as i64, 0)
        .unwrap_or_else(chrono::Utc::now);

    match session_event {
        SessionEvent::Status(event) => {
            let status = match event.status {
                ah_rest_api_contract::SessionStatus::Queued => TaskState::Queued,
                ah_rest_api_contract::SessionStatus::Provisioning => TaskState::Provisioning,
                ah_rest_api_contract::SessionStatus::Running => TaskState::Running,
                ah_rest_api_contract::SessionStatus::Pausing => TaskState::Pausing,
                ah_rest_api_contract::SessionStatus::Paused => TaskState::Paused,
                ah_rest_api_contract::SessionStatus::Resuming => TaskState::Resuming,
                ah_rest_api_contract::SessionStatus::Stopping => TaskState::Stopping,
                ah_rest_api_contract::SessionStatus::Stopped => TaskState::Stopped,
                ah_rest_api_contract::SessionStatus::Completed => TaskState::Completed,
                ah_rest_api_contract::SessionStatus::Failed => TaskState::Failed,
                ah_rest_api_contract::SessionStatus::Cancelled => TaskState::Cancelled,
            };
            Some(TaskEvent::Status {
                status,
                ts: datetime_ts,
            })
        }
        SessionEvent::Log(event) => {
            let level = match event.level {
                ah_rest_api_contract::SessionLogLevel::Debug => LogLevel::Debug,
                ah_rest_api_contract::SessionLogLevel::Info => LogLevel::Info,
                ah_rest_api_contract::SessionLogLevel::Warn => LogLevel::Warn,
                ah_rest_api_contract::SessionLogLevel::Error => LogLevel::Error,
            };
            let message = String::from_utf8_lossy(&event.message).to_string();
            let tool_execution_id = event
                .tool_execution_id
                .as_ref()
                .map(|bytes| String::from_utf8_lossy(bytes).to_string());
            Some(TaskEvent::Log {
                level,
                message,
                tool_execution_id,
                ts: datetime_ts,
            })
        }
        SessionEvent::Error(event) => {
            let message = String::from_utf8_lossy(&event.message).to_string();
            Some(TaskEvent::Log {
                level: LogLevel::Error,
                message,
                tool_execution_id: None, // Agent errors don't have tool execution IDs
                ts: datetime_ts,
            })
        }
        SessionEvent::Thought(event) => {
            let thought = String::from_utf8_lossy(&event.thought).to_string();
            let reasoning =
                event.reasoning.as_ref().map(|bytes| String::from_utf8_lossy(bytes).to_string());
            Some(TaskEvent::Thought {
                thought,
                reasoning,
                ts: datetime_ts,
            })
        }
        SessionEvent::ToolUse(event) => {
            let tool_name = String::from_utf8_lossy(&event.tool_name).to_string();
            let tool_args_str = String::from_utf8_lossy(&event.tool_args).to_string();
            let tool_args = serde_json::from_str(&tool_args_str).unwrap_or(serde_json::Value::Null);
            let tool_execution_id = String::from_utf8_lossy(&event.tool_execution_id).to_string();
            let status = match event.status {
                ah_rest_api_contract::SessionToolStatus::Started => ToolStatus::Started,
                ah_rest_api_contract::SessionToolStatus::Completed => ToolStatus::Completed,
                ah_rest_api_contract::SessionToolStatus::Failed => ToolStatus::Failed,
            };
            Some(TaskEvent::ToolUse {
                tool_name,
                tool_args,
                tool_execution_id,
                status,
                ts: datetime_ts,
            })
        }
        SessionEvent::ToolResult(event) => {
            let tool_name = String::from_utf8_lossy(&event.tool_name).to_string();
            let tool_output = String::from_utf8_lossy(&event.tool_output).to_string();
            let tool_execution_id = String::from_utf8_lossy(&event.tool_execution_id).to_string();
            let status = match event.status {
                ah_rest_api_contract::SessionToolStatus::Started => ToolStatus::Started,
                ah_rest_api_contract::SessionToolStatus::Completed => ToolStatus::Completed,
                ah_rest_api_contract::SessionToolStatus::Failed => ToolStatus::Failed,
            };
            Some(TaskEvent::ToolResult {
                tool_name,
                tool_output,
                tool_execution_id,
                status,
                ts: datetime_ts,
            })
        }
        SessionEvent::FileEdit(event) => {
            let file_path = String::from_utf8_lossy(&event.file_path).to_string();
            let description = event
                .description
                .as_ref()
                .map(|bytes| String::from_utf8_lossy(bytes).to_string());
            Some(TaskEvent::FileEdit {
                file_path,
                lines_added: event.lines_added,
                lines_removed: event.lines_removed,
                description,
                ts: datetime_ts,
            })
        }
    }
}

impl<M> Drop for GenericLocalTaskManager<M>
where
    M: Multiplexer + Send + Sync + 'static,
{
    fn drop(&mut self) {
        // Stop the accept loop
        {
            let mut accept_loop_running = self.accept_loop_running.lock().unwrap();
            *accept_loop_running = false;
        }

        // Wait for the accept loop to finish (with a timeout)
        if let Some(handle) = self.accept_loop_handle.lock().unwrap().take() {
            // We can't block in drop, so we'll spawn a task to wait for completion
            tokio::spawn(async move {
                let _ = tokio::time::timeout(std::time::Duration::from_secs(5), handle).await;
            });
        }

        // Clean up the socket file
        let socket_path = task_manager_socket_path();
        let _ = std::fs::remove_file(&socket_path);
    }
}

/// Type alias for the most common usage: GenericLocalTaskManager with a dynamic multiplexer
pub type LocalTaskManager = GenericLocalTaskManager<Box<dyn Multiplexer + Send + Sync>>;
