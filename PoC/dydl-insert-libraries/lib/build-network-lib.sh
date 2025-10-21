#!/bin/bash
set -e

# Build the network interposition library
echo "Building network-interpose.dylib..."

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
  -o network-interpose.dylib \
  ./network-interpose.c \
  -framework SystemConfiguration \
  -framework CoreFoundation

echo "Built network-interpose.dylib successfully"
