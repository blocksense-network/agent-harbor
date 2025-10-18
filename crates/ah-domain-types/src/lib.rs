//! Domain types for the Agent Harbor software suite
//!
//! This crate contains the core domain types that are shared across
//! different parts of the Agent Harbor system, including the TUI,
//! REST API, local database, and other components.
//!
//! These types represent the business domain entities and should be
//! UI-agnostic, reusable across different contexts.

pub mod task;
pub mod agent;
pub mod repository;

// Re-export commonly used types
pub use task::*;
pub use agent::*;
pub use repository::*;

// Re-export shared enums
pub use task::{TaskExecutionStatus, LogLevel, ToolStatus};
