#!/usr/bin/env bash
# Copyright 2025 Schelling Point Labs Inc
# SPDX-License-Identifier: AGPL-3.0-only

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
LOG_ROOT="$REPO_ROOT/logs"
TIMESTAMP="$(date +%Y%m%d-%H%M%S)"
RUN_DIR="$LOG_ROOT/fuse-basic-ops-$TIMESTAMP"
LOG_FILE="$RUN_DIR/basic-ops.log"
MOUNTPOINT="${1:-/tmp/agentfs-basic-ops}"
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
  local pids="$(agentfs_pids)"
  if [[ -n "$pids" ]]; then
    log "ERROR: agentfs-fuse-host still running (PIDs: $pids)"
    exit 1
  fi
}

cleanup_mount() {
  if mountpoint -q "$MOUNTPOINT" 2>/dev/null; then
    log "Cleaning up existing mount at $MOUNTPOINT"
    "$SCRIPT_DIR/umount-fuse.sh" "$MOUNTPOINT" >>"$LOG_FILE" 2>&1 || true
  fi
  rm -rf "$MOUNTPOINT"
}

trap cleanup_mount EXIT

if [[ -z "$SKIP_BUILD" ]]; then
  log "Building agentfs-fuse-host (with fuse feature)..."
  (
    cd "$REPO_ROOT"
    cargo build -p agentfs-fuse-host --features fuse
  ) >>"$LOG_FILE" 2>&1
fi

ensure_no_agentfs_processes
cleanup_mount
mkdir -p "$MOUNTPOINT"

log "Mounting test filesystem at $MOUNTPOINT"
AGENTFS_FUSE_ALLOW_OTHER=1 "$SCRIPT_DIR/mount-fuse.sh" "$MOUNTPOINT" >>"$LOG_FILE" 2>&1
wait_for_mount_state "$MOUNTPOINT" "mounted"

run_user_ops() {
  local desc="$1"
  local cmd="$2"
  log "[user] $desc"
  bash -c "$cmd" >>"$LOG_FILE" 2>&1
}

run_root_ops() {
  local desc="$1"
  local cmd="$2"
  log "[root] $desc"
  sudo bash -c "$cmd" >>"$LOG_FILE" 2>&1
}

run_root_ops "prepare test directory" "mkdir -p $MOUNTPOINT/testuser && chown $(id -u):$(id -g) $MOUNTPOINT/testuser"
TESTDIR="$MOUNTPOINT/testuser"

log "== File CRUD operations =="
run_user_ops "create file" "echo 'hello world' > $TESTDIR/file.txt"
run_user_ops "append file" "echo 'another line' >> $TESTDIR/file.txt"
run_user_ops "verify content" "grep 'hello world' $TESTDIR/file.txt"
run_user_ops "delete file" "rm $TESTDIR/file.txt"

log "== Directory operations =="
run_user_ops "mkdir tree" "mkdir -p $TESTDIR/dir/subdir"
run_user_ops "create file in subdir" "echo 'nested' > $TESTDIR/dir/subdir/nested.txt"
run_user_ops "list dir" "ls -l $TESTDIR/dir/subdir"
run_user_ops "remove file and dirs" "rm $TESTDIR/dir/subdir/nested.txt && rmdir $TESTDIR/dir/subdir && rmdir $TESTDIR/dir"

log "== Metadata operations =="
run_user_ops "create metadata target" "touch $TESTDIR/meta.txt"
run_root_ops "chmod" "chmod 644 $TESTDIR/meta.txt"
run_root_ops "chown" "chown root:root $TESTDIR/meta.txt"
run_root_ops "utimens" "touch -t 202001010101 $TESTDIR/meta.txt"
run_root_ops "stat" "stat $TESTDIR/meta.txt"
run_user_ops "cleanup metadata target" "rm $TESTDIR/meta.txt"

log "== Symlink operations =="
run_user_ops "create file" "echo 'link target' > $TESTDIR/target.txt"
run_user_ops "create symlink" "ln -s target.txt $TESTDIR/link.txt"
run_user_ops "verify symlink" "[[ \"$(readlink $TESTDIR/link.txt)\" == "target.txt" ]]"
run_user_ops "remove symlink" "rm $TESTDIR/link.txt && rm $TESTDIR/target.txt"

log "== Large file handling =="
run_user_ops "create large file" "dd if=/dev/zero of=$TESTDIR/large.bin bs=1M count=8 status=none"
run_user_ops "verify size" "stat -c '%s' $TESTDIR/large.bin"
run_user_ops "cleanup large file" "rm $TESTDIR/large.bin"

log "Unmounting"
"$SCRIPT_DIR/umount-fuse.sh" "$MOUNTPOINT" >>"$LOG_FILE" 2>&1
wait_for_mount_state "$MOUNTPOINT" "unmounted"
rm -rf "$MOUNTPOINT"
ensure_no_agentfs_processes

log "Basic filesystem operations test complete. Logs: $RUN_DIR"
echo "Basic ops logs available at: $RUN_DIR"
