#!/usr/bin/env bash
# Copyright 2025 Schelling Point Labs Inc
# SPDX-License-Identifier: AGPL-3.0-only

mountpoint="$1"
if [ ! -d "$mountpoint" ]; then
  echo "Creating mount point: $mountpoint"
  mkdir -p "$mountpoint"
fi

# Ensure the mount point is owned by the current user
sudo chown $(whoami) "$mountpoint" || true

echo "Mounting AgentFS FUSE filesystem at $mountpoint..."
echo "Note: This will run in the background. To unmount later: fusermount -u $mountpoint"
echo ""
./target/debug/agentfs-fuse-host "$mountpoint" &
echo "AgentFS FUSE filesystem mounted. PID: $!"
