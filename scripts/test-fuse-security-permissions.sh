#!/usr/bin/env bash
# Copyright 2025 Schelling Point Labs Inc
# SPDX-License-Identifier: AGPL-3.0-only

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
LOG_ROOT="$REPO_ROOT/logs"
TS="$(date +%Y%m%d-%H%M%S)"
RUN_DIR="$LOG_ROOT/fuse-security-permissions-$TS"
SUMMARY_FILE="$RUN_DIR/summary.json"
RESULTS_TMP="$RUN_DIR/results.tmp"
LOG_FILE="$RUN_DIR/security-permissions.log"
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

nobody_ids() {
  id -u "$NOBODY_USER" >/dev/null 2>&1 && id -g "$NOBODY_USER" >/dev/null 2>&1 && id -gn "$NOBODY_USER" >/dev/null 2>&1
}

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
  local mnt="/tmp/agentfs-sec-perm-${label}-$TS"
  cleanup_mount "$mnt"
  mkdir -p "$mnt"
  CURRENT_MOUNT="$mnt"
  local host_log="$RUN_DIR/fuse-host-${label}.log"
  log "Mounting AgentFS ($label) with root_bypass_permissions=$root_bypass"
  (cd "$REPO_ROOT" && AGENTFS_FUSE_ALLOW_OTHER=1 AGENTFS_FUSE_CONFIG="$cfg" AGENTFS_FUSE_LOG_FILE="$host_log" just mount-fuse "$mnt") >>"$LOG_FILE" 2>&1
  wait_for_mount_ready "$mnt"
  sudo chown "$(id -u)":"$(id -g)" "$mnt" >>"$LOG_FILE" 2>&1 || true
  echo "$mnt"
}

check_default_permissions_flag() {
  local label="$1"
  local mnt="$2"
  local expect_present="$3"
  local entry
  entry=$(grep -m1 "$mnt" /proc/mounts || true)
  if [[ "$expect_present" == "true" ]]; then
    if [[ "$entry" == *"default_permissions"* ]]; then
      record_result "default_permissions_${label}" "passed" "entry contains default_permissions"
    else
      record_result "default_permissions_${label}" "failed" "entry=$entry"
      return 1
    fi
  else
    if [[ "$entry" == *"default_permissions"* ]]; then
      record_result "default_permissions_${label}" "failed" "unexpected default_permissions in mount options"
      return 1
    else
      record_result "default_permissions_${label}" "passed" "entry omits default_permissions as expected"
    fi
  fi
}

owner_only_access() {
  local dir="$1"
  local name="$2"
  local file="$dir/${name}-owner.txt"
  echo "secret:$name" >"$file"
  chmod 0600 "$file"
  if ! cat "$file" >/dev/null 2>&1; then
    record_result "owner_access_${name}" "failed" "owner read failed"
    return 1
  fi
  if sudo -u "$NOBODY_USER" -- bash -c "cat '$file' >/dev/null" >>"$LOG_FILE" 2>&1; then
    record_result "owner_access_${name}" "failed" "nobody read owner-only file"
    return 1
  fi
  if ! echo "ok" >>"$file"; then
    record_result "owner_access_${name}" "failed" "owner write failed"
    return 1
  fi
  record_result "owner_access_${name}" "passed" "owner rw ok; other denied"
}

group_read_only() {
  local dir="$1"
  local name="$2"
  local file="$dir/${name}-group-read.txt"
  local gid="$3"
  echo "group-read" >"$file"
  sudo chown "$(id -u):$gid" "$file" >>"$LOG_FILE" 2>&1
  chmod 0640 "$file"
  if ! sudo -u "$NOBODY_USER" -- bash -c "cat '$file' >/dev/null" >>"$LOG_FILE" 2>&1; then
    record_result "group_read_${name}" "failed" "nobody could not read with group r--"
    return 1
  fi
  if sudo -u "$NOBODY_USER" -- bash -c "echo write >>'$file'" >>"$LOG_FILE" 2>&1; then
    record_result "group_read_${name}" "failed" "nobody wrote despite missing group w"
    return 1
  fi
  record_result "group_read_${name}" "passed" "group read ok; group write blocked"
}

group_write_allowed() {
  local dir="$1"
  local name="$2"
  local gid="$3"
  local file="$dir/${name}-group-write.txt"
  echo "group-write" >"$file"
  sudo chown "$(id -u):$gid" "$file" >>"$LOG_FILE" 2>&1
  chmod 0660 "$file"
  if sudo -u "$NOBODY_USER" -- bash -c "echo append >>'$file'" >>"$LOG_FILE" 2>&1; then
    record_result "group_write_${name}" "passed" "group write permitted"
  else
    record_result "group_write_${name}" "failed" "group write rejected unexpectedly"
    return 1
  fi
}

