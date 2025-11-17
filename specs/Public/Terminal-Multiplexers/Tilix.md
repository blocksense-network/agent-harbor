# Tilix — Integration Guide

This document describes automating Tilix layouts and terminal commands via CLI actions, session files, and D‑Bus interfaces.

## Overview

- Product: Tilix (advanced GTK3 tiling terminal emulator)
- Platforms: Linux (X11 and Wayland)
- Installation: available via distro package managers (`apt install tilix`, `dnf install tilix`, etc.) or Flatpak
- Interfaces:
  - CLI: `tilix` with `--action`, `--command`, `--working-directory`, `--session`, `--window-style`, `--maximize`, `--full-screen`
  - Session files: JSON format defining window/terminal layouts, commands, and working directories
  - D‑Bus: programmatic control via `com.gexperts.Tilix` interface (org.gnome.Tilix namespace) [1][2]

## Capabilities Summary

- Tabs/workspaces: creates new windows with `--new-window`, supports session groups/tabs via `app-new-session` action
- Horizontal/vertical splits: actions `session-add-right` and `session-add-down` [1]
- Addressability: limited runtime addressability; primarily via session JSON layouts; window manager integration for focus; D‑Bus methods provide some terminal/session control [2]
- Start commands per pane: `--command` flag or session JSON `command` property; `--working-directory` sets cwd [1]
- Focus/activate: relies on window manager focus; `--maximize`/`--full-screen` flags; D‑Bus methods for terminal/session navigation [2]
- Send keys / scripted answers: no native send-keys mechanism; use application-level non-interactive modes or external tools
- Startup layout recipe: session JSON files define terminal hierarchy, split directions, sizes, commands, and profiles [1]

## Creating a New Tab With Split Layout

**Recommended approach**: Use session JSON files for reliable, declarative layout creation.

Method 1: Using session JSON file (declarative approach, recommended):

```bash
TASK_ID=$1
TITLE="ah-task-${TASK_ID}"

# Create temporary session file
cat > /tmp/ah-session-${TASK_ID}.json <<'EOJSON'
{
  "name": "TITLE_PLACEHOLDER",
  "synchronizedInput": false,
  "child": {
    "type": "Paned",
    "orientation": "horizontal",
    "position": 50,
    "children": [
      {
        "type": "Paned",
        "orientation": "vertical",
        "position": 70,
        "children": [
          {
            "type": "Terminal",
            "command": "nvim .",
            "directory": "PWD_PLACEHOLDER",
            "profile": "Default",
            "readOnly": false
          },
          {
            "type": "Terminal",
            "command": "ah session logs TASKID_PLACEHOLDER -f",
            "directory": "PWD_PLACEHOLDER",
            "profile": "Default",
            "readOnly": true
          }
        ]
      },
      {
        "type": "Terminal",
        "command": "ah tui --follow TASKID_PLACEHOLDER",
        "directory": "PWD_PLACEHOLDER",
        "profile": "Default",
        "readOnly": false
      }
    ]
  }
}
EOJSON

# Replace placeholders with actual values
sed -i "s|TITLE_PLACEHOLDER|$TITLE|g" /tmp/ah-session-${TASK_ID}.json
sed -i "s|TASKID_PLACEHOLDER|$TASK_ID|g" /tmp/ah-session-${TASK_ID}.json
sed -i "s|PWD_PLACEHOLDER|$PWD|g" /tmp/ah-session-${TASK_ID}.json

# Launch Tilix with the session
tilix --session=/tmp/ah-session-${TASK_ID}.json &
```

Method 2: Using CLI actions (fragile, requires active window context):

**Warning**: CLI actions are unreliable because they operate on the currently focused terminal. Use session JSON instead for production workflows.

```bash
TASK_ID=$1

# This approach only works if you're already inside a Tilix window
# and that window remains focused throughout execution

# Split right from current terminal
tilix --action=session-add-right \
  --working-directory="$PWD" \
  --command="bash -lc 'ah tui --follow ${TASK_ID}'" &

sleep 0.3

# Split down from the LEFT terminal (must manually focus it first)
# This is the fundamental problem: we can't reliably control which pane gets split
tilix --action=session-add-down \
  --working-directory="$PWD" \
  --command="bash -lc 'ah session logs ${TASK_ID} -f'" &
```

Notes

- **Session JSON is the only reliable approach** for programmatically creating layouts. CLI actions depend on window focus and are not suitable for automated workflows.
- Session file commands are executed directly (not through a login shell), so ensure your PATH is properly set or use absolute paths.
- The `position` values in `Paned` objects represent percentage split (0-100), with the split bar at that position.
- Session files support nested `Paned` containers for complex layouts. [1]
- The `readOnly` property prevents user input in a terminal (useful for log panes).
- Tilix does not interpolate shell variables in session JSON files - use heredoc with placeholder replacement as shown above.

