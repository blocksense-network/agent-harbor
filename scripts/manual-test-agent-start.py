#!/usr/bin/env python3
# Copyright 2025 Schelling Point Labs Inc
# SPDX-License-Identifier: AGPL-3.0-only

"""
Manual Agent Start/Record Script

This script launches LLM API servers (mock or live proxy) and ah agent start/record commands
using process-compose for manual testing and integration verification.

MODES:
- SCENARIO MODE (default): Uses pre-recorded scenario files for deterministic testing
- PROXY MODE: Forwards requests to real LLM APIs for live testing

This script provides a convenient way to run integration tests between
LLM API servers and the Agent Harbor CLI agent start/record commands.
"""

import atexit
import argparse
import json
import logging
import os
import shutil
import subprocess
import sys
import tempfile
import threading
import time
from pathlib import Path

# Note: We check for yaml availability in create_process_compose_config
# to handle cases where the script runs with system python but commands use nix python


def setup_script_logging(user_home_dir):
    """Set up logging for the script itself."""
    script_log_file = user_home_dir / "script.log"

    # Configure logging to both file and console
    logging.basicConfig(
        level=logging.INFO,
        format='%(asctime)s - %(levelname)s - %(message)s',
        handlers=[
            logging.FileHandler(script_log_file),
            logging.StreamHandler(sys.stdout)
        ]
    )

    logging.info(f"Script logging initialized. Log file: {script_log_file}")
    return script_log_file


class MockServerManager:
    """Manages the mock server process with auto-restart capability."""

    def __init__(self, cmd, env, cwd, log_file, server_environment):
        self.cmd = cmd
        self.env = env
        self.cwd = cwd
        self.log_file = log_file
        self.server_environment = server_environment
        self.process = None
        self.monitoring = False
        self.restart_count = 0
        self.max_restarts = 3
        self.monitor_thread = None

    def start_server(self):
        """Start the mock server process."""
        logging.info(f"Starting mock server with command: {' '.join(self.cmd)}")
        logging.info(f"Mock server logs: {self.log_file}")

        try:
            with open(self.log_file, 'a') as log_f:
                self.process = subprocess.Popen(
                    self.cmd,
                    env=self.env,
                    cwd=self.cwd,
                    stdout=log_f,
                    stderr=log_f
                )
            logging.info(f"Mock server started with PID: {self.process.pid}")
            return True
        except Exception as e:
            logging.error(f"Failed to start mock server: {e}")
            return False

    def stop_server(self):
        """Stop the mock server process."""
        if self.process and self.process.poll() is None:
            logging.info("Terminating mock server...")
            self.process.terminate()
            try:
                self.process.wait(timeout=5)
                logging.info("Mock server terminated gracefully")
            except subprocess.TimeoutExpired:
                logging.warning("Mock server didn't terminate gracefully, killing...")
                self.process.kill()
                self.process.wait()
                logging.info("Mock server killed")
        else:
            logging.info("Mock server already stopped")

    def check_server_health(self):
        """Check if the server is still running."""
        if self.process:
            return self.process.poll() is None
        return False

    def monitor_and_restart(self):
        """Monitor the server and restart if it crashes."""
        logging.info("Starting mock server monitoring thread")
        self.monitoring = True

        while self.monitoring:
            if not self.check_server_health():
                if self.restart_count < self.max_restarts:
                    logging.warning(f"Mock server crashed or stopped. Restarting (attempt {self.restart_count + 1}/{self.max_restarts})")
                    self.restart_count += 1
                    if self.start_server():
                        logging.info("Mock server restarted successfully")
                    else:
                        logging.error("Failed to restart mock server")
                else:
                    logging.error(f"Mock server crashed {self.max_restarts} times. Giving up.")
                    break
            time.sleep(2)  # Check every 2 seconds

        logging.info("Mock server monitoring stopped")

    def start_monitoring(self):
        """Start the monitoring thread."""
        self.monitor_thread = threading.Thread(target=self.monitor_and_restart, daemon=True)
        self.monitor_thread.start()

    def stop_monitoring(self):
        """Stop the monitoring thread."""
        self.monitoring = False
        if self.monitor_thread and self.monitor_thread.is_alive():
            self.monitor_thread.join(timeout=5)


