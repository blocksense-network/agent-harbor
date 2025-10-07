#!/usr/bin/env python3
"""
Script to launch mock LLM server and ah agent start using process-compose.

This script provides a convenient way to run integration tests between
the mock LLM API server and the Agent Harbor CLI agent start command.
"""

import argparse
import json
import os
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path


def find_project_root():
    """Find the project root directory."""
    current = Path(__file__).resolve()
    while current.parent != current:
        if (current / "Cargo.toml").exists():
            return current
        current = current.parent
    raise RuntimeError("Could not find project root (Cargo.toml)")


def get_scenario_files():
    """Get available scenario files."""
    mock_agent_dir = find_project_root() / "tests" / "tools" / "mock-agent"
    scenarios_dir = mock_agent_dir / "scenarios"
    return [f.stem for f in scenarios_dir.glob("*.yaml")]


def get_playbook_files():
    """Get available playbook files."""
    mock_agent_dir = find_project_root() / "tests" / "tools" / "mock-agent"
    examples_dir = mock_agent_dir / "examples"
    return [f.stem for f in examples_dir.glob("*.json")]


def setup_working_directory(scenario_name, working_dir):
    """Set up the working directory with repo and user-home subdirectories."""
    print(f"Setting up working directory: {working_dir}")

    # Clean up existing directory if it exists
    if working_dir.exists():
        print(f"Cleaning up existing working directory: {working_dir}")
        shutil.rmtree(working_dir)

    # Create working directory and subdirectories
    working_dir.mkdir(parents=True)
    repo_dir = working_dir / "repo"
    user_home_dir = working_dir / "user-home"

    repo_dir.mkdir()
    user_home_dir.mkdir()

    print(f"Created repo directory: {repo_dir}")
    print(f"Created user-home directory: {user_home_dir}")

    return repo_dir, user_home_dir


def create_process_compose_config(args, working_dir, repo_dir, user_home_dir):
    """Create process-compose YAML configuration."""

    project_root = find_project_root()
    mock_agent_dir = project_root / "tests" / "tools" / "mock-agent"

    # Build mock server command
    server_cmd = [
        "python3",
        str(mock_agent_dir / "start_test_server.py"),
        "--host", "127.0.0.1",
        "--port", str(args.server_port),
    ]

    if args.scenario:
        scenario_file = mock_agent_dir / "scenarios" / f"{args.scenario}.yaml"
        if not scenario_file.exists():
            print(f"Error: Scenario file {scenario_file} does not exist")
            sys.exit(1)
        server_cmd.extend(["--scenario", str(scenario_file)])
    elif args.playbook:
        playbook_file = mock_agent_dir / "examples" / f"{args.playbook}.json"
        if not playbook_file.exists():
            print(f"Error: Playbook file {playbook_file} does not exist")
            sys.exit(1)
        server_cmd.extend(["--playbook", str(playbook_file)])

    server_cmd.extend(["--format", "codex"])

    # Build ah agent start command
    ah_cmd = [
        str(project_root / "target" / "debug" / "ah"),
        "agent", "start",
        "--agent", args.agent_type,
    ]

    if args.non_interactive:
        ah_cmd.append("--non-interactive")

    if args.output_format:
        ah_cmd.extend(["--output", args.output_format])

    if args.llm_api:
        ah_cmd.extend(["--llm-api", args.llm_api])

    if args.llm_api_key:
        ah_cmd.extend(["--llm-api-key", args.llm_api_key])

    # Add working directory and environment variables
    ah_cmd.extend([
        "--working-copy", "in-place",
        "--repo", str(repo_dir),
    ])

    # Environment variables for ah command
    env_vars = {
        "HOME": str(user_home_dir),
        "AH_HOME": str(user_home_dir / ".ah"),
        "TUI_TESTING_URI": f"tcp://127.0.0.1:{args.tui_port}",
    }

    # Only set mock server environment variables if no custom LLM API is specified
    if not args.llm_api:
        env_vars["CODEX_API_BASE"] = f"http://127.0.0.1:{args.server_port}/v1"
        env_vars["CODEX_API_KEY"] = "mock-key"

    # Create process-compose configuration
    config = {
        "version": "0.5",
        "processes": {
            "mock-server": {
                "command": server_cmd,
                "working_dir": str(mock_agent_dir),
                "environment": ["PYTHONPATH=" + str(mock_agent_dir / "src")],
                "readiness_probe": {
                    "http_get": {
                        "host": "127.0.0.1",
                        "port": args.server_port,
                        "path": "/health"
                    },
                    "initial_delay_seconds": 2,
                    "period_seconds": 1,
                    "timeout_seconds": 5,
                    "success_threshold": 1,
                    "failure_threshold": 3
                }
            },
            "ah-agent": {
                "command": ah_cmd,
                "working_dir": str(repo_dir),
                "environment": [f"{k}={v}" for k, v in env_vars.items()],
                "depends_on": {
                    "mock-server": {
                        "condition": "process_healthy"
                    }
                }
            }
        }
    }

    # Add TUI testing server if enabled
    if args.enable_tui_testing:
        tui_cmd = [
            str(project_root / "target" / "debug" / "tui-testing-cmd"),
            "--uri", f"tcp://127.0.0.1:{args.tui_port}",
            "--cmd", args.tui_command
        ]

        config["processes"]["tui-testing"] = {
            "command": tui_cmd,
            "working_dir": str(repo_dir),
            "depends_on": {
                "ah-agent": {
                    "condition": "process_started"
                }
            }
        }

    return config


