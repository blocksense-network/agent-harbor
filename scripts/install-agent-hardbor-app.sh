#!/usr/bin/env bash
# Copyright 2025 Schelling Point Labs Inc
# SPDX-License-Identifier: AGPL-3.0-only

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

APP_BUILD_PATH="${REPO_ROOT}/apps/macos/AgentHarbor/.build/arm64-apple-macosx/release/AgentHarbor.app"
DEST_APP="/Applications/AgentHarbor.app"

echo "Building AgentHarbor app with embedded FSKit extension..."
just build-agent-harbor

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
