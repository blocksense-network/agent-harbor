#!/usr/bin/env python3
# Copyright 2025 Schelling Point Labs Inc
# SPDX-License-Identifier: AGPL-3.0-only

"""
Start a mock API server for integration testing.

This script starts the mock-agent server with appropriate configuration
for testing with claude and codex CLI tools.
"""

import argparse
import os
import sys
import signal
import tempfile
from pathlib import Path

# Add the src directory to Python path for imports
src_dir = os.path.join(os.path.dirname(__file__), 'src')
sys.path.insert(0, src_dir)

# Import server module directly
import server


def signal_handler(sig, frame):
    """Handle Ctrl+C gracefully."""
    print("\nShutting down server...")
    sys.exit(0)


def main():
    parser = argparse.ArgumentParser(description="Start mock API server for testing")
    parser.add_argument("--host", default="127.0.0.1", help="Server host (default: 127.0.0.1)")
    parser.add_argument("--port", type=int, default=18080, help="Server port (default: 18080)")
    parser.add_argument("--playbook", help="Playbook JSON file")
    parser.add_argument("--scenario", help="Scenario YAML file")
    parser.add_argument("--format", choices=["codex", "claude"], default="codex",
                       help="Session file format (default: codex)")
    parser.add_argument("--session-dir", help="Directory for session files (default: temp dir)")
    parser.add_argument("--tools-profile", choices=["codex", "claude", "gemini", "opencode", "qwen", "cursor-cli", "goose"],
                       default="codex", help="Tools profile for the target coding agent (default: codex)")
    parser.add_argument("--strict-tools-validation", action="store_true",
                       help="Enable strict tools validation - abort on unknown tool definitions")
    parser.add_argument("--agent-version", default="unknown",
                       help="Version of the coding agent being tested (for tracking tool definition changes)")
    parser.add_argument("--request-log-template",
                       help="Template for request logging path (use {scenario} and {key} placeholders). Use 'stdout' for console output")

    args = parser.parse_args()

    # Set up signal handler for graceful shutdown
    signal.signal(signal.SIGINT, signal_handler)

    # Determine playbook or scenario path
    playbook_path = None
    scenario_path = None

    if args.playbook and args.scenario:
        print("Error: Cannot specify both --playbook and --scenario")
        return 1
    elif args.playbook:
        playbook_path = args.playbook
        if not os.path.exists(playbook_path):
            print(f"Error: Playbook file not found: {playbook_path}")
            return 1
    elif args.scenario:
        scenario_path = args.scenario
        if not os.path.exists(scenario_path):
            print(f"Error: Scenario file not found: {scenario_path}")
            return 1
    else:
        # Default to comprehensive playbook
        script_dir = os.path.dirname(__file__)
        playbook_path = os.path.join(script_dir, "examples", "comprehensive_playbook.json")
        if not os.path.exists(playbook_path):
            print(f"Error: Default playbook file not found: {playbook_path}")
            return 1

    # Determine session directory
    if args.session_dir:
        session_dir = args.session_dir
        os.makedirs(session_dir, exist_ok=True)
    else:
        session_dir = tempfile.mkdtemp(prefix="mock_agent_sessions_")
        print(f"Using temporary session directory: {session_dir}")

    print(f"Starting mock API server...")
    print(f"  Host: {args.host}")
    print(f"  Port: {args.port}")
    if playbook_path:
        print(f"  Playbook: {playbook_path}")
    if scenario_path:
        print(f"  Scenario: {scenario_path}")
    print(f"  Format: {args.format}")
    print(f"  Session dir: {session_dir}")
    print()

    # Show usage instructions
    print("Usage with CLI tools:")
    print("=" * 40)

    print("\nFor Codex:")
    print(f"  export CODEX_API_BASE=http://{args.host}:{args.port}/v1")
    print(f"  export CODEX_API_KEY=mock-key")
    print(f"  codex exec --dangerously-bypass-approvals-and-sandbox 'Create hello.py'")

    print("\nFor Claude Code:")
    print("  # Note: Claude Code may not support custom API endpoints")
    print("  # This server primarily supports Codex-style API calls")

    print("\nAPI Endpoints:")
    print(f"  OpenAI-compatible: http://{args.host}:{args.port}/v1/chat/completions")
    print(f"  Anthropic-compatible: http://{args.host}:{args.port}/v1/messages")

    print("\nPress Ctrl+C to stop the server")
    print("=" * 40)

    # Call server.serve directly
    try:
        server.serve(
            host=args.host,
            port=args.port,
            playbook=playbook_path,
            scenario=scenario_path,
            codex_home=session_dir,
            format=args.format,
            workspace=None,
            tools_profile=args.tools_profile,
            strict_tools_validation=args.strict_tools_validation,
            agent_version=args.agent_version,
            request_log_template=args.request_log_template
        )
        return 0
    except KeyboardInterrupt:
        print("\nServer stopped by user")
        return 0


if __name__ == "__main__":
    sys.exit(main())
