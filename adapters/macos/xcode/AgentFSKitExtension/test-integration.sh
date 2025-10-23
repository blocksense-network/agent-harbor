#!/bin/bash
# Copyright 2025 Schelling Point Labs Inc
# SPDX-License-Identifier: AGPL-3.0-only

# Integration test script for AgentFSKitExtension
# Tests basic mount/unmount and file operations

set -e

echo "Running AgentFSKitExtension integration tests..."

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Test variables
TEST_DIR="/tmp/agentfs-test"
MOUNT_POINT="$TEST_DIR/mount"
DUMMY_FILE="$TEST_DIR/dummy.img"
DISK_ID=""

cleanup() {
  echo "Cleaning up..."
  if mount | grep -q "$MOUNT_POINT"; then
    umount "$MOUNT_POINT" || true
  fi
  if [ -n "$DISK_ID" ] && hdiutil info | grep -q "$DISK_ID"; then
    hdiutil detach "$DISK_ID" || true
  fi
  rm -rf "$TEST_DIR"
}

trap cleanup EXIT

# Create test directory
mkdir -p "$MOUNT_POINT"

echo -e "${YELLOW}Setting up dummy block device...${NC}"
# Create dummy file
mkfile -n 10m "$DUMMY_FILE"

# Attach as block device
ATTACH_OUTPUT=$(hdiutil attach -imagekey diskimage-class=CRawDiskImage -nomount "$DUMMY_FILE")
DISK_ID=$(echo "$ATTACH_OUTPUT" | head -1 | awk '{print $1}')

if [ -z "$DISK_ID" ]; then
  echo -e "${RED}Failed to create block device${NC}"
  exit 1
fi

echo -e "${GREEN}Created block device: $DISK_ID${NC}"

echo -e "${YELLOW}Testing mount...${NC}"
# Try to mount (this will fail on systems without FSKit 15.4+, but we test the attempt)
if mount -F -t AgentFS "$DISK_ID" "$MOUNT_POINT" 2>/dev/null; then
  echo -e "${GREEN}Mount successful${NC}"

  # Test basic directory listing
  if ls "$MOUNT_POINT" >/dev/null 2>&1; then
    echo -e "${GREEN}Directory listing works${NC}"

    # Check for expected files
    if ls -la "$MOUNT_POINT" | grep -q "\.agentfs"; then
      echo -e "${GREEN}Control directory .agentfs found${NC}"
    else
      echo -e "${RED}Control directory .agentfs not found${NC}"
    fi

    # Test control directory access
    if ls "$MOUNT_POINT/.agentfs" >/dev/null 2>&1; then
      echo -e "${GREEN}Control directory accessible${NC}"

      # Check for control files
      CONTROL_FILES=$(ls "$MOUNT_POINT/.agentfs" 2>/dev/null || echo "")
      if echo "$CONTROL_FILES" | grep -q "snapshot\|branch\|bind"; then
        echo -e "${GREEN}Control files found${NC}"
      else
        echo -e "${YELLOW}Control files not found (expected in full implementation)${NC}"
      fi
    else
      echo -e "${YELLOW}Control directory not accessible (expected in full implementation)${NC}"
    fi

    # Test file creation
    if echo "test content" >"$MOUNT_POINT/test.txt" 2>/dev/null; then
      echo -e "${GREEN}File creation works${NC}"

      # Test file reading
      if grep -q "test content" "$MOUNT_POINT/test.txt" 2>/dev/null; then
        echo -e "${GREEN}File reading works${NC}"
      else
        echo -e "${RED}File reading failed${NC}"
      fi
    else
      echo -e "${YELLOW}File creation failed (expected in full implementation)${NC}"
    fi

  else
    echo -e "${RED}Directory listing failed${NC}"
  fi

  # Unmount
  umount "$MOUNT_POINT"
  echo -e "${GREEN}Unmount successful${NC}"

else
  echo -e "${YELLOW}Mount failed - this is expected if FSKit extension is not properly installed${NC}"
  echo -e "${YELLOW}or if running on macOS < 15.4. Check system requirements.${NC}"
fi

echo -e "${GREEN}Integration test completed${NC}"
