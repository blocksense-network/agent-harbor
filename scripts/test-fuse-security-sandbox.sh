#!/usr/bin/env bash
# Copyright 2025 Schelling Point Labs Inc
# SPDX-License-Identifier: AGPL-3.0-only

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
LOG_ROOT="$REPO_ROOT/logs"
TS="$(date +%Y%m%d-%H%M%S)"
RUN_DIR="$LOG_ROOT/fuse-security-sandbox-$TS"
SUMMARY_FILE="$RUN_DIR/summary.json"
RESULTS_TMP="$RUN_DIR/results.tmp"
LOG_FILE="$RUN_DIR/security-sandbox.log"
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
  "limits": { "max_open_handles": 2048, "max_branches": 64, "max_snapshots": 128 },
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
  local cfg="$RUN_DIR/sandbox-config.json"
  local store="$RUN_DIR/backstore"
  mkdir -p "$store"
  write_config "$cfg" "$store"
  local mnt="/tmp/agentfs-sec-sandbox-$TS"
  cleanup_mount "$mnt"
  mkdir -p "$mnt"
  CURRENT_MOUNT="$mnt"
  log "Mounting AgentFS for sandbox boundary checks"
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

expect_failure_cmd() {
  local name="$1"
  shift
  if "$@" >>"$LOG_FILE" 2>&1; then
    record_result "$name" "failed" "unexpected success"
    return 1
  fi
  record_result "$name" "passed" "blocked as expected"
}

expect_success_cmd() {
  local name="$1"
  shift
  if "$@" >>"$LOG_FILE" 2>&1; then
    record_result "$name" "passed" "operation succeeded"
  else
    record_result "$name" "failed" "unexpected failure"
    return 1
  fi
}

run_sandbox_checks() {
  local mnt
  mnt=$(mount_filesystem)
  local work="$mnt/.security-sandbox"
  mkdir -p "$work"

  if ln -sf /etc/passwd "$work/passwd-link"; then
    expect_failure_cmd "symlink_outside" bash -c "cat '$work/passwd-link' >/dev/null"
  else
    record_result "symlink_outside" "passed" "symlink creation blocked"
  fi

  if ln -sf / "$work/root-link"; then
    expect_failure_cmd "symlink_root_escape" bash -c "ls '$work/root-link/etc' >/dev/null"
  else
    record_result "symlink_root_escape" "passed" "symlink creation blocked"
  fi

  expect_failure_cmd "relative_traversal" bash -c "cat '$work/../../etc/passwd' >/dev/null"

  # Ensure no accidental copy of host file landed inside the mount
  expect_success_cmd "backstore_integrity" bash -c "test -f '$RUN_DIR/backstore/etc/passwd' && exit 1 || true"

  # Confirm normal in-mount operations still work
  echo "ok" >"$work/inside.txt"
  expect_success_cmd "in_mount_access" bash -c "cat '$work/inside.txt' >/dev/null"

  cleanup_mount "$mnt"
  CURRENT_MOUNT=""
}

log "Starting sandbox boundary harness at $TS"

if [[ -z "$SKIP_BUILD" ]]; then
  log "Building agentfs-fuse-host"
  (cd "$REPO_ROOT" && just build-fuse-host) >>"$LOG_FILE" 2>&1
else
  log "SKIP_FUSE_BUILD set; skipping build"
fi

run_sandbox_checks

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
  log "All sandbox checks passed"
else
  log "One or more sandbox checks failed (see summary)"
  exit 1
fi

log "Sandbox harness complete. Summary: $SUMMARY_FILE"
echo "sandbox harness logs available at: $RUN_DIR"
