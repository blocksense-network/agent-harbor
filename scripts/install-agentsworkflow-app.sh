#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

APP_BUILD_PATH="${REPO_ROOT}/apps/macos/AgentsWorkflow/.build/arm64-apple-macosx/release/AgentsWorkflow.app"
DEST_APP="/Applications/AgentsWorkflow.app"

echo "Building AgentsWorkflow app with embedded FSKit extension..."
just build-agents-workflow

if [[ ! -d "$APP_BUILD_PATH" ]]; then
  echo "Build did not produce app at: $APP_BUILD_PATH" >&2
  exit 1
fi

echo "Installing app to: $DEST_APP"
if cp -R "$APP_BUILD_PATH" "$DEST_APP" 2>/dev/null; then
  :
else
  echo "Copy without sudo failed; trying sudo..."
  sudo cp -R "$APP_BUILD_PATH" "$DEST_APP"
fi

echo "Installed: $DEST_APP"
