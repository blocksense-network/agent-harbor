#!/usr/bin/env bash
# Copyright 2025 Schelling Point Labs Inc
# SPDX-License-Identifier: AGPL-3.0-only

set -euo pipefail

SOCKET_PATH="/tmp/agent-harbor/ah-fs-snapshots-daemon"
REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
CLI_BIN="$REPO_ROOT/target/release/ah-fs-snapshots-daemonctl"
MODE="${AGENTFS_CHECK_MODE:-}"

if [ -z "$MODE" ]; then
  if [ "$(uname -s)" = "Darwin" ]; then
    MODE="interpose"
  else
    MODE="fuse"
  fi
fi

if [ ! -e "$SOCKET_PATH" ]; then
  echo "AH filesystem snapshots daemon is not running (socket missing)"
  exit 1
fi

if [ ! -x "$CLI_BIN" ]; then
  echo "Error: $CLI_BIN not found. Build it with 'cargo build --release --package ah-fs-snapshots-daemon --bins'"
  exit 1
fi

case "$MODE" in
fuse)
  STATUS_JSON="$($CLI_BIN --socket-path "$SOCKET_PATH" fuse status --json --allow-not-ready)"
  ;;
interpose)
  STATUS_JSON="$($CLI_BIN --socket-path "$SOCKET_PATH" interpose status --json --allow-not-ready)"
  ;;
*)
  echo "Unknown AGENTFS_CHECK_MODE '$MODE'"
  exit 1
  ;;
esac

if [ -z "$STATUS_JSON" ]; then
  echo "Error: cli returned empty status payload"
  exit 1
fi

STATUS_JSON_PAYLOAD="$STATUS_JSON" STATUS_MODE="$MODE" python3 - <<'PY'
import json, os, sys
payload = os.environ["STATUS_JSON_PAYLOAD"]
mode = os.environ["STATUS_MODE"]
data = json.loads(payload)
print(f"AH filesystem snapshots daemon status ({mode}):")
print(f"  state      : {data['state']}")
print(f"  pid        : {data['pid']}")
print(f"  log_path   : {data['log_path']}")
print(f"  runtime_dir: {data['runtime_dir']}")

if mode == "fuse":
    print(f"  mount_point: {data['mount_point']}")
    print(f"  backstore  : {data['backstore']}")
elif mode == "interpose":
    print(f"  socket_path: {data['socket_path']}")
    print(f"  repo_root  : {data['repo_root']}")

if data.get('last_error'):
    print(f"  last_error : {data['last_error']}")

if data['state'] != 'running':
    print("Daemon is not running the requested AgentFS service (state mismatch)")
    sys.exit(1)
PY