def main():
    parser = argparse.ArgumentParser(
        description="Launch mock LLM server and ah agent start with process-compose",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog="""
Examples:
  # Basic test with default scenario (realistic_development_scenario)
  %(prog)s --non-interactive

  # Specific scenario with codex
  %(prog)s --scenario test_scenario --non-interactive

  # Claude agent with JSON output
  %(prog)s --scenario feature_implementation_scenario --agent-type claude --output-format json

  # With custom OpenAI API endpoint
  %(prog)s --non-interactive --llm-api https://api.openai.com/v1 --llm-api-key sk-your-key

  # With custom Claude API
  %(prog)s --agent-type claude --llm-api https://api.anthropic.com --llm-api-key sk-ant-your-key

  # With custom Gemini API
  %(prog)s --agent-type gemini --llm-api https://generativelanguage.googleapis.com --llm-api-key your-gemini-key

  # Mock agent for testing
  %(prog)s --agent-type mock --scenario test_scenario

  # With TUI testing
  %(prog)s --non-interactive --enable-tui-testing

  # Generate config only
  %(prog)s --scenario test_scenario --config-only > config.yaml
        """
    )

    # Working directory options
    parser.add_argument(
        "--working-dir",
        help="Working directory for the test run (default: auto-generated from scenario name)"
    )

    # Server options
    parser.add_argument(
        "--server-port",
        type=int,
        default=18081,
        help="Port for the mock LLM API server (default: 18081)"
    )

    parser.add_argument(
        "--tui-port",
        type=int,
        default=5555,
        help="Port for TUI testing IPC server (default: 5555)"
    )

    # Scenario/playbook selection
    scenario_group = parser.add_mutually_exclusive_group()
    scenario_group.add_argument(
        "--scenario",
        choices=get_scenario_files(),
        default="realistic_development_scenario",
        help="YAML scenario file to use for the mock server (default: realistic_development_scenario)"
    )
    scenario_group.add_argument(
        "--playbook",
        choices=get_playbook_files(),
        help="JSON playbook file to use for the mock server"
    )

    # Agent options
    parser.add_argument(
        "--agent-type",
        choices=["mock", "codex", "claude", "gemini", "opencode", "qwen", "cursor-cli", "goose"],
        default="codex",
        help="Agent type to start: mock (testing), codex (OpenAI), claude (Anthropic), gemini (Google), opencode, qwen, cursor-cli, goose (default: codex)"
    )

    parser.add_argument(
        "--non-interactive",
        action="store_true",
        help="Enable non-interactive mode for the agent"
    )

    parser.add_argument(
        "--output-format",
        choices=["text", "text-normalized", "json", "json-normalized"],
        default="json",
        help="Output format for the agent (default: json)"
    )

    parser.add_argument(
        "--llm-api",
        help="Custom LLM API URI for agent backend"
    )

    parser.add_argument(
        "--llm-api-key",
        help="API key for custom LLM API"
    )

    # TUI testing options
    parser.add_argument(
        "--enable-tui-testing",
        action="store_true",
        help="Enable TUI testing integration"
    )

    parser.add_argument(
        "--tui-command",
        default="exit:0",
        help="TUI testing command to send (default: exit:0)"
    )

    # Process compose options
    parser.add_argument(
        "--config-only",
        action="store_true",
        help="Only generate and print the process-compose config, don't run it"
    )

    parser.add_argument(
        "--config-file",
        help="Save config to file instead of using a temporary file"
    )

    args = parser.parse_args()

    # Determine scenario name for working directory
    scenario_name = args.scenario if args.scenario else (args.playbook if args.playbook else "default")

    # Set up working directory
    if args.working_dir:
        working_dir = Path(args.working_dir)
    else:
        # Create working directory in temp location named after scenario
        temp_base = Path(tempfile.gettempdir()) / "agent-test-runs"
        temp_base.mkdir(exist_ok=True)
        working_dir = temp_base / f"test-{scenario_name}"

    repo_dir, user_home_dir = setup_working_directory(scenario_name, working_dir)

    # Create configuration
    config = create_process_compose_config(args, working_dir, repo_dir, user_home_dir)

    # Save config to file
    if args.config_file:
        config_path = Path(args.config_file)
    else:
        temp_file = tempfile.NamedTemporaryFile(mode='w', suffix='.yaml', delete=False)
        config_path = Path(temp_file.name)
        temp_file.close()

    import yaml
    with open(config_path, 'w') as f:
        yaml.dump(config, f, default_flow_style=False)

    print(f"Generated process-compose config: {config_path}")
    print(f"Working directory: {working_dir}")
    print(f"Repository: {repo_dir}")
    print(f"User home: {user_home_dir}")

    if args.config_only:
        print("Configuration:")
        print(json.dumps(config, indent=2))
        return

    if not args.config_only:
        print(f"\nLaunching process-compose with config: {config_path}")

        try:
            # Launch process-compose
            subprocess.run([
                "process-compose", "up",
                "--config", str(config_path),
                "--tui=false"  # Disable TUI for headless operation
            ], check=True)
        except KeyboardInterrupt:
            print("\nProcess interrupted by user")
        except subprocess.CalledProcessError as e:
            print(f"Process-compose failed with exit code {e.returncode}")
            sys.exit(e.returncode)
        finally:
            # Clean up temporary config file
            if not args.config_file:
                config_path.unlink(missing_ok=True)


if __name__ == "__main__":
    main()
