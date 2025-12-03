// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! End-to-end validation of the ACP client against `mock-agent` using the
//! `acp_round_trip` scenario. Covers json-normalized output and headless
//! recording to ensure milestone 1 parity.

use ah_core::task_manager::TaskEvent;
use ah_recorder::replay::{InterleavedItem, create_branch_points};
use serde_json::Value;
use std::{path::PathBuf, process::Command};
use tempfile::tempdir;

fn scenario_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join(format!(
            "../../tests/tools/mock-agent-acp/scenarios/{name}.yaml"
        ))
        .canonicalize()
        .unwrap_or_else(|e| panic!("canonicalize scenario {name}: {e}"))
}

fn parse_task_events_from_stdout(stdout: &str) -> Vec<TaskEvent> {
    stdout
        .lines()
        .filter_map(|line| serde_json::from_str::<TaskEvent>(line).ok())
        .collect()
}

fn assert_round_trip_events(events: &[TaskEvent]) {
    let pos = |pred: fn(&TaskEvent) -> bool, label: &str| {
        events.iter().position(pred).unwrap_or_else(|| panic!("expected {label} event"))
    };

    pos(
        |ev| matches!(ev, TaskEvent::Thought { thought, .. } if thought.contains("Plan (3 steps)")),
        "plan thought",
    );

    pos(
        |ev| matches!(ev, TaskEvent::ToolUse { tool_execution_id, .. } if tool_execution_id == "run-check"),
        "tool use run-check",
    );

    pos(
        |ev| matches!(ev, TaskEvent::FileEdit { file_path, .. } if file_path.contains("acp_round_trip.txt")),
        "file edit marker file",
    );

    pos(
        |ev| matches!(ev, TaskEvent::ToolResult { tool_execution_id, .. } if tool_execution_id == "run-check"),
        "tool result",
    );

    pos(
        |ev| matches!(ev, TaskEvent::Log { message, .. } if message.contains("permission requested")),
        "permission log",
    );
}

#[test]
fn acp_round_trip_json_normalized() {
    if cfg!(windows) {
        return;
    }

    #[allow(deprecated)]
    let ah_bin = assert_cmd::cargo::cargo_bin("ah");
    #[allow(deprecated)]
    let mock_agent_bin = assert_cmd::cargo::cargo_bin("mock-agent");
    let scenario = scenario_path("acp_round_trip");

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
        .arg("ping the system")
        .env("AH_ACP_CLIENT_TEST_WATCHDOG_MS", "15000")
        .output()
        .expect("failed to run acp client");

    assert!(
        output.status.success(),
        "acp client failed: {}\nstdout:\n{}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let json_lines: Vec<Value> = stdout
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .collect();
    assert!(
        !json_lines.is_empty(),
        "no json-normalized lines in stdout:\n{}",
        stdout
    );

    let events = parse_task_events_from_stdout(&stdout);
    assert_round_trip_events(&events);
}

#[test]
fn acp_round_trip_recording() {
    if cfg!(windows) {
        return;
    }

    #[allow(deprecated)]
    let ah_bin = assert_cmd::cargo::cargo_bin("ah");
    #[allow(deprecated)]
    let mock_agent_bin = assert_cmd::cargo::cargo_bin("mock-agent");
    let scenario = scenario_path("acp_round_trip");

    let tmp = tempdir().expect("tmpdir");
    let ahr_path = tmp.path().join("acp_round_trip.ahr");

    let mut cmd = Command::new(&ah_bin);
    cmd.arg("agent")
        .arg("record")
        .arg("--out-file")
        .arg(&ahr_path)
        .arg("--headless")
        .arg("--session-id")
        .arg("acp-round-trip")
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
        .arg("ping the system")
        .env("AH_ACP_CLIENT_TEST_WATCHDOG_MS", "20000");

    let output = cmd.output().expect("failed to run recorder");
    if !output.status.success() {
        panic!(
            "acp recording failed: {:?}\nstdout:\n{}\nstderr:\n{}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    assert!(ahr_path.exists(), "recording not created at {:?}", ahr_path);

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
    let nowhitespace: String = joined.replace("\\n", "").split_whitespace().collect();
    for needle in ["round trip start", "round trip done"] {
        let compact: String = needle.replace("\\n", "").split_whitespace().collect();
        assert!(
            nowhitespace.contains(&compact),
            "recorded AHR missing terminal snippet {:?}; snapshot:\n{}",
            needle,
            joined
        );
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let events = parse_task_events_from_stdout(&stdout);
    assert_round_trip_events(&events);
}
