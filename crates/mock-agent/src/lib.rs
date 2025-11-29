// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Mock ACP Client for testing ACP agents
//!
//! This crate provides a mock ACP client that can execute scenario-driven
//! interactions with ACP agents. It translates scenario events into ACP
//! method calls and validates responses against expectations.

pub mod executor;
mod handlers;

pub use executor::{AcpTranscript, ScenarioAgent, ScenarioExecutor};

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
    use agent_client_protocol::Agent;
    use agent_client_protocol_schema::SessionUpdate;
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
            rules: None,
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

    #[test]
    fn acp_transcript_maps_user_and_assistant_messages() {
        use ah_scenario_format::{AssistantStep, ContentBlock, ResponseElement, SessionStartData};

        let scenario = Arc::new(Scenario {
            name: "acp_mapping".into(),
            tags: vec![],
            terminal_ref: None,
            initial_prompt: None,
            repo: None,
            ah: None,
            server: None,
            acp: None,
            rules: None,
            timeline: vec![
                TimelineEvent::UserInputs {
                    user_inputs: vec![UserInputEntry {
                        relative_time: 0,
                        input: InputContent::Text("hello".into()),
                        target: None,
                        meta: None,
                        expected_response: None,
                    }],
                },
                TimelineEvent::SessionStart {
                    meta: None,
                    session_start: SessionStartData {
                        session_id: Some("sess-123".into()),
                        expected_prompt_response: None,
                    },
                },
                TimelineEvent::LlmResponse {
                    meta: None,
                    llm_response: vec![ResponseElement::Assistant {
                        assistant: vec![AssistantStep {
                            relative_time: 0,
                            content: ContentBlock::Text("hi there".into()),
                        }],
                    }],
                },
            ],
            expect: None,
        });

        let executor = ScenarioExecutor::new(scenario);
        let transcript = executor.to_acp_transcript(None, None, None, None);
        assert_eq!(transcript.session_id.0.as_ref(), "sess-123");

        // Historical (pre-sessionStart) should include the user prompt
        assert_eq!(transcript.historical.len(), 1);
        match &transcript.historical[0].update {
            SessionUpdate::UserMessageChunk { content } => match content {
                agent_client_protocol_schema::ContentBlock::Text(text) => {
                    assert_eq!(text.text, "hello");
                }
                _ => panic!("unexpected content"),
            },
            other => panic!("unexpected update: {:?}", other),
        }

        // Live should include assistant reply
        assert_eq!(transcript.live.len(), 1);
        match &transcript.live[0].update {
            SessionUpdate::AgentMessageChunk { content } => match content {
                agent_client_protocol_schema::ContentBlock::Text(text) => {
                    assert_eq!(text.text, "hi there");
                }
                _ => panic!("unexpected content"),
            },
            other => panic!("unexpected update: {:?}", other),
        }

        // Initialize response is populated
        assert!(!transcript.initialize_response.agent_capabilities.load_session);
        assert_eq!(
            transcript.new_session_response.session_id.0.as_ref(),
            "sess-123"
        );
    }

    #[test]
    fn acp_transcript_captures_tool_calls_and_plan() {
        use ah_scenario_format::{
            AgentPlanData, PlanEntry, ResponseElement, SessionStartData, ToolResultData,
            ToolUseData,
        };
        let scenario = Arc::new(Scenario {
            name: "tool_plan".into(),
            tags: vec![],
            terminal_ref: None,
            initial_prompt: None,
            repo: None,
            ah: None,
            server: None,
            acp: None,
            rules: None,
            timeline: vec![
                TimelineEvent::SessionStart {
                    meta: None,
                    session_start: SessionStartData {
                        session_id: Some("s".into()),
                        expected_prompt_response: None,
                    },
                },
                TimelineEvent::LlmResponse {
                    meta: None,
                    llm_response: vec![
                        ResponseElement::AgentToolUse {
                            agent_tool_use: ToolUseData {
                                tool_name: "runCmd".into(),
                                args: [("cmd".into(), serde_yaml::Value::from("echo hi"))].into(),
                                tool_call_id: None,
                                progress: None,
                                result: None,
                                status: None,
                                tool_execution: None,
                                meta: None,
                            },
                        },
                        ResponseElement::AgentPlan {
                            agent_plan: AgentPlanData {
                                entries: vec![PlanEntry {
                                    content: "do things".into(),
                                    priority: "high".into(),
                                    status: "pending".into(),
                                }],
                                plan_update: None,
                            },
                        },
                        ResponseElement::ToolResult {
                            tool_result: ToolResultData {
                                tool_call_id: "runCmd".into(),
                                content: serde_yaml::Value::from("ok"),
                                is_error: false,
                            },
                        },
                    ],
                },
            ],
            expect: None,
        });

        let executor = ScenarioExecutor::new(scenario);
        let transcript = executor.to_acp_transcript(None, None, None, None);
        assert_eq!(transcript.live.len(), 4); // tool call, initial update, plan, tool result
        assert!(!transcript.cancel_requested);
    }

    #[test]
    fn acp_transcript_records_permissions_and_file_reads() {
        use ah_scenario_format::{
            AgentFileReadsData, AgentPermissionRequestData, FileReadSpec, PermissionOption,
        };
        let scenario = Arc::new(Scenario {
            name: "reqs".into(),
            tags: vec![],
            terminal_ref: None,
            initial_prompt: None,
            repo: None,
            ah: None,
            server: None,
            acp: None,
            rules: None,
            timeline: vec![
                TimelineEvent::AgentPermissionRequest {
                    agent_permission_request: AgentPermissionRequestData {
                        session_id: None,
                        tool_call: None,
                        options: Some(vec![PermissionOption {
                            id: "allow".into(),
                            label: "Allow".into(),
                            kind: "allow_once".into(),
                        }]),
                        decision: None,
                        granted: Some(true),
                    },
                    meta: None,
                },
                TimelineEvent::AgentFileReads {
                    agent_file_reads: AgentFileReadsData {
                        files: vec![FileReadSpec {
                            path: "/tmp/file".into(),
                            expected_content: None,
                        }],
                    },
                    meta: None,
                },
            ],
            expect: None,
        });

        let executor = ScenarioExecutor::new(scenario);
        let transcript = executor.to_acp_transcript(None, None, None, None);
        assert_eq!(transcript.permission_requests.len(), 1);
        assert_eq!(transcript.file_reads.len(), 1);
    }

    #[tokio::test]
    async fn scenario_agent_streams_notifications_and_respects_cancel() {
        use ah_scenario_format::{AssistantStep, ContentBlock, ResponseElement, SessionStartData};
        use tokio::sync::mpsc;
        let scenario = Arc::new(Scenario {
            name: "agent_stream".into(),
            tags: vec![],
            terminal_ref: None,
            initial_prompt: None,
            repo: None,
            ah: None,
            server: None,
            acp: None,
            rules: None,
            timeline: vec![
                TimelineEvent::UserInputs {
                    user_inputs: vec![UserInputEntry {
                        relative_time: 0,
                        input: InputContent::Text("hi".into()),
                        target: None,
                        meta: None,
                        expected_response: None,
                    }],
                },
                TimelineEvent::SessionStart {
                    meta: None,
                    session_start: SessionStartData {
                        session_id: Some("sess".into()),
                        expected_prompt_response: None,
                    },
                },
                TimelineEvent::LlmResponse {
                    meta: None,
                    llm_response: vec![ResponseElement::Assistant {
                        assistant: vec![AssistantStep {
                            relative_time: 0,
                            content: ContentBlock::Text("ok".into()),
                        }],
                    }],
                },
            ],
            expect: None,
        });

        let executor = ScenarioExecutor::new(scenario);
        let transcript = executor.to_acp_transcript(None, None, None, None);
        let agent = crate::executor::ScenarioAgent::new(transcript);
        let (tx, mut rx) = mpsc::unbounded_channel();
        agent.set_notifier(tx).await;

        agent
            .initialize(agent_client_protocol_schema::InitializeRequest {
                protocol_version: agent_client_protocol_schema::VERSION,
                client_capabilities: agent_client_protocol_schema::ClientCapabilities::default(),
                meta: None,
            })
            .await
            .unwrap();
        agent
            .new_session(agent_client_protocol_schema::NewSessionRequest {
                mcp_servers: vec![],
                cwd: std::path::PathBuf::from("/tmp"),
                meta: None,
            })
            .await
            .unwrap();

        agent
            .prompt(agent_client_protocol_schema::PromptRequest {
                session_id: agent_client_protocol_schema::SessionId(Arc::from("sess")),
                prompt: vec![],
                meta: None,
            })
            .await
            .unwrap();

        // Give the forwarder a moment to deliver notifications
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        let mut updates = Vec::new();
        while let Ok(note) = rx.try_recv() {
            updates.push(note);
        }
        assert!(!updates.is_empty(), "expected at least one session/update");

        agent
            .cancel(agent_client_protocol_schema::CancelNotification {
                session_id: agent_client_protocol_schema::SessionId(Arc::from("sess")),
                meta: None,
            })
            .await
            .unwrap();
        let resp = agent
            .prompt(agent_client_protocol_schema::PromptRequest {
                session_id: agent_client_protocol_schema::SessionId(Arc::from("sess")),
                prompt: vec![],
                meta: None,
            })
            .await
            .unwrap();
        assert_eq!(
            resp.stop_reason,
            agent_client_protocol_schema::StopReason::Cancelled
        );
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
            rules: None,
            timeline: vec![TimelineEvent::UserInputs {
                user_inputs: vec![UserInputEntry {
                    relative_time: 100,
                    input: InputContent::Text("Hello agent".to_string()),
                    target: None,
                    meta: None,
                    expected_response: None,
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

    #[test]
    fn build_playbook_partitions_historical_and_live() {
        use ah_scenario_format::{
            AssistantStep, ClientCapabilities, ContentBlock, FilesystemCapabilities,
            InitializeData, InputContent, ResponseElement, TimelineEvent, ToolUseData,
            UserInputEntry,
        };
        let scenario = Arc::new(Scenario {
            name: "partitioned".into(),
            tags: vec![],
            terminal_ref: None,
            initial_prompt: None,
            repo: None,
            ah: None,
            server: None,
            acp: None,
            rules: None,
            timeline: vec![
                TimelineEvent::Initialize {
                    initialize: InitializeData {
                        protocol_version: 1,
                        client_capabilities: ClientCapabilities {
                            fs: Some(FilesystemCapabilities {
                                read_text_file: Some(true),
                                write_text_file: Some(true),
                            }),
                            terminal: Some(true),
                        },
                        client_info: None,
                        meta: None,
                        expected_response: None,
                    },
                },
                TimelineEvent::UserInputs {
                    user_inputs: vec![UserInputEntry {
                        relative_time: 100,
                        input: InputContent::Text("historical prompt".into()),
                        target: None,
                        meta: None,
                        expected_response: None,
                    }],
                },
                TimelineEvent::SessionStart {
                    meta: None,
                    session_start: ah_scenario_format::SessionStartData {
                        session_id: Some("sess-1".into()),
                        expected_prompt_response: None,
                    },
                },
                TimelineEvent::LlmResponse {
                    meta: None,
                    llm_response: vec![ResponseElement::Assistant {
                        assistant: vec![AssistantStep {
                            relative_time: 0,
                            content: ContentBlock::Text("live reply".into()),
                        }],
                    }],
                },
                TimelineEvent::AgentToolUse {
                    agent_tool_use: ToolUseData {
                        tool_name: "runCmd".into(),
                        args: Default::default(),
                        tool_call_id: None,
                        progress: None,
                        result: None,
                        status: None,
                        tool_execution: None,
                        meta: None,
                    },
                    meta: None,
                },
            ],
            expect: None,
        });

        let executor = ScenarioExecutor::new(scenario);
        let playbook = executor.build_playbook();
        assert_eq!(playbook.historical.len(), 2);
        assert_eq!(playbook.live.len(), 2);
        assert!(matches!(
            playbook.session_start.as_ref().and_then(|s| s.session_id.as_deref()),
            Some("sess-1")
        ));
        assert!(playbook.live.iter().any(|a| a.meta.is_none()));
    }
}
