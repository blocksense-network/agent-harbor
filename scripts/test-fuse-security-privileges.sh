#!/usr/bin/env bash
# Copyright 2025 Schelling Point Labs Inc
# SPDX-License-Identifier: AGPL-3.0-only

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
LOG_ROOT="$REPO_ROOT/logs"
TS="$(date +%Y%m%d-%H%M%S)"
RUN_DIR="$LOG_ROOT/fuse-security-privileges-$TS"
SUMMARY_FILE="$RUN_DIR/summary.json"
RESULTS_TMP="$RUN_DIR/results.tmp"
LOG_FILE="$RUN_DIR/security-privileges.log"
SKIP_BUILD="${SKIP_FUSE_BUILD:-}"
NOBODY_USER="${AGENTFS_FUSE_SECURITY_USER:-nobody}"

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
  local root_bypass="$2"
  local backstore_root="$3"
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
    "root_bypass_permissions": $root_bypass
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
  local label="$1"
  local root_bypass="$2"
  local cfg="$RUN_DIR/${label}-config.json"
  local store="$RUN_DIR/backstore-$label"
  mkdir -p "$store"
  write_config "$cfg" "$root_bypass" "$store"
  local mnt="/tmp/agentfs-sec-priv-${label}-$TS"
  cleanup_mount "$mnt"
  mkdir -p "$mnt"
  CURRENT_MOUNT="$mnt"
  log "Mounting AgentFS ($label) with root_bypass_permissions=$root_bypass"
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

assert_fail_as_user() {
  local name="$1"
  shift
  if "$@" >>"$LOG_FILE" 2>&1; then
    record_result "$name" "failed" "unexpected success"
    return 1
  fi
  record_result "$name" "passed" "operation blocked"
}

assert_pass() {
  local name="$1"
  shift
  if "$@" >>"$LOG_FILE" 2>&1; then
    record_result "$name" "passed" "operation succeeded"
  else
    record_result "$name" "failed" "expected success"
    return 1
  fi
}

run_privilege_checks() {
  local label="$1"
  local root_bypass="$2"
  local mnt
  mnt=$(mount_filesystem "$label" "$root_bypass")
  local work="$mnt/.security-privileges"
  mkdir -p "$work"

  local secret="$work/owner-secret.txt"
  echo "secret" >"$secret"
  chmod 0600 "$secret"
  assert_fail_as_user "nobody_read_${label}" sudo -u "$NOBODY_USER" -- bash -c "cat '$secret'"
  assert_fail_as_user "nobody_write_${label}" sudo -u "$NOBODY_USER" -- bash -c "echo hi >>'$secret'"
  assert_fail_as_user "nobody_chown_${label}" sudo -u "$NOBODY_USER" -- chown "$NOBODY_USER" "$secret"

  local sticky="$work/sticky"
  mkdir -p "$sticky"
  chmod 1777 "$sticky"
  local owned="$sticky/owned.txt"
  echo "owned" >"$owned"
  assert_fail_as_user "sticky_delete_${label}" sudo -u "$NOBODY_USER" -- rm -f "$owned"

  local root_target="$work/root-only.txt"
  echo "root-check" >"$root_target"
  chmod 000 "$root_target"
  if [[ "$root_bypass" == "true" ]]; then
    assert_pass "root_read_${label}" sudo -u root -- cat "$root_target"
  else
    assert_fail_as_user "root_read_${label}" sudo -u root -- cat "$root_target"
  fi

  cleanup_mount "$mnt"
  CURRENT_MOUNT=""
}

log "Starting privilege escalation harness at $TS"

if [[ -z "$SKIP_BUILD" ]]; then
  log "Building agentfs-fuse-host"
  (cd "$REPO_ROOT" && just build-fuse-host) >>"$LOG_FILE" 2>&1
else
  log "SKIP_FUSE_BUILD set; skipping build"
fi

run_privilege_checks "bypass_on" "true"
run_privilege_checks "bypass_off" "false"

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
  log "All privilege checks passed"
else
  log "One or more privilege checks failed (see summary)"
  exit 1
fi

log "Privilege harness complete. Summary: $SUMMARY_FILE"
echo "privilege harness logs available at: $RUN_DIR"
