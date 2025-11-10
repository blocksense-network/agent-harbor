#!/usr/bin/env bash
# Copyright 2025 Schelling Point Labs Inc
# SPDX-License-Identifier: AGPL-3.0-only

echo "🔍 Running snapshot tests..."
if cargo insta test --no-quiet >/dev/null 2>&1; then
  echo "✅ All snapshots are up to date!"
else
  echo "📝 Snapshots need review. Use 'just insta-review' to review changes."
fi
