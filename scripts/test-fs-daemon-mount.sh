#!/usr/bin/env bash
# Copyright 2025 Schelling Point Labs Inc
# SPDX-License-Identifier: AGPL-3.0-only

set -euo pipefail

SOCKET_PATH="/tmp/agent-harbor/ah-fs-snapshots-daemon"
REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
CLI_BIN="$REPO_ROOT/target/release/ah-fs-snapshots-daemonctl"

if [ ! -e "$SOCKET_PATH" ]; then
  echo "AH filesystem snapshots daemon socket not found at $SOCKET_PATH"
  echo "Start it first with: just start-ah-fs-snapshots-daemon"
  exit 1
fi

if [ ! -x "$CLI_BIN" ]; then
  echo "Error: $CLI_BIN not found. Build it with 'cargo build --release --package ah-fs-snapshots-daemon --bins'"
  exit 1
fi

status_json() {
  "$CLI_BIN" --socket-path "$SOCKET_PATH" fuse status --json --allow-not-ready
}

json_field() {
  local payload="$1"
  local key="$2"
  STATUS_JSON_PAYLOAD="$payload" python3 - "$key" <<'PY'
import json, os, sys
payload = os.environ["STATUS_JSON_PAYLOAD"]
data = json.loads(payload)
key = sys.argv[1]
value = data.get(key)
if value is None:
    sys.exit(1)
print(value)
PY
}

initial_json=$(status_json)
initial_state=$(json_field "$initial_json" state)
if [ "$initial_state" != "running" ]; then
  echo "Daemon mount isn't running yet (state=$initial_state). Start it before running this harness."
  exit 1
fi

initial_pid=$(json_field "$initial_json" pid)
log_path=$(json_field "$initial_json" log_path)
mount_point=$(json_field "$initial_json" mount_point)

echo "Killing agentfs-fuse-host PID $initial_pid to verify supervised restart..."
sudo kill -KILL "$initial_pid"

new_pid=""
for attempt in {1..60}; do
  sleep 1
  current_json=$(status_json) || continue
  current_state=$(json_field "$current_json" state)
  current_pid=$(json_field "$current_json" pid)
  if [ "$current_state" = "running" ] && [ "$current_pid" != "0" ] && [ "$current_pid" != "$initial_pid" ]; then
    new_pid="$current_pid"
    echo "Daemon restarted agentfs-fuse-host (new PID $new_pid)"
    echo "Mount point $mount_point is healthy"
    exit 0
  fi
  echo "Attempt $attempt: state=$current_state pid=$current_pid (waiting for restart)"
done

echo "ERROR: agentfs-fuse-host did not restart within 60 seconds"
if [ -n "$log_path" ] && [ -f "$log_path" ]; then
  echo "Last 40 lines from $log_path:"
  tail -n 40 "$log_path"
fi
exit 1
