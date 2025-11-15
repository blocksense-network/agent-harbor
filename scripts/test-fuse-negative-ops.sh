#!/usr/bin/env bash
# Copyright 2025 Schelling Point Labs Inc
# SPDX-License-Identifier: AGPL-3.0-only

set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
LOG_ROOT="$REPO_ROOT/logs"
TS="$(date +%Y%m%d-%H%M%S)"
RUN_DIR="$LOG_ROOT/fuse-negative-ops-$TS"
LOG_FILE="$RUN_DIR/negative-ops.log"
MOUNTPOINT="${1:-/tmp/agentfs-negative-ops}"
mkdir -p "$RUN_DIR"
log() { echo "[$(date +%H:%M:%S)] $*" | tee -a "$LOG_FILE"; }
wait_state() {
  local path="$1"
  local expect="$2"
  local a=0
  while ((a < 50)); do
    if mountpoint -q "$path" 2>/dev/null; then [[ "$expect" == mounted ]] && return 0; else [[ "$expect" == unmounted ]] && return 0; fi
    sleep 0.1
    ((a += 1))
  done
  log "timeout waiting $path -> $expect"
  return 1
}
cleanup() {
  if mountpoint -q "$MOUNTPOINT" 2>/dev/null; then "$SCRIPT_DIR/umount-fuse.sh" "$MOUNTPOINT" >>"$LOG_FILE" 2>&1 || true; fi
  rm -rf "$MOUNTPOINT"
}
trap cleanup EXIT
log "Building agentfs-fuse-host (with fuse feature) ..."
(
  cd "$REPO_ROOT"
  cargo build -p agentfs-fuse-host --features fuse
) >>"$LOG_FILE" 2>&1
cleanup
mkdir -p "$MOUNTPOINT"
log "Mounting $MOUNTPOINT"
AGENTFS_FUSE_ALLOW_OTHER=1 "$SCRIPT_DIR/mount-fuse.sh" "$MOUNTPOINT" >>"$LOG_FILE" 2>&1
wait_state "$MOUNTPOINT" mounted
run_expect() {
  local desc="$1"
  local cmd="$2"
  local expect_err="$3"
  log "[root] $desc"
  set +e
  output=$(sudo bash -c "$cmd" 2>&1 >/tmp/negative.out.$$)
  status=$?
  cat /tmp/negative.out.$$ >>"$LOG_FILE"
  rm -f /tmp/negative.out.$$
  set -e
  if [[ $status -eq 0 ]]; then
    log "ERROR: $desc succeeded but expected $expect_err"
    exit 1
  fi
  if ! grep -q "$expect_err" <<<"$output"; then
    log "ERROR: $desc output '$output' missing expected $expect_err"
    exit 1
  fi
}
log "Preparing fixtures"
sudo bash -c "touch $MOUNTPOINT/file_exists && mkdir -p $MOUNTPOINT/dir_exists && echo data > $MOUNTPOINT/dir_exists/file.txt"
run_expect "ENOENT open" "cat $MOUNTPOINT/missing-file" "No such file or directory"
run_expect "EEXIST mkdir" "mkdir $MOUNTPOINT/dir_exists" "File exists"
run_expect "ENOTEMPTY rmdir" "rmdir $MOUNTPOINT/dir_exists" "Directory not empty"
run_expect "EISDIR unlink dir" "rm $MOUNTPOINT/dir_exists" "Is a directory"
run_expect "ENOTDIR mkdir under file" "mkdir $MOUNTPOINT/file_exists/subdir" "Not a directory"
run_expect "ENAMETOOLONG create" "touch $MOUNTPOINT/$(
  python - <<'PY'
s=("a"*260)
print(s)
PY
)" "File name too long"
log "Cleaning up"
"$SCRIPT_DIR/umount-fuse.sh" "$MOUNTPOINT" >>"$LOG_FILE" 2>&1
wait_state "$MOUNTPOINT" unmounted
rm -rf "$MOUNTPOINT"
log "Negative errno test complete. Logs: $RUN_DIR"
echo "Negative errno logs available at: $RUN_DIR"
