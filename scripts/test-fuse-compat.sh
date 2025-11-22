#!/usr/bin/env bash
# Copyright 2025 Schelling Point Labs Inc
# SPDX-License-Identifier: AGPL-3.0-only

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
LOG_ROOT="$REPO_ROOT/logs"
TIMESTAMP="$(date +%Y%m%d-%H%M%S)"
RUN_DIR="$LOG_ROOT/fuse-compat-$TIMESTAMP"
LOG_FILE="$RUN_DIR/compat.log"
SUMMARY_FILE="$RUN_DIR/summary.json"
SKIP_BUILD="${SKIP_FUSE_BUILD:-}"

mkdir -p "$RUN_DIR"

log() {
  echo "[$(date +%H:%M:%S)] $*" | tee -a "$LOG_FILE"
}

if [[ -z "$SKIP_BUILD" ]]; then
  log "Building agentfs-fuse-host (with fuse feature)..."
  (
    cd "$REPO_ROOT"
    cargo build -p agentfs-fuse-host --features fuse
  ) >>"$LOG_FILE" 2>&1
fi

helpers=()
if command -v fusermount >/dev/null 2>&1; then
  helpers+=("fusermount")
fi
if command -v fusermount3 >/dev/null 2>&1; then
  helpers+=("fusermount3")
fi

if [[ ${#helpers[@]} -eq 0 ]]; then
  log "No fusermount helpers found; marking run as failed"
  printf '[{"name":"helpers","status":"failed","detail":"fusermount/fusermount3 unavailable"}]' >"$SUMMARY_FILE"
  exit 1
fi

kernel="$(uname -a)"
log "Kernel: $kernel"
for helper in "${helpers[@]}"; do
  log "Helper $helper version: $($helper -V)"
done

results=()
run_case() {
  local helper="$1"
  local mount_path
  mount_path="$(mktemp -d /tmp/agentfs-compat.XXXXXX)"
  log "Mounting with helper candidate $helper at $mount_path"
  export AGENTFS_FUSE_ALLOW_OTHER=1
  export AGENTFS_FUSE_LOG_FILE="$RUN_DIR/${helper}.fuse-host.log"

  if ! "$SCRIPT_DIR/mount-fuse.sh" "$mount_path" >>"$LOG_FILE" 2>&1; then
    log "Mount failed via $helper"
    results+=("{\"name\":\"$helper\",\"status\":\"failed\",\"detail\":\"mount failed\"}")
    rm -rf "$mount_path"
    return
  fi
  if ! mountpoint -q "$mount_path" 2>/dev/null; then
    log "Mount did not materialize for $helper"
    results+=("{\"name\":\"$helper\",\"status\":\"failed\",\"detail\":\"not mounted\"}")
    rm -rf "$mount_path"
    return
  fi

  log "Unmounting with $helper -u $mount_path"
  if "$helper" -u "$mount_path" >>"$LOG_FILE" 2>&1; then
    results+=("{\"name\":\"$helper\",\"status\":\"passed\",\"detail\":\"mount+unmount succeeded\"}")
  else
    log "Unmount failed via $helper"
    results+=("{\"name\":\"$helper\",\"status\":\"failed\",\"detail\":\"unmount failed\"}")
  fi
  rm -rf "$mount_path"
}

for helper in "${helpers[@]}"; do
  run_case "$helper"
done

{
  printf '[\n'
  for i in "${!results[@]}"; do
    printf "  %s" "${results[$i]}"
    if [[ $i -lt $((${#results[@]} - 1)) ]]; then
      printf ',\n'
    else
      printf '\n'
    fi
  done
  printf ']\n'
} >"$SUMMARY_FILE"

log "Compatibility harness complete. Summary: $SUMMARY_FILE"
echo "FUSE compatibility logs available at: $RUN_DIR"
