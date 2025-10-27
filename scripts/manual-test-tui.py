#!/usr/bin/env python3
# Copyright 2025 Schelling Point Labs Inc
# SPDX-License-Identifier: AGPL-3.0-only

"""
Manual TUI Test Script

This script sets up an example repository in the test filesystem and launches the `ah tui` command
for manual testing and development verification.

The script creates a sample git repository with some example files and then runs the TUI
in local mode for interactive testing.
"""

import argparse
import logging
import os
import shutil
import subprocess
import sys
from pathlib import Path

# Import shared utilities
from test_utils import (
    setup_script_logging,
    find_project_root,
    find_zfs_mount_point,
    print_dry_run_header,
    print_command_info,
    print_filesystem_info
)




def setup_example_repository(repo_dir, repo_name):
    """Set up an example git repository by copying from the example-repos directory."""
    logging.info(f"Setting up example repository: {repo_dir}")

    # Clean up existing repo if it exists
    if repo_dir.exists():
        logging.info(f"Cleaning up existing repository: {repo_dir}")
        print(f"Cleaning up existing repository: {repo_dir}")
        shutil.rmtree(repo_dir)

    # Find the example repository source
    script_dir = Path(__file__).parent
    example_repo_src = script_dir.parent / "tests" / "example-repos" / repo_name

    if not example_repo_src.exists():
        raise FileNotFoundError(f"Example repository not found: {example_repo_src}")

    # Copy the example repository
    shutil.copytree(example_repo_src, repo_dir)

    # Initialize git repository
    subprocess.run(["git", "init"], cwd=repo_dir, check=True, capture_output=True)
    subprocess.run(["git", "config", "user.name", "Test User"], cwd=repo_dir, check=True)
    subprocess.run(["git", "config", "user.email", "test@example.com"], cwd=repo_dir, check=True)

    # Disable GPG signing for commits in test repo
    subprocess.run(["git", "config", "commit.gpgsign", "false"], cwd=repo_dir, check=True)

    # Create initial commit
    subprocess.run(["git", "add", "."], cwd=repo_dir, check=True)
    subprocess.run(["git", "commit", "-m", "Initial commit: Add example repository structure"], cwd=repo_dir, check=True)

    # Count files (excluding .git directory)
    file_count = sum(1 for _ in repo_dir.rglob('*') if _.is_file() and '.git' not in str(_.relative_to(repo_dir)))

    logging.info("Example repository setup complete")
    print(f"Created example repository with {file_count} files")


def main():
    parser = argparse.ArgumentParser(
        description="Set up example repository and launch ah tui for manual testing",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog="""
Examples:
  # Basic TUI test with default repository
  %(prog)s

  # Custom repository name
  %(prog)s --repo-name my-test-repo

  # Use existing repository if available
  %(prog)s --repo-name existing-repo

  # Dry run to see what would be executed
  %(prog)s --dry-run
        """
    )

    # Repository options
    parser.add_argument(
        "--repo-name",
        default="example-repo",
        help="Name of the repository to create (default: example-repo)"
    )

    # Working directory options
    parser.add_argument(
        "--working-dir",
        help="Working directory for the test run (default: auto-generated)"
    )

    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="Print what would be executed without running anything"
    )

    args = parser.parse_args()

    # Determine working directory - prefer ZFS test filesystem if available
    if args.working_dir:
        working_dir = Path(args.working_dir)
    else:
        # Try to use ZFS test filesystem first
        zfs_mount = find_zfs_mount_point()
        if zfs_mount:
            working_dir = zfs_mount / f"tui-test-{args.repo_name}"
            print(f"Using ZFS test filesystem at: {zfs_mount}")
        else:
            # Fall back to project-relative test-runs directory
            project_root = find_project_root()
            stable_base = project_root / "test-runs"
            stable_base.mkdir(exist_ok=True)
            working_dir = stable_base / f"tui-test-{args.repo_name}"
            print("ZFS test filesystem not available, using project test-runs directory")

    # Create working directory and subdirectories
    working_dir.mkdir(parents=True, exist_ok=True)
    user_home_dir = working_dir / "user-home"
    user_home_dir.mkdir(exist_ok=True)

    # Set up script logging as early as possible
    script_log_file = setup_script_logging(user_home_dir)

    # Log what we're doing
    logging.info(f"Repository name: {args.repo_name}")
    logging.info(f"Working directory: {working_dir}")
    logging.info(f"User home: {user_home_dir}")

    # Set up repository
    repo_dir = working_dir / args.repo_name
    setup_example_repository(repo_dir, "python-hello-world")

    print(f"Generated test repository: {repo_dir}")
    print(f"Working directory: {working_dir}")
    print(f"User home: {user_home_dir}")
    print(f"Script log: {script_log_file}")

    # Build ah tui command
    project_root = find_project_root()
    ah_cmd = [
        str(project_root / "target" / "debug" / "ah"),
        "tui"
    ]

    # Build command as string
    ah_command = " ".join(f"'{arg}'" if "'" not in arg and " " in arg else f'"{arg}"' if " " in arg else arg for arg in ah_cmd)

    if args.dry_run:
        print_dry_run_header()
        print_filesystem_info(working_dir, repo_dir)
        print_command_info("AH TUI Command", ah_command, working_dir=str(repo_dir))
        return

    print("\nLaunching AH TUI...")
    print(f"Repository: {repo_dir}")
    print(f"Command: cd {repo_dir} && {ah_command}")
    print("Press Ctrl+C to exit")
    print()

    # Set environment variables
    env = os.environ.copy()
    env["AH_HOME"] = str(user_home_dir / ".ah")

    try:
        # Run AH TUI in foreground - inherit stdin/stdout/stderr from parent process
        # cd into the repository directory and run ah tui there
        result = subprocess.run(ah_command, shell=True, env=env, cwd=str(repo_dir))
        if result.returncode != 0:
            logging.error(f"AH TUI failed with exit code {result.returncode}")
            print(f"AH TUI failed with exit code {result.returncode}")
            sys.exit(result.returncode)
    except KeyboardInterrupt:
        logging.info("TUI interrupted by user")
        print("\nTUI interrupted by user")
    except Exception as e:
        logging.error(f"Unexpected error: {e}")
        print(f"Unexpected error: {e}")
        sys.exit(1)


if __name__ == "__main__":
    main()