## Launching Commands in Each Pane

**Session file method (recommended)**:
- Set `"command"` property in each Terminal object
- Commands are executed directly (not through a shell), so use full paths or ensure PATH is set
- If you need shell features (pipes, redirects, variable expansion), wrap in: `"command": "bash -c 'your command here'"`
- Set `"directory"` property for working directory
- Set `"profile"` to use specific Tilix profile settings (colors, fonts, etc.)
- Set `"readOnly": true` to prevent user input (useful for log panes) [1]

**CLI method** (not recommended for layouts):
- Use `--command` with actions to run a command in splits
- **IMPORTANT**: Commands passed to `--command` bypass shell initialization, so wrap them in a login shell:
  ```bash
  tilix --action=session-add-right --command="bash -l -c 'your-command'"
  ```
- Use `--working-directory` to set the current working directory
- CLI actions (`--action=session-add-*`) cannot reliably specify commands because they depend on which terminal is currently focused

Environment propagation:
- Tilix inherits the parent process environment when launched
- Session file commands run directly (not through login shell), so ensure required environment variables are set before launching Tilix
- For complex environment needs, create a wrapper script that sets up the environment and then launches your command

## Scripting Interactive Answers (Send Keys)

Tilix does not provide native send-keys functionality similar to tmux or screen. Workarounds:

1. Use application non-interactive modes (e.g., `--yes` flags)
2. Use expect(1) or similar tools to script interactive sessions
3. Pipe input via stdin when launching commands: `echo "input" | command`
4. Use D‑Bus to programmatically execute commands in terminals (limited control) [2]

For Agent Harbor workflows, prefer non-interactive command modes over scripted input.

## Focusing an Existing Task's Pane/Window

Tilix provides limited CLI support for focusing existing windows by identifier. Approaches:

Method 1: Window manager integration (wmctrl/xdotool on X11):

```bash
TASK_ID=$1
TITLE="ah-task-${TASK_ID}"

# Find and focus window by title (X11)
xdotool search --name "$TITLE" windowactivate

# Or using wmctrl
wmctrl -a "$TITLE"
```

Method 2: D‑Bus interface (requires active Tilix instance):

```bash
# List Tilix windows via D-Bus
gdbus call --session \
  --dest com.gexperts.Tilix \
  --object-path /com/gexperts/Tilix \
  --method com.gexperts.Tilix.ListWindows

# Focus terminal by UUID (if known)
gdbus call --session \
  --dest com.gexperts.Tilix \
  --object-path /com/gexperts/Tilix \
  --method com.gexperts.Tilix.FocusTerminal \
  "<terminal-uuid>"
```

Method 3: Store process IDs and use kill -0 to detect running sessions:

```bash
# When creating session, capture PID
TILIX_PID=$!
echo "$TILIX_PID" > /tmp/ah-task-${TASK_ID}.pid

# Later, check if process exists and bring to foreground
if kill -0 "$TILIX_PID" 2>/dev/null; then
  xdotool search --pid "$TILIX_PID" windowactivate
fi
```

Limitations:
- No built-in CLI for window/terminal lookup by title or custom identifier
- D‑Bus methods require terminal UUIDs which are not easily discoverable
- Window manager tools are platform-specific (X11 vs Wayland)

## Programmatic Control Interfaces

CLI actions (available actions for `--action` flag):
- `session-add-right`: Split terminal horizontally (new terminal to the right)
- `session-add-down`: Split terminal vertically (new terminal below)
- `app-new-session`: Create new session (tab) in current window
- `app-new-window`: Create new Tilix window

Additional CLI flags:
- `--maximize`, `--minimize`, `--full-screen`: Window state control
- `--focus-window`: Focus the existing window
- `--preferences`: Open preferences dialog
- `--quake`: Toggle quake mode window
- `--geometry=COLSxROWS+X+Y`: Set window size and position (e.g., `80x24+200+200`)
- `--window-style=STYLE`: Override window style (`normal`, `disable-csd`, `disable-csd-hide-toolbar`, `borderless`)

Session file format:
- JSON structure with `"name"`, `"synchronizedInput"`, `"child"` root properties
- `"child"` contains hierarchical layout of `"Paned"` (containers) and `"Terminal"` (leaves) objects
- `"Paned"` objects: `"type": "Paned"`, `"orientation": "horizontal"|"vertical"`, `"position": 0-100`, `"children": [...]`
- `"Terminal"` objects: `"type": "Terminal"`, `"command": "..."`, `"directory": "..."`, `"profile": "..."`, `"readOnly": true|false` [1]

