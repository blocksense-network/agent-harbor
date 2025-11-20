#!/usr/bin/env bash
# Copyright 2025 Schelling Point Labs Inc
# SPDX-License-Identifier: AGPL-3.0-only

mountpoint="$1"
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

# Ensure the current user can write inside the mountpoint. Some runners mount the
# filesystem as root even when allow_other is set, so fix ownership if sudo exists.
if ! test -w "$mountpoint"; then
  if command -v sudo >/dev/null 2>&1; then
    sudo chown "$(id -u)":"$(id -g)" "$mountpoint" 2>/dev/null || true
  fi
fi

echo "Running basic filesystem smoke tests against $mountpoint..."
# Basic smoke tests for FUSE filesystem functionality
cd "$mountpoint" &&
  echo "Testing basic operations..." &&
  echo "test content" >test_file.txt &&
  cat test_file.txt >/dev/null &&
  mkdir test_dir &&
  ls -la >/dev/null &&
  rm test_file.txt &&
  rmdir test_dir &&
  echo "Basic filesystem operations completed successfully"
