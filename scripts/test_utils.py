#!/usr/bin/env python3
# Copyright 2025 Schelling Point Labs Inc
# SPDX-License-Identifier: AGPL-3.0-only

"""
Shared utilities for manual test scripts.

This module contains common functionality used by various manual test scripts
to reduce code duplication and maintain consistency.
"""

import logging
import subprocess
import sys
from pathlib import Path


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
