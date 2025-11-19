#!/usr/bin/env bash
# Copyright 2025 Schelling Point Labs Inc
# SPDX-License-Identifier: AGPL-3.0-only

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
LOG_ROOT="$REPO_ROOT/logs"
TS="$(date +%Y%m%d-%H%M%S)"
RUN_DIR="$LOG_ROOT/fuse-mount-options-$TS"
SUMMARY_FILE="$RUN_DIR/summary.json"
RESULTS_TMP="$RUN_DIR/results.tmp"
SKIP_BUILD="${SKIP_FUSE_BUILD:-}"

mkdir -p "$RUN_DIR"
: >"$RESULTS_TMP"

log() {
  echo "[$(date +%H:%M:%S)] $*" | tee -a "$RUN_DIR/mount-options.log"
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
      (cd "$REPO_ROOT" && just umount-fuse "$mnt") >>"$RUN_DIR/mount-options.log" 2>&1 || true
    fi
    local tries=0
    while [[ -d "$mnt" && $tries -lt 5 ]]; do
      if rm -rf "$mnt"; then
        break
      fi
      tries=$((tries + 1))
      sleep 0.2
    done
    if [[ -d "$mnt" ]]; then
      log "Warning: failed to remove mountpoint $mnt after unmount"
    fi
  fi
}

CURRENT_MOUNT=""
trap 'cleanup_mount "$CURRENT_MOUNT"' EXIT

wait_for_mount_ready() {
  local mnt="$1"
  for _ in {1..20}; do
    if mountpoint -q "$mnt" 2>/dev/null && [[ -d "$mnt/.agentfs" ]]; then
      return 0
    fi
    sleep 0.2
  done
  log "Mount at $mnt did not become ready in time"
  return 1
}

if [[ -z "$SKIP_BUILD" ]]; then
  log "Building agentfs-fuse-host"
  (cd "$REPO_ROOT" && just build-fuse-host) >>"$RUN_DIR/mount-options.log" 2>&1
else
  log "SKIP_FUSE_BUILD set; skipping build"
fi

write_config() {
  local path="$1"
  local attr_ms="$2"
  local entry_ms="$3"
  local neg_ms="$4"
  local enforce="$5"
  cat >"$path" <<JSON
{
  "case_sensitivity": "Sensitive",
  "memory": { "max_bytes_in_memory": 268435456, "spill_directory": null },
  "limits": { "max_open_handles": 4096, "max_branches": 64, "max_snapshots": 128 },
  "cache": {
    "attr_ttl_ms": $attr_ms,
    "entry_ttl_ms": $entry_ms,
    "negative_ttl_ms": $neg_ms,
    "enable_readdir_plus": true,
    "auto_cache": true,
    "writeback_cache": false
  },
  "enable_xattrs": true,
  "enable_ads": false,
  "track_events": false,
  "security": {
    "enforce_posix_permissions": $enforce,
    "default_uid": $(id -u),
    "default_gid": $(id -g),
    "enable_windows_acl_compat": false,
    "root_bypass_permissions": true
  },
  "backstore": {
    "HostFs": { "root": "$RUN_DIR/backstore", "prefer_native_snapshots": false }
  },
  "overlay": { "enabled": false, "lower_root": null, "copyup_mode": "Lazy" },
  "interpose": { "enabled": false, "max_copy_bytes": 1048576, "require_reflink": false, "allow_windows_reparse": false }
}
JSON
}

run_allow_other_case() {
  local flag="$1"
  local expected="$2"
  local cfg="$RUN_DIR/allow-config.json"
  write_config "$cfg" 250 250 250 true
  local mountpoint="/tmp/agentfs-allow-$flag-$TS"
  cleanup_mount "$mountpoint"
  mkdir -p "$mountpoint"
  CURRENT_MOUNT="$mountpoint"
  (cd "$REPO_ROOT" && AGENTFS_FUSE_ALLOW_OTHER="$flag" AGENTFS_FUSE_CONFIG="$cfg" just mount-fuse "$mountpoint") >>"$RUN_DIR/mount-options.log" 2>&1
  wait_for_mount_ready "$mountpoint"
  if sudo -u nobody bash -c "ls '$mountpoint' >/dev/null 2>&1"; then
    result="success"
  else
    result="failure"
  fi
  cleanup_mount "$mountpoint"
  if [[ "$result" == "$expected" ]]; then
    record_result "allow_other_$flag" "passed" "sudo -u nobody => $result"
  else
    record_result "allow_other_$flag" "failed" "expected $expected got $result"
    exit 1
  fi
}