def find_project_root():
    """Find the project root directory."""
    current = Path(__file__).resolve()
    # Start from the script's directory and go up until we find Cargo.toml
    while current.parent != current:
        if (current / "Cargo.toml").exists():
            return current
        current = current.parent
    # Fallback to parent of script if Cargo.toml not found
    return Path(__file__).resolve().parent

def get_scenario_files():
    """Get available scenario files."""
    mock_agent_dir = find_project_root() / "tests" / "tools" / "mock-agent"
    scenarios_dir = mock_agent_dir / "scenarios"
    return [f.stem for f in scenarios_dir.glob("*.yaml")]

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


def setup_working_directory(scenario_name, working_dir, agent_version):
    """Set up the working directory with repo and user-home subdirectories."""
    logging.info(f"Setting up working directory: {working_dir}")

    # Create working directory and subdirectories, preserving existing user-home for logs
    working_dir.mkdir(parents=True, exist_ok=True)

    repo_dir = working_dir / "repo"
    user_home_dir = working_dir / "user-home"

    # Clean up repo directory for fresh test environment
    if repo_dir.exists():
        logging.info(f"Cleaning up existing repo directory: {repo_dir}")
        print(f"Cleaning up existing repo directory: {repo_dir}")
        shutil.rmtree(repo_dir)
    repo_dir.mkdir()

    # Preserve user-home directory for stable logs
    user_home_dir.mkdir(exist_ok=True)

    logging.info(f"Created repo directory: {repo_dir}")
    logging.info(f"Using user-home directory: {user_home_dir} (preserved for stable logs)")
    print(f"Created repo directory: {repo_dir}")
    print(f"Using user-home directory: {user_home_dir} (preserved for stable logs)")

    return repo_dir, user_home_dir


