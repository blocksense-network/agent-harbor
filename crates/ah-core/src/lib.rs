//! Core task and session lifecycle orchestration for Agents Workflow.
//!
//! This crate provides the foundational abstractions and orchestration logic for
//! managing agent tasks and sessions, including lifecycle management, state
//! transitions, and coordination with other AH components.

pub mod agent_executor;
pub mod agent_tasks;
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
pub use editor::{edit_content_interactive, EditorError, EDITOR_HINT};

/// Nix devshell detection and parsing utilities.
pub use devshell::devshell_names;

/// Push operations and remote management utilities.
pub use push::{parse_push_to_remote_flag, PushHandler, PushOptions};

/// Database integration for persistence.
pub use db::DatabaseManager;

/// Task manager abstraction for different execution modes (local, remote, mock).
pub use task_manager::{
    TaskManager, TaskLaunchParams, TaskLaunchResult, TaskEvent,
    SaveDraftResult
};

/// Local task manager for direct execution on the local machine.
pub use local_task_manager::LocalTaskManager;

/// Re-export domain types
pub use ah_domain_types::{TaskExecutionStatus, LogLevel, ToolStatus};

/// Agent execution engine for spawning and managing agent processes.
pub use agent_executor::{AgentExecutor, AgentExecutionConfig, WorkingCopyMode};

/// REST API-based task manager implementation.
pub use rest_task_manager::{GenericRestTaskManager, RestTaskManager, RestApiClient};
