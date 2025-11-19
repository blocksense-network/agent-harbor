#!/usr/bin/env bash
# Copyright 2025 Schelling Point Labs Inc
# SPDX-License-Identifier: AGPL-3.0-only

set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
LOG_ROOT="$REPO_ROOT/logs"
TS="$(date +%Y%m%d-%H%M%S)"
RUN_DIR="$LOG_ROOT/fuse-overlay-ops-$TS"
LOG_FILE="$RUN_DIR/overlay.log"
MOUNTPOINT="${1:-/tmp/agentfs-overlay-ops}"
LOWER_DIR="$RUN_DIR/lower"
UPPER_DIR="$RUN_DIR/upper"
WORK_SUBDIR="workspace"
LOWER_WORK="$LOWER_DIR/$WORK_SUBDIR"
TARGET_DIR="$MOUNTPOINT"
USER_ID="$(id -u)"
GROUP_ID="$(id -g)"
CONFIG_PATH="$RUN_DIR/fuse-config.json"
mkdir -p "$RUN_DIR" "$LOWER_WORK" "$UPPER_DIR"
log() { echo "[$(date +%H:%M:%S)] $*" | tee -a "$LOG_FILE"; }
cleanup() {
  if mountpoint -q "$MOUNTPOINT" 2>/dev/null; then "$SCRIPT_DIR/umount-fuse.sh" "$MOUNTPOINT" >>"$LOG_FILE" 2>&1 || true; fi
  rm -rf "$MOUNTPOINT"
}
wait_state() {
  local p="$1"
  local exp="$2"
  local a=0
  while ((a < 50)); do
    if mountpoint -q "$p" 2>/dev/null; then [[ "$exp" == mounted ]] && return 0; else [[ "$exp" == unmounted ]] && return 0; fi
    sleep 0.1
    ((a += 1))
  done
  log "timeout waiting $p -> $exp"
  return 1
}
trap cleanup EXIT
log "Preparing lower-layer fixtures"
printf "lower file" >"$LOWER_WORK/pass_through.txt"
printf "meta" >"$LOWER_WORK/meta_only.txt"
printf "copy me" >"$LOWER_WORK/copy_up.txt"
printf "whiteout" >"$LOWER_WORK/remove_me.txt"
printf "lower shared" >"$LOWER_WORK/shared.txt"
printf "lower only" >"$LOWER_WORK/lower_only.txt"
log "Writing FsConfig with overlay + HostFs upper: $CONFIG_PATH"
cat >"$CONFIG_PATH" <<JSON
{
  "case_sensitivity": "Sensitive",
  "memory": {
    "max_bytes_in_memory": 268435456,
    "spill_directory": null
  },
  "limits": {
    "max_open_handles": 2048,
    "max_branches": 32,
    "max_snapshots": 64
  },
  "cache": {
    "attr_ttl_ms": 100,
    "entry_ttl_ms": 100,
    "negative_ttl_ms": 100,
    "enable_readdir_plus": false,
    "auto_cache": false,
    "writeback_cache": false
  },
  "enable_xattrs": true,
  "enable_ads": false,
  "track_events": false,
  "security": {
    "enforce_posix_permissions": true,
    "default_uid": $USER_ID,
    "default_gid": $GROUP_ID,
    "enable_windows_acl_compat": false,
    "root_bypass_permissions": true
  },
  "backstore": {
    "HostFs": {
      "root": "$UPPER_DIR",
      "prefer_native_snapshots": false
    }
  },
  "overlay": {
    "enabled": true,
    "lower_root": "$LOWER_WORK",
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
log "Building agentfs-fuse-host (with fuse feature) ..."
(
  cd "$REPO_ROOT"
  cargo build -p agentfs-fuse-host --features fuse
) >>"$LOG_FILE" 2>&1
cleanup
mkdir -p "$MOUNTPOINT"
log "Mounting overlay test target"
AGENTFS_FUSE_ALLOW_OTHER=1 AGENTFS_FUSE_CONFIG="$CONFIG_PATH" "$SCRIPT_DIR/mount-fuse.sh" "$MOUNTPOINT" >>"$LOG_FILE" 2>&1
wait_state "$MOUNTPOINT" mounted
run_root() {
  log "[root] $1"
  sudo bash -c "$2" >>"$LOG_FILE" 2>&1
}
run_user() {
  log "[user] $1"
  bash -c "$2" >>"$LOG_FILE" 2>&1
}
log "== Pass-through read =="
run_user "read lower file" "grep 'lower file' $TARGET_DIR/pass_through.txt"
run_user "lower file intact" "grep 'lower file' $LOWER_WORK/pass_through.txt"
log "== Copy-up on write =="
run_user "modify copy-up file" "echo 'upper change' >> $TARGET_DIR/copy_up.txt"
run_user "verify mount sees change" "grep 'upper change' $TARGET_DIR/copy_up.txt"
run_user "verify lower unaffected" "! grep -q 'upper change' $LOWER_WORK/copy_up.txt"
log "== Metadata-only overlay =="
run_root "chmod meta-only file" "chmod 600 $TARGET_DIR/meta_only.txt"
run_user "verify mode visible via mount" "[[ \$(stat -c %a $TARGET_DIR/meta_only.txt) == 600 ]]"
run_user "lower mode unchanged" "[[ \$(stat -c %a $LOWER_WORK/meta_only.txt) == 644 ]]"
log "== Whiteout validation =="
run_root "remove lower file" "rm $TARGET_DIR/remove_me.txt"
run_user "mount hides removed file" "! ls $TARGET_DIR | grep -q remove_me.txt"
run_user "lower file preserved" "[[ -f $LOWER_WORK/remove_me.txt ]]"
log "== Merged directory listing =="
run_user "list merged directory" "ls $TARGET_DIR > $RUN_DIR/merged.txt"
run_user "verify merged entries" "grep lower_only $RUN_DIR/merged.txt && grep shared $RUN_DIR/merged.txt && grep copy_up $RUN_DIR/merged.txt"
log "Cleaning up overlay mount"
"$SCRIPT_DIR/umount-fuse.sh" "$MOUNTPOINT" >>"$LOG_FILE" 2>&1
wait_state "$MOUNTPOINT" unmounted
rm -rf "$MOUNTPOINT"
log "Overlay semantics test complete. Logs: $RUN_DIR"
echo "Overlay logs available at: $RUN_DIR"
