#!/usr/bin/env bash
set -euo pipefail

SOCKET_PATH="/tmp/agent-harbor/ah-fs-snapshots-daemon"

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

echo "Starting AH filesystem snapshots daemon with sudo..."
echo "The daemon will run in the background and handle privileged filesystem snapshot operations."
echo "Stop it with: just stop-ah-fs-snapshots-daemon"

# Build and run the daemon
cargo build --release --package ah-fs-snapshots-daemon
sudo -b ./target/release/ah-fs-snapshots-daemon
