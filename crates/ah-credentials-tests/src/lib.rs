// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Shared fixtures and mock-agent harnesses for credentials integration tests

pub mod fixtures;
pub mod mock_agent;

/// Re-export common test utilities
pub use fixtures::*;
pub use mock_agent::*;
