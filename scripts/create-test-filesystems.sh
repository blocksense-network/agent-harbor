#!/usr/bin/env bash
# Copyright 2025 Schelling Point Labs Inc
# SPDX-License-Identifier: AGPL-3.0-only

set -euo pipefail

GID=$(id -g "$USER")

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/test-filesystems-config.sh"

echo "Creating reusable test filesystems in $CACHE_DIR"
echo "These filesystems will be shared across git worktrees and persist between test runs."
echo ""

# Enable unprivileged user namespaces for Btrfs operations (Linux only)
if [[ "$(uname -s)" == "Linux" ]]; then
  echo "Enabling unprivileged user namespaces for Btrfs operations..."
  sudo sysctl -w kernel.unprivileged_userns_clone=1 >/dev/null 2>&1 || echo "Warning: Could not enable unprivileged user namespaces"
else
  echo "Skipping Linux-specific kernel configuration on $(uname -s)"
fi

# Create cache directory
mkdir -p "$CACHE_DIR"

# Check if ZFS is available
if command -v zfs >/dev/null 2>&1; then
  echo "Setting up ZFS test filesystem..."

  # Create backing file if it doesn't exist
  if [ ! -f "$ZFS_FILE" ]; then
    echo "Creating 2GB ZFS backing file..."
    sudo truncate -s 2G "$ZFS_FILE"
    sudo chown "$USER:$GID" "$ZFS_FILE"
  else
    echo "ZFS backing file already exists."
  fi

  # Create pool if it doesn't exist
  if ! zpool list "$ZFS_POOL" >/dev/null 2>&1; then
    echo "Creating ZFS pool (requires sudo)..."
    sudo zpool create "$ZFS_POOL" "$ZFS_FILE"
    sudo zfs create "$ZFS_POOL"/test_dataset
    sudo zfs allow "$USER" snapshot,create,destroy,mount,mountpoint "$ZFS_POOL"/test_dataset

    # Set ownership on the mounted dataset (different paths on macOS vs Linux)
    if [[ "$(uname -s)" == "Darwin" ]]; then
      # On macOS, ZFS mounts under /Volumes/
      MOUNTPOINT="/Volumes/$ZFS_POOL/test_dataset"
      echo "Setting ownership on macOS mount point $MOUNTPOINT..."
      sudo chown -R "$USER:$GID" "$MOUNTPOINT"
    else
      # On Linux, ZFS mounts under /pool_name/
      sudo chown -R "$USER:$GID" "/$ZFS_POOL/test_dataset"
    fi
    echo "ZFS pool created and permissions delegated to $USER."
  else
    echo "ZFS pool $ZFS_POOL already exists."
    # Update permissions if needed
    if ! zfs allow "$ZFS_POOL"/test_dataset | grep -q "user $USER mount"; then
      echo "Updating ZFS permissions..."
      sudo zfs allow "$USER" snapshot,create,destroy,mount,mountpoint "$ZFS_POOL"/test_dataset

      # Set ownership on the mounted dataset (different paths on macOS vs Linux)
      if [[ "$(uname -s)" == "Darwin" ]]; then
        MOUNTPOINT="/Volumes/$ZFS_POOL/test_dataset"
        echo "Setting ownership on macOS mount point $MOUNTPOINT..."
        sudo chown -R "$USER:$GID" "$MOUNTPOINT"
      else
        sudo chown -R "$USER:$GID" "/$ZFS_POOL/test_dataset"
      fi
    fi
  fi
else
  echo "ZFS not available, skipping ZFS setup."
fi

# Check if Btrfs is available (skip on macOS)
if [[ "$(uname -s)" == "Darwin" ]]; then
  echo "Btrfs not supported on macOS, skipping Btrfs setup."
elif command -v mkfs.btrfs >/dev/null 2>&1; then
  echo "Setting up Btrfs test filesystem..."

  # Create backing file if it doesn't exist
  if [ ! -f "$BTRFS_FILE" ]; then
    echo "Creating 2GB Btrfs backing file..."
    truncate -s 2G "$BTRFS_FILE"
  else
    echo "Btrfs backing file already exists."
  fi

  # Check if already mounted with correct options
  if mount | grep -q "$BTRFS_LOOP"; then
    if mount | grep -q "$BTRFS_LOOP.*user_subvol_rm_allowed"; then
      echo "Btrfs filesystem already mounted with correct options."
    else
      echo "Remounting Btrfs filesystem with correct options..."
      sudo mount -o remount,user_subvol_rm_allowed "$BTRFS_LOOP" "$CACHE_DIR/btrfs_mount"
      echo "Btrfs filesystem remounted with correct options."
    fi
  else
    echo "Setting up Btrfs loop device and mounting (requires sudo)..."

    # Set up loop device
    sudo losetup "$BTRFS_LOOP" "$BTRFS_FILE" 2>/dev/null || true

    # Format if not already formatted
    if ! sudo blkid "$BTRFS_LOOP" | grep -q btrfs; then
      sudo mkfs.btrfs -f "$BTRFS_LOOP"
    fi

    # Create mount point and mount with user subvolume deletion allowed
    sudo mkdir -p "$CACHE_DIR/btrfs_mount"
    sudo mount -o user_subvol_rm_allowed "$BTRFS_LOOP" "$CACHE_DIR/btrfs_mount"

    # Create subvolume if it doesn't exist
    if [ ! -d "$CACHE_DIR/btrfs_mount/test_subvol" ]; then
      sudo btrfs subvolume create "$CACHE_DIR/btrfs_mount/test_subvol"
    fi

    # Change ownership to user for delegation
    sudo chown -R "$USER:$GID" "$CACHE_DIR/btrfs_mount/test_subvol"

    echo "Btrfs filesystem created and mounted."
  fi
else
  echo "Btrfs not available, skipping Btrfs setup."
fi

echo ""
echo "Test filesystems setup complete!"
echo "You can now run tests that use ZFS and Btrfs providers."
echo ""
echo "To clean up later, run:"
echo "  just cleanup-test-filesystems"
