#!/usr/bin/env bash
# Copyright 2025 Schelling Point Labs Inc
# SPDX-License-Identifier: AGPL-3.0-only

set -euo pipefail

SOCKET_PATH="/tmp/agent-harbor/ah-fs-snapshots-daemon"
REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
CLI_BIN="$REPO_ROOT/target/release/ah-fs-snapshots-daemonctl"

if [ ! -e "$SOCKET_PATH" ]; then
  echo "AH filesystem snapshots daemon is not running (socket missing)"
  exit 1
fi

if [ ! -x "$CLI_BIN" ]; then
  echo "Error: $CLI_BIN not found. Build it with 'cargo build --release --package ah-fs-snapshots-daemon --bins'"
  exit 1
fi

STATUS_JSON="$($CLI_BIN --socket-path "$SOCKET_PATH" fuse status --json --allow-not-ready)"

if [ -z "$STATUS_JSON" ]; then
  echo "Error: cli returned empty status payload"
  exit 1
fi

STATUS_JSON_PAYLOAD="$STATUS_JSON" python3 - <<'PY'
import json, os, sys
payload = os.environ["STATUS_JSON_PAYLOAD"]
data = json.loads(payload)
print("AH filesystem snapshots daemon status:")
print(f"  state      : {data['state']}")
print(f"  mount_point: {data['mount_point']}")
print(f"  pid        : {data['pid']}")
print(f"  log_path   : {data['log_path']}")
print(f"  backstore  : {data['backstore']}")
if data.get('last_error'):
    print(f"  last_error : {data['last_error']}")
if data['state'] != 'running':
    print("Daemon is not running the AgentFS mount (state mismatch)")
    sys.exit(1)
PY
