#!/bin/bash
# Copyright 2025 Schelling Point Labs Inc
# SPDX-License-Identifier: AGPL-3.0-only

# Packaging and signing script for AgentFS macOS app bundle
# Creates signed and notarized app bundle for distribution

set -e

# Configuration
APP_NAME="AgentHarbor"
BUILD_TYPE="${1:-Release}" # Default to Release if not specified
SIGNING_IDENTITY="${2:-}"  # Optional: specify code signing identity
TEAM_ID="${3:-}"           # Optional: specify development team ID

echo "Packaging and signing $APP_NAME app bundle..."
echo "Build type: $BUILD_TYPE"

# Get script directory and project paths
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
XCODE_PROJ_DIR="$SCRIPT_DIR/../../../apps/macos/$APP_NAME"
BUILD_DIR="$XCODE_PROJ_DIR/build"
APP_BUNDLE="$BUILD_DIR/Build/Products/$BUILD_TYPE/$APP_NAME.app"

echo "Script dir: $SCRIPT_DIR"
echo "Xcode project dir: $XCODE_PROJ_DIR"
echo "Build dir: $BUILD_DIR"
echo "App bundle: $APP_BUNDLE"

# Ensure we're in the correct directory
cd "$XCODE_PROJ_DIR"

# Build the app with xcodebuild
echo "Building $APP_NAME with xcodebuild..."

if [ -n "$SIGNING_IDENTITY" ]; then
  echo "Using code signing identity: $SIGNING_IDENTITY"
  XCODEBUILD_ARGS="CODE_SIGN_IDENTITY=\"$SIGNING_IDENTITY\""

  if [ -n "$TEAM_ID" ]; then
    XCODEBUILD_ARGS="$XCODEBUILD_ARGS DEVELOPMENT_TEAM=\"$TEAM_ID\""
  fi

  eval xcodebuild -scheme "$APP_NAME" -configuration "$BUILD_TYPE" -allowProvisioningUpdates $XCODEBUILD_ARGS build
else
  echo "Building without code signing (for development/testing)"
  xcodebuild -scheme "$APP_NAME" -configuration "$BUILD_TYPE" CODE_SIGNING_ALLOWED=NO build
fi

# Verify the app bundle was created
if [ ! -d "$APP_BUNDLE" ]; then
  echo "Error: App bundle not found at $APP_BUNDLE"
  exit 1
fi

echo "App bundle created successfully at: $APP_BUNDLE"

# Verify the extension is embedded
EXTENSION_PATH="$APP_BUNDLE/Contents/PlugIns/AgentFSKitExtension.appex"
if [ ! -d "$EXTENSION_PATH" ]; then
  echo "Error: Extension not found in app bundle at $EXTENSION_PATH"
  exit 1
fi

echo "Extension found at: $EXTENSION_PATH"

# Verify universal binaries in the extension
echo "Verifying universal binaries in extension..."
lipo -info "$EXTENSION_PATH/Contents/MacOS/AgentFSKitExtension" || echo "Warning: Could not verify extension binary architecture"

# Create distribution package if code signing was used
if [ -n "$SIGNING_IDENTITY" ]; then
  echo "Creating distribution package..."

  # Create a temporary directory for packaging
  TEMP_DIR=$(mktemp -d)
  ARCHIVE_NAME="$APP_NAME-$BUILD_TYPE-$(date +%Y%m%d-%H%M%S).pkg"
  ARCHIVE_PATH="$BUILD_DIR/$ARCHIVE_NAME"

  echo "Creating archive at: $ARCHIVE_PATH"

  # Create a signed archive using productbuild
  productbuild --component "$APP_BUNDLE" /Applications --package "$TEMP_DIR/$APP_NAME.pkg"

  # Create the final distribution package
  productbuild --distribution "$SCRIPT_DIR/Distribution.xml" \
    --package-path "$TEMP_DIR" \
    --sign "$SIGNING_IDENTITY" \
    "$ARCHIVE_PATH"

  rm -rf "$TEMP_DIR"

  echo "Distribution package created: $ARCHIVE_PATH"

  # Instructions for notarization
  echo ""
  echo "To notarize the package, run:"
  echo "xcrun notarytool submit \"$ARCHIVE_PATH\" --keychain-profile \"AC_PASSWORD\" --wait"
  echo ""
  echo "After notarization is approved, run:"
  echo "xcrun stapler staple \"$ARCHIVE_PATH\""
  echo ""
  echo "Then the package will be ready for distribution."
else
  echo "Code signing not configured. Package created without signing."
  echo "For distribution, you will need to:"
  echo "1. Set up code signing with a Developer ID certificate"
  echo "2. Run this script with signing identity: ./package.sh $BUILD_TYPE \"Developer ID Application: Your Name\""
  echo "3. Notarize the resulting package with Apple"
fi

echo "Packaging complete!"
echo "App bundle location: $APP_BUNDLE"

if [ -n "$SIGNING_IDENTITY" ] && [ -f "$ARCHIVE_PATH" ]; then
  echo "Distribution package location: $ARCHIVE_PATH"
fi
