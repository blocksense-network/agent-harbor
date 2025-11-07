#!/usr/bin/env bash
# Copyright 2025 Schelling Point Labs Inc
# SPDX-License-Identifier: AGPL-3.0-only

# Shared configuration for test filesystem scripts
# This file defines constants used across create-test-filesystems.sh,
# check-test-filesystems.sh, and cleanup-test-filesystems.sh

# Cache directory for test filesystem backing files
CACHE_DIR="$HOME/.cache/agent-harbor"

# ZFS configuration
ZFS_FILE="$CACHE_DIR/zfs_backing.img"
ZFS_POOL="AH_test_zfs"

# Btrfs configuration
BTRFS_FILE="$CACHE_DIR/btrfs_backing.img"
BTRFS_LOOP="/dev/loop99" # Use a high number to avoid conflicts

# APFS configuration (macOS only)
APFS_FILE="$CACHE_DIR/apfs_backing.sparseimage"
APFS_VOLNAME="AH_test_apfs"
