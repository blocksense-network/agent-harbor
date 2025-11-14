#!/usr/bin/env bash
# Copyright 2025 Schelling Point Labs Inc
# SPDX-License-Identifier: AGPL-3.0-only

mountpoint="$1"

is_mounted() {
  mountpoint -q "$mountpoint" 2>/dev/null
}

if is_mounted; then
  echo "Existing AgentFS mount detected at $mountpoint; attempting to unmount first..."
  if command -v fusermount >/dev/null 2>&1; then
    if ! fusermount -u "$mountpoint" 2>/dev/null; then
      echo "Error: Unable to auto-unmount $mountpoint. Run 'just umount-fuse $mountpoint' and retry."
      exit 1
    fi
  else
    echo "Error: $mountpoint is already mounted and fusermount is unavailable. Unmount manually and retry."
    exit 1
  fi
fi

if [ ! -d "$mountpoint" ]; then
  echo "Creating mount point: $mountpoint"
  if ! mkdir -p "$mountpoint"; then
    echo "Error: Failed to create $mountpoint (insufficient permissions?)"
    exit 1
  fi
fi

# Ensure the mount point is owned by the current user
if ! sudo chown $(whoami) "$mountpoint"; then
  echo "Error: Unable to chown $mountpoint; aborting mount"
  exit 1
fi

FUSE_FLAGS=()
if [ "${AGENTFS_FUSE_ALLOW_OTHER:-}" = "1" ]; then
  FUSE_FLAGS+=("--allow-other")
fi

echo "Mounting AgentFS FUSE filesystem at $mountpoint..."
if [ ${#FUSE_FLAGS[@]} -gt 0 ]; then
  echo "Additional FUSE flags: ${FUSE_FLAGS[*]}"
fi
echo "Note: This will run in the background. To unmount later: fusermount -u $mountpoint"
echo ""
./target/debug/agentfs-fuse-host "${FUSE_FLAGS[@]}" "$mountpoint" &
echo "AgentFS FUSE filesystem mounted. PID: $!"