check_default_permissions_flag() {
  local cfg="$RUN_DIR/default-config.json"
  write_config "$cfg" 250 250 250 true
  local mountpoint="/tmp/agentfs-defaultperm-$TS"
  cleanup_mount "$mountpoint"
  mkdir -p "$mountpoint"
  CURRENT_MOUNT="$mountpoint"
  (cd "$REPO_ROOT" && AGENTFS_FUSE_ALLOW_OTHER=1 AGENTFS_FUSE_CONFIG="$cfg" just mount-fuse "$mountpoint") >>"$RUN_DIR/mount-options.log" 2>&1

  wait_for_mount_ready "$mountpoint"
  local entry
  entry=$(grep -m1 "$mountpoint" /proc/mounts || true)
  if [[ "$entry" == *"default_permissions"* ]]; then
    record_result "default_permissions_flag" "passed" "found in /proc/mounts"
  else
    cleanup_mount "$mountpoint"
    record_result "default_permissions_flag" "failed" "entry=$entry"
    exit 1
  fi

  log "default_permissions root meta: $(stat -c '%u:%g:%a' "$mountpoint" 2>/dev/null || echo 'stat-failed')"
  sudo chown "$(id -u)":"$(id -g)" "$mountpoint" >>"$RUN_DIR/mount-options.log" 2>&1 || true
  log "default_permissions root meta after chown: $(stat -c '%u:%g:%a' "$mountpoint" 2>/dev/null || echo 'stat-failed')"

  local secret="$mountpoint/secret.txt"
  echo "classified" >"$secret"
  chmod 0600 "$secret"
  local enforce_status="failure"
  if sudo -u nobody bash -c "cat '$secret' >/dev/null" >>"$RUN_DIR/mount-options.log" 2>&1; then
    enforce_status="failure"
  else
    enforce_status="success"
  fi
  cleanup_mount "$mountpoint"
  if [[ "$enforce_status" == "success" ]]; then
    record_result "default_permissions_enforce" "passed" "sudo -u nobody denied by kernel"
  else
    record_result "default_permissions_enforce" "failed" "nobody read secret unexpectedly"
    exit 1
  fi
}

check_cache_ttls() {
  local cfg="$RUN_DIR/ttl-config.json"
  local attr_ms=5000
  local entry_ms=7000
  local neg_ms=9000
  write_config "$cfg" "$attr_ms" "$entry_ms" "$neg_ms" true
  local mountpoint="/tmp/agentfs-ttl-$TS"
  cleanup_mount "$mountpoint"
  mkdir -p "$mountpoint"
  CURRENT_MOUNT="$mountpoint"
  local host_log="$RUN_DIR/fuse-host-ttl.log"
  (cd "$REPO_ROOT" && RUST_LOG=info AGENTFS_FUSE_ALLOW_OTHER=1 AGENTFS_FUSE_CONFIG="$cfg" AGENTFS_FUSE_LOG_FILE="$host_log" just mount-fuse "$mountpoint") >>"$RUN_DIR/mount-options.log" 2>&1
  wait_for_mount_ready "$mountpoint"
  local cache_line
  cache_line=$(grep "Cache policy" "$host_log" | tail -n1 || true)
  cleanup_mount "$mountpoint"
  local expected="attr=5000ms entry=7000ms negative=9000ms"
  if [[ "$cache_line" == *"attr=5000ms"* && "$cache_line" == *"entry=7000ms"* && "$cache_line" == *"negative=9000ms"* ]]; then
    record_result "cache_ttl_log" "passed" "$expected"
  else
    record_result "cache_ttl_log" "failed" "line=$cache_line"
    exit 1
  fi
}

log "Running allow_other=0 case"
run_allow_other_case 0 failure
log "Running allow_other=1 case"
run_allow_other_case 1 success
log "Checking default_permissions flag"
check_default_permissions_flag
log "Checking cache TTL propagation"
check_cache_ttls

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

log "Mount option harness complete. Summary: $SUMMARY_FILE"
echo "mount option logs available at: $RUN_DIR"
