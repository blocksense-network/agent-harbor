// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Smoke test to ensure key task flags appear in help output.

use assert_cmd::assert::OutputAssertExt;

#[test]
fn task_help_snapshot() {
    let bin = option_env!("CARGO_BIN_EXE_ah").unwrap_or("target/debug/ah");
    let mut cmd = std::process::Command::new(bin);
    cmd.args(["task", "create", "--help"]);
    let output = cmd.assert().success().get_output().stdout.clone();
    let help = String::from_utf8_lossy(&output);
    for needle in [
        "--agent",
        "--delivery",
        "--follow",
        "--create-task-files",
        "--notifications",
    ] {
        assert!(
            help.contains(needle),
            "help output should include flag {needle}"
        );
    }
}
