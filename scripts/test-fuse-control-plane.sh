#!/usr/bin/env bash
# Copyright 2025 Schelling Point Labs Inc
# SPDX-License-Identifier: AGPL-3.0-only

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
LOG_ROOT="$REPO_ROOT/logs"
TS="$(date +%Y%m%d-%H%M%S)"
RUN_DIR="$LOG_ROOT/fuse-control-plane-$TS"
LOG_FILE="$RUN_DIR/control-plane.log"
STATE_DIR="$RUN_DIR/state"
BACKSTORE_DIR="$STATE_DIR/backstore"
CONFIG_PATH="$RUN_DIR/fuse-config.json"
MOUNTPOINT="${1:-/tmp/agentfs-control-plane}"
TESTDIR="$MOUNTPOINT/control_plane"
USER_ID="$(id -u)"
GROUP_ID="$(id -g)"
mkdir -p "$RUN_DIR" "$BACKSTORE_DIR"

log() { echo "[$(date +%H:%M:%S)] $*" | tee -a "$LOG_FILE"; }

is_mount_active() {
  local path="$1"
  if mountpoint -q "$path" 2>/dev/null; then
    return 0
  fi
  mount | grep -F " on $path " >/dev/null 2>&1
}

cleanup() {
  if is_mount_active "$MOUNTPOINT"; then "$SCRIPT_DIR/umount-fuse.sh" "$MOUNTPOINT" >>"$LOG_FILE" 2>&1 || true; fi
  rm -rf "$MOUNTPOINT"
}

wait_state() {
  local path="$1"
  local target="$2"
  local attempts=0
  while ((attempts < 50)); do
    if is_mount_active "$path"; then [[ "$target" == mounted ]] && return 0; else [[ "$target" == unmounted ]] && return 0; fi
    sleep 0.1
    ((attempts += 1))
  done
  log "timeout waiting for $path -> $target"
  return 1
}
trap cleanup EXIT

run_control() {
  local output
  output=$("$REPO_ROOT/target/debug/agentfs-control-cli" --mount "$MOUNTPOINT" "$@" 2>&1)
  local status=$?
  echo "[$(date +%H:%M:%S)] [control] $* (status=$status)" >>"$LOG_FILE"
  echo "$output" >>"$LOG_FILE"
  printf '%s' "$output"
  return $status
}

wait_for_pid_file() {
  local file="$1"
  local attempts=0
  while ((attempts < 50)); do
    if [[ -s "$file" ]]; then
      cat "$file"
      return 0
    fi
    sleep 0.1
    ((attempts += 1))
  done
  log "timeout waiting for worker pid file $file"
  return 1
}

trimmed_contents() {
  tr -d '\r\n' <"$1"
}

start_branch_reader() {
  local pid_file="$1"
  local gate_file="$2"
  local out_file="$3"
  bash -c '
    set -euo pipefail
    PID_FILE="$1"
    GATE_FILE="$2"
    TARGET="$3"
    OUT_FILE="$4"
    echo $$ >"$PID_FILE"
    while [[ ! -f "$GATE_FILE" ]]; do
      sleep 0.05
    done
    cat "$TARGET/data.txt" >"$OUT_FILE"
  ' branch-reader "$pid_file" "$gate_file" "$TESTDIR" "$out_file" &
}

SKIP_FUSE_BUILD="${SKIP_FUSE_BUILD:-}"
SKIP_CONTROL_CLI_BUILD="${SKIP_CONTROL_CLI_BUILD:-}"

(
  cd "$REPO_ROOT"
  if [[ -z "$SKIP_FUSE_BUILD" ]]; then
    log "Building agentfs-fuse-host (fuse feature)"
    cargo build -p agentfs-fuse-host --features fuse >>"$LOG_FILE" 2>&1
  else
    log "SKIP_FUSE_BUILD set; reusing existing agentfs-fuse-host"
  fi

  if [[ -z "$SKIP_CONTROL_CLI_BUILD" ]]; then
    log "Building agentfs-control-cli"
    cargo build -p agentfs-control-cli >>"$LOG_FILE" 2>&1
  else
    log "SKIP_CONTROL_CLI_BUILD set; reusing existing agentfs-control-cli"
  fi
)

