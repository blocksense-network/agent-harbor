#!/usr/bin/env bash
# Copyright 2025 Schelling Point Labs Inc
# SPDX-License-Identifier: AGPL-3.0-only

set -euo pipefail

# Source shared configuration
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/test-filesystems-config.sh"

echo "Checking test filesystems status..."
echo "Cache directory: $CACHE_DIR"
echo ""

# Check if cache directory exists
if [ -d "$CACHE_DIR" ]; then
  echo "✅ Cache directory exists"
  ls -la "$CACHE_DIR" | grep -E "(zfs_backing|btrfs_backing|\.img$)" || echo "   No backing files found"
else
  echo "❌ Cache directory does not exist"
  echo "   Run 'just create-test-filesystems' to set up test filesystems"
  exit 1
fi

echo ""

# Check ZFS status
echo "ZFS Status:"
if command -v zfs >/dev/null 2>&1; then
  echo "  ✅ ZFS tools available"

  if [ -f "$ZFS_FILE" ]; then
    echo "  ✅ ZFS backing file exists ($(du -h "$ZFS_FILE" | cut -f1))"
  else
    echo "  ❌ ZFS backing file missing"
  fi

  if zpool list "$ZFS_POOL" >/dev/null 2>&1; then
    echo "  ✅ ZFS pool '$ZFS_POOL' exists"

    # Check if dataset exists
    if zfs list "$ZFS_POOL/test_dataset" >/dev/null 2>&1; then
      echo "  ✅ ZFS dataset '$ZFS_POOL/test_dataset' exists"

      # Check mountpoint
      MOUNTPOINT=$(zfs get -H -o value mountpoint "$ZFS_POOL/test_dataset")
      if [ -d "$MOUNTPOINT" ]; then
        echo "  ✅ Dataset mounted at $MOUNTPOINT"
      else
        echo "  ❌ Dataset not mounted (expected at $MOUNTPOINT)"
      fi
    else
      echo "  ❌ ZFS dataset '$ZFS_POOL/test_dataset' missing"
    fi
  else
    echo "  ❌ ZFS pool '$ZFS_POOL' does not exist"
  fi
else
  echo "  ❌ ZFS tools not available"
fi

echo ""

# Check Btrfs status
echo "Btrfs Status:"
if command -v mkfs.btrfs >/dev/null 2>&1; then
  echo "  ✅ Btrfs tools available"

  if [ -f "$BTRFS_FILE" ]; then
    echo "  ✅ Btrfs backing file exists ($(du -h "$BTRFS_FILE" | cut -f1))"
  else
    echo "  ❌ Btrfs backing file missing"
  fi

  if [ -b "$BTRFS_LOOP" ]; then
    echo "  ✅ Btrfs loop device $BTRFS_LOOP exists"

    # Check if mounted by checking if the expected mount point is a mount point
    if mountpoint -q "$CACHE_DIR/btrfs_mount"; then
      echo "  ✅ Btrfs filesystem mounted at $CACHE_DIR/btrfs_mount"

      if [ -d "$CACHE_DIR/btrfs_mount/test_subvol" ]; then
        echo "  ✅ Test subvolume exists at $CACHE_DIR/btrfs_mount/test_subvol"
      else
        echo "  ❌ Test subvolume missing"
      fi
    else
      echo "  ❌ Btrfs filesystem not mounted"
    fi
  else
    echo "  ❌ Btrfs loop device $BTRFS_LOOP does not exist"
  fi
else
  echo "  ❌ Btrfs tools not available"
fi

echo ""

# Summary
ZFS_READY=false
BTRFS_READY=false

if command -v zfs >/dev/null 2>&1 && zpool list "$ZFS_POOL" >/dev/null 2>&1 && zfs list "$ZFS_POOL/test_dataset" >/dev/null 2>&1; then
  ZFS_READY=true
fi

if command -v mkfs.btrfs >/dev/null 2>&1 && [ -b "$BTRFS_LOOP" ] && mountpoint -q "$CACHE_DIR/btrfs_mount"; then
  BTRFS_READY=true
fi

echo "Summary:"
if $ZFS_READY; then
  echo "  ✅ ZFS test filesystem ready"
else
  echo "  ❌ ZFS test filesystem not ready"
fi

if $BTRFS_READY; then
  echo "  ✅ Btrfs test filesystem ready"
else
  echo "  ❌ Btrfs test filesystem not ready"
fi

if $ZFS_READY || $BTRFS_READY; then
  echo ""
  echo "Test filesystems are ready! You can run ZFS/Btrfs provider tests."
  exit 0
else
  echo ""
  echo "No test filesystems are ready."
  echo "Run 'just create-test-filesystems' to set up test filesystems."
  exit 1
fi
