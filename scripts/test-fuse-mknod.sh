#!/usr/bin/env bash
# Copyright 2025 Schelling Point Labs Inc
# SPDX-License-Identifier: AGPL-3.0-only

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
LOG_ROOT="$REPO_ROOT/logs"
TS="$(date +%Y%m%d-%H%M%S)"
RUN_DIR="$LOG_ROOT/fuse-mknod-$TS"
SUMMARY_FILE="$RUN_DIR/summary.json"
MOUNTPOINT="/tmp/agentfs-mknod-$TS"
BACKSTORE_DIR="$RUN_DIR/backstore"
FUSE_CONFIG="$RUN_DIR/fuse-config.json"
RESULTS_TMP="$RUN_DIR/results.tmp"
SKIP_BUILD="${SKIP_FUSE_BUILD:-}"

mkdir -p "$RUN_DIR" "$BACKSTORE_DIR"
: >"$RESULTS_TMP"

log() {
  echo "[$(date +%H:%M:%S)] $*" | tee -a "$RUN_DIR/mknod.log"
}

record_result() {
  local name="$1"
  local status="$2"
  local detail="$3"
  echo "$name|$status|$detail" >>"$RESULTS_TMP"
}

cleanup() {
  if mountpoint -q "$MOUNTPOINT" 2>/dev/null; then
    log "Unmounting $MOUNTPOINT"
    (cd "$REPO_ROOT" && just umount-fuse "$MOUNTPOINT") >>"$RUN_DIR/mknod.log" 2>&1 || true
  fi
  rm -rf "$MOUNTPOINT"
}

trap cleanup EXIT

if [[ -z "$SKIP_BUILD" ]]; then
  log "Building agentfs-fuse-host"
  (cd "$REPO_ROOT" && just build-fuse-host) >>"$RUN_DIR/mknod.log" 2>&1
else
  log "SKIP_FUSE_BUILD set; skipping build"
fi

cat >"$FUSE_CONFIG" <<JSON
{
  "case_sensitivity": "Sensitive",
  "memory": {
    "max_bytes_in_memory": 268435456,
    "spill_directory": null
  },
  "limits": {
    "max_open_handles": 4096,
    "max_branches": 64,
    "max_snapshots": 128
  },
  "cache": {
    "attr_ttl_ms": 250,
    "entry_ttl_ms": 250,
    "negative_ttl_ms": 250,
    "enable_readdir_plus": true,
    "auto_cache": true,
    "writeback_cache": false
  },
  "enable_xattrs": true,
  "enable_ads": false,
  "track_events": false,
  "security": {
    "enforce_posix_permissions": true,
    "default_uid": $(id -u),
    "default_gid": $(id -g),
    "enable_windows_acl_compat": false,
    "root_bypass_permissions": true
  },
  "backstore": {
    "HostFs": {
      "root": "$BACKSTORE_DIR",
      "prefer_native_snapshots": false
    }
  },
  "overlay": {
    "enabled": false,
    "lower_root": null,
    "copyup_mode": "Lazy"
  },
  "interpose": {
    "enabled": false,
    "max_copy_bytes": 1048576,
    "require_reflink": false,
    "allow_windows_reparse": false
  }
}
JSON

WORKDIR="$MOUNTPOINT/mknod-work"
log "Mounting AgentFS at $MOUNTPOINT"
rm -rf "$MOUNTPOINT"
mkdir -p "$MOUNTPOINT"
(cd "$REPO_ROOT" && AGENTFS_FUSE_ALLOW_OTHER=1 AGENTFS_FUSE_CONFIG="$FUSE_CONFIG" just mount-fuse "$MOUNTPOINT") >>"$RUN_DIR/mknod.log" 2>&1
sudo mkdir -p "$WORKDIR"
sudo chown "$(id -u)":"$(id -g)" "$WORKDIR"

fifo_path="$WORKDIR/test.fifo"
log "Creating FIFO $fifo_path"
mkfifo "$fifo_path"
(echo "payload" >"$fifo_path" &)
read read_back <"$fifo_path"
if [[ "$read_back" == "payload" ]]; then
  record_result "fifo_roundtrip" "passed" "payload"
else
  record_result "fifo_roundtrip" "failed" "expected payload got $read_back"
  exit 1
fi

char_path="$WORKDIR/dev-char"
sudo mknod "$char_path" c 12 34
stat_output=$(stat -c '%F|%t|%T' "$char_path")
IFS='|' read -r ftype majhex minhex <<<"$stat_output"
if [[ "$ftype" == "character special file" ]]; then
  record_result "char_type" "passed" "$ftype"
else
  record_result "char_type" "failed" "$ftype"
  exit 1
fi
if [[ "$majhex" == "c" && "$minhex" == "22" ]]; then
  record_result "char_rdev" "passed" "major=12 minor=34"
else
  record_result "char_rdev" "failed" "maj=$majhex min=$minhex"
  exit 1
fi

block_path="$WORKDIR/dev-block"
sudo mknod "$block_path" b 8 16
stat_output=$(stat -c '%F|%t|%T' "$block_path")
IFS='|' read -r ftype majhex minhex <<<"$stat_output"
if [[ "$ftype" == "block special file" ]]; then
  record_result "block_type" "passed" "$ftype"
else
  record_result "block_type" "failed" "$ftype"
  exit 1
fi
if [[ "$majhex" == "8" && "$minhex" == "10" ]]; then
  record_result "block_rdev" "passed" "major=8 minor=16"
else
  record_result "block_rdev" "failed" "maj=$majhex min=$minhex"
  exit 1
fi

log "Writing summary"
python3 - "$RESULTS_TMP" "$SUMMARY_FILE" <<'PY'
import json
import sys
from pathlib import Path

source = Path(sys.argv[1])
target = Path(sys.argv[2])
summary = []
if source.exists():
    for line in source.read_text().splitlines():
        if not line.strip():
            continue
        name, status, detail = line.split("|", 2)
        summary.append({"name": name, "status": status, "detail": detail})
else:
    summary.append({"name": "harness", "status": "failed", "detail": "no results"})

target.write_text(json.dumps(summary, indent=2) + "\n")
PY

log "mknod harness complete. Summary: $SUMMARY_FILE"
echo "mknod logs available at: $RUN_DIR"
