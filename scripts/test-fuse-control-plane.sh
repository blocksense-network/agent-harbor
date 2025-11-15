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
MOUNTPOINT="${1:-/tmp/agentfs-control-plane}"
TESTDIR="$MOUNTPOINT/control_plane"
mkdir -p "$RUN_DIR"

log() { echo "[$(date +%H:%M:%S)] $*" | tee -a "$LOG_FILE"; }
cleanup() {
  if mountpoint -q "$MOUNTPOINT" 2>/dev/null; then "$SCRIPT_DIR/umount-fuse.sh" "$MOUNTPOINT" >>"$LOG_FILE" 2>&1 || true; fi
  rm -rf "$MOUNTPOINT"
}
wait_state() {
  local path="$1"
  local target="$2"
  local attempts=0
  while ((attempts < 50)); do
    if mountpoint -q "$path" 2>/dev/null; then [[ "$target" == mounted ]] && return 0; else [[ "$target" == unmounted ]] && return 0; fi
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
  echo "[$(date +%H:%M:%S)] [control] $* => $output" >>"$LOG_FILE"
  printf '%s' "$output"
}

log "Building agentfs-fuse-host and control CLI"
(
  cd "$REPO_ROOT"
  cargo build -p agentfs-fuse-host --features fuse >>"$LOG_FILE" 2>&1
  cargo build -p agentfs-control-cli >>"$LOG_FILE" 2>&1
)

log "Preparing mount point"
rm -rf "$MOUNTPOINT"
mkdir -p "$MOUNTPOINT"

log "Mounting AgentFS"
AGENTFS_FUSE_ALLOW_OTHER=1 "$SCRIPT_DIR/mount-fuse.sh" "$MOUNTPOINT" >>"$LOG_FILE" 2>&1
wait_state "$MOUNTPOINT" mounted

log "Creating workspace"
sudo mkdir -p "$TESTDIR"
sudo chown "$(id -u):$(id -g)" "$TESTDIR"
printf "base-v1" >"$TESTDIR/data.txt"
chmod 666 "$TESTDIR/data.txt"

log "T4.1 – snapshot list (control file reachable)"
run_control snapshot-list >/dev/null || true

log "T4.2 – snapshot create/list"
SNAP_LINE=$(run_control snapshot-create --name clean)
SNAP_ID=${SNAP_LINE#SNAPSHOT_ID=}
SNAP_ID=${SNAP_ID%%$'\t'*}
LIST_OUTPUT=$(run_control snapshot-list)
if ! grep -q "$SNAP_ID" <<<"$LIST_OUTPUT"; then
  log "Snapshot $SNAP_ID missing from list"
  exit 1
fi

log "T4.3 – branch create"
BRANCH_LINE=$(run_control branch-create --snapshot "$SNAP_ID" --name feature)
BRANCH_ID=${BRANCH_LINE#BRANCH_ID=}
BRANCH_ID=${BRANCH_ID%%$'\t'*}

log "T4.4 – branch bind current shell"
BIND_OUTPUT=$(run_control branch-bind --branch "$BRANCH_ID" --pid $$)
if ! grep -q "BRANCH_BIND_OK" <<<"$BIND_OUTPUT"; then
  log "Branch bind failed"
  exit 1
fi

log "Control-plane smoke test complete"
"$SCRIPT_DIR/umount-fuse.sh" "$MOUNTPOINT" >>"$LOG_FILE" 2>&1
wait_state "$MOUNTPOINT" unmounted
rm -rf "$MOUNTPOINT"
log "Logs stored at $RUN_DIR"
echo "Control-plane logs available at: $RUN_DIR"
