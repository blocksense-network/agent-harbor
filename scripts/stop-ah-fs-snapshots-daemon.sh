#!/usr/bin/env bash
# Copyright 2025 Schelling Point Labs Inc
# SPDX-License-Identifier: AGPL-3.0-only
#
# Stop the AgentFS snapshots daemon (uses sudo).

set -euo pipefail

SOCKET_PATH="/tmp/agent-harbor/ah-fs-snapshots-daemon"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
DAEMON_BIN="$REPO_ROOT/target/release/ah-fs-snapshots-daemon"

if [ ! -e "$SOCKET_PATH" ]; then
  echo "AH filesystem snapshots daemon is not running (socket not found: $SOCKET_PATH)"
  exit 0
fi

echo "Stopping AH filesystem snapshots daemon..."

# Graceful stop
sudo pkill -INT -f "$DAEMON_BIN" || true

for _ in {1..10}; do
  if [ ! -e "$SOCKET_PATH" ]; then
    echo "AH filesystem snapshots daemon stopped successfully"
    exit 0
  fi
  sleep 0.5
done

echo "Warning: Daemon didn't shut down gracefully, force killing..."
sudo pkill -KILL -f "$DAEMON_BIN" || true
sleep 1

if [ -e "$SOCKET_PATH" ]; then
  echo "Warning: Socket still exists, cleaning up..."
  sudo rm -f "$SOCKET_PATH"
fi

echo "AH filesystem snapshots daemon forcefully stopped"
