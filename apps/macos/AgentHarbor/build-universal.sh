#!/bin/bash
# Copyright 2025 Schelling Point Labs Inc
# SPDX-License-Identifier: AGPL-3.0-only

# Build script for AgentHarbor with universal binary support
# This script builds the Rust libraries for both architectures and creates universal binaries

set -e

# Configuration
PROJECT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
RUST_PROJECT_DIR="$PROJECT_DIR/../../../"
BUILD_DIR="$PROJECT_DIR/build"
LIBS_DIR="$PROJECT_DIR/libs"

# Target architectures
ARCHS=("aarch64-apple-darwin" "x86_64-apple-darwin")
# Build the FFI crate as static library for Swift linking
STATIC_CRATES=("agentfs-ffi")

# Create directories
mkdir -p "$BUILD_DIR"
mkdir -p "$LIBS_DIR"

echo "Building universal binaries for AgentHarbor..."

# Build Rust libraries for each architecture
for ARCH in "${ARCHS[@]}"; do
  echo "Building for architecture: $ARCH"

  for CRATE in "${STATIC_CRATES[@]}"; do
    echo "  Building $CRATE for $ARCH..."

    cd "$RUST_PROJECT_DIR/crates/$CRATE"

    # Build the crate as a static library
    cargo build --release --target "$ARCH"

    # Copy the built library (from workspace target directory)
    LIB_NAME="lib$(echo $CRATE | tr '-' '_').a"
    cp "$RUST_PROJECT_DIR/target/$ARCH/release/$LIB_NAME" "$BUILD_DIR/${CRATE}_${ARCH}.a"
  done
done

# Create universal binaries using lipo
echo "Creating universal binaries..."

for CRATE in "${STATIC_CRATES[@]}"; do
  LIB_NAME="lib$(echo $CRATE | tr '-' '_').a"
  ARM64_LIB="$BUILD_DIR/${CRATE}_aarch64-apple-darwin.a"
  X86_64_LIB="$BUILD_DIR/${CRATE}_x86_64-apple-darwin.a"
  UNIVERSAL_LIB="$LIBS_DIR/$LIB_NAME"

  if [ -f "$ARM64_LIB" ] && [ -f "$X86_64_LIB" ]; then
    echo "  Creating universal binary for $CRATE..."
    lipo -create "$ARM64_LIB" "$X86_64_LIB" -output "$UNIVERSAL_LIB"
    echo "  Created $UNIVERSAL_LIB"
  else
    echo "  Warning: Missing libraries for $CRATE universal binary"
    echo "  ARM64: $ARM64_LIB ($(if [ -f "$ARM64_LIB" ]; then echo "exists"; else echo "missing"; fi))"
    echo "  X86_64: $X86_64_LIB ($(if [ -f "$X86_64_LIB" ]; then echo "missing"; else echo "missing"; fi))"
  fi
done

# Copy header files
echo "Copying header files..."
HEADER_DIR="$PROJECT_DIR/PlugIns/AgentFSKitExtension.appex"
mkdir -p "$HEADER_DIR"

# Generate or copy FFI headers
if command -v cbindgen &>/dev/null; then
  echo "Generating C headers with cbindgen..."
  cd "$RUST_PROJECT_DIR/crates/agentfs-ffi"
  cbindgen --config cbindgen.toml --output "$HEADER_DIR/AgentFSKitFFI.h"
else
  echo "cbindgen not found, copying existing headers..."
  cp "$RUST_PROJECT_DIR/crates/agentfs-ffi/c/agentfs_ffi.h" "$HEADER_DIR/AgentFSKitFFI.h" 2>/dev/null || echo "Warning: Could not find FFI header"
fi

# Generate C headers using cbindgen
echo "Generating C headers..."
if command -v cbindgen &>/dev/null; then
  cd "$RUST_PROJECT_DIR/crates/agentfs-ffi"
  cbindgen --config cbindgen.toml --output "$PROJECT_DIR/AgentFSKitFFI.h"
  echo "Generated header: $PROJECT_DIR/AgentFSKitFFI.h"
else
  echo "Warning: cbindgen not found, skipping header generation"
fi

echo "Universal binary build complete!"
echo "Libraries are available in: $LIBS_DIR"
echo "Headers are available in: $PROJECT_DIR"

# Note: Xcode project build (requires Xcode command-line tools)
echo "Note: Xcode project build requires Xcode command-line tools to be available."
echo "If xcodebuild is available, you can build with:"
echo "  cd apps/macos/AgentHarbor"
echo "  xcodebuild build -project AgentHarbor.xcodeproj -scheme AgentHarbor -configuration Release"
echo ""
echo "The Rust static libraries have been built successfully and are ready for Swift integration."
