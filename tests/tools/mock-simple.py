#!/usr/bin/env python3
# Copyright 2025 Schelling Point Labs Inc
# SPDX-License-Identifier: AGPL-3.0-only

"""
Mock Simple - A simple debugging tool that outputs natural numbers and takes snapshots.

This script outputs natural numbers one per line, and takes a filesystem snapshot
after every 5 numbers. Used for testing snapshot indicator functionality in the recorder.
"""

import argparse
import os
import subprocess
import sys
import time
from pathlib import Path

def main():
    parser = argparse.ArgumentParser(description="Mock Simple agent for snapshot debugging")
    parser.add_argument("--workspace", help="Workspace directory to run in (for snapshots)")
    args = parser.parse_args()

    print("Mock Simple: Starting natural number output with snapshots")
    print("Will take a snapshot after every 5 numbers")
    print()

    # Find the project root to locate the ah binary
    script_dir = Path(__file__).parent
    project_root = script_dir.parent.parent  # Go up to project root
    ah_binary = project_root / "target" / "debug" / "ah"

    if not ah_binary.exists():
        print(f"Error: AH binary not found at {ah_binary}")
        print("Please build the project first with 'cargo build'")
        sys.exit(1)

    # Change to workspace directory if specified (for snapshots to work)
    if args.workspace:
        workspace_dir = Path(args.workspace)
        if workspace_dir.exists():
            os.chdir(workspace_dir)
            print(f"Changed working directory to: {workspace_dir}")
        else:
            print(f"Warning: Workspace directory {workspace_dir} does not exist")
    else:
        print("No workspace specified - snapshots may not work on this filesystem")

    number = 1

    ipc_socket = os.environ.get('AH_RECORDER_IPC_SOCKET')
    if not ipc_socket:
        print("AH_RECORDER_IPC_SOCKET not set, not taking snapshots")

    while True:
        # Output the current number
        print(f"Number: {number}")

        # Take a snapshot after every 5 numbers
        if number % 5 == 0:
            if ipc_socket:
                try:
                    print(f"Taking snapshot after number {number}...")
                    result = subprocess.run([
                        str(ah_binary), "agent", "fs", "snapshot"
                    ], capture_output=True, text=True, timeout=10)

                    if result.returncode == 0:
                        print(f"✓ Snapshot taken successfully")
                        print(f"DEBUG: stdout: {result.stdout.strip()}")
                    else:
                        print(f"✗ Snapshot failed: {result.stderr.strip()}")
                except subprocess.TimeoutExpired:
                    print("✗ Snapshot timed out")
                except Exception as e:
                    print(f"✗ Snapshot error: {e}")

        # Small delay to make it readable
        time.sleep(0.5)
        number += 1

if __name__ == "__main__":
    main()
