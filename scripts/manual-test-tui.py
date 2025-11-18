#!/usr/bin/env python3
# Copyright 2025 Schelling Point Labs Inc
# SPDX-License-Identifier: AGPL-3.0-only

"""
Manual TUI Test Script

This script sets up an example repository in the specified test filesystem and launches the `ah tui` command
for manual testing and development verification.

The script creates a sample git repository with some example files and then runs the TUI
in local mode for interactive testing.

Supported filesystem types:
- zfs: ZFS test filesystem (default)
- btrfs: Btrfs test filesystem (Linux only)
- apfs: APFS test filesystem (macOS only)
- tmp: Temporary directory in project test-runs folder
"""

import argparse
import logging
import os
import subprocess
import sys
from pathlib import Path

# Import shared utilities
from test_utils import (
    ensure_ah_binary,
    find_project_root,
    find_zfs_mount_point,
    initialize_example_git_repo,
    print_command_info,
    print_dry_run_header,
    print_filesystem_info,
    setup_script_logging,
)


def find_filesystem_mount_point(fs_type):
    """Find the mount point for the specified filesystem type."""
    if fs_type == "zfs":
        return find_zfs_mount_point()
    elif fs_type == "btrfs":
        # Check if Btrfs test filesystem is available
        try:
            # Try to find Btrfs mount point (this is a simplified check)
            result = subprocess.run(["mount"], capture_output=True, text=True)
            for line in result.stdout.split('\n'):
                if 'btrfs' in line and 'AH_test_btrfs' in line:
                    # Extract mount point from line like: /dev/loop99 on /tmp/AH_test_btrfs type btrfs (rw,relatime)
                    parts = line.split()
                    if len(parts) >= 3 and parts[2] == 'type':
                        return Path(parts[0])
        except Exception:
            pass
        return None
    elif fs_type == "apfs":
        # Check if APFS test filesystem is available (macOS only)
        try:
            result = subprocess.run(["mount"], capture_output=True, text=True)
            for line in result.stdout.split('\n'):
                if 'apfs' in line and 'AH_test_apfs' in line:
                    # Extract mount point
                    parts = line.split()
                    if len(parts) >= 3:
                        return Path(parts[2])
        except Exception:
            pass
        return None
    elif fs_type == "tmp":
        # Use project-relative test-runs directory
        project_root = find_project_root()
        return project_root / "test-runs"
    else:
        raise ValueError(f"Unknown filesystem type: {fs_type}")



def main():
    parser = argparse.ArgumentParser(
        description="Set up example repository and launch ah tui for manual testing",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog="""
Examples:
  # Basic TUI test with default repository and filesystem
  %(prog)s

  # Custom repository name
  %(prog)s --repo my-test-repo

  # Use specific filesystem type
  %(prog)s --fs btrfs

  # Custom repository and filesystem
  %(prog)s --repo my-test-repo --fs zfs

  # Use temporary directory instead of test filesystem
  %(prog)s --fs tmp

  # Enable experimental features (multiple flags)
  %(prog)s --experimental-features copilot --experimental-features gemini

  # Enable experimental features (comma-separated)
  %(prog)s --experimental-features copilot,gemini

  # Dry run to see what would be executed
  %(prog)s --dry-run
        """
    )

    # Repository options
    parser.add_argument(
        "--repo",
        default="example-repo",
        help="Name of the repository to create (default: example-repo)"
    )

    # Filesystem options
    parser.add_argument(
        "--fs",
        choices=["zfs", "btrfs", "apfs", "tmp"],
        default="zfs",
        help="Filesystem type to use for testing (default: zfs)"
    )

    # Working directory options
    parser.add_argument(
        "--working-dir",
        help="Working directory for the test run (default: auto-generated based on --fs)"
    )

    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="Print what would be executed without running anything"
    )

    # Experimental features options
    parser.add_argument(
        "--experimental-features",
        action="append",
        help="Enable experimental features (can be specified multiple times or as comma-separated values)"
    )

    args = parser.parse_args()

    # Determine working directory
    if args.working_dir:
        working_dir = Path(args.working_dir)
        fs_mount = None
    else:
        # Try to use the specified filesystem type
        fs_mount = find_filesystem_mount_point(args.fs)
        if fs_mount:
            if args.fs == "tmp":
                # For tmp, fs_mount is already the base directory
                working_dir = fs_mount / f"tui-test-{args.repo}"
                print(f"Using temporary directory filesystem at: {fs_mount}")
            else:
                # For real filesystems, create subdirectory in the mount
                working_dir = fs_mount / f"tui-test-{args.repo}"
                print(f"Using {args.fs.upper()} test filesystem at: {fs_mount}")
        else:
            # Fall back to project-relative test-runs directory
            project_root = find_project_root()
            stable_base = project_root / "test-runs"
            stable_base.mkdir(exist_ok=True)
            working_dir = stable_base / f"tui-test-{args.repo}"
            print(f"{args.fs.upper()} test filesystem not available, using project test-runs directory")

    # Create working directory and subdirectories
    working_dir.mkdir(parents=True, exist_ok=True)
    user_home_dir = working_dir / "user-home"
    user_home_dir.mkdir(exist_ok=True)

    # Set up script logging as early as possible
    script_log_file = setup_script_logging(user_home_dir)

    # Log what we're doing
    logging.info(f"Repository name: {args.repo}")
    logging.info(f"Filesystem type: {args.fs}")
    logging.info(f"Working directory: {working_dir}")
    logging.info(f"User home: {user_home_dir}")

    # Set up repository
    repo_dir = working_dir / args.repo
    file_count = initialize_example_git_repo(
        repo_dir,
        user_name="Test User",
        user_email="test@example.com",
        commit_message="Initial commit: Add example repository structure",
    )

    print(f"Created example repository with {file_count} files")
    print(f"Generated test repository: {repo_dir}")
    print(f"Working directory: {working_dir}")
    print(f"User home: {user_home_dir}")
    print(f"Script log: {script_log_file}")

    # Build ah tui command
    project_root = find_project_root()
    ah_binary = ensure_ah_binary(project_root, release=False)
    ah_cmd = [str(ah_binary), "tui"]

    # Add experimental features if specified
    if args.experimental_features:
        for feature_arg in args.experimental_features:
            # Split comma-separated values
            features = [f.strip() for f in feature_arg.split(',') if f.strip()]
            for feature in features:
                ah_cmd.extend(["--experimental-features", feature])

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
