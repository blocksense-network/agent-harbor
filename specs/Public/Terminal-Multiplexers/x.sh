#!/usr/bin/env bash
set -euo pipefail

TASK_ID=${1:?Missing task ID}
TITLE="ah-task-${TASK_ID}"
SESSION_FILE="/tmp/ah-tilix-session-${TASK_ID}.json"
PID_FILE="/tmp/ah-tilix-pid-${TASK_ID}"

# Check if session already exists
if [ -f "$PID_FILE" ]; then
  TILIX_PID=$(cat "$PID_FILE")
  if kill -0 "$TILIX_PID" 2>/dev/null; then
    # Session exists, focus it
    if command -v xdotool >/dev/null 2>&1; then
      xdotool search --name "$TITLE" windowactivate || true
    elif command -v wmctrl >/dev/null 2>&1; then
      wmctrl -a "$TITLE" || true
    fi
    echo "Focused existing session for task $TASK_ID"
    exit 0
  fi
fi

## Example: TUI Follow Flow

# Complete script to create or focus an Agent Harbor task session:
