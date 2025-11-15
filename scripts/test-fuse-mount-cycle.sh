#!/usr/bin/env bash
# Copyright 2025 Schelling Point Labs Inc
# SPDX-License-Identifier: AGPL-3.0-only

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
LOG_ROOT="$REPO_ROOT/logs"
TIMESTAMP="$(date +%Y%m%d-%H%M%S)"
RUN_DIR="$LOG_ROOT/fuse-mount-cycle-$TIMESTAMP"
LOG_FILE="$RUN_DIR/mount-cycle.log"
ITERATIONS="${MOUNT_CYCLE_ITERS:-1}"
REQUESTED_MOUNTPOINT="${1:-}"
SKIP_BUILD="${SKIP_FUSE_BUILD:-}"

mkdir -p "$RUN_DIR"

log() {
  echo "[$(date +%H:%M:%S)] $*" | tee -a "$LOG_FILE"
}

wait_for_mount_state() {
  local mount_path="$1"
  local expect_mounted="$2"
  local max_attempts=50
  local attempt=0
  while ((attempt < max_attempts)); do
    if mountpoint -q "$mount_path" 2>/dev/null; then
      if [[ "$expect_mounted" == "mounted" ]]; then
        return 0
      fi
    else
      if [[ "$expect_mounted" == "unmounted" ]]; then
        return 0
      fi
    fi
    sleep 0.1
    ((attempt += 1))
  done
  log "Timed out waiting for $mount_path to become $expect_mounted"
  return 1
}

if [[ -z "$SKIP_BUILD" ]]; then
  log "Building agentfs-fuse-host (with fuse feature)..."
  (
    cd "$REPO_ROOT"
    cargo build -p agentfs-fuse-host --features fuse
  ) >>"$LOG_FILE" 2>&1
fi

agentfs_pids() {
  pgrep -f agentfs-fuse-host || true
}

ensure_no_agentfs_processes() {
  local pids
  pids="$(agentfs_pids)"
  if [[ -n "$pids" ]]; then
    log "ERROR: agentfs-fuse-host still running (PIDs: $pids)"
    exit 1
  fi
}

existing_pids="$(agentfs_pids)"
if [[ -n "$existing_pids" ]]; then
  log "ERROR: Found existing agentfs-fuse-host processes (PIDs: $existing_pids). Abort to avoid interference."
  exit 1
fi

cleanup_mountpoints=()

cleanup() {
  for mount_path in "${cleanup_mountpoints[@]}"; do
    if mountpoint -q "$mount_path" 2>/dev/null; then
      log "Cleaning up mounted path $mount_path"
      "$SCRIPT_DIR/umount-fuse.sh" "$mount_path" >>"$LOG_FILE" 2>&1 || true
    fi
    rm -rf "$mount_path"
  done
}
trap cleanup EXIT

log "Starting FUSE mount cycle test (iterations=$ITERATIONS)"

for ((iter = 1; iter <= ITERATIONS; iter++)); do
  if [[ -n "$REQUESTED_MOUNTPOINT" ]]; then
    mount_path="$REQUESTED_MOUNTPOINT"
    mkdir -p "$mount_path"
  else
    mount_path="$(mktemp -d /tmp/agentfs-cycle.XXXXXX)"
    cleanup_mountpoints+=("$mount_path")
  fi

  log "--- Iteration $iter/$ITERATIONS using mount point $mount_path ---"

  if mountpoint -q "$mount_path" 2>/dev/null; then
    log "Mount point $mount_path is already mounted. Aborting."
    exit 1
  fi

  AGENTFS_FUSE_ALLOW_OTHER=1 "$SCRIPT_DIR/mount-fuse.sh" "$mount_path" >>"$LOG_FILE" 2>&1
  wait_for_mount_state "$mount_path" "mounted"

  test_file="$mount_path/.agentfs-mount-cycle"
  log "Verifying filesystem operations inside $mount_path"
  sudo sh -c "echo mount-cycle-iter-$iter > '$test_file'"
  sudo cat "$test_file" >/dev/null
  sudo rm -f "$test_file"

  "$SCRIPT_DIR/umount-fuse.sh" "$mount_path" >>"$LOG_FILE" 2>&1
  wait_for_mount_state "$mount_path" "unmounted"

  if [[ -z "$REQUESTED_MOUNTPOINT" ]]; then
    rm -rf "$mount_path"
  fi
  ensure_no_agentfs_processes
done

for mount_path in "${cleanup_mountpoints[@]}"; do
  if [[ -e "$mount_path" ]]; then
    log "ERROR: temporary mount path $mount_path still exists after cleanup."
    exit 1
  fi
done

ensure_no_agentfs_processes

log "Mount cycle test complete. Logs: $RUN_DIR"
echo "Mount cycle logs available at: $RUN_DIR"