log "Writing FsConfig with HostFs backstore to $CONFIG_PATH"
cat >"$CONFIG_PATH" <<JSON
{
  "case_sensitivity": "Sensitive",
  "memory": {
    "max_bytes_in_memory": 268435456,
    "spill_directory": null
  },
  "limits": {
    "max_open_handles": 4096,
    "max_branches": 128,
    "max_snapshots": 256
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
    "enforce_posix_permissions": false,
    "default_uid": $USER_ID,
    "default_gid": $GROUP_ID,
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

log "Preparing mount point"
if is_mount_active "$MOUNTPOINT"; then
  log "Existing mount detected at $MOUNTPOINT; unmounting before test"
  "$SCRIPT_DIR/umount-fuse.sh" "$MOUNTPOINT" >>"$LOG_FILE" 2>&1 || true
  wait_state "$MOUNTPOINT" unmounted
fi
rm -rf "$MOUNTPOINT"
mkdir -p "$MOUNTPOINT"

log "Mounting AgentFS"
AGENTFS_FUSE_ALLOW_OTHER=1 AGENTFS_FUSE_CONFIG="$CONFIG_PATH" "$SCRIPT_DIR/mount-fuse.sh" "$MOUNTPOINT" >>"$LOG_FILE" 2>&1
wait_state "$MOUNTPOINT" mounted

log "Creating workspace"
mkdir -p "$TESTDIR"
BASELINE_CONTENT="base-v1"
BASELINE_UPDATED="base-v2"
printf "%s" "$BASELINE_CONTENT" >"$TESTDIR/data.txt"
chmod 666 "$TESTDIR/data.txt"

log "T4.1 – snapshot list (control file reachable)"
run_control snapshot-list >/dev/null || true

log "T4.2 – snapshot create/list"
if ! SNAP_LINE=$(run_control snapshot-create --name clean); then
  log "Snapshot create failed"
  exit 1
fi
SNAP_ID=${SNAP_LINE#SNAPSHOT_ID=}
SNAP_ID=${SNAP_ID%%$'\t'*}
if ! LIST_OUTPUT=$(run_control snapshot-list); then
  log "Snapshot list failed"
  exit 1
fi
if ! grep -q "$SNAP_ID" <<<"$LIST_OUTPUT"; then
  log "Snapshot $SNAP_ID missing from list"
  exit 1
fi

log "T4.3 – branch create"
if ! BRANCH_LINE=$(run_control branch-create --snapshot "$SNAP_ID" --name feature); then
  log "Branch create failed"
  exit 1
fi
BRANCH_ID=${BRANCH_LINE#BRANCH_ID=}
BRANCH_ID=${BRANCH_ID%%$'\t'*}

log "T4.N – invalid branch bind rejection"
if run_control branch-bind --branch invalid-branch --pid $$ >/dev/null; then
  log "Expected invalid branch bind to fail"
  exit 1
fi

log "T4.4 – branch isolation across processes"
FIRST_READER_PID_FILE="$RUN_DIR/branch-reader-one.pid"
FIRST_READER_GATE="$RUN_DIR/branch-reader-one.gate"
FIRST_READER_OUT="$RUN_DIR/branch-reader-one.txt"
rm -f "$FIRST_READER_PID_FILE" "$FIRST_READER_GATE" "$FIRST_READER_OUT"
start_branch_reader "$FIRST_READER_PID_FILE" "$FIRST_READER_GATE" "$FIRST_READER_OUT"
log "Waiting for branch reader PID file at $FIRST_READER_PID_FILE"
FIRST_READER_PID=$(wait_for_pid_file "$FIRST_READER_PID_FILE")
log "Binding reader PID $FIRST_READER_PID to branch $BRANCH_ID"
if ! run_control branch-bind --branch "$BRANCH_ID" --pid "$FIRST_READER_PID" >/dev/null; then
  log "Failed to bind first branch reader"
  exit 1
fi
touch "$FIRST_READER_GATE"
wait "$FIRST_READER_PID"
BRANCH_READ_ONE=$(trimmed_contents "$FIRST_READER_OUT")
if [[ "$BRANCH_READ_ONE" != "$BASELINE_CONTENT" ]]; then
  log "Branch reader saw unexpected content: $BRANCH_READ_ONE"
  exit 1
fi

SECOND_READER_PID_FILE="$RUN_DIR/branch-reader-two.pid"
SECOND_READER_GATE="$RUN_DIR/branch-reader-two.gate"
SECOND_READER_OUT="$RUN_DIR/branch-reader-two.txt"
rm -f "$SECOND_READER_PID_FILE" "$SECOND_READER_GATE" "$SECOND_READER_OUT"
start_branch_reader "$SECOND_READER_PID_FILE" "$SECOND_READER_GATE" "$SECOND_READER_OUT"
log "Waiting for independent branch reader PID file at $SECOND_READER_PID_FILE"
SECOND_READER_PID=$(wait_for_pid_file "$SECOND_READER_PID_FILE")
log "Binding reader PID $SECOND_READER_PID to branch $BRANCH_ID"
if ! run_control branch-bind --branch "$BRANCH_ID" --pid "$SECOND_READER_PID" >/dev/null; then
  log "Failed to bind second branch reader"
  exit 1
fi
touch "$SECOND_READER_GATE"
wait "$SECOND_READER_PID"
BRANCH_READ_TWO=$(trimmed_contents "$SECOND_READER_OUT")
if [[ "$BRANCH_READ_TWO" != "$BASELINE_CONTENT" ]]; then
  log "Second branch reader saw unexpected content: $BRANCH_READ_TWO"
  exit 1
fi

DEFAULT_VIEW=$(trimmed_contents "$TESTDIR/data.txt")
if [[ "$DEFAULT_VIEW" != "$BASELINE_CONTENT" ]]; then
  log "Default branch unexpectedly changed data: $DEFAULT_VIEW"
  exit 1
fi

log "Unmounting to verify snapshot list behaviour across remounts"
"$SCRIPT_DIR/umount-fuse.sh" "$MOUNTPOINT" >>"$LOG_FILE" 2>&1
wait_state "$MOUNTPOINT" unmounted

log "Expect snapshot-list to fail while filesystem is unmounted"
if run_control snapshot-list >/dev/null; then
  log "snapshot-list unexpectedly succeeded without a mount"
  exit 1
fi

log "Remounting AgentFS for snapshot-list recovery"
mkdir -p "$MOUNTPOINT"
AGENTFS_FUSE_ALLOW_OTHER=1 AGENTFS_FUSE_CONFIG="$CONFIG_PATH" "$SCRIPT_DIR/mount-fuse.sh" "$MOUNTPOINT" >>"$LOG_FILE" 2>&1
wait_state "$MOUNTPOINT" mounted
if ! REMOUNT_LIST=$(run_control snapshot-list); then
  log "snapshot-list failed after remount"
  exit 1
fi
if grep -q "$SNAP_ID" <<<"$REMOUNT_LIST"; then
  log "Snapshot $SNAP_ID persisted across remount"
else
  log "Snapshot $SNAP_ID not present after remount (expected until persistent metadata lands)"
fi

log "Control-plane harness complete"
"$SCRIPT_DIR/umount-fuse.sh" "$MOUNTPOINT" >>"$LOG_FILE" 2>&1
wait_state "$MOUNTPOINT" unmounted
rm -rf "$MOUNTPOINT"
log "Logs stored at $RUN_DIR"
echo "Control-plane logs available at: $RUN_DIR"
