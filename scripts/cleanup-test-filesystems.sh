#!/usr/bin/env bash
# Copyright 2025 Schelling Point Labs Inc
# SPDX-License-Identifier: AGPL-3.0-only

set -euo pipefail

# Source shared configuration
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/test-filesystems-config.sh"

echo "Cleaning up test filesystems..."

# Clean up ZFS
if command -v zfs >/dev/null 2>&1 && zpool list "$ZFS_POOL" >/dev/null 2>&1; then
  echo "Destroying ZFS pool..."
  sudo zfs destroy -r "$ZFS_POOL" 2>/dev/null || true
  sudo zpool destroy "$ZFS_POOL" 2>/dev/null || true
fi

# Clean up Btrfs
if [ -b "$BTRFS_LOOP" ]; then
  echo "Unmounting and cleaning up Btrfs..."
  sudo umount "$CACHE_DIR/btrfs_mount" 2>/dev/null || true
  sudo losetup -d "$BTRFS_LOOP" 2>/dev/null || true
fi

# Remove files
rm -rf "$CACHE_DIR"

echo "Cleanup complete."