def create_process_compose_config(args, working_dir, repo_dir, user_home_dir, agent_version, foreground=False, tui=False):
    """Create process-compose YAML configuration."""

    project_root = find_project_root()
    mock_agent_dir = project_root / "tests" / "tools" / "mock-agent"

    # Determine scenario file and agent type
    use_scenario_mode = args.scenario is not None

    if use_scenario_mode:
        # Explicit scenario requested - use test-server mode
        scenario_file = mock_agent_dir / "scenarios" / f"{args.scenario}.yaml"
        if not scenario_file.exists():
            print(f"Error: Scenario file {scenario_file} does not exist")
            sys.exit(1)
    else:
        # No scenario specified - use proxy mode
        scenario_file = None

    # Load scenario file to extract initialPrompt (only in scenario mode)
    initial_prompt = "Please help me with development tasks"
    if use_scenario_mode:
        try:
            import yaml
        except (ImportError, ModuleNotFoundError):
            print("Warning: PyYAML not available. " +
                  "Please run this script in the nix dev shell, provided by the nix flake at the root of the repository.")
            sys.exit(1)

        with open(scenario_file, 'r') as f:
            scenario_data = yaml.safe_load(f)
        initial_prompt = scenario_data.get('initialPrompt', 'Please execute this scenario')

    # Configure logging defaults prior to building the server command
    session_log_path = user_home_dir / "session.log"
    logging_enabled = not args.no_logging

    if logging_enabled:
        if args.request_log is None:
            args.request_log = str(session_log_path)
        logging.info(
            "Request logging enabled: path=%s, headers=%s, body=%s, responses=%s",
            args.request_log,
            args.log_headers,
            args.log_body,
            args.log_responses,
        )
    else:
        logging.info("All request logging disabled")

    # Build server command - use Rust llm-api-proxy with clap
    if use_scenario_mode:
        # Use test-server mode for scenario playback
        server_cmd = [
            "cargo",
            "run",
            "-p",
            "llm-api-proxy",
            "--",
            "test-server",
            "--port", str(args.server_port),
            "--scenario-file", str(scenario_file),
            "--agent-type", args.agent_type,
            "--agent-version", agent_version,  # Use the actual agent version
        ]
    else:
        # Use proxy mode for live API calls
        server_cmd = [
            "cargo",
            "run",
            "-p",
            "llm-api-proxy",
            "--",
            "proxy",
            "--port", str(args.server_port),
        ]

        # Determine provider based on agent type or OpenRouter option
        if args.use_openrouter:
            server_cmd.extend(["--provider", "openrouter"])
            server_cmd.extend(["--api-key", args.use_openrouter])
        else:
            # Map agent type to provider
            provider_map = {
                "codex": "openai",
                "claude": "anthropic",
                "gemini": "google",
                "opencode": "openrouter",
                "qwen": "tongyi",
                "cursor-cli": "openai",
                "goose": "openai",
            }
            provider = provider_map.get(args.agent_type, args.agent_type)
            server_cmd.extend(["--provider", provider])

            # Add API key if provided
            if args.llm_api_key:
                server_cmd.extend(["--api-key", args.llm_api_key])

    # Note: strict_tools_validation defaults to false in clap, so we don't need to pass it unless enabling it
    # If we wanted to enable it by default in the script, we could add --strict-tools-validation here

    # Add logging options if enabled
    if logging_enabled and args.request_log:
        server_cmd.extend(["--request-log", args.request_log])
    if logging_enabled and args.log_headers:
        server_cmd.append("--log-headers")
    if logging_enabled and args.log_body:
        server_cmd.append("--log-body")
    if logging_enabled and args.log_responses:
        server_cmd.append("--log-responses")

    # Build server environment
    server_environment = []
    if os.environ.get("FORCE_TOOLS_VALIDATION_FAILURE"):
        server_environment.append(f"FORCE_TOOLS_VALIDATION_FAILURE={os.environ['FORCE_TOOLS_VALIDATION_FAILURE']}")

    # Build server command as string (process-compose requires this)
    server_command = " ".join(f"'{arg}'" if "'" not in arg and " " in arg else f'"{arg}"' if " " in arg else arg for arg in server_cmd)

    # Build ah agent command (start or record)
    ah_cmd = [
        str(project_root / "target" / "debug" / "ah"),
        "agent",
    ]

    if args.record:
        ah_cmd.extend(["record", "--out-file", str(user_home_dir / "session.ahr")])
    else:
        ah_cmd.append("start")

    ah_cmd.extend(["--agent", args.agent_type])

    if args.non_interactive:
        ah_cmd.append("--non-interactive")

    if args.output_format:
        ah_cmd.extend(["--output", args.output_format])

    # Handle LLM API configuration
    mock_server_url = f"http://127.0.0.1:{args.server_port}"

    if use_scenario_mode:
        # In scenario mode, always use the mock server
        if args.llm_api:
            # When --llm-api is explicitly overridden, use it as provided
            ah_cmd.extend(["--llm-api", args.llm_api])
        else:
            # Claude uses base URL, others use /v1 suffix
            if args.agent_type == "claude":
                ah_cmd.extend(["--llm-api", mock_server_url])
            else:
                ah_cmd.extend(["--llm-api", f"{mock_server_url}/v1"])
    else:
        # In proxy mode, use the proxy server (which is the mock_server_url)
        if args.agent_type == "claude":
            ah_cmd.extend(["--llm-api", mock_server_url])
        else:
            ah_cmd.extend(["--llm-api", f"{mock_server_url}/v1"])

    # Handle API key configuration
    if use_scenario_mode:
        # In scenario mode, use provided key or generate random
        if args.llm_api_key:
            ah_cmd.extend(["--llm-api-key", args.llm_api_key])
        else:
            import secrets
            random_api_key = secrets.token_hex(16)  # 32 character random hex string
            ah_cmd.extend(["--llm-api-key", random_api_key])

    # Use initial prompt from scenario file
    ah_cmd.extend(["--prompt", initial_prompt])

    # Add working directory and environment variables
    ah_cmd.extend([
        "--working-copy", "in-place",
        "--cwd", str(repo_dir),
    ])

    # Build ah command as string
    ah_command = " ".join(f"'{arg}'" if "'" not in arg and " " in arg else f'"{arg}"' if " " in arg else arg for arg in ah_cmd)

    # Environment variables for ah command
    env_vars = {
        # Don't set HOME here - let AH CLI build config with custom home
        # and agent will copy credentials from real system HOME
        "AH_HOME": str(user_home_dir / ".ah"),
    }

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


    return mock_agent_dir, config, env_vars, server_environment, server_cmd, ah_command


