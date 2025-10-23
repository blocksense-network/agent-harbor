#!/usr/bin/env python3
# Copyright 2025 Schelling Point Labs Inc
# SPDX-License-Identifier: AGPL-3.0-only

"""
Test runner for the mock coding agent.

This script runs all tests and provides a simple way to verify
the agent functionality without requiring manual testing.
"""

import subprocess
import sys
from pathlib import Path


def main():
    """Run the test suite."""
    project_root = Path(__file__).parent.parent
    
    print("Installing test dependencies...")
    try:
        subprocess.run([
            sys.executable, "-m", "pip", "install", "-e", ".[test]"
        ], cwd=project_root, check=True)
    except subprocess.CalledProcessError as e:
        print(f"Failed to install dependencies: {e}")
        return 1
    
    print("\nRunning mock agent tests...")
    try:
        result = subprocess.run([
            sys.executable, "-m", "pytest", "tests/", "-v"
        ], cwd=project_root)
        return result.returncode
    except subprocess.CalledProcessError as e:
        print(f"Tests failed: {e}")
        return 1


if __name__ == "__main__":
    sys.exit(main())