other_exec_denied() {
  local dir="$1"
  local name="$2"
  local script="$dir/${name}-owner-script.sh"
  cat >"$script" <<'SH'
#!/usr/bin/env bash
exit 0
SH
  chmod 0700 "$script"
  if sudo -u "$NOBODY_USER" -- "$script" >>"$LOG_FILE" 2>&1; then
    record_result "other_exec_${name}" "failed" "nobody executed owner-only script"
    return 1
  fi
  record_result "other_exec_${name}" "passed" "exec denied to other as expected"
}

sticky_prevents_unlink() {
  local dir="$1"
  local name="$2"
  local sticky="$dir/${name}-sticky"
  mkdir -p "$sticky"
  chmod 1777 "$sticky"
  local file="$sticky/owned-by-self"
  echo "sticky" >"$file"
  if sudo -u "$NOBODY_USER" -- bash -c "rm '$file'" >>"$LOG_FILE" 2>&1; then
    record_result "sticky_unlink_${name}" "failed" "nobody unlinked file in sticky dir"
    return 1
  fi
  record_result "sticky_unlink_${name}" "passed" "sticky dir blocked unlink"
}

chmod_requires_owner() {
  local dir="$1"
  local name="$2"
  local file="$dir/${name}-chmod.txt"
  echo "perm" >"$file"
  chmod 0644 "$file"
  if sudo -u "$NOBODY_USER" -- bash -c "chmod 777 '$file'" >>"$LOG_FILE" 2>&1; then
    record_result "chmod_authz_${name}" "failed" "nobody changed mode unexpectedly"
    return 1
  fi
  record_result "chmod_authz_${name}" "passed" "chmod restricted to owner/root"
}

root_bypass_disabled_check() {
  local dir="$1"
  local name="$2"
  local file="$dir/${name}-root-blocked.txt"
  echo "root bypass test" >"$file"
  chmod 000 "$file"
  if sudo -u root -- bash -c "cat '$file' >/dev/null" >>"$LOG_FILE" 2>&1; then
    record_result "root_bypass_${name}" "failed" "root read despite root_bypass_permissions=false"
    return 1
  fi
  record_result "root_bypass_${name}" "passed" "root denied when bypass disabled"
}

run_matrix() {
  local label="$1"
  local root_bypass="$2"
  local mountpoint
  mountpoint=$(mount_filesystem "$label" "$root_bypass")
  local workdir="$mountpoint/.security-permissions"
  mkdir -p "$workdir"
  check_default_permissions_flag "$label" "$mountpoint" "$root_bypass"

  local nobody_gid
  nobody_gid=$(id -g "$NOBODY_USER")

  owner_only_access "$workdir" "$label" || true
  group_read_only "$workdir" "$label" "$nobody_gid" || true
  group_write_allowed "$workdir" "$label" "$nobody_gid" || true
  other_exec_denied "$workdir" "$label" || true
  sticky_prevents_unlink "$workdir" "$label" || true
  chmod_requires_owner "$workdir" "$label" || true

  if [[ "$root_bypass" == "false" ]]; then
    root_bypass_disabled_check "$workdir" "$label" || true
  fi

  cleanup_mount "$mountpoint"
  CURRENT_MOUNT=""
}

log "Starting security permission harness run at $TS"

if [[ -z "$SKIP_BUILD" ]]; then
  log "Building agentfs-fuse-host"
  (cd "$REPO_ROOT" && just build-fuse-host) >>"$LOG_FILE" 2>&1
else
  log "SKIP_FUSE_BUILD set; skipping build"
fi

if ! nobody_ids; then
  record_result "harness" "failed" "user $NOBODY_USER not available"
  log "User $NOBODY_USER unavailable; aborting"
else
  run_matrix "bypass_on" "true"
  run_matrix "bypass_off" "false"
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

if python3 - "$SUMMARY_FILE" <<'PY'; then
import json, sys
data = json.load(open(sys.argv[1]))
sys.exit(0 if all(item.get("status") == "passed" for item in data) else 1)
PY
  log "All security permission checks passed"
else
  log "One or more security permission checks failed (see summary)"
  exit 1
fi

log "Security permission harness complete. Summary: $SUMMARY_FILE"
echo "security permission logs available at: $RUN_DIR"
