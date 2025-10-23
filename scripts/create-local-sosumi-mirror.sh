#!/usr/bin/env bash
# Copyright 2025 Schelling Point Labs Inc
# SPDX-License-Identifier: AGPL-3.0-only

set -euo pipefail

# Script to download documentation from sosumi.ai
# Usage: create-local-sosumi-mirror.sh <doc-type>
# Example: create-local-sosumi-mirror.sh endpointsecurity
# Example: create-local-sosumi-mirror.sh fskit

if [ $# -ne 1 ]; then
  echo "Usage: $0 <doc-type>"
  echo "Example: $0 endpointsecurity"
  echo "Example: $0 fskit"
  exit 1
fi

DOC_TYPE="$1"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
RESOURCES_DIR="$REPO_ROOT/resources"
TARGET_DIR="$RESOURCES_DIR/$DOC_TYPE"
SOURCE_URL="https://sosumi.ai/documentation/$DOC_TYPE"

echo "🔄 Downloading $DOC_TYPE documentation from $SOURCE_URL"
echo "📁 Target directory: $TARGET_DIR"
echo ""

# Check if python3 is available
if ! command -v python3 >/dev/null 2>&1; then
  echo "❌ Error: python3 is required but not installed."
  exit 1
fi

# Create target directory if it doesn't exist
mkdir -p "$RESOURCES_DIR"
mkdir -p "$TARGET_DIR"

# Change to resources directory
cd "$RESOURCES_DIR"

# Remove existing target directory if it exists
if [ -d "$DOC_TYPE" ]; then
  echo "🗑️  Removing existing $DOC_TYPE directory..."
  rm -rf "$DOC_TYPE"
fi

echo "📥 Starting download process..."

# Use wget's built-in mirroring capabilities
# This is much more reliable than custom crawling logic

echo "🔍 Using wget mirror mode to download documentation..."

wget \
  --mirror \
  --convert-links \
  --adjust-extension \
  --page-requisites \
  --no-parent \
  --no-host-directories \
  --cut-dirs=2 \
  --directory-prefix="$DOC_TYPE" \
  --reject="*.png,*.jpg,*.jpeg,*.gif,*.svg,*.css,*.js,*.ico" \
  --no-check-certificate \
  --wait=1 \
  --random-wait \
  --quiet \
  --show-progress \
  "$SOURCE_URL"

echo "✅ Mirror download completed!"

# Rename files without extensions to .md
find "$DOC_TYPE" -type f ! -name "*.*" -exec sh -c 'mv "$1" "${1}.md"' _ {} \;

# Post-process downloaded files to convert absolute links to relative links
echo "🔄 Converting links to relative paths..."
if [ -d "$DOC_TYPE" ]; then
  find "$DOC_TYPE" \( -name "*.md" -o -name "*.markdown" \) | while IFS= read -r file; do
    # Convert absolute sosumi.ai links to relative paths
    # /documentation/endpointsecurity -> ./index.md
    # /documentation/endpointsecurity/page -> ./page.md
    sed -i.bak \
      -e "s|](/documentation/$DOC_TYPE)$|](./index.md)|g" \
      -e "s|](/documentation/$DOC_TYPE/\\([^)]*\\))|](./\\1.md)|g" \
      "$file" && rm "${file}.bak"
  done
  echo "✅ Link conversion completed!"
fi

# Count downloaded files
if [ -d "$DOC_TYPE" ]; then
  FILE_COUNT=$(find "$DOC_TYPE" \( -name "*.md" -o -name "*.markdown" \) | wc -l)
  echo "📊 Downloaded $FILE_COUNT markdown files"

  # Show directory structure
  echo ""
  echo "📂 Directory structure:"
  find "$DOC_TYPE" \( -name "*.md" -o -name "*.markdown" \) | head -20 | sed 's|^|   |'

  if [ "$FILE_COUNT" -gt 20 ]; then
    REMAINING=$((FILE_COUNT - 20))
    echo "   ... and $REMAINING more files"
  fi
else
  echo "⚠️  No $DOC_TYPE directory found. Download may have failed."
  exit 1
fi

echo ""
echo "🎉 $DOC_TYPE documentation download complete!"
echo "   Files are available in: $TARGET_DIR"
