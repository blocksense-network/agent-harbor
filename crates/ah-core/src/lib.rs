// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Core task and session lifecycle orchestration for Agent Harbor.
//!
//! This crate provides the foundational abstractions and orchestration logic for
//! managing agent tasks and sessions, including lifecycle management, state
//! transitions, and coordination with other AH components.

pub mod agent_binary;
pub mod agent_executor;
pub mod agent_tasks;
pub mod agent_types;
pub mod db;
pub mod devshell;
pub mod editor;
pub mod error;
pub mod local_task_manager;
pub mod push;
pub mod rest_task_manager;
pub mod session;
pub mod task;
pub mod task_manager;
pub mod workspace_files_enumerator;

/// Core result type used throughout the AH system.
pub type Result<T> = std::result::Result<T, Error>;

/// Core error type that encompasses all AH operations.
pub use error::Error;

/// Task lifecycle management and orchestration.
pub use task::{Task, TaskId, TaskStateManager, TaskStatus};

/// Session lifecycle management and orchestration.
pub use session::{Session, SessionId, SessionManager, SessionStatus};

/// Agent task file management and operations.
pub use agent_tasks::AgentTasks;

/// Interactive editor integration for task content creation.
pub use editor::{EDITOR_HINT, EditorError, edit_content_interactive};

/// Nix devshell detection and parsing utilities.
pub use devshell::devshell_names;

/// Push operations and remote management utilities.
pub use push::{PushHandler, PushOptions, parse_push_to_remote_flag};

/// Database integration for persistence.
pub use db::DatabaseManager;

/// Re-export SplitMode from ah-mux-core for convenience
pub use ah_mux_core::SplitMode;
/// Task manager abstraction for different execution modes (local, remote, mock).
pub use task_manager::{
    SaveDraftResult, TaskEvent, TaskLaunchParams, TaskLaunchResult, TaskManager,
};

/// Local task manager for direct execution on the local machine.
/// Uses a dynamic multiplexer implementation.
pub use local_task_manager::LocalTaskManager;

/// Re-export domain types
pub use ah_domain_types::{LogLevel, TaskExecutionStatus, ToolStatus};

/// Agent execution engine for spawning and managing agent processes.
pub use agent_executor::{AgentExecutionConfig, AgentExecutor, WorkingCopyMode};

/// REST API-based task manager implementation.
pub use rest_task_manager::{GenericRestTaskManager, RestApiClient, RestTaskManager};

/// Workspace files enumeration for repository file discovery.
pub use workspace_files_enumerator::{
    FileStream, RepositoryError, RepositoryFile, WorkspaceFilesEnumerator,
};
