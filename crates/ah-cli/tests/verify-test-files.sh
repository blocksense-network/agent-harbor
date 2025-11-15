#!/bin/bash
# Copyright 2025 Schelling Point Labs Inc
# SPDX-License-Identifier: AGPL-3.0-only

# verify-test-files.sh - Verifies that test files exist in AgentFS overlay
# This script checks that filesystem operations from previous overlay sessions
# are properly accessible in subsequent overlay sessions

# Verify that files created in previous overlay session are accessible
if [ -f "test_file.txt" ]; then
  content=$(cat test_file.txt)
  if [ "$content" = "test content" ]; then
    echo "test_file.txt exists with correct content"
  else
    echo "test_file.txt exists but has wrong content: $content"
    exit 1
  fi
else
  echo "test_file.txt does not exist in overlay"
  exit 1
fi

if [ -d "test_dir" ] && [ -f "test_dir/subfile.txt" ]; then
  subcontent=$(cat test_dir/subfile.txt)
  if [ "$subcontent" = "subdir content" ]; then
    echo "test_dir/subfile.txt exists with correct content"
  else
    echo "test_dir/subfile.txt exists but has wrong content: $subcontent"
    exit 1
  fi
else
  echo "test_dir or test_dir/subfile.txt does not exist in overlay"
  exit 1
fi

echo "overlay verification successful"
