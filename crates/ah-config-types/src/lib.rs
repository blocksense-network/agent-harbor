// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Distributed, strongly-typed configuration structs for Agent Harbor modules.
//!
//! This crate provides the individual configuration types that are used by different
//! parts of the agent-harbor system. These types are designed to be extracted from
//! the final merged configuration JSON.
//!
//! The `UiRoot` struct gets flattened into the main schema, while other structs
//! like `RepoConfig` remain as nested sections.

pub mod repo;
pub mod sandbox;
pub mod server;
pub mod ui;
