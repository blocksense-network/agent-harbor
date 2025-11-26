// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only
//! Recorder/command-trace bridge scaffolding for ACP (Milestone 5).
//!
//! This module will eventually translate recorder/command-trace events into ACP
//! `session/update` payloads and construct follower commands (`ah
//! show-sandbox-execution ...`) for IDEs. The current implementation only
//! provides helpers to format the follower CLI invocation so higher layers can
//! reuse a single source of truth.

/// Build the canonical follower command that IDEs should run to attach to a
/// sandbox execution. This mirrors the design described in
/// `specs/ACP.server.status.md` (Milestone 5).
pub fn follower_command(execution_id: &str, session_id: &str, original_cmd: &str) -> String {
    format!(
        "ah show-sandbox-execution \"{}\" --id {} --session {} --follow",
        original_cmd.replace('"', "\\\""),
        execution_id,
        session_id
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn follower_command_formats_expected_invocation() {
        let cmd = follower_command("exec-123", "sess-abc", "make test");
        assert!(cmd.contains("show-sandbox-execution"));
        assert!(cmd.contains("--id exec-123"));
        assert!(cmd.contains("--session sess-abc"));
        assert!(cmd.contains("make test"));
    }
}
