#!/bin/bash
# Copyright 2025 Schelling Point Labs Inc
# SPDX-License-Identifier: AGPL-3.0-only

set -e

# Build the filesystem interposition library
echo "Building fs-interpose.dylib..."

# Check if clang is available
if ! command -v clang &>/dev/null; then
  echo "Error: clang not found. Please install Xcode command line tools."
  exit 1
fi

# Compile the library with necessary frameworks
# Use architecture detection for proper compilation
ARCH_FLAGS=""
if [[ $(uname -m) == "arm64" ]]; then
  ARCH_FLAGS="-arch arm64"
fi

clang $ARCH_FLAGS -dynamiclib \
  -o fs-interpose.dylib \
  fs-interpose.c \
  -framework SystemConfiguration \
  -framework CoreFoundation \
  -lpthread

echo "Built fs-interpose.dylib successfully"