D‑Bus interface (`com.gexperts.Tilix`):
- Methods: `ListWindows`, `ListSessions`, `FocusTerminal`, `ExecuteCommand`
- Not extensively documented; requires introspection for full API [2]

Exit codes:
- Returns 0 on success, non-zero on failure (typical UNIX convention)

Security considerations:
- Session files execute arbitrary commands; validate sources
- D‑Bus interface runs with user privileges; ensure proper access controls on session bus

## Detection and Version Compatibility

Detection:
```bash
if command -v tilix >/dev/null 2>&1; then
  TILIX_VERSION=$(tilix --version | grep -oP '\d+\.\d+\.\d+')
  echo "Tilix $TILIX_VERSION detected"
fi
```

Version compatibility:
- Session file format and CLI actions documented here are stable since Tilix 1.8.x (2018+)
- D‑Bus interface introduced in early versions (1.5.x+) but not extensively documented
- Current stable versions: 1.9.x series (as of 2021)
- Tilix development has slowed; verify installed version matches documented features

Feature gates in Agent Harbor:
- Minimum version: 1.8.0 for reliable session file support
- CLI actions available in all modern versions
- D‑Bus support considered experimental due to limited documentation

## Cross‑Platform Notes

Linux:
- GTK3-based; requires GNOME/GTK dependencies
- Works on both X11 and Wayland

X11-specific:
- Window focus via xdotool, wmctrl, xprop
- Global hotkeys may require X11 keyboard grab

Wayland-specific:
- Window focus limited by compositor security policies
- Use compositor-specific tools (e.g., swaymsg for Sway/SwayWM)
- Global hotkeys may not work; rely on compositor keybindings

macOS:
- Not available; use iTerm2, Kitty, or WezTerm

Windows:
- Not available; use Windows Terminal or WezTerm

## Example: TUI Follow Flow

Complete script to create or focus an Agent Harbor task session:

```bash
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

Complete script to create or focus an Agent Harbor task session:

```bash
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

# Create session JSON with placeholders
# Using 'EOJSON' heredoc to prevent variable expansion
cat > "$SESSION_FILE" <<'EOJSON'
{
  "name": "TITLE_PLACEHOLDER",
  "synchronizedInput": false,
  "child": {
    "type": "Paned",
    "orientation": "horizontal",
    "position": 50,
    "children": [
      {
        "type": "Paned",
        "orientation": "vertical",
        "position": 70,
        "children": [
          {
            "type": "Terminal",
            "command": "bash -c 'cd PWD_PLACEHOLDER && exec nvim .'",
            "directory": "PWD_PLACEHOLDER",
            "profile": "Default",
            "readOnly": false
          },
          {
            "type": "Terminal",
            "command": "bash -c 'exec ah session logs TASKID_PLACEHOLDER -f'",
            "directory": "PWD_PLACEHOLDER",
            "profile": "Default",
            "readOnly": true
          }
        ]
      },
      {
        "type": "Terminal",
        "command": "bash -c 'exec ah tui --follow TASKID_PLACEHOLDER'",
        "directory": "PWD_PLACEHOLDER",
        "profile": "Default",
        "readOnly": false
      }
    ]
  }
}
EOJSON

# Replace placeholders with actual values (properly escaped)
# Using @ operator to handle paths with spaces
PWD_ESCAPED=$(printf '%s' "$PWD" | sed 's/[\/&]/\\&/g')
TITLE_ESCAPED=$(printf '%s' "$TITLE" | sed 's/[\/&]/\\&/g')

sed -i "s/TITLE_PLACEHOLDER/$TITLE_ESCAPED/g" "$SESSION_FILE"
sed -i "s/TASKID_PLACEHOLDER/$TASK_ID/g" "$SESSION_FILE"
sed -i "s|PWD_PLACEHOLDER|$PWD_ESCAPED|g" "$SESSION_FILE"

echo "Launching Tilix with session file $SESSION_FILE"
tilix --session="$SESSION_FILE" &
echo $! > "$PID_FILE"

echo "Created new Tilix session for task $TASK_ID"
```

**Key points**:
1. Session JSON is created with placeholders to avoid quoting issues
2. Commands use `bash -c 'exec command'` to ensure proper shell initialization and replace the bash process
3. The script tracks PIDs for session reuse
4. Focus is handled via window manager tools (xdotool/wmctrl)
5. All paths and variables are properly escaped for sed replacement

## References

1. Tilix Manual — CLI options, actions, session file format: [Tilix Manual][1]
2. Tilix GitHub — Source code, D‑Bus interface implementation: [Tilix GitHub][2]

[1]: https://gnunn1.github.io/tilix-web/manual/
[2]: https://github.com/gnunn1/tilix
