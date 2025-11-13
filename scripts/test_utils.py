#!/usr/bin/env python3
# Copyright 2025 Schelling Point Labs Inc
# SPDX-License-Identifier: AGPL-3.0-only

"""
Shared utilities for manual test scripts.

This module contains common functionality used by various manual test scripts
to reduce code duplication and maintain consistency.
"""

import logging
import shutil
import subprocess
import sys
from datetime import datetime, timezone
from pathlib import Path
from typing import Dict, Optional, Sequence


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


def find_zfs_mount_point():
    """Find the ZFS test filesystem mount point if available."""
    try:
        # Check if ZFS tools are available
        result = subprocess.run(["which", "zfs"], capture_output=True, text=True)
        if result.returncode != 0:
            return None

        # Check if our test pool exists
        result = subprocess.run(["zpool", "list", "AH_test_zfs"], capture_output=True)
        if result.returncode != 0:
            return None

        # Check if dataset exists
        result = subprocess.run(["zfs", "list", "AH_test_zfs/test_dataset"], capture_output=True, text=True)
        if result.returncode != 0:
            return None

        # Get mountpoint
        result = subprocess.run(["zfs", "get", "-H", "-o", "value", "mountpoint", "AH_test_zfs/test_dataset"],
                              capture_output=True, text=True)
        if result.returncode == 0:
            mountpoint = result.stdout.strip()
            if Path(mountpoint).exists():
                return Path(mountpoint)

    except Exception as e:
        logging.warning(f"Error detecting ZFS filesystem: {e}")

    return None


def print_dry_run_header():
    """Print the standard dry-run header."""
    print("\nDRY RUN - Commands that would be executed:")
    print("=" * 50)


def print_command_info(title, command, working_dir=None, environment=None):
    """Print formatted command information for dry-run output."""
    print(f"{title}:")
    if working_dir:
        print(f"  Working Directory: {working_dir}")
    print(f"  Command: {command}")
    if environment:
        print("  Environment Variables:")
        for env_var in environment:
            print(f"    {env_var}")
    print()


def print_filesystem_info(working_dir, repo_dir=None):
    """Print filesystem information for dry-run output."""
    zfs_mount = find_zfs_mount_point()
    if zfs_mount and working_dir.is_relative_to(zfs_mount):
        filesystem_info = f"ZFS test filesystem ({zfs_mount})"
    else:
        filesystem_info = "Project test-runs directory"

    print(f"Filesystem: {filesystem_info}")
    print(f"Working Directory: {working_dir}")
    if repo_dir:
        print(f"Repository: {repo_dir}")
        print(f"Repository Contents: {list(repo_dir.glob('**/*'))}")


def run_command(
    cmd: Sequence[str],
    *,
    cwd: Optional[Path] = None,
    env: Optional[Dict[str, str]] = None,
) -> subprocess.CompletedProcess:
    """Run a subprocess command with logging and error handling."""
    cmd_list = [str(part) for part in cmd]
    logging.debug("Running command: %s", " ".join(cmd_list))
    return subprocess.run(
        cmd_list,
        cwd=str(cwd) if cwd else None,
        env=env,
        check=True,
    )


def ensure_ah_binary(project_root: Path, *, release: bool = False) -> Path:
    """Ensure the `ah` CLI binary is built and return its path."""
    profile = "release" if release else "debug"
    binary = project_root / "target" / profile / "ah"
    if binary.exists():
        return binary

    cmd = ["cargo", "build", "-p", "ah-cli"]
    if release:
        cmd.append("--release")
    run_command(cmd, cwd=project_root)

    if not binary.exists():
        raise RuntimeError(f"Failed to build ah binary at {binary}")
    return binary


def initialize_example_git_repo(
    repo_path: Path,
    *,
    example_name: str = "python-hello-world",
    user_name: str = "Agent Harbor Demo",
    user_email: str = "demo@example.com",
    commit_message: str = "Initial commit",
) -> int:
    """
    Copy an example repository and initialise it as a git repository.

    Returns the number of files (excluding .git metadata) present in the repository.
    """
    project_root = find_project_root()
    example_src = project_root / "tests" / "example-repos" / example_name
    if not example_src.exists():
        raise FileNotFoundError(f"Example repository missing at {example_src}")

    if repo_path.exists():
        shutil.rmtree(repo_path)
    repo_path.parent.mkdir(parents=True, exist_ok=True)

    shutil.copytree(example_src, repo_path)

    run_command(["git", "init"], cwd=repo_path)
    run_command(["git", "config", "user.name", user_name], cwd=repo_path)
    run_command(["git", "config", "user.email", user_email], cwd=repo_path)
    run_command(["git", "config", "commit.gpgsign", "false"], cwd=repo_path)
    run_command(["git", "add", "."], cwd=repo_path)
    run_command(["git", "commit", "-m", commit_message], cwd=repo_path)

    file_count = sum(
        1
        for path in repo_path.rglob("*")
        if path.is_file() and ".git" not in path.relative_to(repo_path).parts
    )
    logging.debug(
        "Initialised example repository at %s with %d files", repo_path, file_count
    )
    return file_count


def resolve_scenario_path(project_root: Path, name: str) -> Path:
    """Resolve a scenario path relative to standard scenario directories."""
    candidate = Path(name)
    if candidate.is_absolute() and candidate.exists():
        return candidate

    search_roots = [
        project_root / "tests" / "tools" / "mock-agent" / "scenarios",
        project_root / "tests" / "tools" / "mock-agent" / "examples",
        project_root / "specs" / "Public",
        project_root / "test_scenarios",
    ]
    for root in search_roots:
        candidate = root / name
        if candidate.exists():
            return candidate
    raise FileNotFoundError(f"Unable to locate scenario file '{name}' in known paths")


def isoformat_utc(dt: datetime) -> str:
    """Format a datetime as an ISO-8601 UTC string."""
    if dt.tzinfo is None:
        dt = dt.replace(tzinfo=timezone.utc)
    return dt.astimezone(timezone.utc).isoformat(timespec="seconds").replace("+00:00", "Z")


def create_timestamped_run_id(
    prefix: str,
    mode: Optional[str] = None,
    tag: Optional[str] = None,
) -> str:
    """Create a timestamped identifier suitable for manual test runs."""
    timestamp = datetime.now(timezone.utc).strftime("%Y%m%d-%H%M%S")
    parts = [prefix]
    if mode:
        parts.append(mode)
    parts.append(timestamp)
    if tag:
        parts.append(Path(tag).stem if tag else tag)
    sanitised = [str(part).replace(" ", "_") for part in parts if part]
    return "-".join(sanitised)
