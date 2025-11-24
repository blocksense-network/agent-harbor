#!/usr/bin/env python3
# Copyright 2025 Schelling Point Labs Inc
# SPDX-License-Identifier: AGPL-3.0-only
"""
Remote manual testing harness for the Agent Harbor TUI.

This script orchestrates a complete remote-mode workflow for the dashboard.
It can launch either the real Rust REST server or the TypeScript mock server,
seed them with curated demo data, and then start `ah tui --remote-server ...`
in the foreground so a developer can interact with the dashboard.

It also powers the automated smoke test (`just test-manual-remote-smoke`) by
providing a headless mode that exercises the orchestration without requiring
user interaction.
"""

from __future__ import annotations

import argparse
import atexit
import json
import logging
import os
import shutil
import sqlite3
import subprocess
import sys
import time
import urllib.error
import urllib.request
from contextlib import suppress
from datetime import datetime, timedelta, timezone
from pathlib import Path
from typing import Dict, List, Optional

# Make sure we can import shared helpers.
SCRIPT_DIR = Path(__file__).resolve().parent
if str(SCRIPT_DIR) not in sys.path:
    sys.path.insert(0, str(SCRIPT_DIR))

from test_utils import (  # noqa: E402
    create_timestamped_run_id,
    ensure_ah_binary,
    find_project_root,
    initialize_example_git_repo,
    isoformat_utc,
    print_command_info,
    print_dry_run_header,
    resolve_scenario_path,
    run_command,
    setup_script_logging,
)


