#!/usr/bin/env bash
# Copyright 2025 Schelling Point Labs Inc
# SPDX-License-Identifier: AGPL-3.0-only

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
LOG_ROOT="$REPO_ROOT/logs"
TS="$(date +%Y%m%d-%H%M%S)"
RUN_DIR="$LOG_ROOT/fuse-security-robustness-$TS"
SUMMARY_FILE="$RUN_DIR/summary.json"
RESULTS_TMP="$RUN_DIR/results.tmp"
LOG_FILE="$RUN_DIR/security-robustness.log"
SKIP_BUILD="${SKIP_FUSE_BUILD:-}"

mkdir -p "$RUN_DIR"
: >"$RESULTS_TMP"

log() {
  echo "[$(date +%H:%M:%S)] $*" | tee -a "$LOG_FILE" >&2
}

record_result() {
  local name="$1"
  local status="$2"
  local detail="$3"
  echo "$name|$status|$detail" >>"$RESULTS_TMP"
}

cleanup_mount() {
  local mnt="$1"
  if [[ -n "$mnt" && -d "$mnt" ]]; then
    if mountpoint -q "$mnt" 2>/dev/null; then
      (cd "$REPO_ROOT" && just umount-fuse "$mnt") >>"$LOG_FILE" 2>&1 || true
    fi
    rm -rf "$mnt" || true
  fi
}

CURRENT_MOUNT=""
trap 'cleanup_mount "$CURRENT_MOUNT"' EXIT

write_config() {
  local path="$1"
  local backstore_root="$2"
  cat >"$path" <<JSON
{
  "case_sensitivity": "Sensitive",
  "memory": { "max_bytes_in_memory": 134217728, "spill_directory": null },
  "limits": { "max_open_handles": 4096, "max_branches": 64, "max_snapshots": 128 },
  "cache": {
    "attr_ttl_ms": 500,
    "entry_ttl_ms": 500,
    "negative_ttl_ms": 500,
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
    "HostFs": { "root": "$backstore_root", "prefer_native_snapshots": false }
  },
  "overlay": { "enabled": false, "lower_root": null, "copyup_mode": "Lazy" },
  "interpose": { "enabled": false, "max_copy_bytes": 1048576, "require_reflink": false, "allow_windows_reparse": false }
}
JSON
}

mount_filesystem() {
  local cfg="$RUN_DIR/robustness-config.json"
  local store="$RUN_DIR/backstore"
  mkdir -p "$store"
  write_config "$cfg" "$store"
  local mnt="/tmp/agentfs-sec-robustness-$TS"
  cleanup_mount "$mnt"
  mkdir -p "$mnt"
  CURRENT_MOUNT="$mnt"
  log "Mounting AgentFS for robustness checks"
  (cd "$REPO_ROOT" && AGENTFS_FUSE_ALLOW_OTHER=1 AGENTFS_FUSE_CONFIG="$cfg" just mount-fuse "$mnt") >>"$LOG_FILE" 2>&1
  for _ in {1..20}; do
    if mountpoint -q "$mnt" 2>/dev/null && [[ -d "$mnt/.agentfs" ]]; then
      break
    fi
    sleep 0.2
  done
  sudo chown "$(id -u)":"$(id -g)" "$mnt" >>"$LOG_FILE" 2>&1 || true
  echo "$mnt"
}

expect_result_python() {
  local name="$1"
  local expect="$2"
  if python3 - "$RUN_DIR" "$CURRENT_MOUNT" >>"$LOG_FILE" 2>&1; then
    if [[ "$expect" == "success" ]]; then
      record_result "$name" "passed" "operation succeeded"
    else
      record_result "$name" "failed" "unexpected success"
      return 1
    fi
  else
    if [[ "$expect" == "failure" ]]; then
      record_result "$name" "passed" "operation failed as expected"
    else
      record_result "$name" "failed" "unexpected failure"
      return 1
    fi
  fi
}

run_robustness_checks() {
  local mnt
  mnt=$(mount_filesystem)
  local work="$mnt/.security-robustness"
  mkdir -p "$work"

  # Lower the soft nofile limit to provoke EMFILE without destabilizing the host.
  ulimit -n 128 || true

  expect_result_python "fd_exhaustion" "success" <<'PY'
import errno
import os
import sys

work = sys.argv[2]
paths = []
for i in range(0, 1024):
    try:
        path = os.path.join(work, f"fd-{i}.txt")
        fh = os.open(path, os.O_CREAT | os.O_RDWR, 0o644)
        paths.append(fh)
    except OSError as exc:
        if exc.errno in (errno.EMFILE, errno.ENFILE):
            break
        raise
if not paths:
    raise SystemExit("failed to open any handles")
for fh in paths:
    os.write(fh, b"x")
for fh in paths:
    os.close(fh)
PY

  expect_result_python "fd_after_exhaustion" "success" <<'PY'
import os, sys
work = sys.argv[2]
path = os.path.join(work, "after-exhaust.txt")
with open(path, "w") as fh:
    fh.write("ok")
with open(path) as fh:
    assert fh.read() == "ok"
PY

  # Temporarily cap file size to trigger EFBIG/EDQUOT style failures quickly.
  local prev_fsize
  prev_fsize=$(ulimit -f || true)
  ulimit -f 64 2>>"$LOG_FILE" || true
  expect_result_python "file_size_cap" "failure" <<'PY'
import os, sys, errno
work = sys.argv[2]
path = os.path.join(work, "fsize-cap.bin")
try:
    with open(path, "wb") as fh:
        fh.write(b"x" * (1024 * 1024))
except OSError as exc:
    if exc.errno in (errno.EFBIG, errno.EDQUOT):
        sys.exit(1)
    raise
sys.exit(0)
PY
  ulimit -f "$prev_fsize" 2>>"$LOG_FILE" || true

  cleanup_mount "$mnt"
  CURRENT_MOUNT=""
}

log "Starting robustness harness at $TS"

if [[ -z "$SKIP_BUILD" ]]; then
  log "Building agentfs-fuse-host"
  (cd "$REPO_ROOT" && just build-fuse-host) >>"$LOG_FILE" 2>&1
else
  log "SKIP_FUSE_BUILD set; skipping build"
fi

run_robustness_checks

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

if python3 - "$SUMMARY_FILE" <<'PY'; then
import json, sys
data = json.load(open(sys.argv[1]))
sys.exit(0 if all(item.get("status") == "passed" for item in data) else 1)
PY
  log "All robustness checks passed"
else
  log "One or more robustness checks failed (see summary)"
  exit 1
fi

log "Robustness harness complete. Summary: $SUMMARY_FILE"
echo "robustness harness logs available at: $RUN_DIR"
