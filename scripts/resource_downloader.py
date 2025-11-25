#!/usr/bin/env python3
# Copyright 2025 Schelling Point Labs Inc
# SPDX-License-Identifier: AGPL-3.0-only

"""
Reusable resource downloader for managing git worktree-compatible downloads.

This script provides utilities for downloading resources that should be shared
across git worktrees. Resources are stored in the main repository's resources/
folder, and worktrees get symlinks to avoid duplication.
"""

import argparse
import logging
import os
import shutil
import subprocess
import sys
from pathlib import Path
from typing import Callable, Optional


def setup_logging():
    """Set up basic logging configuration."""
    logging.basicConfig(
        level=logging.INFO,
        format='%(levelname)s: %(message)s'
    )


def find_main_repo_path(current_path: Optional[Path] = None) -> Path:
    """
    Find the main repository path from a potentially worktree checkout.

    Uses git rev-parse --git-common-dir to locate the main repository.
    Falls back to the current directory if not in a git repository.

    Args:
        current_path: Starting path to check (defaults to current working directory)

    Returns:
        Path to the main repository

    Raises:
        RuntimeError: If git commands fail or repository structure is invalid
    """
    if current_path is None:
        current_path = Path.cwd()

    try:
        # Use git rev-parse --git-common-dir to find the main repo
        result = subprocess.run(
            ['git', 'rev-parse', '--git-common-dir'],
            cwd=current_path,
            capture_output=True,
            text=True,
            check=True
        )

        common_dir = Path(result.stdout.strip())

        # If the path is relative, make it absolute relative to current_path
        if not common_dir.is_absolute():
            common_dir = current_path / common_dir

        # The common dir should be the .git directory of the main repo
        if common_dir.name == '.git':
            return common_dir.parent
        else:
            # In worktrees, the common dir might point to the main .git
            # Try to resolve to the actual repository root
            result = subprocess.run(
                ['git', 'rev-parse', '--show-toplevel'],
                cwd=common_dir.parent if common_dir.name == '.git' else common_dir,
                capture_output=True,
                text=True,
                check=True
            )
            return Path(result.stdout.strip())

    except subprocess.CalledProcessError as e:
        logging.warning(f"Failed to determine main repo path: {e}")
        # Fall back to assuming we're already in the main repo
        try:
            result = subprocess.run(
                ['git', 'rev-parse', '--show-toplevel'],
                cwd=current_path,
                capture_output=True,
                text=True,
                check=True
            )
            return Path(result.stdout.strip())
        except subprocess.CalledProcessError:
            raise RuntimeError("Not in a git repository")


def check_resource_exists(main_repo_path: Path, resource_name: str) -> bool:
    """
    Check if a resource already exists in the main repository's resources folder.

    Args:
        main_repo_path: Path to the main repository
        resource_name: Name of the resource directory

    Returns:
        True if the resource exists, False otherwise
    """
    resource_path = main_repo_path / 'resources' / resource_name
    return resource_path.exists() and resource_path.is_dir()


def create_symlink_if_needed(main_repo_path: Path, resource_name: str, current_repo_path: Optional[Path] = None) -> bool:
    """
    Create a symlink from the current worktree to the main repo's resource if needed.

    Args:
        main_repo_path: Path to the main repository
        resource_name: Name of the resource directory
        current_repo_path: Path to the current repository (defaults to current working directory)

    Returns:
        True if a symlink was created, False if it already existed or wasn't needed
    """
    if current_repo_path is None:
        current_repo_path = Path.cwd()

    # If we're already in the main repo, no symlink needed
    if current_repo_path.resolve() == main_repo_path.resolve():
        return False

    resource_path = main_repo_path / 'resources' / resource_name
    local_resource_path = current_repo_path / 'resources' / resource_name

    # Check if local symlink already exists and points to the right place
    if local_resource_path.exists():
        if local_resource_path.is_symlink():
            target = local_resource_path.readlink()
            # Calculate the expected relative path from current repo resources dir to main repo resource
            local_resources_dir = current_repo_path / 'resources'
            expected_target = os.path.relpath(resource_path, local_resources_dir)
            if str(target) == expected_target:
                logging.info(f"Symlink for {resource_name} already exists and is correct")
                return False
            else:
                logging.warning(f"Symlink for {resource_name} exists but points to wrong location: {target}")
                local_resource_path.unlink()
        else:
            logging.warning(f"Local resource directory {resource_name} exists but is not a symlink, removing")
            if local_resource_path.is_dir():
                shutil.rmtree(local_resource_path)
            else:
                local_resource_path.unlink()

    # Ensure resources directory exists in current repo
    local_resources_dir = current_repo_path / 'resources'
    local_resources_dir.mkdir(parents=True, exist_ok=True)

    # Create relative symlink from current repo to main repo
    try:
        relative_path = os.path.relpath(resource_path, local_resources_dir)
        local_resource_path.symlink_to(relative_path, target_is_directory=True)
        logging.info(f"Created symlink for {resource_name} pointing to main repo")
        return True
    except OSError as e:
        logging.error(f"Failed to create symlink for {resource_name}: {e}")
        return False


