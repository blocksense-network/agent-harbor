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
echo "Running basic filesystem smoke tests against $mountpoint..."
# Basic smoke tests for FUSE filesystem functionality
cd "$mountpoint" && \
echo "Testing basic operations..." && \
echo "test content" > test_file.txt && \
cat test_file.txt > /dev/null && \
mkdir test_dir && \
ls -la > /dev/null && \
rm test_file.txt && \
rmdir test_dir && \
echo "Basic filesystem operations completed successfully"
