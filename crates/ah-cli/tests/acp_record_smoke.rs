// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Smoke tests for ACP client recording and json-normalized output against the
//! `acp_terminal` scenario. We read the scenario file with `ah-scenario-format`
//! and assert that the described events surface in both the recorded `.ahr`
//! and the json-normalized TaskEvent stream.

use ah_core::task_manager::TaskEvent;
use ah_recorder::replay::{InterleavedItem, create_branch_points};
use ah_scenario_format::{ContentBlock, InputContent, Scenario, TimelineEvent};
use serde_json::Value;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use tempfile::tempdir;

fn load_expectations(
    scenario_path: &PathBuf,
) -> (Vec<String>, Vec<String>, Vec<String>, Vec<String>) {
    let data = fs::read_to_string(scenario_path)
        .unwrap_or_else(|e| panic!("read scenario {:?}: {}", scenario_path, e));
    let scenario: Scenario = serde_yaml::from_str(&data)
        .unwrap_or_else(|e| panic!("parse scenario {:?}: {}", scenario_path, e));

    let mut user_inputs = Vec::new();
    let mut tool_names = Vec::new();
    let mut assistant_texts = Vec::new();
    let mut terminal_snippets = Vec::new();

    for event in &scenario.timeline {
        match event {
            TimelineEvent::UserInputs {
                user_inputs: inputs,
                ..
            } => {
                for input in inputs {
                    if let InputContent::Text(t) = &input.input {
                        user_inputs.push(t.clone());
                    }
                }
            }
            TimelineEvent::AgentToolUse { agent_tool_use, .. } => {
                tool_names.push(agent_tool_use.tool_name.clone());
                if let Some(cmd_val) = agent_tool_use.args.get("cmd") {
                    if let Some(s) = cmd_val.as_str() {
                        terminal_snippets.push(s.replace("\\n", "\n"));
                    }
                }
            }
            TimelineEvent::LlmResponse { llm_response, .. } => {
                for el in llm_response {
                    if let ah_scenario_format::ResponseElement::Assistant { assistant } = el {
                        for step in assistant {
                            match &step.content {
                                ContentBlock::Text(t) => assistant_texts.push(t.clone()),
                                ContentBlock::Rich(
                                    ah_scenario_format::RichContentBlock::Text { text, .. },
                                ) => assistant_texts.push(text.clone()),
                                _ => {}
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }

    (user_inputs, tool_names, assistant_texts, terminal_snippets)
}

fn parse_task_events_from_stdout(stdout: &str) -> Vec<TaskEvent> {
    stdout
        .lines()
        .filter_map(|line| serde_json::from_str::<TaskEvent>(line).ok())
        .collect()
}

#[test]
fn acp_recording_captures_terminal_output() {
    if cfg!(windows) {
        return;
    }

    #[allow(deprecated)]
    let ah_bin = assert_cmd::cargo::cargo_bin("ah");
    #[allow(deprecated)]
    let mock_agent_bin = assert_cmd::cargo::cargo_bin("mock-agent");
    let scenario = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/tools/mock-agent-acp/scenarios/acp_terminal.yaml")
        .canonicalize()
        .expect("canonicalize scenario path");
    assert!(scenario.exists(), "scenario file missing: {:?}", scenario);

    let (_expected_inputs, expected_tools, expected_assistant, expected_term) =
        load_expectations(&scenario);

    let tmp = tempdir().expect("tmpdir");
    let ahr_path = tmp.path().join("acp_record.ahr");

    let mut cmd = Command::new(&ah_bin);
    cmd.arg("agent")
        .arg("record")
        .arg("--out-file")
        .arg(&ahr_path)
        .arg("--headless")
        .arg("--session-id")
        .arg("acp-record-smoke")
        .arg("--")
        .arg(&ah_bin)
        .arg("tui")
        .arg("acp-client")
        .arg("--acp-agent-cmd")
        .arg(format!(
            "{} --disable-follower --scenario {}",
            mock_agent_bin.display(),
            scenario.to_string_lossy()
        ))
        .arg("--prompt")
        .arg("Run a terminal command")
        .env("AH_ACP_CLIENT_TEST_WATCHDOG_MS", "15000");

    let output = cmd.output().expect("failed to run recorder");
    if !output.status.success() {
        panic!(
            "acp recording command failed\nstatus: {:?}\nstdout:\n{}\nstderr:\n{}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    assert!(
        ahr_path.exists(),
        "recording file was not created at {:?}",
        ahr_path
    );

    let interleaved =
        create_branch_points(ahr_path.clone(), Option::<PathBuf>::None).expect("replay recording");
    let joined = interleaved
        .items
        .iter()
        .filter_map(|item| match item {
            InterleavedItem::Line(line) => Some(line.trim().to_string()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n");
    let joined_nowhitespace: String = joined.replace("\\n", "").split_whitespace().collect();

    for snippet in expected_term {
        let needle: String = snippet.replace("\\n", "").split_whitespace().collect();
        assert!(
            joined_nowhitespace.contains(&needle),
            "recorded AHR missing expected terminal snippet {:?}; snapshot:\n{}",
            snippet,
            joined
        );
    }

    let task_events = parse_task_events_from_stdout(&String::from_utf8_lossy(&output.stdout));
    for tool in expected_tools {
        assert!(
            task_events
                .iter()
                .any(|ev| matches!(ev, TaskEvent::ToolUse { tool_name, .. } if tool_name == &tool)),
            "expected ToolUse for {tool} in json-normalized stream"
        );
    }
    for thought in expected_assistant {
        assert!(
            task_events.iter().any(
                |ev| matches!(ev, TaskEvent::Thought { thought: t, .. } if t.contains(&thought))
            ),
            "expected assistant thought containing {:?} in json-normalized stream",
            thought
        );
    }
}

#[test]
fn acp_json_normalized_emits_events() {
    if cfg!(windows) {
        return;
    }

    #[allow(deprecated)]
    let ah_bin = assert_cmd::cargo::cargo_bin("ah");
    #[allow(deprecated)]
    let mock_agent_bin = assert_cmd::cargo::cargo_bin("mock-agent");
    let scenario = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/tools/mock-agent-acp/scenarios/acp_terminal.yaml")
        .canonicalize()
        .expect("canonicalize scenario path");

    let (expected_inputs, expected_tools, expected_assistant, _) = load_expectations(&scenario);

    let output = Command::new(&ah_bin)
        .arg("tui")
        .arg("acp-client")
        .arg("--acp-agent-cmd")
        .arg(format!(
            "{} --disable-follower --scenario {}",
            mock_agent_bin.display(),
            scenario.to_string_lossy()
        ))
        .arg("--prompt")
        .arg("Run a terminal command")
        .env("AH_ACP_CLIENT_TEST_WATCHDOG_MS", "15000")
        .output()
        .expect("failed to run ah agent start");

    assert!(
        output.status.success(),
        "json-normalized run failed: {}\nstdout:\n{}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut json_lines: Vec<Value> = Vec::new();
    for line in stdout.lines() {
        if let Ok(val) = serde_json::from_str::<Value>(line) {
            json_lines.push(val);
        }
    }

    assert!(
        !json_lines.is_empty(),
        "json-normalized output did not contain any JSON lines; stdout was:\n{}",
        stdout
    );

    // Parse into TaskEvents for structure-aware checks.
    let events = parse_task_events_from_stdout(&stdout);

    for input in expected_inputs {
        assert!(
            events.iter().any(
                |ev| matches!(ev, TaskEvent::UserInput { content, .. } if content.contains(&input))
            ),
            "expected user input {:?} in TaskEvent stream",
            input
        );
    }
    for tool in expected_tools {
        assert!(
            events
                .iter()
                .any(|ev| matches!(ev, TaskEvent::ToolUse { tool_name, .. } if tool_name == &tool)),
            "expected ToolUse for {tool} in TaskEvent stream"
        );
    }
    for thought in expected_assistant {
        assert!(
            events.iter().any(
                |ev| matches!(ev, TaskEvent::Thought { thought: t, .. } if t.contains(&thought))
            ),
            "expected assistant thought containing {:?} in TaskEvent stream",
            thought
        );
    }
}
