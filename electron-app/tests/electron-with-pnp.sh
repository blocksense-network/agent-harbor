#!/usr/bin/env bash
# Wrapper script to launch Electron with Yarn PnP resolution
# This ensures native addons like @agent-harbor/gui-core can be resolved

# Get the directory of this script
SCRIPT_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"
REPO_ROOT="$SCRIPT_DIR/../.."

# Find Electron executable
ELECTRON_DIR="$REPO_ROOT/.yarn/unplugged"
ELECTRON_PATH=$(find "$ELECTRON_DIR" -name "Electron.app" -path "*/electron-npm-*/node_modules/electron/dist/Electron.app" | head -1)

if [ -z "$ELECTRON_PATH" ]; then
    echo "Error: Could not find Electron executable" >&2
    exit 1
fi

ELECTRON_EXEC="$ELECTRON_PATH/Contents/MacOS/Electron"

# Run Electron through yarn to enable PnP resolution
cd "$REPO_ROOT"
exec yarn node --import ./.pnp.loader.mjs "$ELECTRON_EXEC" "$@"
