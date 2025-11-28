// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

// Smoke tests for parsing the new named-field scenario format.
use ah_scenario_format::Scenario;

#[test]
fn parse_llm_response_with_tool_use() {
    let yaml = r#"
name: tool_scenario
timeline:
  - agentToolUse:
      toolName: "writeFile"
      args:
        path: "output.txt"
        content: "Generated content"
      result: "File created"
      status: "ok"
  - llmResponse:
      - assistant:
          - relativeTime: 100
            content: "File created successfully"
expect:
  exitCode: 0
"#;

    let _scenario: Scenario = serde_yaml::from_str(yaml).expect("should parse");
}

#[test]
fn parse_llm_response_with_agent_edits() {
    let yaml = r#"
name: anthropic_thinking
timeline:
  - llmResponse:
      - think:
          - relativeTime: 120
            content: "Reasoning about the task"
      - assistant:
          - relativeTime: 200
            content: "All done"
  - agentEdits:
      path: "foo.txt"
      linesAdded: 2
      linesRemoved: 1
expect:
  exitCode: 0
"#;

    let _scenario: Scenario = serde_yaml::from_str(yaml).expect("should parse");
}
