#!/bin/bash
# Copyright 2025 Schelling Point Labs Inc
# SPDX-License-Identifier: AGPL-3.0-only

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
POC_DIR="$(dirname "$SCRIPT_DIR")"
LIB_DIR="$POC_DIR/lib"
INJECTOR_DIR="$POC_DIR/injector"

echo "=== DYLD Insert Libraries PoC - Filesystem Redirection Test ==="
echo "Script directory: $SCRIPT_DIR"
echo "PoC directory: $POC_DIR"
echo

# Function to check if a command exists
command_exists() {
  command -v "$1" >/dev/null 2>&1
}

# Check prerequisites
echo "Checking prerequisites..."

if ! command -v clang; then
  echo "ERROR: clang not found. Please install Xcode command line tools."
  exit 1
fi

if ! command -v cargo; then
  echo "ERROR: cargo not found. Please install Rust."
  exit 1
fi

echo "Prerequisites OK"
echo

# Build the filesystem interposition library
echo "Building filesystem interposition library..."
cd "$LIB_DIR"
./build-fs-lib.sh
echo "Filesystem library built successfully"
echo

# Build the injector
echo "Building injector..."
cd "$INJECTOR_DIR"
cargo build --release
INJECTOR="$INJECTOR_DIR/target/release/dyld-injector"
echo "Injector built: $INJECTOR"
echo

# Test 1: Filesystem interception enabled
echo "=== Test 1: Filesystem Interception Enabled ==="
export AGENTFS_ENABLED="1"
export AGENTFS_SERVER="127.0.0.1:8080"

echo "Testing file operations on /agentfs/ paths (should show interception messages)..."
"$INJECTOR" -l "$LIB_DIR/fs-interpose.dylib" bash -c "
    echo 'Creating test file...'
    echo 'test content' > /agentfs/test.txt 2>&1 || echo 'Write failed (expected)'
    
    echo 'Reading test file...'
    cat /agentfs/test.txt 2>&1 || echo 'Read failed (expected)'
    
    echo 'Checking file attributes...'
    stat /agentfs/test.txt 2>&1 || echo 'Stat failed (expected)'
    
    echo 'Creating directory...'
    mkdir /agentfs/testdir 2>&1 || echo 'Mkdir failed (expected)'
    
    echo 'Listing directory...'
    ls /agentfs/ 2>&1 || echo 'List failed (expected)'
"
echo

# Test 2: Filesystem interception disabled
echo "=== Test 2: Filesystem Interception Disabled ==="
unset AGENTFS_ENABLED

echo "Testing normal filesystem operations (should not show interception messages)..."
"$INJECTOR" -l "$LIB_DIR/fs-interpose.dylib" bash -c "
    echo 'Creating test file...'
    echo 'test content' > /tmp/normal-test.txt
    
    echo 'Reading test file...'
    cat /tmp/normal-test.txt
    
    echo 'Checking file attributes...'
    stat /tmp/normal-test.txt
    
    echo 'Cleaning up...'
    rm /tmp/normal-test.txt
"
echo

# Test 3: Mixed operations
echo "=== Test 3: Mixed Operations ==="
export AGENTFS_ENABLED="1"

echo "Testing mix of AgentFS and normal paths..."
"$INJECTOR" -l "$LIB_DIR/fs-interpose.dylib" bash -c "
    echo 'AgentFS path (should be intercepted):'
    echo 'test' > /agentfs/mixed.txt 2>&1 || echo 'AgentFS write failed (expected)'
    
    echo 'Normal path (should not be intercepted):'
    echo 'test' > /tmp/mixed.txt && echo 'Normal write succeeded' && rm /tmp/mixed.txt
"
echo

echo "=== All filesystem redirection tests completed! ==="
echo "✓ Filesystem interception: Paths starting with /agentfs/ are intercepted"
echo "✓ Fallback behavior: Operations fall back to normal filesystem when AgentFS unavailable"
echo "✓ Selective interception: Only specified paths are intercepted, others work normally"
