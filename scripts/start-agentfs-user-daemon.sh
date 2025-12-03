#!/usr/bin/env bash
# Copyright 2025 Schelling Point Labs Inc
# SPDX-License-Identifier: AGPL-3.0-only

# Start an AgentFS snapshots daemon and FUSE mount in user space (no sudo).
# The daemon, socket, runtime, logs, and mountpoint are isolated to a
# per-run session directory so this helper will not clash with an existing
# privileged daemon. It builds the needed binaries when they are missing.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# Session-scoped paths (overridable for CI/debugging)
SESSION_DIR="${AGENTFS_USER_SESSION_DIR:-$REPO_ROOT/logs/agentfs-user-$(date +%Y%m%d-%H%M%S)}"
SOCKET_PATH="${AGENTFS_USER_SOCKET:-$SESSION_DIR/agentfs.sock}"
RUNTIME_DIR="${AGENTFS_FUSE_RUNTIME_DIR:-$SESSION_DIR/runtime}"
MOUNT_POINT="${AGENTFS_USER_MOUNT:-$SESSION_DIR/mnt}"
LOG_DIR="${AGENTFS_USER_LOG_DIR:-$SESSION_DIR}"

DAEMON_BIN="${AGENTFS_DAEMON_BIN:-$REPO_ROOT/target/debug/ah-fs-snapshots-daemon}"
CTL_BIN="${AGENTFS_DAEMONCTL_BIN:-$REPO_ROOT/target/debug/ah-fs-snapshots-daemonctl}"
HOST_BIN="${AGENTFS_FUSE_HOST_BIN:-$REPO_ROOT/target/debug/agentfs-fuse-host}"

BACKSTORE="${AGENTFS_FUSE_BACKSTORE:-in-memory}"
AUTO_UNMOUNT="${AGENTFS_FUSE_AUTO_UNMOUNT:-1}"
MATERIALIZATION="${AGENTFS_FUSE_MATERIALIZATION:-lazy}"
RUST_LOG_LEVEL="${RUST_LOG:-info,agentfs_fuse_host=info,agentfs::fuse=info}"

mkdir -p "$SESSION_DIR" "$RUNTIME_DIR" "$MOUNT_POINT" "$LOG_DIR" "$(dirname "$SOCKET_PATH")"

# Build binaries on demand (debug profile keeps turnaround fast for tests)
if [ ! -x "$DAEMON_BIN" ] || [ ! -x "$CTL_BIN" ]; then
  echo "Building ah-fs-snapshots-daemon (daemon + ctl)..."
  cargo build --package ah-fs-snapshots-daemon --bins
fi

echo "Building agentfs-fuse-host (debug, fuse feature)..."
cargo build --package agentfs-fuse-host --no-default-features --features fuse

# Remove stale socket if present
if [ -e "$SOCKET_PATH" ]; then
  echo "Removing stale socket at $SOCKET_PATH"
  rm -f "$SOCKET_PATH"
fi

DAEMON_LOG="$LOG_DIR/ah-fs-snapshots-daemon.log"
DAEMON_STDOUT="$LOG_DIR/ah-fs-snapshots-daemon.stdout"

echo "Starting user-mode ah-fs-snapshots-daemon (socket: $SOCKET_PATH)"
AGENTFS_FUSE_RUNTIME_DIR="$RUNTIME_DIR" \
  AGENTFS_FUSE_HOST_BIN="$HOST_BIN" \
  RUST_LOG="$RUST_LOG_LEVEL" \
  "$DAEMON_BIN" --socket-path "$SOCKET_PATH" --log-dir "$LOG_DIR" --log-level debug \
  >"$DAEMON_STDOUT" 2>&1 &
DAEMON_PID=$!

echo "Waiting for daemon socket..."
for _ in {1..40}; do
  if [ -S "$SOCKET_PATH" ]; then
    break
  fi
  sleep 0.25
done

if [ ! -S "$SOCKET_PATH" ]; then
  echo "Daemon failed to create socket at $SOCKET_PATH (see $DAEMON_STDOUT)"
  exit 1
fi

MOUNT_LOG="$LOG_DIR/agentfs-mount.log"
MOUNT_ARGS=("--socket-path" "$SOCKET_PATH" fuse mount "--mount-point" "$MOUNT_POINT" "--backstore" "$BACKSTORE" "--materialization" "$MATERIALIZATION" "--mount-timeout-ms" "20000")
if [ "$AUTO_UNMOUNT" = "1" ]; then
  MOUNT_ARGS+=("--auto-unmount")
fi

echo "Requesting user-mode FUSE mount at $MOUNT_POINT"
AGENTFS_FUSE_RUNTIME_DIR="$RUNTIME_DIR" \
  AGENTFS_FUSE_HOST_BIN="$HOST_BIN" \
  "$CTL_BIN" "${MOUNT_ARGS[@]}" >"$MOUNT_LOG" 2>&1

CONTROL_PATH="$MOUNT_POINT/.agentfs/control"
for _ in {1..60}; do
  if [ -e "$CONTROL_PATH" ]; then
    break
  fi
  sleep 0.25
done

if [ ! -e "$CONTROL_PATH" ]; then
  echo "Mount did not become ready (missing $CONTROL_PATH). See $MOUNT_LOG"
  kill "$DAEMON_PID" >/dev/null 2>&1 || true
  exit 1
fi

# Persist session metadata for tooling/teardown
cat >"$SESSION_DIR/metadata.json" <<EOF
{
  "socket_path": "${SOCKET_PATH}",
  "runtime_dir": "${RUNTIME_DIR}",
  "mount_point": "${MOUNT_POINT}",
  "log_dir": "${LOG_DIR}",
  "daemon_pid": ${DAEMON_PID},
  "daemon_log": "${DAEMON_LOG}",
  "mount_log": "${MOUNT_LOG}",
  "host_bin": "${HOST_BIN}",
  "daemon_bin": "${DAEMON_BIN}"
}
EOF

ln -sfn "$SESSION_DIR" "$REPO_ROOT/logs/agentfs-user-latest"

echo "User-mode AgentFS daemon running (pid=$DAEMON_PID) with mount at $MOUNT_POINT"
echo "Metadata: $SESSION_DIR/metadata.json"
