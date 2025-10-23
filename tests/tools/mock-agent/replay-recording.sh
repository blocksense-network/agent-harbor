#!/usr/bin/env bash
# Copyright 2025 Schelling Point Labs Inc
# SPDX-License-Identifier: AGPL-3.0-only

#
# Replay mock-agent session recordings
#
# This script shows a menu of available recordings and allows selection
# or plays the most recent recording if --latest is specified.

set -euo pipefail

# Check if we're in Nix shell and asciinema is available
if [ -n "${IN_NIX_SHELL:-}" ] && ! command -v asciinema >/dev/null 2>&1; then
  echo "asciinema is not available in the Nix dev shell. Please add it to flake.nix devShell.buildInputs." >&2
  exit 127
fi

RECORDING_DIR="tests/tools/mock-agent/recordings"

# Check if --latest flag is provided
if [ "${1:-}" = "--latest" ]; then
  # Find all recordings and get the most recent
  RECORDINGS=$(find "$RECORDING_DIR" -name "*.json" -type f 2>/dev/null | xargs ls -t 2>/dev/null || true)
  if [ -z "$RECORDINGS" ]; then
    echo "No session recordings found in $RECORDING_DIR"
    echo "Run 'just test-mock-agent-integration' first to create recordings"
    exit 1
  fi
  LATEST_RECORDING=$(echo "$RECORDINGS" | head -1)
  echo "Replaying latest: $(basename "$LATEST_RECORDING")"
  asciinema play "$LATEST_RECORDING"
  exit 0
fi

# Find all recordings
RECORDINGS=$(find "$RECORDING_DIR" -name "*.json" -type f 2>/dev/null | sort || true)
if [ -z "$RECORDINGS" ]; then
  echo "No session recordings found in $RECORDING_DIR"
  echo "Run 'just test-mock-agent-integration' first to create recordings"
  exit 1
fi

# Create fzf input with formatted display
FZF_INPUT=""
while IFS= read -r recording; do
  filename=$(basename "$recording")
  filepath="$recording"

  # Extract timestamp and description from filename
  if [[ $filename =~ ([a-z]+)_(.+)_([0-9]{8}_[0-9]{6})\.json ]]; then
    tool="${BASH_REMATCH[1]}"
    description="${BASH_REMATCH[2]//_/ }"
    timestamp="${BASH_REMATCH[3]}"
    formatted_date=$(date -j -f "%Y%m%d_%H%M%S" "$timestamp" "+%Y-%m-%d %H:%M:%S" 2>/dev/null || echo "$timestamp")
    display_name="$tool: $description ($formatted_date)"
  else
    display_name="$filename"
  fi

  # Format: display_name|filepath
  FZF_INPUT="${FZF_INPUT}${display_name}|${filepath}"$'\n'
done <<<"$RECORDINGS"

# Use fzf for interactive selection
SELECTED=$(echo "$FZF_INPUT" | fzf \
  --delimiter='|' \
  --with-nth=1 \
  --prompt="Select recording to replay: " \
  --header="Use ↑↓ to navigate, type to filter, Enter to select, Esc to cancel" \
  --height=20 \
  --border \
  --margin=1 \
  --padding=1 \
  --preview="echo 'Recording: {}'" \
  --preview-window=bottom:1:wrap)

# Check if user selected something
if [ -z "$SELECTED" ]; then
  echo "No recording selected."
  exit 0
fi

# Extract the file path (everything after the first |)
SELECTED_FILE=$(echo "$SELECTED" | cut -d'|' -f2-)

if [ -n "$SELECTED_FILE" ] && [ -f "$SELECTED_FILE" ]; then
  echo "Replaying: $(basename "$SELECTED_FILE")"
  asciinema play "$SELECTED_FILE"
else
  echo "Error: Selected recording not found"
  exit 1
fi
