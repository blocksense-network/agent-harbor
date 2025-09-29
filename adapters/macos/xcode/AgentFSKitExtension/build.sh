#!/bin/bash
# Build script for AgentFSKitExtension
# This script builds the Rust crates and prepares them for Swift linking

set -e

echo "Building AgentFS FSKit Extension..."

# Get the project root (assuming this script is in adapters/macos/xcode/AgentFSKitExtension/)
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../../../.." && pwd)"
echo "Script dir: $SCRIPT_DIR"
echo "Calculated project root: $PROJECT_ROOT"

# Verify we have the correct project root by checking for Cargo.toml
if [ ! -f "$PROJECT_ROOT/Cargo.toml" ]; then
  echo "Error: Could not find Cargo.toml in $PROJECT_ROOT"
  echo "Script dir: $SCRIPT_DIR"
  echo "Calculated project root: $PROJECT_ROOT"
  exit 1
fi

echo "Project root: $PROJECT_ROOT"

# Build the Rust crates first
echo "Building Rust crates..."
cd "$PROJECT_ROOT"
cargo build --release -p agentfs-fskit-sys -p agentfs-fskit-bridge -p agentfs-core -p agentfs-proto -p agentfs-ffi

# Copy the built libraries to the Swift project
echo "Copying Rust libraries to Swift project..."
mkdir -p "$SCRIPT_DIR/libs"
cp "$PROJECT_ROOT/target/release/libagentfs_fskit_sys.a" "$SCRIPT_DIR/libs/"
cp "$PROJECT_ROOT/target/release/libagentfs_fskit_bridge.a" "$SCRIPT_DIR/libs/"
cp "$PROJECT_ROOT/target/release/libagentfs_ffi.a" "$SCRIPT_DIR/libs/"

# Copy system dependencies (if any)
# Note: On macOS, we might need to link against system libraries
# This is handled by the Package.swift linker settings

echo "Building Swift extension with Swift Package Manager..."
cd "$SCRIPT_DIR"

# Build with SwiftPM to avoid Xcode linker issues
CONFIGURATION=${CONFIGURATION:-release}
echo "Building in $CONFIGURATION configuration"
swift build --configuration "$CONFIGURATION"

# Create the .appex bundle structure manually
echo "Creating extension bundle..."
mkdir -p "$SCRIPT_DIR/AgentFSKitExtension.appex/Contents/MacOS"
mkdir -p "$SCRIPT_DIR/AgentFSKitExtension.appex/Contents/Resources"

# Copy the built binary (find the correct path based on architecture and configuration)
BINARY_PATH=""
if [ -f "$SCRIPT_DIR/.build/arm64-apple-macosx/$CONFIGURATION/AgentFSKitExtension" ]; then
  BINARY_PATH="$SCRIPT_DIR/.build/arm64-apple-macosx/$CONFIGURATION/AgentFSKitExtension"
elif [ -f "$SCRIPT_DIR/.build/x86_64-apple-macosx/$CONFIGURATION/AgentFSKitExtension" ]; then
  BINARY_PATH="$SCRIPT_DIR/.build/x86_64-apple-macosx/$CONFIGURATION/AgentFSKitExtension"
elif [ -f "$SCRIPT_DIR/.build/apple/Products/$CONFIGURATION/AgentFSKitExtension" ]; then
  BINARY_PATH="$SCRIPT_DIR/.build/apple/Products/$CONFIGURATION/AgentFSKitExtension"
elif [ -f "$SCRIPT_DIR/.build/apple/Products/Release/AgentFSKitExtension" ] && [ "$CONFIGURATION" = "release" ]; then
  BINARY_PATH="$SCRIPT_DIR/.build/apple/Products/Release/AgentFSKitExtension"
elif [ -f "$SCRIPT_DIR/.build/apple/Products/Debug/AgentFSKitExtension" ] && [ "$CONFIGURATION" = "debug" ]; then
  BINARY_PATH="$SCRIPT_DIR/.build/apple/Products/Debug/AgentFSKitExtension"
else
  echo "Error: Could not find AgentFSKitExtension binary in $CONFIGURATION configuration"
  find "$SCRIPT_DIR/.build" -name "AgentFSKitExtension" -type f -executable 2>/dev/null || echo "No executable found"
  exit 1
fi

echo "Found binary at: $BINARY_PATH"
cp "$BINARY_PATH" "$SCRIPT_DIR/AgentFSKitExtension.appex/Contents/MacOS/"

# Create Info.plist for the extension
cat >"$SCRIPT_DIR/AgentFSKitExtension.appex/Contents/Info.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>CFBundleDevelopmentRegion</key>
	<string>en</string>
	<key>CFBundleExecutable</key>
	<string>AgentFSKitExtension</string>
	<key>CFBundleIdentifier</key>
	<string>com.agentsworkflow.AgentFSKitExtension</string>
	<key>CFBundleInfoDictionaryVersion</key>
	<string>6.0</string>
	<key>CFBundleName</key>
	<string>AgentFSKitExtension</string>
	<key>CFBundlePackageType</key>
	<string>XPC!</string>
	<key>CFBundleShortVersionString</key>
	<string>1.0</string>
	<key>CFBundleVersion</key>
	<string>1</string>
	<key>NSExtension</key>
	<dict>
		<key>NSExtensionPointIdentifier</key>
		<string>com.apple.filesystems</string>
		<key>NSExtensionPrincipalClass</key>
		<string>AgentFSKitExtension.AgentFSKitExtension</string>
	</dict>
</dict>
</plist>
EOF

echo "Extension build complete. Bundle created at: $SCRIPT_DIR/AgentFSKitExtension.appex"