def main():
    parser = argparse.ArgumentParser(
        description="Launch LLM API server (mock or live proxy) and ah agent start/record with process-compose",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog="""
Examples:
  # Basic test with default scenario (realistic_development_scenario) - interactive mode
  %(prog)s

  # Non-interactive mode with default scenario
  %(prog)s --non-interactive

  # Specific scenario with codex in interactive mode
  %(prog)s --scenario test_scenario

  # Claude agent with JSON output
  %(prog)s --scenario feature_implementation_scenario --agent-type claude --output-format json

  # PROXY MODE: Live API calls to Claude (requires ANTHROPIC_API_KEY env var)
  %(prog)s --agent-type claude

  # PROXY MODE: Live API calls to OpenAI via OpenRouter
  %(prog)s --use-openrouter sk-or-v1-your-openrouter-key

  # PROXY MODE: Live API calls with custom API key
  %(prog)s --agent-type claude --llm-api-key sk-ant-your-key

  # With custom OpenAI API endpoint
  %(prog)s --non-interactive --llm-api https://api.openai.com/v1 --llm-api-key sk-your-key

  # With custom Claude API
  %(prog)s --agent-type claude --llm-api https://api.anthropic.com --llm-api-key sk-ant-your-key

  # With custom Gemini API
  %(prog)s --agent-type gemini --llm-api https://generativelanguage.googleapis.com --llm-api-key your-gemini-key

  # Mock agent for testing
  %(prog)s --agent-type mock --scenario test_scenario

  # Disable all request logging
  %(prog)s --no-logging --agent-type claude

  # Disable specific logging components
  %(prog)s --no-log-headers --agent-type claude
  %(prog)s --no-log-body --agent-type claude
  %(prog)s --no-log-responses --agent-type claude

  # Custom logging configuration (headers, body, and responses enabled by default)
  %(prog)s --request-log /tmp/my-log.json --agent-type claude

  # Record a session instead of just starting agent
  %(prog)s --record --agent-type claude

  # Dry run to see what would be executed
  %(prog)s --dry-run

  # Run in foreground mode (ah agent gets your TTY, auto-cleanup)
  %(prog)s --foreground --agent-type claude

  # Run with TUI for monitoring all processes
  %(prog)s --tui --agent-type claude

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

    # Logging options
    parser.add_argument(
        "--request-log",
        help="Path to log request details (default: auto-generated in test-runs)"
    )

    parser.add_argument(
        "--log-headers",
        action="store_true",
        help="Include request headers in logs (enabled by default)"
    )

    parser.add_argument(
        "--no-log-headers",
        action="store_true",
        help="Disable logging of request headers"
    )

    parser.add_argument(
        "--log-body",
        action="store_true",
        help="Include request body in logs (enabled by default)"
    )

    parser.add_argument(
        "--no-log-body",
        action="store_true",
        help="Disable logging of request body"
    )

    parser.add_argument(
        "--log-responses",
        action="store_true",
        help="Include responses in logs (enabled by default)"
    )

    parser.add_argument(
        "--no-log-responses",
        action="store_true",
        help="Disable logging of responses"
    )

    parser.add_argument(
        "--no-logging",
        action="store_true",
        help="Disable all request logging"
    )

    # Scenario selection
    parser.add_argument(
        "--scenario",
        help="YAML scenario file to use for the mock server (if not specified, runs in live proxy mode)"
    )

    # Agent options
    parser.add_argument(
        "--agent-type",
        choices=["mock", "codex", "claude", "gemini", "opencode", "qwen", "cursor-cli", "goose"],
        default="codex",
        help="Agent type to start: mock (testing), codex (OpenAI), claude (Anthropic), gemini (Google), opencode, qwen, cursor-cli, goose (default: codex)"
    )

    parser.add_argument(
        "--record",
        action="store_true",
        help="Use 'ah agent record' instead of 'ah agent start' to record the session"
    )

    parser.add_argument(
        "--non-interactive",
        action="store_true",
        help="Enable non-interactive mode for the agent"
    )

    parser.add_argument(
        "--output-format",
        choices=["text", "text-normalized", "json", "json-normalized"],
        default="text",
        help="Output format for the agent (default: text)"
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
        "--use-openrouter",
        metavar="API_KEY",
        help="Route traffic through OpenRouter using the specified API key"
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

    parser.add_argument(
        "--process-compose",
        action="store_true",
        help="Use process-compose for orchestration instead of direct execution"
    )

    args = parser.parse_args()

    # Configure default logging behavior for testing
    # Enable full logging by default unless --no-logging is specified
    if not args.no_logging:
        # Set defaults for testing: enable full logging
        args.log_headers = True
        args.log_body = True
        args.log_responses = True

        # But allow disabling individual components
        if args.no_log_headers:
            args.log_headers = False
        if args.no_log_body:
            args.log_body = False
        if args.no_log_responses:
            args.log_responses = False

    # Validate scenario if provided
    if args.scenario:
        available_scenarios = get_scenario_files()
        if args.scenario not in available_scenarios:
            print(f"ERROR: Scenario '{args.scenario}' not found. Available scenarios:")
            for scenario in sorted(available_scenarios):
                print(f"  - {scenario}")
            sys.exit(1)

    # Get agent version early for both setup and config creation
    agent_version = get_agent_version(args.agent_type)
    print(f"Detected {args.agent_type} version: {agent_version}")
    # Note: logging not set up yet, will be logged later

    # Determine scenario name for working directory
    scenario_name = args.scenario if args.scenario else "default"

    # Set up working directory
    if args.working_dir:
        working_dir = Path(args.working_dir)
    else:
        # Create working directory in stable location relative to project root
        project_root = find_project_root()
        stable_base = project_root / "test-runs"
        stable_base.mkdir(exist_ok=True)
        working_dir = stable_base / f"test-{scenario_name}"

    # Create user-home directory early for logging
    user_home_dir = working_dir / "user-home"
    user_home_dir.mkdir(parents=True, exist_ok=True)

    # Set up script logging as early as possible
    script_log_file = setup_script_logging(user_home_dir)

    # Log what we've done so far
    logging.info(f"Agent type: {args.agent_type}")
    logging.info(f"Agent version: {agent_version}")
    logging.info(f"Scenario: {scenario_name}")
    logging.info(f"Working directory: {working_dir}")

    repo_dir, user_home_dir = setup_working_directory(scenario_name, working_dir, agent_version)

    # Create configuration
    mock_agent_dir, config, env_vars, server_environment, server_cmd, ah_command = create_process_compose_config(args, working_dir, repo_dir, user_home_dir, agent_version, args.foreground, args.tui)

    # Save config to file
    if args.config_file:
        config_path = Path(args.config_file)
    else:
        temp_file = tempfile.NamedTemporaryFile(mode='w', suffix='.yaml', delete=False)
        config_path = Path(temp_file.name)
        temp_file.close()

    try:
        import yaml
    except (ImportError, ModuleNotFoundError):
        print("Warning: PyYAML not available. " +
              "Please run this script in the nix dev shell, provided by the nix flake at the root of the repository.")
        sys.exit(1)

    with open(config_path, 'w') as f:
        yaml.dump(config, f, default_flow_style=False)

    print(f"Generated process-compose config: {config_path}")
    print(f"Working directory: {working_dir}")
    print(f"Repository: {repo_dir}")
    print(f"User home: {user_home_dir}")
    print(f"Script log: {script_log_file}")

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
        if args.process_compose:
            print(f"\nLaunching process-compose with config: {config_path}")

            try:
                # Set XDG_CONFIG_HOME for process-compose to avoid config directory issues
                env = os.environ.copy()
                # Create a temp directory for process-compose config
                xdg_config_dir = tempfile.mkdtemp(prefix="process_compose_config_")
                env["XDG_CONFIG_HOME"] = xdg_config_dir

                if args.foreground:
                    # Use process-compose run for foreground mode - attaches ah-agent to current TTY
                    cmd = [
                        "process-compose", "run", "ah-agent",
                        "--config", str(config_path)
                    ]
                    print(f"Running: {' '.join(cmd)}")
                    subprocess.run(cmd, check=True, env=env)
                else:
                    # Use process-compose up for background/TUI mode
                    cmd = [
                        "process-compose", "up",
                        "--config", str(config_path)
                    ]
                    if not args.tui:
                        cmd.append("--tui=false")  # Disable TUI for headless operation when not in TUI mode
                print(f"Running: {' '.join(cmd)}")
                result = subprocess.run(cmd, env=env, capture_output=True, text=True)
                if result.returncode != 0:
                    print(f"Process-compose stdout: {result.stdout}")
                    print(f"Process-compose stderr: {result.stderr}")
                    print(f"Process-compose failed with exit code {result.returncode}")
                    raise subprocess.CalledProcessError(result.returncode, cmd)
            except KeyboardInterrupt:
                print("\nProcess interrupted by user")
            except subprocess.CalledProcessError as e:
                print(f"Process-compose failed with exit code {e.returncode}")
                sys.exit(e.returncode)
            finally:
                # Clean up temporary config file
                if not args.config_file:
                    config_path.unlink(missing_ok=True)
        else:
            logging.info("Running commands directly without process-compose...")

            # Set environment variables for the processes
            env = os.environ.copy()
            env.update(env_vars)  # Add the AH agent environment variables

            # Set up log file for mock server output
            log_file = user_home_dir / "mock-server.log"

            try:
                # Set up mock server environment
                mock_env = os.environ.copy()
                for env_var in server_environment:
                    if '=' in env_var:
                        key, value = env_var.split('=', 1)
                        mock_env[key] = value

                # Create mock server manager with auto-restart capability
                server_manager = MockServerManager(
                    cmd=server_cmd,
                    env=mock_env,
                    cwd=str(mock_agent_dir),
                    log_file=log_file,
                    server_environment=server_environment
                )

                # Start the mock server with monitoring
                if not server_manager.start_server():
                    logging.error("Failed to start mock server initially")
                    print("ERROR: Failed to start mock server initially")
                    sys.exit(1)

                print(f"Mock server started. Monitoring for crashes with auto-restart.")
                print(f"Mock server logs: {log_file}")

                # Start monitoring thread for auto-restart
                server_manager.start_monitoring()

                # Register cleanup function to terminate mock server when script exits
                def cleanup_mock_server():
                    logging.info("Cleaning up mock server...")
                    server_manager.stop_monitoring()
                    server_manager.stop_server()

                atexit.register(cleanup_mock_server)

                # Wait a bit for server to start and stabilize
                logging.info("Waiting for mock server to start...")
                print("Waiting for mock server to start...")
                time.sleep(3)

                # Check if server is still running after initial wait
                if not server_manager.check_server_health():
                    logging.error("Mock server failed to start properly")
                    print("ERROR: Mock server failed to start properly")
                    sys.exit(1)

                logging.info("Starting AH agent in foreground...")
                print("Starting AH agent in foreground...")
                logging.info(f"AH agent command: {ah_command}")

                # Run AH agent in foreground - inherit stdin/stdout/stderr from parent process
                ah_process = subprocess.run(ah_command, shell=True, env=env, cwd=str(repo_dir))

                if ah_process.returncode != 0:
                    logging.error(f"AH agent failed with exit code {ah_process.returncode}")
                    print(f"AH agent failed with exit code {ah_process.returncode}")
                    sys.exit(ah_process.returncode)

            except KeyboardInterrupt:
                logging.info("Process interrupted by user")
                print("\nProcess interrupted by user")
            except subprocess.CalledProcessError as e:
                logging.error(f"Command failed with exit code {e.returncode}")
                print(f"Command failed with exit code {e.returncode}")
                sys.exit(e.returncode)
            except Exception as e:
                logging.error(f"Unexpected error: {e}")
                print(f"Unexpected error: {e}")
                sys.exit(1)


if __name__ == "__main__":
    main()
