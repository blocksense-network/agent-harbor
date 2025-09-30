#!/bin/bash
set -e

# Build the test interposition library
echo "Building test-interpose.dylib..."

# Check if clang is available
if ! command -v clang &> /dev/null; then
    echo "Error: clang not found. Please install Xcode command line tools."
    exit 1
fi

# Compile the library
clang -dynamiclib -o test-interpose.dylib test-interpose.c

echo "Built test-interpose.dylib successfully"
