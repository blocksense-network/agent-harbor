#!/usr/bin/env bash
# Copyright 2025 Schelling Point Labs Inc
# SPDX-License-Identifier: AGPL-3.0-only

set -euo pipefail

# Help function
show_help() {
  cat <<EOF
Usage: $0 [OPTIONS] [mountpoint]

Run complete pjdfstest workflow: setup (if needed), mount, test, unmount

Arguments:
  mountpoint    Mount point for the filesystem (default: /tmp/agentfs)

Options:
  -h, --help    Show this help message

Examples:
  $0                    # Use default mountpoint /tmp/agentfs
  $0 /mnt/agentfs      # Use custom mountpoint
EOF
}

# Parse arguments
while [[ $# -gt 0 ]]; do
  case $1 in
  -h | --help)
    show_help
    exit 0
    ;;
  -*)
    echo "Unknown option: $1" >&2
    echo "Use '$0 --help' for usage information." >&2
    exit 1
    ;;
  *)
    # First non-option argument is the mountpoint
    if [[ -n "${MOUNTPOINT:-}" ]]; then
      echo "Too many arguments. Use '$0 --help' for usage information." >&2
      exit 1
    fi
    MOUNTPOINT="$1"
    ;;
  esac
  shift
done

# Set default mountpoint
MOUNTPOINT="${MOUNTPOINT:-/tmp/agentfs}"

# Check if pjdfstest is set up
if [ ! -d "resources/pjdfstest/tests" ]; then
  echo "Setting up pjdfstest suite..."
  just setup-pjdfstest-suite
else
  echo "pjdfstest suite already set up, skipping setup..."
fi

# Ensure we unmount on exit (success or failure)
cleanup() {
  echo "Cleaning up: unmounting $MOUNTPOINT..."
  just umount-fuse "$MOUNTPOINT" || true
}
trap cleanup EXIT

# Build the FUSE test binaries
echo "Building FUSE test binaries..."
just build-fuse-test-binaries

# Mount the filesystem with allow-other for root access
echo "Mounting AgentFS at $MOUNTPOINT..."
AGENTFS_FUSE_ALLOW_OTHER=1 just mount-fuse "$MOUNTPOINT"

# Give the mount a moment to settle
sleep 2

# Run the test suite
echo "Running pjdfstest subset..."
sudo -E just test-pjdfs-subset "$MOUNTPOINT"

echo "pjdfstest suite completed successfully!"
