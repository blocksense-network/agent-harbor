#!/usr/bin/env bash
# Copyright 2025 Schelling Point Labs Inc
# SPDX-License-Identifier: AGPL-3.0-only

# Stop the user-mode AgentFS daemon and unmount the corresponding FUSE mount
# started by scripts/start-agentfs-user-daemon.sh. No sudo is used.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

SESSION_DIR="${AGENTFS_USER_SESSION_DIR:-$REPO_ROOT/logs/agentfs-user-latest}"
METADATA_FILE="$SESSION_DIR/metadata.json"

if [ ! -f "$METADATA_FILE" ]; then
  echo "No metadata found at $METADATA_FILE. Nothing to stop."
  exit 0
fi

SOCKET_PATH=$(
  python3 - <<'PY'
import json, pathlib, sys
meta = json.loads(pathlib.Path(sys.argv[1]).read_text())
print(meta.get("socket_path", ""))
PY
  "$METADATA_FILE"
)

MOUNT_POINT=$(
  python3 - <<'PY'
import json, pathlib, sys
meta = json.loads(pathlib.Path(sys.argv[1]).read_text())
print(meta.get("mount_point", ""))
PY
  "$METADATA_FILE"
)

RUNTIME_DIR=$(
  python3 - <<'PY'
import json, pathlib, sys
meta = json.loads(pathlib.Path(sys.argv[1]).read_text())
print(meta.get("runtime_dir", ""))
PY
  "$METADATA_FILE"
)

DAEMON_PID=$(
  python3 - <<'PY'
import json, pathlib, sys
meta = json.loads(pathlib.Path(sys.argv[1]).read_text())
print(meta.get("daemon_pid", 0))
PY
  "$METADATA_FILE"
)

HOST_BIN=$(
  python3 - <<'PY'
import json, pathlib, sys
meta = json.loads(pathlib.Path(sys.argv[1]).read_text())
print(meta.get("host_bin", ""))
PY
  "$METADATA_FILE"
)

CTL_BIN="${AGENTFS_DAEMONCTL_BIN:-$REPO_ROOT/target/debug/ah-fs-snapshots-daemonctl}"

echo "Stopping user-mode AgentFS daemon (session: $SESSION_DIR)"

# Try graceful unmount via daemonctl
if [ -x "$CTL_BIN" ] && [ -n "$SOCKET_PATH" ] && [ -S "$SOCKET_PATH" ]; then
  if AGENTFS_FUSE_RUNTIME_DIR="$RUNTIME_DIR" AGENTFS_FUSE_HOST_BIN="$HOST_BIN" \
    "$CTL_BIN" --socket-path "$SOCKET_PATH" fuse unmount >/dev/null 2>&1; then
    echo "Unmounted AgentFS FUSE mount at $MOUNT_POINT"
  fi
fi

# Kill daemon if still running
if [ "$DAEMON_PID" -ne 0 ] && kill -0 "$DAEMON_PID" >/dev/null 2>&1; then
  kill "$DAEMON_PID" >/dev/null 2>&1 || true
  for _ in {1..20}; do
    if ! kill -0 "$DAEMON_PID" >/dev/null 2>&1; then
      break
    fi
    sleep 0.1
  done
fi

# Remove stale socket
[ -S "$SOCKET_PATH" ] && rm -f "$SOCKET_PATH"

echo "User-mode AgentFS session stopped"
