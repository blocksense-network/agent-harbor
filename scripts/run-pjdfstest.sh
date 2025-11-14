#!/usr/bin/env bash
# Copyright 2025 Schelling Point Labs Inc
# SPDX-License-Identifier: AGPL-3.0-only

mountpoint="$1"

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
PJDFSTEST_DIR="$REPO_ROOT/resources/pjdfstest"
if [ ! -d "$mountpoint" ]; then
  echo "Error: Mount point $mountpoint does not exist"
  echo "Hint: Mount the filesystem first with: just mount-fuse $mountpoint"
  exit 1
fi
if ! mountpoint -q "$mountpoint"; then
  echo "Error: $mountpoint is not a mount point"
  echo "Hint: Mount the filesystem first with: just mount-fuse $mountpoint"
  exit 1
fi
if [ ! -d "$PJDFSTEST_DIR" ]; then
  echo "Error: pjdfstest suite not set up. Run 'just setup-pjdfstest-suite' first"
  exit 1
fi
echo "Running pjdfstest suite against $mountpoint..."
echo "Note: This requires root privileges and may take a long time"
echo "Press Ctrl+C to interrupt the test suite"
cd "$mountpoint" && prove -rv "$PJDFSTEST_DIR/tests"
