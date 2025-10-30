# Copyright 2025 Schelling Point Labs Inc
# SPDX-License-Identifier: AGPL-3.0-only

import argparse
import json
import os
import sys

try:
    # Try relative imports (when run as part of package)
    from .agent import run_scenario, demo_scenario
    from .server import serve
except ImportError:
    # Fall back to script mode imports
    import sys
    import os

    # Add the src directory to sys.path so we can import the modules
    script_dir = os.path.dirname(__file__)
    src_dir = os.path.dirname(script_dir)  # Go up one level from src/ to mock-agent/
    if src_dir not in sys.path:
        sys.path.insert(0, src_dir)

    # Also try to find the mock-agent directory if we're running from project root
    mock_agent_dir = os.path.join(os.getcwd(), 'tests', 'tools', 'mock-agent')
    if os.path.exists(mock_agent_dir) and mock_agent_dir not in sys.path:
        sys.path.insert(0, mock_agent_dir)

    # Now import as src.agent, src.server, etc.
    import src.agent as agent
    import src.server as server
    run_scenario = agent.run_scenario
    demo_scenario = agent.demo_scenario
    serve = server.serve

def main():
    ap = argparse.ArgumentParser(prog="mockagent", description="Mock Coding Agent")
    sub = ap.add_subparsers(dest="cmd", required=True)

    runp = sub.add_parser("run", help="Run a scenario JSON")
    runp.add_argument("--scenario", required=True, help="Path to scenario JSON")
    runp.add_argument("--workspace", required=True, help="Workspace directory")
    runp.add_argument("--codex-home", default=os.path.expanduser("~/.codex"))
    runp.add_argument("--format", choices=["codex", "claude"], default="codex",
                     help="Session file format to use (codex or claude)")
    runp.add_argument("--checkpoint-cmd", help="Command to execute after each agentToolUse and agentEdits event")
    runp.add_argument("--with-snapshots", action="store_true", help="Create filesystem snapshots after each tool use (implies --checkpoint-cmd 'ah agent fs snapshot')")
    runp.add_argument("--fast-mode", action="store_true", help="Fast mode: sort events by time and execute sequentially without timing delays")
    runp.add_argument("--tui-testing-uri", help="ZeroMQ URI for TUI testing IPC (tcp://127.0.0.1:5555)")
    runp.add_argument("--interactive", action="store_true", help="Enable interactive mode for scenarios with user input steps")
    runp.add_argument("--no-colors", action="store_true", help="Disable colored output")

    demop = sub.add_parser("demo", help="Run built-in demo scenario")
    demop.add_argument("--workspace", required=True)
    demop.add_argument("--codex-home", default=os.path.expanduser("~/.codex"))
    demop.add_argument("--format", choices=["codex", "claude"], default="codex",
                      help="Session file format to use (codex or claude)")
    demop.add_argument("--checkpoint-cmd", help="Command to execute after each agentToolUse and agentEdits event")
    demop.add_argument("--with-snapshots", action="store_true", help="Create filesystem snapshots after each tool use (implies --checkpoint-cmd 'ah agent fs snapshot')")
    demop.add_argument("--fast-mode", action="store_true", help="Fast mode: sort events by time and execute sequentially without timing delays")
    demop.add_argument("--tui-testing-uri", help="ZeroMQ URI for TUI testing IPC (tcp://127.0.0.1:5555)")
    demop.add_argument("--interactive", action="store_true", help="Enable interactive mode for scenarios with user input steps")
    demop.add_argument("--no-colors", action="store_true", help="Disable colored output")

    srv = sub.add_parser("server", help="Run mock OpenAI/Anthropic API server")
    srv.add_argument("--host", default="127.0.0.1")
    srv.add_argument("--port", type=int, default=8080)
    srv.add_argument("--playbook", help="Playbook JSON with rules")
    srv.add_argument("--scenario", help="Scenario YAML file")
    srv.add_argument("--codex-home", default=os.path.expanduser("~/.codex"))
    srv.add_argument("--format", choices=["codex", "claude"], default="codex",
                    help="Session file format to use (codex or claude)")
    srv.add_argument("--tools-profile", choices=["codex", "claude", "gemini", "opencode", "qwen", "cursor-cli", "goose"],
                    default="codex", help="Tools profile for the target coding agent (default: codex)")
    srv.add_argument("--strict-tools-validation", action="store_true",
                    help="Abort on unknown tool definitions to help identify missing mappings")
    srv.add_argument("--agent-version", default="unknown",
                    help="Version of the coding agent being tested (for tracking tool definition changes)")

    args = ap.parse_args()

    # Handle --with-snapshots parameter by setting checkpoint_cmd
    checkpoint_cmd = getattr(args, 'checkpoint_cmd', None)
    if getattr(args, 'with_snapshots', False):
        checkpoint_cmd = 'ah agent fs snapshot'

    if args.cmd == "run":
        path = run_scenario(args.scenario, args.workspace, codex_home=args.codex_home, format=args.format, checkpoint_cmd=checkpoint_cmd, fast_mode=getattr(args, 'fast_mode', False), tui_testing_uri=getattr(args, 'tui_testing_uri', None), interactive=getattr(args, 'interactive', False), no_colors=getattr(args, 'no_colors', False))
        print(f"Session file written to: {path}")
    elif args.cmd == "demo":
        scen = demo_scenario(args.workspace)
        scen_path = os.path.join(args.workspace, "_demo_scenario.json")
        os.makedirs(args.workspace, exist_ok=True)
        with open(scen_path, "w", encoding="utf-8") as f:
            json.dump(scen, f, indent=2)
        path = run_scenario(scen_path, args.workspace, codex_home=args.codex_home, format=args.format, checkpoint_cmd=checkpoint_cmd, fast_mode=getattr(args, 'fast_mode', False), tui_testing_uri=getattr(args, 'tui_testing_uri', None), interactive=getattr(args, 'interactive', False), no_colors=getattr(args, 'no_colors', False))
        print(f"Session file written to: {path}")
    elif args.cmd == "server":
        serve(args.host, args.port, playbook=args.playbook, scenario=getattr(args, 'scenario', None), codex_home=args.codex_home, format=args.format, tools_profile=getattr(args, 'tools_profile', None), strict_tools_validation=getattr(args, 'strict_tools_validation', False), agent_version=getattr(args, 'agent_version', 'unknown'))
    else:
        ap.print_help()
        return 1

if __name__ == "__main__":
    sys.exit(main())
