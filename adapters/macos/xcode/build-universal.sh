#!/bin/bash
# Copyright 2025 Schelling Point Labs Inc
# SPDX-License-Identifier: AGPL-3.0-only

# Universal binary build script for AgentFS FSKit Extension
# Builds Rust crates for both Apple Silicon (arm64) and Intel (x86_64) Macs
# and creates universal binaries using lipo

set -e

echo "Building AgentFS FSKit Extension dependencies for universal binary..."

# Get the project root (assuming this script is in adapters/macos/xcode/)
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"
LIBS_DIR="$SCRIPT_DIR/libs"

echo "Script dir: $SCRIPT_DIR"
echo "Project root: $PROJECT_ROOT"

# Verify we have the correct project root by checking for Cargo.toml
if [ ! -f "$PROJECT_ROOT/Cargo.toml" ]; then
  echo "Error: Could not find Cargo.toml in $PROJECT_ROOT"
  echo "Script dir: $SCRIPT_DIR"
  echo "Calculated project root: $PROJECT_ROOT"
  exit 1
fi

# Add target architectures for universal binary
echo "Adding Rust targets for universal binary support..."
rustup target add aarch64-apple-darwin x86_64-apple-darwin

# Build for Apple Silicon (aarch64)
echo "Building Rust crates for Apple Silicon (aarch64)..."
cd "$PROJECT_ROOT"
cargo build --release --target=aarch64-apple-darwin -p agentfs-fskit-sys -p agentfs-fskit-bridge -p agentfs-core -p agentfs-proto -p agentfs-ffi

# Build for Intel (x86_64)
echo "Building Rust crates for Intel (x86_64)..."
cargo build --release --target=x86_64-apple-darwin -p agentfs-fskit-sys -p agentfs-fskit-bridge -p agentfs-core -p agentfs-proto -p agentfs-ffi

# Create universal binaries using lipo
echo "Creating universal binaries with lipo..."
mkdir -p "$LIBS_DIR"

# Create universal libraries for each crate
lipo -create \
  "$PROJECT_ROOT/target/aarch64-apple-darwin/release/libagentfs_fskit_sys.a" \
  "$PROJECT_ROOT/target/x86_64-apple-darwin/release/libagentfs_fskit_sys.a" \
  -output "$LIBS_DIR/libagentfs_fskit_sys.a"

lipo -create \
  "$PROJECT_ROOT/target/aarch64-apple-darwin/release/libagentfs_fskit_bridge.a" \
  "$PROJECT_ROOT/target/x86_64-apple-darwin/release/libagentfs_fskit_bridge.a" \
  -output "$LIBS_DIR/libagentfs_fskit_bridge.a"

lipo -create \
  "$PROJECT_ROOT/target/aarch64-apple-darwin/release/libagentfs_ffi.a" \
  "$PROJECT_ROOT/target/x86_64-apple-darwin/release/libagentfs_ffi.a" \
  -output "$LIBS_DIR/libagentfs_ffi.a"

# Verify the universal binaries
echo "Verifying universal binaries..."
lipo -info "$LIBS_DIR/libagentfs_fskit_sys.a"
lipo -info "$LIBS_DIR/libagentfs_fskit_bridge.a"
lipo -info "$LIBS_DIR/libagentfs_ffi.a"

echo "Universal binary build complete!"
echo "Libraries are ready for Swift linking on both Apple Silicon and Intel Macs."
echo "Universal libraries location: $LIBS_DIR"
