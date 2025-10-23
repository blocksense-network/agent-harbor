#!/usr/bin/env bash
# Copyright 2025 Schelling Point Labs Inc
# SPDX-License-Identifier: AGPL-3.0-only

set -euo pipefail

SOCKET_PATH="/tmp/agent-harbor/ah-fs-snapshots-daemon"

if [ -e "$SOCKET_PATH" ]; then
  # Check if socket is actually accepting connections
  if ruby -e "require 'socket'; UNIXSocket.open('$SOCKET_PATH').close" 2>/dev/null; then
    echo "AH filesystem snapshots daemon is running (socket exists: $SOCKET_PATH)"
  else
    echo "AH filesystem snapshots daemon socket exists but is not responding"
    exit 1
  fi
else
  echo "AH filesystem snapshots daemon is not running"
  exit 1
fi
