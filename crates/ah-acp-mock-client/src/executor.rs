// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Scenario execution engine for the mock ACP client

use ah_scenario_format::{
    AgentFileReadsData, AgentPermissionRequestData, Scenario, TimelineEvent, ToolUseData,
};
use anyhow::Result;
use std::sync::Arc;
use tokio::time::{Duration, sleep};

/// Executor for running ACP scenarios
#[derive(Clone)]
pub struct ScenarioExecutor {
    scenario: Arc<Scenario>,
}

impl ScenarioExecutor {
    /// Create a new scenario executor
    pub fn new(scenario: Arc<Scenario>) -> Self {
        Self { scenario }
    }

    /// Simulate scenario execution (placeholder until ACP SDK is available)
    pub async fn simulate_scenario(&self) -> Result<()> {
        tracing::info!("Simulating scenario execution: {}", self.scenario.name);

        // Execute timeline events in simulation mode
        for event in &self.scenario.timeline {
            self.simulate_event(event).await?;
        }

        tracing::info!("Scenario simulation completed");
        Ok(())
    }

    /// Simulate a single timeline event
    async fn simulate_event(&self, event: &TimelineEvent) -> Result<()> {
        match event {
            TimelineEvent::UserInputs { user_inputs } => {
                // Simulate sending prompts to the agent
                for input in user_inputs {
                    let block_count = match &input.input {
                        ah_scenario_format::InputContent::Text(_) => 1,
                        ah_scenario_format::InputContent::Rich(blocks) => blocks.len(),
                    };
                    tracing::info!("Simulating prompt with {} content blocks", block_count);

                    // Simulate prompt response
                    tracing::info!("Prompt simulation completed");

                    // Wait for the relative time
                    sleep(Duration::from_millis(input.relative_time)).await;
                }
            }

            TimelineEvent::AgentToolUse { agent_tool_use } => {
                self.simulate_tool_use(agent_tool_use).await?;
            }

            TimelineEvent::AgentFileReads { agent_file_reads } => {
                self.simulate_file_reads(agent_file_reads).await?;
            }

            TimelineEvent::AgentPermissionRequest {
                agent_permission_request,
            } => {
                self.simulate_permission_request(agent_permission_request).await?;
            }

            TimelineEvent::AdvanceMs { base_time_delta } => {
                sleep(Duration::from_millis(*base_time_delta)).await;
            }

            // Other events are handled at the scenario level or ignored for ACP testing
            _ => {
                tracing::debug!(
                    "Ignoring unsupported event: {:?}",
                    std::mem::discriminant(event)
                );
            }
        }

        Ok(())
    }

    /// Simulate a tool use event (placeholder for ACP terminal calls)
    async fn simulate_tool_use(&self, tool_use: &ToolUseData) -> Result<()> {
        if tool_use.tool_name == "runCmd" {
            // Extract command from args
            if let Some(serde_yaml::Value::String(cmd)) = tool_use.args.get("cmd") {
                tracing::info!("Simulating terminal command: {}", cmd);

                // Simulate terminal creation
                let terminal_id = format!("simulated-terminal-{}", uuid::Uuid::new_v4());
                tracing::info!("Simulated terminal created: {}", terminal_id);

                // Simulate command execution
                tracing::info!("Simulated command execution completed");

                // Simulate terminal cleanup
                tracing::info!("Simulated terminal released: {}", terminal_id);
            }
        } else {
            tracing::info!("Simulating tool use: {}", tool_use.tool_name);
        }

        Ok(())
    }

    /// Simulate file reads (placeholder for ACP filesystem calls)
    async fn simulate_file_reads(&self, file_reads: &AgentFileReadsData) -> Result<()> {
        for file_spec in &file_reads.files {
            tracing::info!("Simulating read of file: {}", file_spec.path);

            // Simulate file read response
            tracing::info!("Simulated file read completed: {} bytes", 42);

            // TODO: Validate against expected_content if provided
        }

        Ok(())
    }

    /// Simulate permission requests
    async fn simulate_permission_request(
        &self,
        permission_request: &AgentPermissionRequestData,
    ) -> Result<()> {
        tracing::info!(
            "Simulating permission request for tool call: {:?}",
            permission_request.tool_call
        );

        // Simulate permission approval
        let approved_option = permission_request
            .options
            .as_ref()
            .and_then(|opts| opts.first())
            .map(|opt| opt.id.clone())
            .unwrap_or_else(|| "allow".to_string());

        tracing::info!(
            "Permission simulation completed - approved: {}",
            approved_option
        );

        Ok(())
    }
}
