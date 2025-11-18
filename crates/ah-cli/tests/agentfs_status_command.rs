#![cfg(feature = "agentfs")]
// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

use assert_cmd::cargo::cargo_bin_cmd;
use serde_json::Value;

#[test]
fn status_reports_agentfs_capabilities_in_json() {
    let mut cmd = cargo_bin_cmd!("ah");
    let output = cmd
        .arg("agent")
        .arg("fs")
        .arg("status")
        .arg("--json")
        .env("AH_ENABLE_AGENTFS_PROVIDER", "1")
        .env("AH_DISABLE_ANALYTICS", "1")
        .env("AH_SUPPRESS_TIPS", "1")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: Value = serde_json::from_slice(&output).expect("status output is valid JSON");
    assert!(
        json.get("selected").is_some(),
        "selected provider section missing"
    );

    let agentfs = json
        .get("agentfs")
        .and_then(|value| value.as_object())
        .expect("agentfs section missing in JSON output");
    assert_eq!(
        agentfs
            .get("name")
            .and_then(|v| v.as_str())
            .expect("agentfs provider name missing"),
        "AgentFs"
    );

    let capabilities = agentfs
        .get("capabilities")
        .and_then(|v| v.as_object())
        .expect("agentfs capabilities missing in JSON output");
    let score = capabilities
        .get("score")
        .and_then(|v| v.as_u64())
        .expect("agentfs capability score missing");
    assert!(
        score >= 10,
        "expected agentfs experimental score >= 10 when opt-in is enabled, got {score}"
    );
    assert!(
        capabilities
            .get("supports_cow_overlay")
            .and_then(|v| v.as_bool())
            .expect("agentfs supports_cow_overlay missing"),
        "agentfs should report CoW overlay support"
    );

    let notes = agentfs
        .get("detection_notes")
        .and_then(|v| v.as_array())
        .expect("agentfs detection notes missing");
    assert!(
        !notes.is_empty(),
        "expected at least one detection note for agentfs provider"
    );
}
