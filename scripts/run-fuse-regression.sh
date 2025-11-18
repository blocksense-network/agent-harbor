#!/usr/bin/env bash
# Copyright 2025 Schelling Point Labs Inc
# SPDX-License-Identifier: AGPL-3.0-only

set -euo pipefail

MOUNTPOINT=${1:-/tmp/agentfs}
HARNESS_OPTIONS=${HARNESS_OPTIONS:-j32}

cleanup() {
  fusermount -uz "$MOUNTPOINT" 2>/dev/null || true
}

trap cleanup EXIT

cleanup

# Manual smoke mount
AGENTFS_FUSE_ALLOW_OTHER=1 just mount-fuse "$MOUNTPOINT"
sudo just test-fuse-basic "$MOUNTPOINT"
just umount-fuse "$MOUNTPOINT"

# Scripted suites
just test-fuse-basic-ops
just test-fuse-mount-cycle
just test-fuse-mount-concurrent

# Full pjdfstest with parallel prove
HARNESS_OPTIONS="$HARNESS_OPTIONS" just test-pjdfstest-full
