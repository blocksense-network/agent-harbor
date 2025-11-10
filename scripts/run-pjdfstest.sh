#!/usr/bin/env bash
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
if [ ! -d "resources/pjdfstest" ]; then
    echo "Error: pjdfstest suite not set up. Run 'just setup-pjdfstest-suite' first"
    exit 1
fi
echo "Running pjdfstest suite against $mountpoint..."
echo "Note: This requires root privileges and may take a long time"
echo "Press Ctrl+C to interrupt the test suite"
cd "$mountpoint" && prove -rv ../../resources/pjdfstest/tests
