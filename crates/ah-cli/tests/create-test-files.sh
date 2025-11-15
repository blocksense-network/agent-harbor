#!/bin/bash
# Copyright 2025 Schelling Point Labs Inc
# SPDX-License-Identifier: AGPL-3.0-only

# create-test-files.sh - Creates test files for AgentFS overlay isolation testing
# This script generates filesystem side effects to verify that overlay operations
# are properly isolated from the underlying filesystem

echo "Creating test files..."
mkdir -p test_dir
echo "test content" >test_file.txt
echo "subdir content" >test_dir/subfile.txt
echo "daemon reuse test successful"
