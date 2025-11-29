// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Mock ACP Client for testing ACP agents
//!
//! This crate provides a mock ACP client that can execute scenario-driven
//! interactions with ACP agents. It translates scenario events into ACP
//! method calls and validates responses against expectations.

mod executor;
mod handlers;

pub use executor::ScenarioExecutor;

use ah_scenario_format::{Scenario, TimelineEvent};
use anyhow::Result;
use async_trait::async_trait;
use std::sync::Arc;

/// Configuration for the mock ACP client
#[derive(Debug, Clone)]
pub struct MockClientConfig {
    /// Scenario to execute
    pub scenario: Arc<Scenario>,
    /// Protocol version to use
    pub protocol_version: u32,
    /// Working directory for file operations
    pub cwd: Option<String>,
}

/// Mock ACP client that executes scenarios against real ACP agents
///
/// TODO: This is a placeholder implementation until the ACP Rust SDK is available.
/// Currently provides the interface and basic structure for scenario execution.
pub struct MockAcpClient {
    config: MockClientConfig,
    executor: ScenarioExecutor,
}

impl MockAcpClient {
    /// Create a new mock ACP client
    pub fn new(config: MockClientConfig) -> Self {
        let executor = ScenarioExecutor::new(config.scenario.clone());
        Self { config, executor }
    }

    /// Connect to an ACP agent via stdio
    ///
    /// TODO: Implement actual ACP connection when SDK is available
    pub async fn connect_stdio(
        &mut self,
        _agent_stdin: impl tokio::io::AsyncWrite + Unpin + Send + 'static,
        _agent_stdout: impl tokio::io::AsyncRead + Unpin + Send + 'static,
    ) -> Result<()> {
        tracing::info!("Mock ACP connection established (placeholder)");
        Ok(())
    }

    /// Execute the scenario
    ///
    /// TODO: Implement actual ACP communication when SDK is available
    pub async fn execute_scenario(&mut self) -> Result<()> {
        tracing::info!("Starting scenario execution: {}", self.config.scenario.name);

        // For now, just simulate scenario execution without actual ACP calls
        self.executor.simulate_scenario().await?;

        tracing::info!("Scenario execution completed (simulated)");
        Ok(())
    }

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

/// Placeholder types for ACP SDK (to be replaced when SDK is available)

/// Placeholder ACP connection
#[derive(Clone)]
pub struct MockAcpConnection;

/// Placeholder ACP client trait
#[async_trait]
pub trait MockAcpAgent: Send + Sync {
    async fn mock_method_call(&self, method: &str) -> Result<String>;
}

/// Placeholder ACP error
#[derive(Debug, thiserror::Error)]
#[error("Mock ACP error: {message}")]
pub struct MockAcpError {
    pub message: String,
}

/// Trait for handling scenario execution
#[async_trait]
pub trait ScenarioHandler: Send + Sync {
    /// Handle a timeline event
    async fn handle_event(&self, event: &TimelineEvent) -> Result<()>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use ah_scenario_format::{InputContent, TimelineEvent, UserInputEntry};

    #[tokio::test]
    async fn test_mock_client_creation() {
        let scenario = Arc::new(Scenario {
            name: "test_scenario".to_string(),
            tags: vec![],
            terminal_ref: None,
            initial_prompt: None,
            repo: None,
            ah: None,
            server: None,
            acp: None,
            timeline: vec![],
            expect: None,
        });

        let config = MockClientConfig {
            scenario,
            protocol_version: 1,
            cwd: Some("/tmp".to_string()),
        };

        let client = MockAcpClient::new(config);

        assert_eq!(client.scenario_name(), "test_scenario");
        assert_eq!(client.protocol_version(), 1);
        assert_eq!(client.working_directory(), Some("/tmp"));
    }

    #[tokio::test]
    async fn test_scenario_simulation() {
        let scenario = Arc::new(Scenario {
            name: "simulation_test".to_string(),
            tags: vec![],
            terminal_ref: None,
            initial_prompt: None,
            repo: None,
            ah: None,
            server: None,
            acp: None,
            timeline: vec![TimelineEvent::UserInputs {
                user_inputs: vec![UserInputEntry {
                    relative_time: 100,
                    input: InputContent::Text("Hello agent".to_string()),
                    target: None,
                    meta: None,
                }],
            }],
            expect: None,
        });

        let config = MockClientConfig {
            scenario,
            protocol_version: 1,
            cwd: None,
        };

        let mut client = MockAcpClient::new(config);
        let result = client.execute_scenario().await;

        assert!(result.is_ok());
    }
}
