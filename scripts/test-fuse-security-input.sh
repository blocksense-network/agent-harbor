#!/usr/bin/env bash
# Copyright 2025 Schelling Point Labs Inc
# SPDX-License-Identifier: AGPL-3.0-only

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
LOG_ROOT="$REPO_ROOT/logs"
TS="$(date +%Y%m%d-%H%M%S)"
RUN_DIR="$LOG_ROOT/fuse-security-input-$TS"
SUMMARY_FILE="$RUN_DIR/summary.json"
RESULTS_TMP="$RUN_DIR/results.tmp"
LOG_FILE="$RUN_DIR/security-input.log"
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
  "memory": { "max_bytes_in_memory": 268435456, "spill_directory": null },
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
  local cfg="$RUN_DIR/input-config.json"
  local store="$RUN_DIR/backstore"
  mkdir -p "$store"
  write_config "$cfg" "$store"
  local mnt="/tmp/agentfs-sec-input-$TS"
  cleanup_mount "$mnt"
  mkdir -p "$mnt"
  CURRENT_MOUNT="$mnt"
  log "Mounting AgentFS for input validation"
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

expect_failure_python() {
  local name="$1"
  if python3 - "$RUN_DIR" "$CURRENT_MOUNT" >>"$LOG_FILE" 2>&1; then
    record_result "$name" "failed" "unexpected success"
    return 1
  else
    record_result "$name" "passed" "operation rejected"
  fi
}

expect_success_python() {
  local name="$1"
  if python3 - "$RUN_DIR" "$CURRENT_MOUNT" >>"$LOG_FILE" 2>&1; then
    record_result "$name" "passed" "operation succeeded"
  else
    record_result "$name" "failed" "unexpected failure"
    return 1
  fi
}

run_input_checks() {
  local mnt
  mnt=$(mount_filesystem)
  local work="$mnt/.security-input"
  mkdir -p "$work"

  expect_failure_python "path_traversal" <<'PY'
import os, sys, errno
work = sys.argv[2]
target = os.path.join(work, "../../etc/passwd")
try:
    with open(target, "rb") as fh:
        fh.read(16)
    sys.exit(0)
except OSError as exc:
    if exc.errno in (errno.ENOENT, errno.EACCES, errno.ENOTDIR, errno.ELOOP):
        sys.exit(1)
    raise
PY

  expect_failure_python "overlong_name" <<'PY'
import os, sys, errno
work = sys.argv[2]
name = "a" * 300
path = os.path.join(work, name)
try:
    os.open(path, os.O_CREAT | os.O_WRONLY, 0o644)
    sys.exit(0)
except OSError as exc:
    if exc.errno in (errno.ENAMETOOLONG,):
        sys.exit(1)
    raise
PY

  expect_failure_python "invalid_utf8" <<'PY'
import os, sys, errno
work = sys.argv[2]
name = b"\xff\xfe\xfa\xfbname"
path = os.fsencode(work) + b"/" + name
try:
    os.open(path, os.O_CREAT | os.O_WRONLY, 0o644)
    sys.exit(0)
except OSError as exc:
    if exc.errno in (errno.EINVAL, errno.ENOENT, errno.EILSEQ):
        sys.exit(1)
    raise
PY

  expect_success_python "special_chars" <<'PY'
import os, sys
work = sys.argv[2]
name = "weird name !@#$%^&*()[]{}"
path = os.path.join(work, name)
with open(path, "w", encoding="utf-8") as fh:
    fh.write("ok")
with open(path, "r", encoding="utf-8") as fh:
    data = fh.read()
if data != "ok":
    raise SystemExit("data mismatch")
PY

  expect_success_python "post_checks_alive" <<'PY'
import os, sys
work = sys.argv[2]
path = os.path.join(work, "liveness.txt")
with open(path, "w") as fh:
    fh.write("alive")
with open(path, "r") as fh:
    assert fh.read() == "alive"
PY

  cleanup_mount "$mnt"
  CURRENT_MOUNT=""
}

log "Starting input validation harness at $TS"

if [[ -z "$SKIP_BUILD" ]]; then
  log "Building agentfs-fuse-host"
  (cd "$REPO_ROOT" && just build-fuse-host) >>"$LOG_FILE" 2>&1
else
  log "SKIP_FUSE_BUILD set; skipping build"
fi

run_input_checks

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
  log "All input validation checks passed"
else
  log "One or more input validation checks failed (see summary)"
  exit 1
fi

log "Input validation harness complete. Summary: $SUMMARY_FILE"
echo "input validation logs available at: $RUN_DIR"
