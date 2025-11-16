# Kitty — Integration Guide

This document describes automating Kitty via its remote‑control interface for Agent Harbor workflows.

## Overview

- **Product name and version(s) tested**: Kitty 0.26.0+ (current: 0.35.x)
- **Platforms**: Linux (X11/Wayland), macOS
- **License**: GPLv3
- **CLI used**: `kitty @` commands communicate via socket set in `$KITTY_LISTEN_ON`

## Capabilities Summary

- **Tabs/workspaces support**: Yes — OS windows, tabs, and splits within tabs
- **Horizontal/vertical splits**: Yes — `--location=hsplit|vsplit` with optional percentage (e.g., `vsplit:30%`)
- **Addressability**: By numeric ID or `--match` patterns (`title:pattern`, `id:123`, `state:focused`)
- **Start commands per pane automatically**: Pass command after `--` in `launch`: `kitty @ launch -- bash -c 'cmd'`
- **Focus/activate existing pane**: `kitty @ focus-window --match <selector>`
- **Send keys / scripted answers**: `kitty @ send-text --match <selector> -- "text\r"`
- **Startup layout recipe**: Use `kitty @` commands or session files

## Creating a New Tab With Split Layout

Agent Harbor's 3-pane layout: editor (left), TUI and logs (right, stacked).

```bash
#!/usr/bin/env bash
# Create AH session layout

TASK_ID="${1:-test-task}"
TITLE="ah-task-${TASK_ID}"
CWD="${2:-$PWD}"

# Create tab with editor
kitty @ launch --type=tab --cwd "$CWD" --title "$TITLE-editor" --tab-title "$TITLE" \
    -- bash -lc 'nvim .'

# Split for TUI (right, 30%)
kitty @ launch --type=window --location=vsplit:30% --cwd "$CWD" --title "$TITLE-tui" \
    -- bash -lc "ah tui --follow ${TASK_ID}"

# Split TUI for logs (bottom)
kitty @ launch --type=window --location=hsplit --cwd "$CWD" --title "$TITLE-logs" \
    -- bash -lc "ah session logs ${TASK_ID} -f"
```

### Notes

- **Session naming**: Use `ah-task-<id>` prefix for discovery
- **Remote control**: Must run inside Kitty or configure `allow_remote_control` in `kitty.conf`
- **Split sizing**: Default is 50/50; specify percentage like `vsplit:30%`

## Launching Commands in Each Pane

Commands go after `--`:

```bash
# With working directory
kitty @ launch --cwd /path/to/project -- bash -lc 'npm start'

# With environment variables
kitty @ launch --env VAR=value -- bash -lc 'echo $VAR'

# Keep pane open after exit
kitty @ launch --hold -- python script.py
```

## Scripting Interactive Answers (Send Keys)

```bash
# Send text with Enter
kitty @ send-text --match title:"my-window" -- "yes\r"

# Send without newline
kitty @ send-text --no-newline --match title:"my-window" -- "text"

# Special keys: \r (Enter), \t (Tab), \x03 (Ctrl+C)
```

## Focusing an Existing Task's Pane/Window

```bash
TASK_ID="$1"

# Focus tab by title
kitty @ focus-tab --match title:"ah-task-${TASK_ID}"

# Focus specific window
kitty @ focus-window --match title:"ah-task-${TASK_ID}-tui"

# Discover by listing
kitty @ ls | jq -r '.[] | .tabs[] | select(.title | contains("ah-task")) | .id'
```

## Programmatic Control Interfaces

### Remote Control Setup

Kitty automatically creates a socket when `allow_remote_control` is enabled. Configure in `~/.config/kitty/kitty.conf`:

```
# Enable remote control (from Kitty itself or socket connections)
allow_remote_control yes

# Or restrict to socket only
allow_remote_control socket-only

# Or require password (recommended for socket access)
allow_remote_control password
remote_control_password mypassword
```

When running inside Kitty, `$KITTY_LISTEN_ON` is automatically set. For external control, use the socket path from this variable.

### Common Commands

```bash
# List all windows/tabs (JSON)
kitty @ ls

# Close window
kitty @ close-window --match id:42

# Set title
kitty @ set-window-title --match id:42 "New Title"

# Get text from window
kitty @ get-text --match id:42
```

### Exit Codes

- `0`: Success
- `1`: Command failed (no match, invalid args)
- Non-zero: Socket unavailable or permission denied

## Detection and Version Compatibility

```bash
# Check if Kitty is available
if ! command -v kitty &>/dev/null; then
    echo "Kitty not found"
    exit 1
fi

# Check version
VERSION=$(kitty --version | grep -oP 'kitty \K[0-9.]+')

# Check if remote control works (must be inside Kitty or have socket)
if ! kitty @ ls &>/dev/null; then
    echo "Remote control not available"
    echo "Enable with: allow_remote_control yes in kitty.conf"
    exit 1
fi
```

### Minimum Versions

- **0.13.0**: Basic remote control
- **0.26.0**: Password protection (recommended minimum)
- **0.35.0**: Current stable

## Cross‑Platform Notes

### macOS

```bash
# Install
brew install --cask kitty

# Bring to foreground
osascript -e 'tell application "kitty" to activate'
```

### Linux (X11)

```bash
# Install
sudo apt install kitty  # Debian/Ubuntu
sudo pacman -S kitty    # Arch

# Bring to foreground
wmctrl -x -a kitty || xdotool search --class kitty windowactivate
```

### Linux (Wayland)

Window focus requires compositor-specific tools:

- Sway: `swaymsg '[app_id="kitty"] focus'`
- Hyprland: `hyprctl dispatch focuswindow kitty`

### Windows

Not supported. Use WezTerm or Windows Terminal instead.

## Example: TUI Follow Flow

Complete script to create or focus a task session:

```bash
#!/usr/bin/env bash
set -euo pipefail

TASK_ID="${1:?Usage: $0 <task-id>}"
CWD="${2:-$PWD}"
TITLE="ah-task-${TASK_ID}"

# Check if session exists
existing=$(kitty @ ls 2>/dev/null | jq -r \
    --arg title "$TITLE" \
    '.[] | .tabs[] | select(.title | startswith($title)) | .id' | head -1)

if [[ -n "${existing:-}" ]]; then
    echo "Focusing existing session..."
    kitty @ focus-tab --match id:"$existing"
    exit 0
fi

echo "Creating new session..."

# Editor pane
kitty @ launch --type=tab --cwd "$CWD" --title "$TITLE-editor" --tab-title "$TITLE" \
    -- bash -lc 'nvim .'

# TUI pane (right)
kitty @ launch --type=window --location=vsplit:30% --cwd "$CWD" --title "$TITLE-tui" \
    -- bash -lc "ah tui --follow ${TASK_ID}"

# Logs pane (bottom-right)
kitty @ launch --type=window --location=hsplit --cwd "$CWD" --title "$TITLE-logs" \
    -- bash -lc "ah session logs ${TASK_ID} -f"

echo "Session ready: $TASK_ID"
```

## References

1. **Kitty Remote Control**: https://sw.kovidgoyal.net/kitty/remote-control/
2. **Kitty Configuration**: https://sw.kovidgoyal.net/kitty/conf/#opt-kitty.allow_remote_control
3. **Kitty Launch Command**: https://sw.kovidgoyal.net/kitty/launch/
4. **Kitty on GitHub**: https://github.com/kovidgoyal/kitty
