// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Placeholder handlers for ACP functionality (to be implemented when SDK is available)

use crate::executor::ScenarioExecutor;

/// Placeholder mock client handler
///
/// TODO: Implement actual ACP message handling when SDK is available
#[derive(Clone)]
pub struct MockClientHandler {
    _executor: ScenarioExecutor,
}

impl MockClientHandler {
    /// Create a new mock client handler
    pub fn new(executor: ScenarioExecutor) -> Self {
        Self {
            _executor: executor,
        }
    }
}

// TODO: Implement Client trait when ACP SDK is available
/*
#[async_trait]
impl Client for MockClientHandler {
    // ... ACP method implementations
}
*/
