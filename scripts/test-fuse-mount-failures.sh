#!/usr/bin/env bash
# Copyright 2025 Schelling Point Labs Inc
# SPDX-License-Identifier: AGPL-3.0-only

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
LOG_ROOT="$REPO_ROOT/logs"
TIMESTAMP="$(date +%Y%m%d-%H%M%S)"
RUN_DIR="$LOG_ROOT/fuse-mount-failures-$TIMESTAMP"
LOG_FILE="$RUN_DIR/failure.log"

mkdir -p "$RUN_DIR"

log() {
  echo "[$(date +%H:%M:%S)] $*" | tee -a "$LOG_FILE"
}

record_case() {
  local name="$1"
  local description="$2"
  log "--- Case: $name ---"
  log "$description"
}

verify_failure() {
  local scenario="$1"
  local cmd="$2"
  local cleanup="$3"

  record_case "$scenario" "$cmd"
  set +e
  bash -c "$cmd" >>"$LOG_FILE" 2>&1
  local status=$?
  set -e
  if [[ $status -eq 0 ]]; then
    log "ERROR: scenario '$scenario' succeeded but should have failed."
    [[ -n "$cleanup" ]] && bash -c "$cleanup" >>"$LOG_FILE" 2>&1 || true
    return 1
  fi
  log "Scenario '$scenario' failed as expected."
  [[ -n "$cleanup" ]] && bash -c "$cleanup" >>"$LOG_FILE" 2>&1 || true
  return 0
}

log "Starting mount failure scenarios"

# Scenario A: mount point path already exists as a regular file
non_dir_path="$(mktemp /tmp/agentfs-failure-file.XXXXXX)"
verify_failure \
  "non-directory mount point" \
  "AGENTFS_FUSE_ALLOW_OTHER=1 '$SCRIPT_DIR/mount-fuse.sh' '$non_dir_path'" \
  "rm -f '$non_dir_path'"

# Scenario B: mount point located under /root (permission denied)
protected_path="/root/agentfs-failure-$TIMESTAMP"
verify_failure \
  "permission denied mount point" \
  "AGENTFS_FUSE_ALLOW_OTHER=1 '$SCRIPT_DIR/mount-fuse.sh' '$protected_path'" \
  ""

log "Mount failure scenarios complete. Logs: $RUN_DIR"
echo "Mount failure logs available at: $RUN_DIR"
