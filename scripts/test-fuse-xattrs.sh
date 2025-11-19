#!/usr/bin/env bash
# Copyright 2025 Schelling Point Labs Inc
# SPDX-License-Identifier: AGPL-3.0-only

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
LOG_ROOT="$REPO_ROOT/logs"
TS="$(date +%Y%m%d-%H%M%S)"
RUN_DIR="$LOG_ROOT/fuse-xattrs-$TS"
SUMMARY_FILE="$RUN_DIR/summary.json"
MOUNTPOINT="/tmp/agentfs-xattrs-$TS"
BACKSTORE_DIR="$RUN_DIR/backstore"
FUSE_CONFIG="$RUN_DIR/fuse-config.json"
SKIP_BUILD="${SKIP_FUSE_BUILD:-}"
RESULTS_TMP="$RUN_DIR/results.tmp"

mkdir -p "$RUN_DIR" "$BACKSTORE_DIR"
: >"$RESULTS_TMP"

log() {
  echo "[$(date +%H:%M:%S)] $*" | tee -a "$RUN_DIR/xattrs.log"
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
    (cd "$REPO_ROOT" && just umount-fuse "$MOUNTPOINT") >>"$RUN_DIR/xattrs.log" 2>&1 || true
  fi
  rm -rf "$MOUNTPOINT"
}

trap cleanup EXIT

if [[ -z "$SKIP_BUILD" ]]; then
  log "Building agentfs-fuse-host"
  (cd "$REPO_ROOT" && just build-fuse-host) >>"$RUN_DIR/xattrs.log" 2>&1
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

log "Mounting AgentFS at $MOUNTPOINT"
rm -rf "$MOUNTPOINT"
mkdir -p "$MOUNTPOINT"
WORKDIR="$MOUNTPOINT/xattr-work"
(cd "$REPO_ROOT" && AGENTFS_FUSE_ALLOW_OTHER=1 AGENTFS_FUSE_CONFIG="$FUSE_CONFIG" just mount-fuse "$MOUNTPOINT") >>"$RUN_DIR/xattrs.log" 2>&1
sudo mkdir -p "$WORKDIR"
sudo chown "$(id -u)":"$(id -g)" "$WORKDIR"

user_file="$WORKDIR/xattr-user.txt"
touch "$user_file"
log "Setting user namespace xattr"
setfattr -n user.demo -v alpha "$user_file"
READ_VALUE=$(getfattr --only-values -n user.demo "$user_file")
if [[ "$READ_VALUE" == "alpha" ]]; then
  record_result "user_xattr_roundtrip" "passed" "value=alpha"
else
  record_result "user_xattr_roundtrip" "failed" "expected alpha got $READ_VALUE"
  exit 1
fi

if getfattr -d "$user_file" | grep -q "user.demo"; then
  record_result "user_xattr_list" "passed" "list contains user.demo"
else
  record_result "user_xattr_list" "failed" "list missing user.demo"
  exit 1
fi

trusted_file="$WORKDIR/xattr-trusted.txt"
touch "$trusted_file"
log "Setting trusted namespace xattr via sudo"
sudo setfattr -n trusted.secret -v beta "$trusted_file"
TRUST_VAL=$(sudo getfattr --only-values -n trusted.secret "$trusted_file")
if [[ "$TRUST_VAL" == "beta" ]]; then
  record_result "trusted_xattr_roundtrip" "passed" "value=beta"
else
  record_result "trusted_xattr_roundtrip" "failed" "expected beta got $TRUST_VAL"
  exit 1
fi

log "Testing removexattr"
sudo setfattr -n user.temp -v temp "$user_file"
sudo setfattr -x user.temp "$user_file"
if sudo getfattr --only-values -n user.temp "$user_file" >/dev/null 2>&1; then
  record_result "user_xattr_remove" "failed" "user.temp still present"
  exit 1
else
  record_result "user_xattr_remove" "passed" "user.temp removed"
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

log "xattr harness complete. Summary: $SUMMARY_FILE"
echo "xattr logs available at: $RUN_DIR"
