#!/usr/bin/env bash
# Copyright 2025 Schelling Point Labs Inc
# SPDX-License-Identifier: AGPL-3.0-only
#
# Start the AgentFS snapshots daemon. Requires sudo for the daemon process and mount.

set -euo pipefail

OS_NAME="$(uname -s)"

# Runtime directories (avoid /tmp to prevent sandbox clashes)
if [ "$OS_NAME" = "Darwin" ]; then
  RUNTIME_BASE="${XDG_RUNTIME_DIR:-$HOME/Library/Caches/agent-harbor/run}"
else
  RUNTIME_BASE="${XDG_RUNTIME_DIR:-/run/user/$(id -u)}"
fi

SOCKET_DIR="$RUNTIME_BASE/agentfsd"
SOCKET_PATH="$SOCKET_DIR/ah-fs-snapshots-daemon"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
DAEMON_BIN="$REPO_ROOT/target/release/ah-fs-snapshots-daemon"
CLI_BIN="$REPO_ROOT/target/release/ah-fs-snapshots-daemonctl"
HOST_BIN="${AGENTFS_FUSE_HOST_BIN:-$REPO_ROOT/target/release/agentfs-fuse-host}"
LOG_DIR="$HOME/Library/Logs/agent-harbor"

# Ensure runtime dir exists and is private
mkdir -p "$SOCKET_DIR"
chmod 700 "$SOCKET_DIR" || true

# If socket exists and responds, refuse to start a second instance
if [ -e "$SOCKET_PATH" ]; then
  if ruby -e "require 'socket'; UNIXSocket.open('$SOCKET_PATH').close" 2>/dev/null; then
    echo "AH filesystem snapshots daemon is already running (socket exists: $SOCKET_PATH)"
    exit 1
  else
    echo "Warning: found stale socket, removing..."
    sudo rm -f "$SOCKET_PATH"
  fi
fi

echo "Building ah-fs-snapshots-daemon (daemon + ctl) and agentfs-fuse-host (release)..."
cargo build --release --package ah-fs-snapshots-daemon --bins
cargo build --release --package agentfs-fuse-host --features fuse

echo "Launching AH filesystem snapshots daemon with sudo (debug logging to file)..."
echo "Stop it with: just stop-ah-fs-snapshots-daemon"
echo "Logs: $LOG_DIR/ah-fs-snapshots-daemon.log (platform-default if dir missing)"
sudo -b AGENTFS_FUSE_HOST_BIN="$HOST_BIN" "$DAEMON_BIN" --socket-path "$SOCKET_PATH" --log-level debug --log-dir "$LOG_DIR"

echo -n "Waiting for daemon socket $SOCKET_PATH ..."
for _ in {1..30}; do
  if [ -e "$SOCKET_PATH" ]; then
    echo " ready"
    break
  fi
  sleep 1
done

if [ ! -e "$SOCKET_PATH" ]; then
  echo
  echo "Timed out waiting for daemon socket; check sudo/logs"
  exit 1
fi

# Platform detection - choose mount mode (FUSE on Linux, interpose on macOS)
if [ "$OS_NAME" = "Linux" ]; then
  AUTOMOUNT="${AGENTFS_FUSE_AUTOMOUNT:-1}"
else
  AUTOMOUNT="${AGENTFS_FUSE_AUTOMOUNT:-1}"
fi

# Default mount point uses XDG_RUNTIME_DIR to avoid /tmp conflicts
if [ -n "${AGENTFS_FUSE_MOUNT_POINT:-}" ]; then
  MOUNT_POINT="$AGENTFS_FUSE_MOUNT_POINT"
else
  MOUNT_POINT="$RUNTIME_BASE/agentfs"
fi

RUNTIME_INTERPOSE_DIR="${AGENTFS_INTERPOSE_RUNTIME_DIR:-$RUNTIME_BASE/agentfs-interpose}"

mkdir -p "$(dirname "$MOUNT_POINT")" "$RUNTIME_INTERPOSE_DIR"
ALLOW_OTHER="${AGENTFS_FUSE_ALLOW_OTHER:-1}"
ALLOW_ROOT="${AGENTFS_FUSE_ALLOW_ROOT:-0}"
AUTO_UNMOUNT="${AGENTFS_FUSE_AUTO_UNMOUNT:-1}"
WRITEBACK="${AGENTFS_FUSE_WRITEBACK_CACHE:-0}"
BACKSTORE="${AGENTFS_FUSE_BACKSTORE:-in-memory}"
HOSTFS_ROOT="${AGENTFS_FUSE_HOSTFS_ROOT:-$REPO_ROOT/tmp/agentfs-hostfs}"
RAMDISK_MB="${AGENTFS_FUSE_RAMDISK_MB:-2048}"

if [ "$AUTOMOUNT" = "1" ]; then
  if [ "$OS_NAME" = "Linux" ]; then
    [ "$BACKSTORE" = "hostfs" ] && mkdir -p "$HOSTFS_ROOT"
    echo "Requesting AgentFS FUSE mount at $MOUNT_POINT via daemonctl (sudo)..."
    MOUNT_CMD=("$CLI_BIN" --socket-path "$SOCKET_PATH" fuse mount --mount-point "$MOUNT_POINT")
    [ "$ALLOW_OTHER" = "1" ] && MOUNT_CMD+=(--allow-other)
    [ "$ALLOW_ROOT" = "1" ] && MOUNT_CMD+=(--allow-root)
    [ "$AUTO_UNMOUNT" = "1" ] && MOUNT_CMD+=(--auto-unmount)
    [ "$WRITEBACK" = "1" ] && MOUNT_CMD+=(--writeback-cache)
    MOUNT_CMD+=(--backstore "$BACKSTORE")
    case "$BACKSTORE" in
    hostfs) MOUNT_CMD+=(--hostfs-root "$HOSTFS_ROOT") ;;
    ramdisk) MOUNT_CMD+=(--ramdisk-size-mb "$RAMDISK_MB") ;;
    esac
    if ! sudo "${MOUNT_CMD[@]}"; then
      echo "Daemon mount request failed; inspect the logs and rerun manually."
      exit 1
    fi

    echo "Ensuring $MOUNT_POINT is owned by $USER for test runs..."
    sudo chown -R "$USER":"$(id -gn)" "$MOUNT_POINT" || true
  else
    echo "Requesting AgentFS interpose mount via daemonctl..."
    MOUNT_CMD=("$CLI_BIN" --socket-path "$SOCKET_PATH" interpose mount --runtime-dir "$RUNTIME_INTERPOSE_DIR" --repo-root "${AGENTFS_REPO_ROOT:-$REPO_ROOT}")
    if ! sudo "${MOUNT_CMD[@]}"; then
      echo "Daemon interpose mount request failed; inspect the logs and rerun manually."
      exit 1
    fi
  fi
fi

if [ "$OS_NAME" = "Linux" ]; then
  echo "AH filesystem snapshots daemon is running. Use ah-fs-snapshots-daemonctl to manage FUSE mounts."
else
  echo "AH filesystem snapshots daemon is running. Use ah-fs-snapshots-daemonctl to manage interpose mounts."
fi
