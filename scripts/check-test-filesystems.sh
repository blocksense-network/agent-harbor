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
  ls -la "$CACHE_DIR" | grep -E "(zfs_backing|btrfs_backing|apfs_backing|\.img$|\.sparseimage$)" || echo "   No backing files found"
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

# Check APFS status (macOS only)
echo "APFS Status:"
if [[ "$(uname -s)" == "Darwin" ]]; then
  echo "  ✅ Running on macOS"

  if [ -f "$APFS_FILE" ]; then
    echo "  ✅ APFS backing file exists ($(du -h "$APFS_FILE" | cut -f1))"
  else
    echo "  ❌ APFS backing file missing"
  fi

  # Check if volume is attached
  if hdiutil info | grep -q "$APFS_VOLNAME"; then
    echo "  ✅ APFS volume '$APFS_VOLNAME' is attached"

    # Check if mounted
    if [ -d "/Volumes/$APFS_VOLNAME" ]; then
      echo "  ✅ APFS volume mounted at /Volumes/$APFS_VOLNAME"

      # Check filesystem type
      if command -v diskutil >/dev/null 2>&1; then
        FS_TYPE=$(diskutil info "/Volumes/$APFS_VOLNAME" 2>/dev/null | grep "File System Personality:" | sed 's/.*: //' | tr -d '\n' | xargs)
        if [[ "$FS_TYPE" == "APFS" ]]; then
          echo "  ✅ Filesystem type: $FS_TYPE"
        else
          echo "  ⚠️  Unexpected filesystem type: $FS_TYPE"
        fi
      fi

      # Check if writable
      if [ -w "/Volumes/$APFS_VOLNAME" ]; then
        echo "  ✅ Mount point is writable"
      else
        echo "  ❌ Mount point is not writable"
      fi
    else
      echo "  ❌ APFS volume not mounted at expected location /Volumes/$APFS_VOLNAME"
    fi
  else
    echo "  ❌ APFS volume '$APFS_VOLNAME' not attached"
  fi
else
  echo "  ℹ️  APFS check skipped (not on macOS)"
fi

echo ""

# Summary
ZFS_READY=false
BTRFS_READY=false
APFS_READY=false

if command -v zfs >/dev/null 2>&1 && zpool list "$ZFS_POOL" >/dev/null 2>&1 && zfs list "$ZFS_POOL/test_dataset" >/dev/null 2>&1; then
  ZFS_READY=true
fi

if command -v mkfs.btrfs >/dev/null 2>&1 && [ -b "$BTRFS_LOOP" ] && mountpoint -q "$CACHE_DIR/btrfs_mount"; then
  BTRFS_READY=true
fi

if [[ "$(uname -s)" == "Darwin" ]] && [ -f "$APFS_FILE" ] && hdiutil info | grep -q "$APFS_VOLNAME" && [ -d "/Volumes/$APFS_VOLNAME" ] && [ -w "/Volumes/$APFS_VOLNAME" ]; then
  APFS_READY=true
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

if [[ "$(uname -s)" == "Darwin" ]]; then
  if $APFS_READY; then
    echo "  ✅ APFS test filesystem ready"
  else
    echo "  ❌ APFS test filesystem not ready"
  fi
fi

if $ZFS_READY || $BTRFS_READY || $APFS_READY; then
  echo ""
  echo "Test filesystems are ready! You can run ZFS/Btrfs/APFS provider tests."
  exit 0
else
  echo ""
  echo "No test filesystems are ready."
  echo "Run 'just create-test-filesystems' to set up test filesystems."
  exit 1
fi