# ---------------------------------------------------------------------------
# Argument parsing
# ---------------------------------------------------------------------------


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Launch Agent Harbor TUI in remote mode against a local REST/mock server.",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog="""
Examples:
  # Launch against the real REST server with default dataset
  %(prog)s

  # Launch against the mock server using a specific scenario
  %(prog)s --mode mock --scenario tui_testing_scenario.yaml

  # Run in headless/smoke mode (used by CI)
  %(prog)s --mode mock --smoke

  # Pass extra flags to the TUI (after `--`)
  %(prog)s -- --multiplexer tmux
        """,
    )

    parser.add_argument(
        "--mode",
        choices=("rest", "mock", "typescript-mock"),
        default="rest",
        help="Server type to launch (rest, rust mock, or TypeScript mock)",
    )
    parser.add_argument(
        "--port",
        type=int,
        help="TCP port for the server. Defaults to 38080 (rest) or 38180 (mock).",
    )
    parser.add_argument(
        "--repo",
        default="example-remote-repo",
        help="Name for the demo repository that will be created for the REST server demo.",
    )
    parser.add_argument(
        "--scenario",
        help="Scenario file (YAML) to load when running in mock mode. "
        "If relative, it is resolved against known scenario directories.",
    )
    parser.add_argument(
        "--scenario-speed",
        type=float,
        default=1.0,
        help="Playback speed multiplier for mock scenarios (Rust mock server only).",
    )
    parser.add_argument(
        "--api-key",
        help="API key for the remote server. Defaults to AH_REMOTE_API_KEY environment variable.",
    )
    parser.add_argument(
        "--bearer-token",
        help="Bearer token for the remote server. Defaults to AH_REMOTE_BEARER_TOKEN environment variable.",
    )
    parser.add_argument(
        "--tenant-id",
        help="Tenant ID to pass along to remote API calls (env: AH_REMOTE_TENANT_ID).",
    )
    parser.add_argument(
        "--project-id",
        help="Project ID to use for remote API calls (env: AH_REMOTE_PROJECT_ID).",
    )
    parser.add_argument(
        "--timeout",
        type=float,
        default=60.0,
        help="Seconds to wait for server health checks before giving up (default: 60).",
    )
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="Print planned actions and exit without launching anything.",
    )
    parser.add_argument(
        "--no-build",
        action="store_true",
        help="Assume binaries are already built; skip cargo/yarn build checks.",
    )
    parser.add_argument(
        "--smoke",
        action="store_true",
        help="Headless mode for automated smoke tests. "
        "Skips interactive TUI launch and validates orchestration deterministically.",
    )
    parser.add_argument(
        "--keep-running",
        action="store_true",
        help="Keep background processes alive after the TUI exits (for debugging). "
        "Ignored in smoke mode.",
    )
    parser.add_argument(
        "tui_args",
        nargs=argparse.REMAINDER,
        help="Additional arguments passed through to `ah tui` after `--`.",
    )

    return parser.parse_args()


# ---------------------------------------------------------------------------
# Utility helpers
# ---------------------------------------------------------------------------


class ProcessHandle:
    """Small helper to track background processes and ensure they terminate."""

    def __init__(self, name: str, proc: subprocess.Popen, log_path: Path):
        self.name = name
        self.proc = proc
        self.log_path = log_path

    def has_exited(self) -> bool:
        return self.proc.poll() is not None

    def exit_status(self) -> Optional[int]:
        return self.proc.poll()

    def terminate(self, grace_seconds: float = 5.0) -> None:
        if self.proc.poll() is not None:
            return
        logging.info("Stopping %s (pid=%s)...", self.name, self.proc.pid)
        with suppress(ProcessLookupError):
            self.proc.terminate()
        try:
            self.proc.wait(timeout=grace_seconds)
        except subprocess.TimeoutExpired:
            logging.warning("%s did not exit in %.1fs; killing", self.name, grace_seconds)
            with suppress(ProcessLookupError):
                self.proc.kill()
            with suppress(subprocess.TimeoutExpired):
                self.proc.wait(timeout=grace_seconds)
def wait_for_http_health(
    url: str,
    timeout: float,
    process_handle: Optional["ProcessHandle"] = None,
) -> None:
    """Poll a health endpoint until it returns HTTP 200 or the timeout expires."""
    deadline = time.time() + timeout
    last_error: Optional[Exception] = None
    logging.info("Waiting for server health check at %s ...", url)
    while time.time() < deadline:
        if process_handle and process_handle.has_exited():
            raise RuntimeError(
                f"{process_handle.name} exited early with code "
                f"{process_handle.exit_status()} (see {process_handle.log_path})"
            )
        try:
            with urllib.request.urlopen(url, timeout=5) as resp:
                if resp.status == 200:
                    logging.info("Health check succeeded: %s", url)
                    return
        except urllib.error.URLError as exc:  # noqa: PERF203 (fine for polling loop)
            last_error = exc
        time.sleep(0.5)
    raise RuntimeError(f"Server did not become healthy at {url}: {last_error}")


def seed_rest_database(
    db_path: Path,
    repo_path: Path,
    *,
    repo_name: str,
    remote_url: str,
) -> None:
    """Insert demo catalog entries into the REST server database."""
    logging.info("Seeding REST database with demo records: %s", db_path)
    created_at = isoformat_utc(datetime.now(timezone.utc))
    with sqlite3.connect(db_path) as conn:
        conn.row_factory = sqlite3.Row

        # Repositories
        root_path = str(repo_path)
        cur = conn.execute("SELECT id FROM repos WHERE root_path = ?", (root_path,))
        repo_row = cur.fetchone()
        if repo_row is None:
            conn.execute(
                """
                INSERT INTO repos (vcs, root_path, remote_url, default_branch, created_at)
                VALUES (?, ?, ?, ?, ?)
                """,
                ("git", root_path, remote_url, "main", created_at),
            )
            logging.info("Registered repository '%s' at %s", repo_name, root_path)

        # Agents
        conn.execute(
            """
            INSERT OR IGNORE INTO agents (name, version, metadata) VALUES (?, ?, ?)
            """,
            ("claude-code", "latest", None),
        )

        # Runtimes
        conn.execute(
            """
            INSERT OR IGNORE INTO runtimes (type, devcontainer_path, metadata)
            VALUES (?, ?, ?)
            """,
            ("local", None, None),
        )

        conn.commit()


def seed_mock_sessions(db_path: Path, run_dir: Path) -> None:
    """Populate the REST database with a curated set of mock sessions."""
    logging.info("Seeding mock REST dataset with active sessions: %s", db_path)

    now = datetime.now(timezone.utc)
    dataset = [
        {
            "id": "01HVZ6K9T1N8S6M3V3Q3F0X4",
            "prompt": "Refactor database queries for better performance",
            "status": "running",
            "started": now - timedelta(minutes=30),
            "ended": None,
            "repo_url": "https://github.com/user/backend-api",
            "branch": "feature/db-optimization",
            "agent_type": "openhands",
            "runtime_type": "devcontainer",
        },
        {
            "id": "01HVZ6K9T1N8S6M3V3Q3F0X5",
            "prompt": "Write comprehensive E2E tests for the checkout flow",
            "status": "running",
            "started": now - timedelta(minutes=10),
            "ended": None,
            "repo_url": "https://github.com/user/e-commerce",
            "branch": "feature/e2e-tests",
            "agent_type": "claude-code",
            "runtime_type": "local",
        },
        {
            "id": "01HVZ6K9T1N8S6M3V3Q3F0X1",
            "prompt": "Implement user authentication with email/password",
            "status": "completed",
            "started": now - timedelta(hours=2),
            "ended": now - timedelta(hours=1),
            "repo_url": "https://github.com/user/my-app",
            "branch": "main",
            "agent_type": "claude-code",
            "runtime_type": "devcontainer",
        },
        {
            "id": "01HVZ6K9T1N8S6M3V3Q3F0X2",
            "prompt": "Add payment processing with Stripe integration",
            "status": "completed",
            "started": now - timedelta(hours=4),
            "ended": now - timedelta(hours=3),
            "repo_url": "https://github.com/user/e-commerce",
            "branch": "develop",
            "agent_type": "openhands",
            "runtime_type": "devcontainer",
        },
        {
            "id": "01HVZ6K9T1N8S6M3V3Q3F0X3",
            "prompt": "Fix responsive design issues on mobile devices",
            "status": "completed",
            "started": now - timedelta(hours=6),
            "ended": now - timedelta(hours=5),
            "repo_url": "https://github.com/user/frontend",
            "branch": "hotfix/mobile-layout",
            "agent_type": "claude-code",
            "runtime_type": "local",
        },
    ]

    workspaces_root = run_dir / "workspaces"
    workspaces_root.mkdir(parents=True, exist_ok=True)

    with sqlite3.connect(db_path) as conn:
        conn.execute("DELETE FROM events")
        conn.execute("DELETE FROM tasks")
        conn.execute("DELETE FROM sessions")

        for entry in dataset:
            agent_config = json.dumps(
                {
                    "type": entry["agent_type"],
                    "version": "latest",
                    "settings": {},
                }
            )
            runtime_config = json.dumps(
                {
                    "type": entry["runtime_type"],
                    "devcontainer_path": None,
                    "resources": None,
                }
            )

            session_id = entry["id"]
            workspace_path = workspaces_root / session_id
            workspace_path.mkdir(parents=True, exist_ok=True)

            conn.execute(
                """
                INSERT INTO sessions (
                    id, status, started_at, ended_at, multiplexer_kind,
                    workspace_path, agent_config, runtime_config
                )
                VALUES (?, ?, ?, ?, ?, ?, ?, ?)
                """,
                (
                    session_id,
                    entry["status"],
                    isoformat_utc(entry["started"]),
                    isoformat_utc(entry["ended"]) if entry["ended"] else None,
                    "tmux",
                    str(workspace_path),
                    agent_config,
                    runtime_config,
                ),
            )

            conn.execute(
                """
                INSERT INTO tasks (
                    session_id, prompt, repo_url, branch, delivery, instances, labels
                )
                VALUES (?, ?, ?, ?, ?, ?, ?)
                """,
                (
                    session_id,
                    entry["prompt"],
                    entry["repo_url"],
                    entry["branch"],
                    None,
                    1,
                    None,
                ),
            )

        conn.commit()

    logging.info("Seeded %d mock session(s) with active workloads", len(dataset))


def create_sample_task(base_url: str) -> None:
    """Submit a sample task to the REST server so the dashboard has sessions to show."""
    payload = {
        "prompt": "Explore remote manual mode features",
        "repo": {
            "mode": "git",
            "url": "https://github.com/agent-harbor/demo-repo.git",
            "branch": "main",
        },
        "runtime": {
            "type": "local",
        },
        "agents": [
            {
                "agent": {
                    "software": "Claude",
                    "version": "latest",
                },
                "model": "claude-3.5-sonnet",
                "count": 1,
                "settings": {},
                "display_name": "Claude (Mock)",
            }
        ],
        "labels": {
            "origin": "manual-test-remote",
        },
    }
    data = json.dumps(payload).encode("utf-8")
    request = urllib.request.Request(
        f"{base_url}/tasks",
        data=data,
        method="POST",
        headers={"Content-Type": "application/json"},
    )
    try:
        with urllib.request.urlopen(request, timeout=10) as resp:
            if resp.status != 200:
                logging.warning("Unexpected status from create_task: %s", resp.status)
            else:
                logging.info("Submitted sample task to REST server")
    except urllib.error.URLError as exc:
        logging.warning("Failed to submit sample task: %s", exc)


def launch_process(
    cmd: List[str],
    *,
    cwd: Path,
    env: Dict[str, str],
    log_path: Path,
    dry_run: bool = False,
) -> ProcessHandle:
    log_path.parent.mkdir(parents=True, exist_ok=True)
    logging.info("Launching %s (logging to %s)", " ".join(cmd), log_path)
    if dry_run:
        print_command_info(
            "Server command",
            " ".join(cmd),
            working_dir=str(cwd),
            environment=[f"{key}={value}" for key, value in env.items()],
        )
        return ProcessHandle(cmd[0], subprocess.Popen(["true"]), log_path)
    log_file = open(log_path, "ab", buffering=0)
    proc = subprocess.Popen(
        cmd,
        cwd=str(cwd),
        env=env,
        stdout=log_file,
        stderr=subprocess.STDOUT,
    )
    return ProcessHandle(cmd[0], proc, log_path)


# ---------------------------------------------------------------------------
# Mode-specific orchestration
# ---------------------------------------------------------------------------


def start_rest_mode(
    project_root: Path,
    run_dir: Path,
    log_dir: Path,
    args: argparse.Namespace,
    *,
    dry_run: bool,
) -> Dict[str, object]:
    port = args.port or 38080
    base_url = f"http://127.0.0.1:{port}/api/v1"
    rest_log_path = log_dir / "rest-server.log"
    repo_dir = run_dir / args.repo
    if dry_run:
        print(f"[dry-run] Would initialise example repository at {repo_dir}")
    else:
        initialize_example_git_repo(
            repo_dir,
            commit_message="Initial commit for remote demo",
        )

    db_path = run_dir / "rest-server.sqlite"
    if args.no_build:
        binary = (
            project_root
            / "target"
            / ("release" if os.environ.get("AH_BUILD_RELEASE") else "debug")
            / "ah-rest-server"
        )
        if binary.exists():
            cmd = [
                str(binary),
                "--bind",
                f"127.0.0.1:{port}",
                "--database",
                str(db_path),
                "--cors",
            ]
        else:
            logging.debug(
                "ah-rest-server binary not found at %s, falling back to cargo run",
                binary,
            )
            cmd = [
                "cargo",
                "run",
                "-p",
                "ah-rest-server",
                "--",
                "--bind",
                f"127.0.0.1:{port}",
                "--database",
                str(db_path),
                "--cors",
            ]
    else:
        cmd = [
            "cargo",
            "run",
            "-p",
            "ah-rest-server",
            "--",
            "--bind",
            f"127.0.0.1:{port}",
            "--database",
            str(db_path),
            "--cors",
        ]
    env = os.environ.copy()
    env.setdefault("RUST_LOG", "info,ah_rest_server=info")

    handle = launch_process(
        cmd,
        cwd=project_root,
        env=env,
        log_path=rest_log_path,
        dry_run=dry_run,
    )
    if dry_run:
        return {
            "port": port,
            "base_url": base_url,
            "server_handle": handle,
            "server_type": "Mock REST (Rust)",
        }
    if dry_run:
        print(f"[dry-run] Would seed REST database at {db_path}")
        print(f"[dry-run] Would submit sample task to {base_url}/tasks")
        return {
            "port": port,
            "base_url": base_url,
            "repo_dir": repo_dir,
            "server_handle": handle,
            "server_type": "REST",
        }
    wait_for_http_health(f"{base_url}/healthz", timeout=args.timeout, process_handle=handle)

    seed_rest_database(
        db_path,
        repo_dir,
        repo_name=args.repo,
        remote_url="https://github.com/agent-harbor/demo-repo.git",
    )
    create_sample_task(base_url)

    return {
        "port": port,
        "base_url": base_url,
        "repo_dir": repo_dir,
        "server_handle": handle,
        "server_type": "REST",
    }


def start_rust_mock_mode(
    project_root: Path,
    run_dir: Path,
    log_dir: Path,
    args: argparse.Namespace,
    *,
    dry_run: bool,
) -> Dict[str, object]:
    port = args.port or 38180
    base_url = f"http://127.0.0.1:{port}/api/v1"
    rest_log_path = log_dir / "rest-server.log"

    scenario_paths: List[Path] = []
    if args.scenario:
        try:
            scenario_paths.append(resolve_scenario_path(project_root, args.scenario))
        except FileNotFoundError:
            logging.warning(
                "Scenario file '%s' not found; the mock server will fall back to default data",
                args.scenario,
            )
    else:
        default_dir = project_root / "test_scenarios"
        if default_dir.exists():
            scenario_paths.append(default_dir)
        else:
            logging.warning(
                "No --scenario provided and %s does not exist; mock server will use in-memory data",
                default_dir,
            )

    def append_common_args(cmd: List[str]) -> None:
        cmd.extend(["--bind", f"127.0.0.1:{port}", "--cors"])
        for path in scenario_paths:
            cmd.extend(["--scenario", str(path)])
        if args.scenario_speed and args.scenario_speed != 1.0:
            cmd.extend(["--scenario-speed", f"{args.scenario_speed:.3f}"])

    if args.no_build:
        binary = (
            project_root
            / "target"
            / ("release" if os.environ.get("AH_BUILD_RELEASE") else "debug")
            / "mock_server"
        )
        if binary.exists():
            cmd = [str(binary)]
            append_common_args(cmd)
        else:
            logging.debug(
                "mock_server binary not found at %s, falling back to cargo run",
                binary,
            )
            cmd = [
                "cargo",
                "run",
                "-p",
                "ah-rest-server",
                "--bin",
                "mock_server",
                "--",
            ]
            append_common_args(cmd)
    else:
        cmd = [
            "cargo",
            "run",
            "-p",
            "ah-rest-server",
            "--bin",
            "mock_server",
            "--",
        ]
        append_common_args(cmd)

    env = os.environ.copy()
    env.setdefault("RUST_LOG", "info,ah_rest_server=info")

    handle = launch_process(
        cmd,
        cwd=project_root,
        env=env,
        log_path=rest_log_path,
        dry_run=dry_run,
    )
    wait_for_http_health(
        f"{base_url}/healthz",
        timeout=args.timeout,
        process_handle=handle,
    )

    if not dry_run:
        logging.info("Submitting sample task to mock REST server for smoke validation")
        create_sample_task(base_url)

    return {
        "port": port,
        "base_url": base_url,
        "server_handle": handle,
        "server_type": "Mock REST (Rust)",
    }


def start_typescript_mock_mode(
    project_root: Path,
    run_dir: Path,
    log_dir: Path,
    args: argparse.Namespace,
    *,
    dry_run: bool,
) -> Dict[str, object]:
    port = args.port or 3001
    base_url = f"http://127.0.0.1:{port}/api/v1"
    rest_log_path = log_dir / "typescript-mock-server.log"

    scenario_args: List[str] = []
    if args.scenario:
        try:
            scenario_path = resolve_scenario_path(project_root, args.scenario)
            scenario_args = ["--", "--scenario", str(scenario_path)]
        except FileNotFoundError:
            logging.warning("Scenario file '%s' not found; continuing without it", args.scenario)
    if args.scenario_speed and args.scenario_speed != 1.0:
        logging.warning(
            "--scenario-speed is ignored for the TypeScript mock server (value %.2f)",
            args.scenario_speed,
        )

    cmd = [
        "yarn",
        "workspace",
        "ah-webui-mock-server",
        "run",
        "dev",
    ]
    if scenario_args:
        cmd.extend(scenario_args)

    env = os.environ.copy()
    env.setdefault("PORT", str(port))
    env.setdefault("SERVER_LOG_FILE", str(rest_log_path))

    handle = launch_process(
        cmd,
        cwd=project_root,
        env=env,
        log_path=rest_log_path,
        dry_run=dry_run,
    )

    if dry_run:
        return {
            "port": port,
            "base_url": base_url,
            "server_handle": handle,
            "server_type": "Mock REST (TypeScript)",
        }

    wait_for_http_health(
        f"http://127.0.0.1:{port}/health",
        timeout=args.timeout,
        process_handle=handle,
    )

    return {
        "port": port,
        "base_url": base_url,
        "server_handle": handle,
        "server_type": "Mock REST (TypeScript)",
    }


# ---------------------------------------------------------------------------
# TUI Invocation and smoke validation
# ---------------------------------------------------------------------------


def prepare_tui_environment(run_dir: Path, args: argparse.Namespace) -> Dict[str, str]:
    env = os.environ.copy()
    ah_home = run_dir / "ah-home"
    ah_home.mkdir(parents=True, exist_ok=True)
    env["AH_HOME"] = str(ah_home)

    if args.tenant_id or os.environ.get("AH_REMOTE_TENANT_ID"):
        env["AH_REMOTE_TENANT_ID"] = args.tenant_id or os.environ.get("AH_REMOTE_TENANT_ID", "")
    if args.project_id or os.environ.get("AH_REMOTE_PROJECT_ID"):
        env["AH_REMOTE_PROJECT_ID"] = args.project_id or os.environ.get("AH_REMOTE_PROJECT_ID", "")

    return env


def launch_tui(
    project_root: Path,
    run_dir: Path,
    remote_info: Dict[str, object],
    args: argparse.Namespace,
) -> None:
    binary = ensure_ah_binary(project_root, release=False)
    env = prepare_tui_environment(run_dir, args)

    base_url = remote_info["base_url"]
    cmd = [str(binary), "tui", "--remote-server", str(base_url)]
    api_key = args.api_key or os.environ.get("AH_REMOTE_API_KEY")
    bearer = args.bearer_token or os.environ.get("AH_REMOTE_BEARER_TOKEN")
    if api_key:
        cmd.extend(["--api-key", api_key])
    if bearer:
        cmd.extend(["--bearer-token", bearer])
    if args.tui_args:
        cmd.extend(args.tui_args)

    logging.info("Launching TUI with command: %s", " ".join(cmd))
    if args.dry_run:
        print_command_info(
            "TUI command",
            " ".join(cmd),
            working_dir=str(run_dir),
            environment=[f"{k}={v}" for k, v in env.items()],
        )
        return
    subprocess.run(cmd, cwd=str(run_dir), env=env, check=True)


def run_smoke_validation(remote_info: Dict[str, object]) -> None:
    """Simple validation used by the automated smoke test."""
    base_url = str(remote_info["base_url"])
    sessions_endpoint = f"{base_url}/sessions"
    server_type = remote_info.get("server_type", "REST")
    try:
        with urllib.request.urlopen(sessions_endpoint, timeout=10) as resp:
            data = json.loads(resp.read().decode("utf-8"))
            if isinstance(data, dict):
                items = data.get("items", [])
            else:
                items = data
            logging.info(
                "Smoke validation: fetched %d session(s) from %s",
                len(items),
                sessions_endpoint,
            )
            active = [
                item
                for item in items
                if isinstance(item, dict) and item.get("status") == "running"
            ]
            if server_type.lower().startswith("mock"):
                if not active:
                    raise RuntimeError(
                        "Smoke validation expected at least one running session in mock dataset"
                    )
                logging.info(
                    "Smoke validation: %d session(s) reporting status=running", len(active)
                )
    except Exception as exc:  # noqa: BLE001
        raise RuntimeError(f"Smoke validation failed for {sessions_endpoint}: {exc}") from exc


# ---------------------------------------------------------------------------
# Main orchestration
# ---------------------------------------------------------------------------


def main() -> None:
    args = parse_args()
    project_root = find_project_root()
    run_id = create_timestamped_run_id("remote", args.mode, args.scenario)

    runs_dir = project_root / "manual-tests" / "runs"
    logs_dir = project_root / "manual-tests" / "logs"
    run_dir = runs_dir / run_id
    log_dir = logs_dir / run_id

    run_dir.mkdir(parents=True, exist_ok=True)
    log_dir.mkdir(parents=True, exist_ok=True)

    script_log_path = setup_script_logging(log_dir)
    logging.info("Remote manual test run id: %s", run_id)
    logging.info("Project root: %s", project_root)
    logging.info("Run directory: %s", run_dir)
    logging.info("Log directory: %s", log_dir)
    logging.info("Script log: %s", script_log_path)

    if args.dry_run:
        print_dry_run_header()

    active_processes: List[ProcessHandle] = []

    def cleanup() -> None:
        if args.keep_running and not args.smoke:
            logging.info("Skipping automatic teardown because --keep-running was specified.")
            return
        while active_processes:
            handle = active_processes.pop()
            handle.terminate()

    atexit.register(cleanup)

    try:
        if args.mode == "rest":
            remote_info = start_rest_mode(project_root, run_dir, log_dir, args, dry_run=args.dry_run)
        elif args.mode == "mock":
            remote_info = start_rust_mock_mode(
                project_root, run_dir, log_dir, args, dry_run=args.dry_run
            )
        else:
            remote_info = start_typescript_mock_mode(
                project_root, run_dir, log_dir, args, dry_run=args.dry_run
            )

        if not args.dry_run:
            active_processes.append(remote_info["server_handle"])

        if args.dry_run:
            print(f"[dry-run] Run directory: {run_dir}")
            print(f"[dry-run] Log directory: {log_dir}")
            print("[dry-run] Skipping smoke validation and TUI launch.")
            return
        if args.smoke:
            run_smoke_validation(remote_info)
        else:
            launch_tui(project_root, run_dir, remote_info, args)

        logging.info("Run artifacts available at: %s", run_dir)
        logging.info("Server logs stored at: %s", remote_info["server_handle"].log_path)
    finally:
        if args.keep_running and not args.smoke:
            logging.info(
                "Background processes left running due to --keep-running. "
                "Use Ctrl+C to terminate manually."
            )
            try:
                while True:
                    time.sleep(1)
            except KeyboardInterrupt:
                cleanup()


if __name__ == "__main__":
    try:
        main()
    except KeyboardInterrupt:
        logging.info("Interrupted by user")
        sys.exit(130)
    except Exception as exc:  # noqa: BLE001
        logging.exception("Manual remote test harness failed: %s", exc)
        sys.exit(1)

