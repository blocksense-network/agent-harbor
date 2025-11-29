#![allow(clippy::disallowed_methods)]
// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

use std::process::Command;

fn project_root() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .unwrap()
        .to_path_buf()
}

#[test]
fn run_smoke_via_just() {
    // Keep this fast: it runs the predefined smoke wrapper that uses the example client.
    // Skip gracefully if `just` is unavailable in the environment.
    if Command::new("just").arg("--version").output().is_err() {
        eprintln!("just not available; skipping smoke");
        return;
    }

    let status = Command::new("just")
        .arg("run-mock-agent-acp-smoke")
        .current_dir(project_root())
        .status()
        .expect("failed to invoke just");

    assert!(
        status.success(),
        "run-mock-agent-acp-smoke failed with status {status}"
    );
}
