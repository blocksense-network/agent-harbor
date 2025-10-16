#!/usr/bin/env bash
# Copy native addon to dist-electron so it can be found without PnP
# This is needed for Playwright testing where PnP loader doesn't work properly

SCRIPT_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"
DIST_DIR="$SCRIPT_DIR/../dist-electron"
REPO_ROOT="$SCRIPT_DIR/../.."

# Find the native addon in PnP cache
ADDON_FILE=$(find "$REPO_ROOT/.yarn/unplugged" -name "ah-gui-core.*.node" -type f | head -1)

if [ -z "$ADDON_FILE" ]; then
    echo "Error: Could not find native addon file" >&2
    exit 1
fi

# Create node_modules/@agent-harbor/gui-core in dist-electron
mkdir -p "$DIST_DIR/node_modules/@agent-harbor/gui-core"

# Get the addon directory
ADDON_DIR=$(dirname "$ADDON_FILE")

# Copy the addon
cp "$ADDON_FILE" "$DIST_DIR/node_modules/@agent-harbor/gui-core/"

# Copy package.json
if [ -f "$ADDON_DIR/package.json" ]; then
    cp "$ADDON_DIR/package.json" "$DIST_DIR/node_modules/@agent-harbor/gui-core/"
fi

# Copy index.js (required for module loading)
if [ -f "$ADDON_DIR/index.js" ]; then
    cp "$ADDON_DIR/index.js" "$DIST_DIR/node_modules/@agent-harbor/gui-core/"
fi

# Copy index.d.ts (TypeScript definitions)
if [ -f "$ADDON_DIR/index.d.ts" ]; then
    cp "$ADDON_DIR/index.d.ts" "$DIST_DIR/node_modules/@agent-harbor/gui-core/"
fi

echo "Copied native addon to $DIST_DIR/node_modules/@agent-harbor/gui-core/"
