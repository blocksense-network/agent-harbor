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
pub mod branches_enumerator;
pub mod db;
pub mod devshell;
pub mod editor;
pub mod error;
pub mod local_branches_enumerator;
pub mod local_repositories_enumerator;
pub mod local_task_manager;
pub mod push;
pub mod remote_branches_enumerator;
pub mod remote_repositories_enumerator;
pub mod remote_workspace_files_enumerator;
pub mod repositories_enumerator;
pub mod rest_task_manager;
pub mod session;
pub mod task;
pub mod task_manager;
pub mod task_manager_init;
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
pub use task_manager::{TaskEvent, TaskLaunchParams, TaskLaunchResult, TaskManager};

/// Task manager initialization utilities.
pub use task_manager_init::{
    MultiplexerPreference, TaskManagerConfig, create_dashboard_task_manager,
    create_local_task_manager, create_local_task_manager_with_multiplexer,
    create_session_viewer_task_manager, create_task_manager_no_recording,
};

/// Local task manager for direct execution on the local machine.
/// Uses a dynamic multiplexer implementation.
pub use local_task_manager::LocalTaskManager;

/// Workspace file enumeration for repositories.
pub use remote_workspace_files_enumerator::RemoteWorkspaceFilesEnumerator;

/// Re-export domain types
pub use ah_domain_types::{LogLevel, TaskState, ToolStatus};

/// Agent execution engine for spawning and managing agent processes.
pub use agent_executor::{AgentExecutionConfig, AgentExecutor, WorkingCopyMode};

/// REST API-based task manager implementation.
pub use rest_task_manager::{GenericRestTaskManager, RestApiClient, RestTaskManager};

/// Workspace files enumeration for repository file discovery.
pub use workspace_files_enumerator::{
    FileStream, RepositoryError, RepositoryFile, WorkspaceFilesEnumerator,
};

/// Repository enumeration for discovering available repositories.
pub use repositories_enumerator::RepositoriesEnumerator;

/// Branch enumeration for discovering branches within repositories.
pub use branches_enumerator::BranchesEnumerator;

/// Local repository enumerator implementation.
pub use local_repositories_enumerator::LocalRepositoriesEnumerator;

/// Remote repository enumerator implementation.
pub use remote_repositories_enumerator::RemoteRepositoriesEnumerator;

/// Local branch enumerator implementation.
pub use local_branches_enumerator::LocalBranchesEnumerator;

/// Remote branch enumerator implementation.
pub use remote_branches_enumerator::RemoteBranchesEnumerator;
