#!/usr/bin/env bash
# Copyright 2025 Schelling Point Labs Inc
# SPDX-License-Identifier: AGPL-3.0-only

set -euo pipefail

if [[ $# -lt 1 ]]; then
  echo "Usage: $0 <mountpoint>" >&2
  exit 1
fi

mountpoint="$1"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
PJDFSTEST_DIR="$REPO_ROOT/resources/pjdfstest"

if [[ ! -d "$mountpoint" ]]; then
  echo "Error: mount point $mountpoint does not exist" >&2
  exit 1
fi

if ! mountpoint -q "$mountpoint"; then
  echo "Error: $mountpoint is not a mount point; run 'just mount-fuse $mountpoint' first" >&2
  exit 1
fi

# Ensure root can access the mount (requires mounting with --allow-other)
if ! ls "$mountpoint" >/dev/null 2>&1; then
  echo "Error: Unable to access $mountpoint as root. Mount with 'AGENTFS_FUSE_ALLOW_OTHER=1 just mount-fuse $mountpoint'" >&2
  exit 1
fi

if [[ $(id -u) -ne 0 ]]; then
  echo "Error: pjdfstest subset must run as root (tests require mknod/chown)." >&2
  exit 1
fi

if [[ ! -d "$PJDFSTEST_DIR/tests" ]]; then
  echo "Error: pjdfstest suite not found; run 'just setup-pjdfstest-suite'" >&2
  exit 1
fi

subset_env=${PJDFSTEST_SUBSET:-"unlink rename mkdir rmdir"}
read -r -a subset <<<"$subset_env"
if [[ ${#subset[@]} -eq 0 ]]; then
  echo "Error: PJDFSTEST_SUBSET resolved to an empty list" >&2
  exit 1
fi

log_root="$REPO_ROOT/logs"
ts=$(date +%Y%m%d-%H%M%S)
run_dir="$log_root/pjdfs-subset-$ts"
mkdir -p "$run_dir"

echo "Running pjdfstest subset (${subset[*]}) against $mountpoint"
echo "Logs: $run_dir"

for suite in "${subset[@]}"; do
  log_file="$run_dir/${suite}.log"
  echo "\n===> pjdfstest $suite" | tee "$log_file"
  if ! (cd "$mountpoint" && prove -rv "$PJDFSTEST_DIR/tests/$suite") | tee -a "$log_file"; then
    echo "Command failed; logs captured in $log_file" >&2
    exit 1
  fi
  echo "---" >>"$log_file"
  echo "Suite $suite complete" | tee -a "$log_file"
  echo "Log saved to $log_file"
done

echo "pjdfstest subset completed successfully"
echo "Log directory: $run_dir"
exit 0
