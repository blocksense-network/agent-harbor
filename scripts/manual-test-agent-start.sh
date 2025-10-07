#!/usr/bin/env python3
"""
Manual Agent Start Script

This script launches mock LLM servers and ah agent start commands using process-compose
for manual testing and integration verification.

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

# Note: We check for yaml availability in create_process_compose_config
# to handle cases where the script runs with system python but commands use nix python


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


def get_agent_version(agent_type):
    """Get the version of the specified agent by running its version command."""
    version_commands = {
        "claude": ["claude", "--version"],
        "codex": ["codex", "--version"],
        "gemini": ["gemini", "--version"],
        "opencode": ["opencode", "--version"],
        "qwen": ["qwen", "--version"],
        "cursor-cli": ["cursor", "--version"],
        "goose": ["goose", "--version"],
    }

    if agent_type not in version_commands:
        return "unknown"

    try:
        result = subprocess.run(
            version_commands[agent_type],
            capture_output=True,
            text=True,
            timeout=5  # 5 second timeout
        )

        if result.returncode == 0:
            # Extract version from output - look for patterns like "0.4.0", "v1.2.3", etc.
            import re
            version_match = re.search(r'(\d+\.\d+\.\d+)', result.stdout.strip())
            if version_match:
                return version_match.group(1)

            # Fallback: return the first line if no version pattern found
            first_line = result.stdout.strip().split('\n')[0]
            if first_line:
                return first_line

        return "unknown"

    except (subprocess.TimeoutExpired, subprocess.SubprocessError, FileNotFoundError):
        # Agent not installed or command failed
        return "unknown"


def _create_claude_config(user_home_dir, repo_dir, claude_version):
    """Create Claude Code configuration file to avoid startup screens."""
    import json
    from datetime import datetime, UTC

    # Create the .claude directory
    claude_dir = user_home_dir / ".claude"
    claude_dir.mkdir(exist_ok=True)

    # Get current time once for all timestamp fields
    now = datetime.now(UTC)

    # Base configuration from the example file
    config = {
        "numStartups": 2,
        "installMethod": "unknown",
        "autoUpdates": False,
        "customApiKeyResponses": {
            "approved": [
                "sk-your-api-key"
            ],
            "rejected": []
        },
        "promptQueueUseCount": 3,
        "cachedStatsigGates": {
            "tengu_disable_bypass_permissions_mode": False,
            "tengu_use_file_checkpoints": False
        },
        "firstStartTime": now.strftime("%Y-%m-%dT%H:%M:%S.%fZ"),
        "userID": "",
        "projects": {
            str(repo_dir): {
                "allowedTools": [],
                "history": [
                    {
                        "display": "print the current time",
                        "pastedContents": {}
                    }
                ],
                "mcpContextUris": [],
                "mcpServers": {},
                "enabledMcpjsonServers": [],
                "disabledMcpjsonServers": [],
                "hasTrustDialogAccepted": True,
                "projectOnboardingSeenCount": 0,
                "hasClaudeMdExternalIncludesApproved": True,
                "hasClaudeMdExternalIncludesWarningShown": True,
                "hasCompletedProjectOnboarding": True,
                "lastTotalWebSearchRequests": 0,
                "lastCost": 0,
                "lastAPIDuration": 15,
                "lastToolDuration": 0,
                "lastDuration": 13312,
                "lastLinesAdded": 0,
                "lastLinesRemoved": 0,
                "lastTotalInputTokens": 0,
                "lastTotalOutputTokens": 0,
                "lastTotalCacheCreationInputTokens": 0,
                "lastTotalCacheReadInputTokens": 0,
                "lastSessionId": "9fdef27f-462a-4c46-ae37-7623a8b1d951"
            }
        },
        "sonnet45MigrationComplete": True,
        "changelogLastFetched": int(now.timestamp() * 1000),
        "shiftEnterKeyBindingInstalled": True,
        "hasCompletedOnboarding": True,
        "lastOnboardingVersion": claude_version,
        "hasOpusPlanDefault": False,
        "lastReleaseNotesSeen": claude_version,
        "hasIdeOnboardingBeenShown": {
            "cursor": True
        },
        "isQualifiedForDataSharing": False
    }

    # Write the configuration file
    config_path = user_home_dir / ".claude.json"
    with open(config_path, 'w') as f:
        json.dump(config, f, indent=2)

    print(f"Created Claude config file: {config_path}")


def setup_working_directory(scenario_name, working_dir, agent_version):
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

    # Create Claude Code configuration file to avoid startup screens
    _create_claude_config(user_home_dir, repo_dir, agent_version)

    return repo_dir, user_home_dir


def create_process_compose_config(args, working_dir, repo_dir, user_home_dir, agent_version, foreground=False, tui=False):
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
        # Explicit scenario requested
        scenario_file = mock_agent_dir / "scenarios" / f"{args.scenario}.yaml"
        if not scenario_file.exists():
            print(f"Error: Scenario file {scenario_file} does not exist")
            sys.exit(1)
        server_cmd.extend(["--scenario", str(scenario_file)])
    elif args.playbook:
        # Explicit playbook requested
        playbook_file = mock_agent_dir / "examples" / f"{args.playbook}.json"
        if not playbook_file.exists():
            print(f"Error: Playbook file {playbook_file} does not exist")
            sys.exit(1)
        server_cmd.extend(["--playbook", str(playbook_file)])
    else:
        scenario_file = mock_agent_dir / "scenarios" / "realistic_development_scenario.yaml"
        server_cmd.extend(["--scenario", str(scenario_file)])

    server_cmd.extend(["--format", "codex"])

    # Add tools profile and strict validation options
    if args.server_tools_profile:
        server_cmd.extend(["--tools-profile", args.server_tools_profile])
    else:
        # Default tools profile based on agent type
        server_cmd.extend(["--tools-profile", args.agent_type])

    # Add agent version for tracking tool definition changes
    server_cmd.extend(["--agent-version", agent_version])

    # Enable strict tools validation by default (can be disabled with --no-strict-tools-validation)
    if not args.no_strict_tools_validation:
        server_cmd.append("--strict-tools-validation")

    # Add request logging - use session.log in the user home directory
    session_log_path = user_home_dir / "session.log"
    server_cmd.extend(["--request-log-template", str(session_log_path)])

    # Build server environment
    server_environment = []
    if os.environ.get("FORCE_TOOLS_VALIDATION_FAILURE"):
        server_environment.append(f"FORCE_TOOLS_VALIDATION_FAILURE={os.environ['FORCE_TOOLS_VALIDATION_FAILURE']}")

    # Build server command as string (process-compose requires this)
    server_command = " ".join(f"'{arg}'" if "'" not in arg and " " in arg else f'"{arg}"' if " " in arg else arg for arg in server_cmd)

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

    if args.prompt:
        ah_cmd.extend(["--prompt", args.prompt])

    # Add working directory and environment variables
    ah_cmd.extend([
        "--working-copy", "in-place",
        "--cwd", str(repo_dir),
    ])

    # Build ah command as string
    ah_command = " ".join(f"'{arg}'" if "'" not in arg and " " in arg else f'"{arg}"' if " " in arg else arg for arg in ah_cmd)

    # Environment variables for ah command
    env_vars = {
        "HOME": str(user_home_dir),
        "AH_HOME": str(user_home_dir / ".ah"),
        "TUI_TESTING_URI": f"tcp://127.0.0.1:{args.tui_port}",
    }

    # Only set mock server environment variables if no custom LLM API is specified
    if not args.llm_api:
        if args.agent_type == "codex":
            env_vars["CODEX_API_BASE"] = f"http://127.0.0.1:{args.server_port}/v1"
            env_vars["CODEX_API_KEY"] = "mock-key"
        elif args.agent_type == "claude":
            env_vars["ANTHROPIC_BASE_URL"] = f"http://127.0.0.1:{args.server_port}"
            env_vars["ANTHROPIC_API_KEY"] = "mock-key"
        elif args.agent_type == "gemini":
            env_vars["GOOGLE_AI_BASE_URL"] = f"http://127.0.0.1:{args.server_port}/v1"
            env_vars["GOOGLE_API_KEY"] = "mock-key"
        elif args.agent_type == "opencode":
            env_vars["OPENCODE_API_BASE"] = f"http://127.0.0.1:{args.server_port}/v1"
            env_vars["OPENCODE_API_KEY"] = "mock-key"
        elif args.agent_type == "qwen":
            env_vars["QWEN_API_BASE"] = f"http://127.0.0.1:{args.server_port}/v1"
            env_vars["QWEN_API_KEY"] = "mock-key"
        elif args.agent_type == "cursor-cli":
            env_vars["CURSOR_API_BASE"] = f"http://127.0.0.1:{args.server_port}/v1"
            env_vars["CURSOR_API_KEY"] = "mock-key"
        elif args.agent_type == "goose":
            env_vars["GOOSE_API_BASE"] = f"http://127.0.0.1:{args.server_port}/v1"
            env_vars["GOOSE_API_KEY"] = "mock-key"

    # Create process-compose configuration
    config = {
        "version": "0.5",
        "processes": {
            "mock-server": {
                "command": server_command,
                "working_dir": str(mock_agent_dir),
                "environment": server_environment,
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
                "command": ah_command,
                "working_dir": str(repo_dir),
                "environment": [f"{k}={v}" for k, v in env_vars.items()],
                "depends_on": {
                    "mock-server": {
                        "condition": "process_healthy"
                    }
                },
                "availability": {
                    "exit_on_end": True
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

        tui_command = " ".join(f"'{arg}'" if "'" not in arg and " " in arg else f'"{arg}"' if " " in arg else arg for arg in tui_cmd)

        config["processes"]["tui-testing"] = {
            "command": tui_command,
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

  # Dry run to see what would be executed
  %(prog)s --dry-run --prompt "Create a hello world program"

  # Run in foreground mode (ah agent gets your TTY, auto-cleanup)
  %(prog)s --foreground --agent-type claude --prompt "Create hello.py"

  # Run with TUI for monitoring all processes
  %(prog)s --tui --agent-type claude --prompt "Create hello.py"

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

    parser.add_argument(
        "--server-tools-profile",
        choices=["codex", "claude", "gemini", "opencode", "qwen", "cursor-cli", "goose"],
        default=None,
        help="Override the default tools profile for the mock server"
    )

    parser.add_argument(
        "--no-strict-tools-validation",
        action="store_true",
        help="Disable strict tools validation on the mock server (enabled by default)"
    )

    # Scenario/playbook selection
    scenario_group = parser.add_mutually_exclusive_group()
    scenario_group.add_argument(
        "--scenario",
        choices=get_scenario_files(),
        help="YAML scenario file to use for the mock server"
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

    parser.add_argument(
        "--prompt",
        help="Custom prompt text to pass to the agent"
    )

    parser.add_argument(
        "-f", "--foreground",
        action="store_true",
        help="Run the ah agent start process in foreground with automatic cleanup (uses process-compose run)"
    )

    parser.add_argument(
        "--tui",
        action="store_true",
        help="Enable process-compose TUI mode for interactive monitoring"
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
        "--dry-run",
        action="store_true",
        help="Print the commands that would be executed and their environment variables, don't run anything"
    )

    parser.add_argument(
        "--config-file",
        help="Save config to file instead of using a temporary file"
    )

    args = parser.parse_args()

    # Get agent version early for both setup and config creation
    agent_version = get_agent_version(args.agent_type)
    print(f"Detected {args.agent_type} version: {agent_version}")

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

    repo_dir, user_home_dir = setup_working_directory(scenario_name, working_dir, agent_version)

    # Create configuration
    config = create_process_compose_config(args, working_dir, repo_dir, user_home_dir, agent_version, args.foreground, args.tui)

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

    if args.dry_run:
        print("DRY RUN - Commands that would be executed:")
        print("=" * 50)

        # Print mock server command
        if "mock-server" in config.get("processes", {}):
            server_proc = config["processes"]["mock-server"]
            print("Mock Server Command:")
            print(f"  Working Directory: {server_proc.get('working_dir', 'current')}")
            print(f"  Command: {server_proc['command']}")
            if "environment" in server_proc and server_proc["environment"]:
                print("  Environment Variables:")
                for env_var in server_proc["environment"]:
                    print(f"    {env_var}")
            print()

        # Print ah agent command
        if "ah-agent" in config.get("processes", {}):
            agent_proc = config["processes"]["ah-agent"]
            print("AH Agent Command:")
            print(f"  Working Directory: {agent_proc.get('working_dir', 'current')}")
            print(f"  Command: {agent_proc['command']}")
            if "environment" in agent_proc and agent_proc["environment"]:
                print("  Environment Variables:")
                for env_var in agent_proc["environment"]:
                    print(f"    {env_var}")
            print()

        # Print TUI testing command if present
        if "tui-testing" in config.get("processes", {}):
            tui_proc = config["processes"]["tui-testing"]
            print("TUI Testing Command:")
            print(f"  Working Directory: {tui_proc.get('working_dir', 'current')}")
            print(f"  Command: {tui_proc['command']}")
            print()

        # Print process-compose command
        print("Process Compose Command:")
        if args.foreground:
            print(f"  process-compose run ah-agent --config {config_path}")
        else:
            tui_flag = "" if args.tui else " --tui=false"
            print(f"  process-compose up --config {config_path}{tui_flag}")
        print()

        print("Working directory setup:")
        print(f"  Test working directory: {working_dir}")
        print(f"  Repository: {repo_dir}")
        print(f"  User home: {user_home_dir}")
        print(f"  Claude project path: {repo_dir}")
        return

    if not args.config_only:
        print(f"\nLaunching process-compose with config: {config_path}")

        try:
            if args.foreground:
                # Use process-compose run for foreground mode - attaches ah-agent to current TTY
                cmd = [
                    "process-compose", "run", "ah-agent",
                    "--config", str(config_path)
                ]
                print(f"Running: {' '.join(cmd)}")
                subprocess.run(cmd, check=True)
            else:
                # Use process-compose up for background/TUI mode
                cmd = [
                    "process-compose", "up",
                    "--config", str(config_path)
                ]
                if not args.tui:
                    cmd.append("--tui=false")  # Disable TUI for headless operation when not in TUI mode
                print(f"Running: {' '.join(cmd)}")
                subprocess.run(cmd, check=True)
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
