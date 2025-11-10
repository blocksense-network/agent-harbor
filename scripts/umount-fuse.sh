#!/usr/bin/env bash
mountpoint="$1"
if [ ! -d "$mountpoint" ]; then
    echo "Error: Mount point $mountpoint does not exist"
    echo "Hint: Mount the filesystem first with: just mount-fuse $mountpoint"
    exit 1
fi

echo "🔌 Unmounting FUSE filesystem from $mountpoint..."

# Try fusermount first (Linux), then umount
if command -v fusermount >/dev/null 2>&1; then
    fusermount -u "$mountpoint" 2>/dev/null || true
elif command -v umount >/dev/null 2>&1; then
    umount "$mountpoint" 2>/dev/null || true
else
    echo "⚠️  Neither fusermount nor umount found, manual unmounting may be required"
fi

echo "✅ FUSE filesystem unmounted from $mountpoint"