def download_resource(
    resource_name: str,
    download_func: Callable[[Path], None],
    current_path: Optional[Path] = None
) -> bool:
    """
    Download a resource using the provided download function.

    This function handles the logic of finding the main repo, checking if the
    resource already exists, downloading if needed, and creating symlinks in worktrees.

    Args:
        resource_name: Name of the resource to download
        download_func: Function that performs the actual download. Takes the
                      resources directory path as argument.
        current_path: Current repository path (defaults to current working directory)

    Returns:
        True if successful, False otherwise
    """
    try:
        if current_path is None:
            current_path = Path.cwd()

        logging.info(f"Processing resource: {resource_name}")

        # Find the main repository
        main_repo_path = find_main_repo_path(current_path)
        logging.info(f"Main repository path: {main_repo_path}")

        # Check if resource already exists in main repo
        if check_resource_exists(main_repo_path, resource_name):
            logging.info(f"Resource {resource_name} already exists in main repository")

            # Create symlink if we're in a worktree
            create_symlink_if_needed(main_repo_path, resource_name, current_path)
            return True

        # Resource doesn't exist, need to download
        logging.info(f"Resource {resource_name} not found, downloading...")

        # Ensure resources directory exists in main repo
        resources_dir = main_repo_path / 'resources'
        resources_dir.mkdir(parents=True, exist_ok=True)

        # Change to resources directory and run download function
        original_cwd = Path.cwd()
        try:
            os.chdir(resources_dir)
            download_func(resources_dir)
        finally:
            os.chdir(original_cwd)

        # Verify download succeeded
        if not check_resource_exists(main_repo_path, resource_name):
            logging.error(f"Download function did not create expected resource: {resource_name}")
            return False

        logging.info(f"Successfully downloaded resource: {resource_name}")

        # Create symlink if we're in a worktree
        create_symlink_if_needed(main_repo_path, resource_name, current_path)

        return True

    except Exception as e:
        logging.error(f"Failed to download resource {resource_name}: {e}")
        return False


def download_acp_specs(resources_dir: Path) -> None:
    """
    Download the Agent Client Protocol specifications.

    Args:
        resources_dir: Path to the resources directory where specs should be cloned
    """
    repo_url = "https://github.com/agentclientprotocol/agent-client-protocol"
    target_dir = resources_dir / "acp-specs"

    logging.info(f"Cloning ACP specs from {repo_url} to {target_dir}")

    # Clone the repository
    subprocess.run(
        ['git', 'clone', repo_url, 'acp-specs'],
        check=True,
        cwd=resources_dir
    )

    logging.info("Successfully cloned ACP specifications")


def main():
    """Main entry point for command-line usage."""
    setup_logging()

    parser = argparse.ArgumentParser(description='Download resources for git worktree-compatible storage')
    parser.add_argument('resource_name', help='Name of the resource to download')
    parser.add_argument('--current-path', type=Path, default=None,
                       help='Current repository path (defaults to current working directory)')

    args = parser.parse_args()

    # Map resource names to download functions
    download_funcs = {
        'acp-specs': download_acp_specs,
    }

    if args.resource_name not in download_funcs:
        logging.error(f"Unknown resource: {args.resource_name}")
        logging.error(f"Available resources: {', '.join(download_funcs.keys())}")
        sys.exit(1)

    success = download_resource(
        args.resource_name,
        download_funcs[args.resource_name],
        args.current_path
    )

    sys.exit(0 if success else 1)


if __name__ == '__main__':
    main()
