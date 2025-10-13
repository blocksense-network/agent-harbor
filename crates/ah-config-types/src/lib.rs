//! Distributed, strongly-typed configuration structs for Agents Workflow modules.
//!
//! This crate provides the individual configuration types that are used by different
//! parts of the agent-harbor system. These types are designed to be extracted from
//! the final merged configuration JSON.
//!
//! The `UiRoot` struct gets flattened into the main schema, while other structs
//! like `RepoConfig` remain as nested sections.

pub mod ui;
pub mod repo;
pub mod server;
