#!/usr/bin/env bash
# Copyright 2025 Schelling Point Labs Inc
# SPDX-License-Identifier: AGPL-3.0-only

set -euo pipefail

echo "=== Mock Agent Demo Test ==="
echo "This script tests the mock agent by running the hello scenario"
echo "and verifying that it produces the expected output and files."
echo

# Get the project directory (parent of tests directory)
PROJECT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$PROJECT_DIR"

# Create temporary directories
WS="$(mktemp -d)"
CODEX_HOME="$(mktemp -d)"

cleanup() {
  echo "Cleaning up temporary directories..."
  rm -rf "$WS" "$CODEX_HOME"
}
trap cleanup EXIT

echo "Workspace: $WS"
echo "Codex home: $CODEX_HOME"
echo

# Install the package in development mode
echo "Installing mock agent package..."
pip install -e . >/dev/null 2>&1

# Run the hello scenario
echo "Running hello scenario..."
python -m src.cli run --scenario examples/hello_scenario.yaml --workspace "$WS" --codex-home "$CODEX_HOME"

# Verify the expected file was created
echo
echo "Verifying results..."
if [ -f "$WS/hello.py" ]; then
  echo "✓ hello.py was created successfully"
  echo "Content:"
  cat "$WS/hello.py"
else
  echo "✗ hello.py was not created"
  exit 1
fi

# Check for rollout files
if find "$CODEX_HOME/sessions" -name "rollout-*.jsonl" 2>/dev/null | grep -q .; then
  echo "✓ Rollout files were created"
  echo "Rollout files:"
  find "$CODEX_HOME/sessions" -name "rollout-*.jsonl" 2>/dev/null
else
  echo "✗ No rollout files found"
fi

# Check for session log files
if find "$CODEX_HOME/logs" -name "session-*.jsonl" 2>/dev/null | grep -q .; then
  echo "✓ Session log files were created"
  echo "Session log files:"
  find "$CODEX_HOME/logs" -name "session-*.jsonl" 2>/dev/null
else
  echo "✗ No session log files found"
fi

echo
echo "Demo test completed successfully!"
