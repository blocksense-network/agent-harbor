#!/usr/bin/env python3
# Copyright 2025 Schelling Point Labs Inc
# SPDX-License-Identifier: AGPL-3.0-only

"""
Filesystem snapshot hook for testing Agent Time-Travel functionality.

This hook simulates taking a filesystem snapshot by creating evidence that it was executed.
It works with both mock-agent scenarios and real Claude Code/Codex agents.
"""

import json
import sys
import os
from datetime import datetime


def main():
    """Main hook execution logic."""
    # Get current timestamp
    timestamp = datetime.now().isoformat()

    # Determine execution context
    hook_type = "unknown"
    agent_type = "unknown"
    session_id = "unknown"
    cwd = os.getcwd()

    # Try to determine context from input
    try:
        if len(sys.argv) > 1:
            # Codex rollout-hook format
            hook_type = "codex_rollout"
            agent_type = "codex"
            # Last argument should be JSON, but we don't need to parse it for basic execution evidence
            session_id = f"codex-session-{timestamp.replace(':', '-').replace('.', '-')[:10]}"
        else:
            # Claude Code format: JSON from stdin
            hook_type = "claude_posttool"
            agent_type = "claude"
            try:
                input_data = json.load(sys.stdin)
                session_id = input_data.get("session_id", f"claude-session-{timestamp.replace(':', '-').replace('.', '-')[:10]}")
                cwd = input_data.get("cwd", cwd)
            except:
                session_id = f"claude-session-{timestamp.replace(':', '-').replace('.', '-')[:10]}"
    except:
        # Fallback
        session_id = f"hook-session-{timestamp.replace(':', '-').replace('.', '-')[:10]}"

    # Use CLAUDE_PROJECT_DIR if available (set by Claude Code)
    if "CLAUDE_PROJECT_DIR" in os.environ:
        cwd = os.environ["CLAUDE_PROJECT_DIR"]

    # Define evidence file paths
    evidence_file = os.path.join(cwd, ".aw", "snapshots", "evidence.log")
    hook_execution_log = os.path.join(cwd, ".aw", "snapshots", "hook_executions.log")

    # Ensure directory exists
    os.makedirs(os.path.dirname(evidence_file), exist_ok=True)

    # Create hook execution evidence (simple proof that hook ran)
    execution_entry = {
        "timestamp": timestamp,
        "hook_type": hook_type,
        "agent_type": agent_type,
        "session_id": session_id,
        "working_directory": cwd,
        "command_line": " ".join(sys.argv) if len(sys.argv) > 1 else "stdin",
        "execution_id": f"exec-{timestamp.replace(':', '-').replace('.', '-')}"
    }

    # Create snapshot evidence entry (for Agent Time-Travel compatibility)
    snapshot_entry = {
        "timestamp": timestamp,
        "session_id": session_id,
        "tool_name": "hook_execution",  # Placeholder since we don't parse JSON
        "tool_input": {},
        "tool_response": {"success": True},
        "event": hook_type,
        "snapshot_id": f"snapshot-{timestamp.replace(':', '-').replace('.', '-')}",
        "provider": "integration-test-fs-snapshot",
        "agent_type": agent_type
    }

    try:
        # Write hook execution log (simple proof of execution)
        with open(hook_execution_log, "a", encoding="utf-8") as f:
            f.write(json.dumps(execution_entry, ensure_ascii=False) + "\n")

        # Write snapshot evidence (for compatibility with existing tests)
        with open(evidence_file, "a", encoding="utf-8") as f:
            f.write(json.dumps(snapshot_entry, ensure_ascii=False) + "\n")

        # Print success message to stdout
        print(f"Hook executed successfully: {execution_entry['execution_id']}")

    except Exception as e:
        print(f"Error writing hook evidence: {e}", file=sys.stderr)
        sys.exit(1)

    sys.exit(0)


if __name__ == "__main__":
    main()

