#!/usr/bin/env bash
# Copyright 2025 Schelling Point Labs Inc
# SPDX-License-Identifier: AGPL-3.0-only

mountpoint="$1"
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

echo "Mounting AgentFS FUSE filesystem at $mountpoint..."
echo "Note: This will run in the background. To unmount later: fusermount -u $mountpoint"
echo ""
./target/debug/agentfs-fuse-host "$mountpoint" &
echo "AgentFS FUSE filesystem mounted. PID: $!"
