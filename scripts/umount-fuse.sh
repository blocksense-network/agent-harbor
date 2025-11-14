#!/usr/bin/env bash
# Copyright 2025 Schelling Point Labs Inc
# SPDX-License-Identifier: AGPL-3.0-only

mountpoint="$1"

is_mounted() {
  mount | grep -F " on $mountpoint " >/dev/null 2>&1
}

if [ ! -d "$mountpoint" ]; then
  if is_mounted; then
    echo "⚠️  Mount point $mountpoint is unreachable but still mounted; attempting forced unmount..."
  else
    echo "Error: Mount point $mountpoint does not exist"
    echo "Hint: Mount the filesystem first with: just mount-fuse $mountpoint"
    exit 1
  fi
fi

echo "🔌 Unmounting FUSE filesystem from $mountpoint..."

kill_fuse_host() {
  if pgrep -f agentfs-fuse-host >/dev/null 2>&1; then
    echo "⚠️  Killing lingering agentfs-fuse-host processes"
    pkill -f agentfs-fuse-host >/dev/null 2>&1 || true
    sleep 1
  fi
}

# Try fusermount first (Linux), then umount
if command -v fusermount >/dev/null 2>&1; then
  if ! fusermount -u "$mountpoint" 2>/dev/null; then
    kill_fuse_host
    fusermount -u "$mountpoint" 2>/dev/null || true
  fi
elif command -v umount >/dev/null 2>&1; then
  umount "$mountpoint" 2>/dev/null || true
else
  echo "⚠️  Neither fusermount nor umount found, manual unmounting may be required"
fi

if is_mounted; then
  echo "⚠️  FUSE filesystem still appears mounted at $mountpoint; manual cleanup may be required"
  exit 1
fi

echo "✅ FUSE filesystem unmounted from $mountpoint"
