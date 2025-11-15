#!/usr/bin/env bash
# Copyright 2025 Schelling Point Labs Inc
# SPDX-License-Identifier: AGPL-3.0-only

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
LOG_ROOT="$REPO_ROOT/logs"
TIMESTAMP="$(date +%Y%m%d-%H%M%S)"
RUN_DIR="$LOG_ROOT/fuse-mount-concurrent-$TIMESTAMP"
LOG_FILE="$RUN_DIR/concurrent.log"
COUNT="${FUSE_CONCURRENT_MOUNTS:-2}"
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

if [[ -z "$SKIP_BUILD" ]]; then
  log "Building agentfs-fuse-host (with fuse feature)..."
  (
    cd "$REPO_ROOT"
    cargo build -p agentfs-fuse-host --features fuse
  ) >>"$LOG_FILE" 2>&1
fi

existing_pids="$(agentfs_pids)"
if [[ -n "$existing_pids" ]]; then
  log "ERROR: found existing agentfs-fuse-host processes (PIDs: $existing_pids). Abort to avoid interference."
  exit 1
fi

log "Starting concurrent mount test (count=$COUNT)"

mount_points=()
cleanup() {
  for mount_path in "${mount_points[@]}"; do
    if mountpoint -q "$mount_path" 2>/dev/null; then
      log "Cleaning up mounted path $mount_path"
      "$SCRIPT_DIR/umount-fuse.sh" "$mount_path" >>"$LOG_FILE" 2>&1 || true
    fi
    rm -rf "$mount_path"
  done
}
trap cleanup EXIT

for ((i = 1; i <= COUNT; i++)); do
  mount_path="$(mktemp -d /tmp/agentfs-concurrent.XXXXXX)"
  mount_points+=("$mount_path")
  log "Mounting instance $i at $mount_path"
  AGENTFS_FUSE_ALLOW_OTHER=1 "$SCRIPT_DIR/mount-fuse.sh" "$mount_path" >>"$LOG_FILE" 2>&1
  wait_for_mount_state "$mount_path" "mounted"
  log "Instance $i mounted"
  test_file="$mount_path/.agentfs-concurrent"
  sudo sh -c "echo concurrent-$i > '$test_file'"
  sudo cat "$test_file" >/dev/null
  sudo rm -f "$test_file"
  log "Instance $i sanity checks complete"
done

log "All instances mounted; verifying simultaneous access"
for mount_path in "${mount_points[@]}"; do
  test_file="$mount_path/.agentfs-concurrent-verify"
  sudo sh -c "echo verify > '$test_file'"
  sudo rm -f "$test_file"
  wait_for_mount_state "$mount_path" "mounted"
  log "Verified $mount_path still mounted"
done

for mount_path in "${mount_points[@]}"; do
  log "Unmounting $mount_path"
  "$SCRIPT_DIR/umount-fuse.sh" "$mount_path" >>"$LOG_FILE" 2>&1
  wait_for_mount_state "$mount_path" "unmounted"
  rm -rf "$mount_path"
  log "Unmounted $mount_path"
done

ensure_no_agentfs_processes

log "Concurrent mount test complete. Logs: $RUN_DIR"
echo "Concurrent mount logs available at: $RUN_DIR"
