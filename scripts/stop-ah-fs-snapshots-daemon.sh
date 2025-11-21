#!/usr/bin/env bash
# Copyright 2025 Schelling Point Labs Inc
# SPDX-License-Identifier: AGPL-3.0-only

set -euo pipefail

SOCKET_PATH="/tmp/agent-harbor/ah-fs-snapshots-daemon"
REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
DAEMON_BIN="$REPO_ROOT/target/release/ah-fs-snapshots-daemon"
CLI_BIN="$REPO_ROOT/target/release/ah-fs-snapshots-daemonctl"

if [ ! -e "$SOCKET_PATH" ]; then
  echo "AH filesystem snapshots daemon is not running (socket not found: $SOCKET_PATH)"
  exit 1
fi

echo "Stopping AH filesystem snapshots daemon..."

# Send interrupt signal to the Rust daemon process (let it clean up gracefully)
# The Rust daemon is built from the ah-fs-snapshots-daemon crate
sudo pkill -INT -f "$DAEMON_BIN" || true

# Wait for graceful shutdown
for i in {1..10}; do
  if [ ! -e "$SOCKET_PATH" ]; then
    echo "AH filesystem snapshots daemon stopped successfully"
    exit 0
  fi
  sleep 0.5
done

# If still not cleaned up, force kill
echo "Warning: Daemon didn't shut down gracefully, force killing..."
sudo pkill -KILL -f "$DAEMON_BIN" || true
sleep 1

if [ -e "$SOCKET_PATH" ]; then
  echo "Warning: Socket still exists, manually cleaning up..."
  sudo rm -f "$SOCKET_PATH"
fi

echo "AH filesystem snapshots daemon forcefully stopped"
