// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

use ah_rest_server::acp::recorder::follower_command;

#[test]
fn follower_command_includes_all_flags() {
    let cmd = follower_command("exec-001", "sess-999", "npm test");
    assert!(cmd.contains("show-sandbox-execution"));
    assert!(cmd.contains("--id exec-001"));
    assert!(cmd.contains("--session sess-999"));
    assert!(cmd.contains("npm test"));
    assert!(cmd.ends_with("--follow"));
}
