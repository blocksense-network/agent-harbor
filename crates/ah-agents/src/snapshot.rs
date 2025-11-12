// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

//! Shared utilities for building snapshot commands across different agent implementations

/// Build a snapshot command string with recorder socket parameter if available
pub fn build_snapshot_command(base_command: &str) -> String {
    if let Ok(recorder_socket) = std::env::var("AH_RECORDER_IPC_SOCKET") {
        format!("{} --recorder-socket {}", base_command, recorder_socket)
    } else {
        base_command.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_snapshot_command_without_socket() {
        // Clear any existing env var
        std::env::remove_var("AH_RECORDER_IPC_SOCKET");

        let result = build_snapshot_command("ah agent fs snapshot");
        assert_eq!(result, "ah agent fs snapshot");
    }

    #[test]
    fn test_build_snapshot_command_with_socket() {
        std::env::set_var("AH_RECORDER_IPC_SOCKET", "/tmp/test.sock");

        let result = build_snapshot_command("ah agent fs snapshot");
        assert_eq!(
            result,
            "ah agent fs snapshot --recorder-socket /tmp/test.sock"
        );

        // Clean up
        std::env::remove_var("AH_RECORDER_IPC_SOCKET");
    }
}
