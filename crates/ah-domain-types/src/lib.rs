// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Domain types for the Agent Harbor software suite
//!
//! This crate contains the core domain types that are shared across
//! different parts of the Agent Harbor system, including the TUI,
//! REST API, local database, and other components.
//!
//! These types represent the business domain entities and should be
//! UI-agnostic, reusable across different contexts.

pub mod agent;
pub mod cli;
pub mod experimental_features;
pub mod repository;
pub mod task;

// Re-export commonly used types
pub use agent::{
    AcpLaunchCommand, AgentCapabilities, AgentCapability, AgentCatalog, AgentChoice, AgentMetadata,
    AgentSoftware, AgentSoftwareBuild,
};
pub use experimental_features::ExperimentalFeature;
pub use repository::*;
pub use task::*;

// Re-export shared enums
pub use task::{MultiplexerType, OutputFormat, TaskState, TaskStatus, ToolStatus};
