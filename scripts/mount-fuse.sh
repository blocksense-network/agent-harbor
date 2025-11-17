#!/usr/bin/env bash
# Copyright 2025 Schelling Point Labs Inc
# SPDX-License-Identifier: AGPL-3.0-only

set -euo pipefail

if [ $# -ne 1 ]; then
  echo "Usage: $0 /path/to/mountpoint"
  exit 1
fi

mountpoint="$1"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

is_mounted() {
  mountpoint -q "$mountpoint" 2>/dev/null
}

if is_mounted; then
  echo "Existing AgentFS mount detected at $mountpoint; attempting to unmount first..."
  if command -v fusermount >/dev/null 2>&1; then
    if ! fusermount -u "$mountpoint" 2>/dev/null; then
      echo "Error: Unable to auto-unmount $mountpoint. Run 'just umount-fuse $mountpoint' and retry."
      exit 1
    fi
  else
    echo "Error: $mountpoint is already mounted and fusermount is unavailable. Unmount manually and retry."
    exit 1
  fi
fi

if [ ! -d "$mountpoint" ]; then
  echo "Creating mount point: $mountpoint"
  if ! mkdir -p "$mountpoint"; then
    echo "Error: Failed to create $mountpoint (insufficient permissions?)"
    exit 1
  fi
fi

# Ensure the mount point is owned by the current user
if ! sudo chown "$(whoami)" "$mountpoint"; then
  echo "Error: Unable to chown $mountpoint; aborting mount"
  exit 1
fi

FUSE_FLAGS=()
if [ "${AGENTFS_FUSE_ALLOW_OTHER:-}" = "1" ]; then
  FUSE_FLAGS+=("--allow-other")
fi

CONFIG_PATH_DEFAULT="$REPO_ROOT/fuse_config.json"
CONFIG_PATH="${AGENTFS_FUSE_CONFIG:-$CONFIG_PATH_DEFAULT}"
CONFIG_ARGS=()

if [ -n "${AGENTFS_FUSE_CONFIG:-}" ]; then
  if [ ! -f "$CONFIG_PATH" ]; then
    echo "Error: Custom config '$CONFIG_PATH' (from AGENTFS_FUSE_CONFIG) not found."
    exit 1
  fi
  echo "Using custom FUSE config: $CONFIG_PATH"
  CONFIG_ARGS=(--config "$CONFIG_PATH")
else
  if [ -f "$CONFIG_PATH" ]; then
    echo "Using FUSE config: $CONFIG_PATH"
    CONFIG_ARGS=(--config "$CONFIG_PATH")
  else
    echo "No fuse_config.json supplied; falling back to AgentFS defaults."
  fi
fi

echo "Mounting AgentFS FUSE filesystem at $mountpoint..."
if [ ${#FUSE_FLAGS[@]} -gt 0 ]; then
  echo "Additional FUSE flags: ${FUSE_FLAGS[*]}"
fi
if [ ${#CONFIG_ARGS[@]} -gt 0 ]; then
  echo "Config args: ${CONFIG_ARGS[*]}"
fi
echo "Note: This will run in the background. To unmount later: fusermount -u $mountpoint"
echo ""
HOST_BIN="${AGENTFS_FUSE_HOST_BIN:-$REPO_ROOT/target/debug/agentfs-fuse-host}"
echo "AgentFS host binary: $HOST_BIN"
if [ -n "${AGENTFS_FUSE_LOG_FILE:-}" ]; then
  mkdir -p "$(dirname "$AGENTFS_FUSE_LOG_FILE")"
  echo "Logging FUSE host output to $AGENTFS_FUSE_LOG_FILE"
  "$HOST_BIN" "${CONFIG_ARGS[@]}" "${FUSE_FLAGS[@]}" "$mountpoint" >>"$AGENTFS_FUSE_LOG_FILE" 2>&1 &
else
  "$HOST_BIN" "${CONFIG_ARGS[@]}" "${FUSE_FLAGS[@]}" "$mountpoint" &
fi
echo "AgentFS FUSE filesystem mounted. PID: $!"
