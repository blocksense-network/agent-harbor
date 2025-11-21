#!/usr/bin/env bash
# Copyright 2025 Schelling Point Labs Inc
# SPDX-License-Identifier: AGPL-3.0-only

set -euo pipefail

SOCKET_PATH="/tmp/agent-harbor/ah-fs-snapshots-daemon"
REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
DAEMON_BIN="$REPO_ROOT/target/release/ah-fs-snapshots-daemon"
CLI_BIN="$REPO_ROOT/target/release/ah-fs-snapshots-daemonctl"
HOST_BIN="${AGENTFS_FUSE_HOST_BIN:-$REPO_ROOT/target/release/agentfs-fuse-host}"

if [ -e "$SOCKET_PATH" ]; then
  # Check if socket is actually accepting connections by trying to connect
  if ruby -e "require 'socket'; UNIXSocket.open('$SOCKET_PATH').close" 2>/dev/null; then
    echo "AH filesystem snapshots daemon is already running (socket exists: $SOCKET_PATH)"
    exit 1
  else
    echo "Warning: Found stale socket file, cleaning up..."
    sudo rm -f "$SOCKET_PATH"
  fi
fi

echo "Building ah-fs-snapshots-daemon (daemon + ctl) and agentfs-fuse-host (release)..."
cargo build --release --package ah-fs-snapshots-daemon --bins
cargo build --release --package agentfs-fuse-host --features fuse

echo "Launching AH filesystem snapshots daemon with sudo..."
echo "Stop it with: just stop-ah-fs-snapshots-daemon"
sudo -b AGENTFS_FUSE_HOST_BIN="$HOST_BIN" "$DAEMON_BIN"

echo -n "Waiting for daemon socket $SOCKET_PATH ..."
for _ in {1..30}; do
  if [ -e "$SOCKET_PATH" ]; then
    echo " ready"
    break
  fi
  sleep 1
done

if [ ! -e "$SOCKET_PATH" ]; then
  echo "\nTimed out waiting for daemon socket; check sudo logs"
  exit 1
fi
echo "AH filesystem snapshots daemon is running. Use ah-fs-snapshots-daemonctl to manage FUSE mounts as needed."
