// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Mock ACP client implementation

use crate::MockAcpClient;

/// Additional client functionality (placeholder for now)
///
/// TODO: Implement actual client-side ACP functionality when SDK is available
impl MockAcpClient {
    /// Get the current scenario name
    pub fn scenario_name(&self) -> &str {
        &self.config.scenario.name
    }

    /// Get the configured protocol version
    pub fn protocol_version(&self) -> u32 {
        self.config.protocol_version
    }

    /// Get the configured working directory
    pub fn working_directory(&self) -> Option<&str> {
        self.config.cwd.as_deref()
    }
}
