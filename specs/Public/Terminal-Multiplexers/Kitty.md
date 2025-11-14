# Kitty — Integration Guide

This document describes automating Kitty via its remote‑control interface `kitty @`.

## Overview

- **Product name and version(s) tested:** Kitty 0.43.1 (fast, GPU‑based terminal emulator)
- **Platforms:** Linux, macOS (Windows not natively supported)
- **License and install path(s):** GPLv3; installable via package managers (brew, apt, etc.) or official binaries from <https://sw.kovidgoyal.net/kitty/>
- **CLI(s) or RPC used:** Remote control via `kitty @` command over UNIX domain socket or stdio; controlled via `--to` option or `KITTY_LISTEN_ON` environment variable

**Configuration Required:**

Remote control must be enabled in `~/.config/kitty/kitty.conf`:

```conf
# Enable remote control (required for all kitty @ commands)
allow_remote_control yes

# Set up UNIX socket for external control (scripts, other terminals)
# Kitty creates this socket file automatically when it starts
listen_on unix:/tmp/kitty-ah.sock
```

**How it works:**

- `allow_remote_control yes` - Enables remote control; required for all `kitty @` commands
- `listen_on unix:/tmp/kitty-ah.sock` - Tells Kitty to create a UNIX domain socket file at this path when it starts
- Kitty automatically creates the socket file (type `srwxr-xr-x`) using the `bind()` system call
- The socket file is automatically removed when Kitty exits
- Commands from **within** Kitty windows don't need `--to` (they use stdio/environment)
- Commands from **outside** Kitty (other terminals, scripts) need `--to unix:/tmp/kitty-ah.sock` to connect to the socket

See <https://sw.kovidgoyal.net/kitty/conf/#opt-kitty.allow_remote_control> for security options including `remote_control_password` for fine-grained permissions.

Without `allow_remote_control`, `kitty @` commands will fail with "Remote control is disabled". After adding this configuration, restart Kitty or reload config with `Ctrl+Shift+F5` (or `Cmd+Shift+F5` on Mac).

## Capabilities Summary

- **Tabs/workspaces support:** Yes, via `kitty @ launch --type=tab` and tab management commands
- **Horizontal/vertical splits:** Yes, via `--location=hsplit|vsplit|split` (requires `splits` layout)
- **Addressability:** Windows by `id`, `title`, `pid`, `cwd`, `cmdline`, `num`, `env`, `var`, `state`, `neighbor`, `session`, `recent`; tabs by `id`, `index`, `title`, `window_id`, `window_title`, `pid`, `cwd`, `cmdline`, `env`, `var`, `state`, `session`, `recent`
- **Start commands per pane automatically:** Pass command after `launch ... -- <cmd>`; supports `--cwd`, `--env`, `--copy-env`
- **Focus/activate existing pane:** `kitty @ focus-window --match <criteria>` and `kitty @ focus-tab --match <criteria>`
- **Send keys / scripted answers:** `kitty @ send-text --match <criteria> -- <text>` (with escape sequences) and `kitty @ send-key --match <criteria> <keys>`
- **Startup layout recipe:** Use session files with `kitty --session <file>` or programmatically via remote control commands

## Creating a New Tab With Split Layout

To create a tab with editor, TUI, and logs layout (requires `splits` layout enabled):

```bash
TASK_ID=$1
TITLE="ah-task-${TASK_ID}"
WORKDIR="$PWD"

# For external scripts: connect to socket
# For commands within Kitty: --to is not needed (omit TO line)
TO="--to unix:/tmp/kitty-ah.sock"

# Create new tab with editor (full-width top window)
kitty @ $TO launch \
  --type=tab \
  --tab-title "$TITLE" \
  --cwd "$WORKDIR" \
  --title "Editor" \
  -- bash -lc 'nvim .'

# Add TUI window (horizontal split below editor)
kitty @ $TO launch \
  --type=window \
  --location=hsplit \
  --cwd "$WORKDIR" \
  --title "TUI" \
  -- bash -lc "ah tui --follow ${TASK_ID}"

# Add logs window (vertical split right of TUI)
kitty @ $TO launch \
  --type=window \
  --location=vsplit \
  --cwd "$WORKDIR" \
  --title "Logs" \
  -- bash -lc "ah session logs ${TASK_ID} -f"
```

**Notes:**

- Requires `allow_remote_control yes` in kitty.conf
- **From within Kitty:** Commands work without `--to` flag (uses stdio/environment)
- **From external scripts/terminals:** Must use `--to unix:/tmp/kitty-ah.sock` to connect to socket
- Alternative: Set `export KITTY_LISTEN_ON=unix:/tmp/kitty-ah.sock` to avoid `--to` in external scripts
- Requires `listen_on unix:/tmp/kitty-ah.sock` in kitty.conf for external control
- Tab naming via `--tab-title`; window naming via `--title`
- Working directory control via `--cwd` (supports `current`, `last_reported`, `oldest`, `root`)

## Launching Commands in Each Pane

Commands are specified after `--` separator in `launch` command:

```bash
# Simple command
kitty @ launch --type=window -- htop

# Command with arguments
kitty @ launch --type=window -- bash -c "echo Hello && sleep 10"

# With environment variables
kitty @ launch --type=window --env PATH=/custom/path --env VAR=value -- mycommand

# Copy environment from source window
kitty @ launch --type=window --copy-env -- mycommand

# Set working directory
kitty @ launch --type=window --cwd=/path/to/dir -- mycommand
```

**Environment propagation:**

- `--env KEY=VALUE` sets individual environment variables
- `--copy-env` copies all env vars from source window (at creation time only)
- `PATH` and other env vars pass through unless explicitly overridden
- Use `--cwd` for working directory control (absolute paths or special values)

## Scripting Interactive Answers (Send Keys)

Two commands available: `send-text` (literal text) and `send-key` (key events):

```bash
# Send literal text with newline (for prompts)
kitty @ send-text --match title:"$TITLE" -- "yes\r"

# Send text without automatic newline
kitty @ send-text --match title:"$TITLE" --no-newline -- "password"

# Send from stdin
echo "multi-line text" | kitty @ send-text --match title:"$TITLE" --stdin

# Send key combinations (press and release)
kitty @ send-key --match title:"$TITLE" ctrl+c

# Send multiple keys
kitty @ send-key --match title:"$TITLE" ctrl+a ctrl+k
```

**Quoting/escaping:**

- `send-text` uses Python escape sequences: `\r` (CR), `\n` (LF), `\e` (ESC), `\u` (Unicode)
- Use single quotes in shell to avoid shell interpretation: `'text\r'`
- `--stdin` and `--from-file` send content as-is without escape processing
- `send-key` takes key names (modifier+key format)

**Timing considerations:**

- No built-in delays; use `sleep` between commands if needed
- Use `--no-response` to avoid waiting for command completion
- Programs must be ready to receive input (check via window state if needed)

## Focusing an Existing Task's Pane/Window

```bash
TASK_ID=$1

# Focus window by title
kitty @ focus-window --match title:"ah-task-${TASK_ID}"

# Focus tab by title (focuses active window in tab)
kitty @ focus-tab --match title:"ah-task-${TASK_ID}"

# Focus by window id
kitty @ focus-window --match id:42

# Focus by recent activity (0=current, 1=previous, etc.)
kitty @ focus-window --match recent:1

# Focus by position in tab
kitty @ focus-window --match num:0

# Complex match (Boolean expressions)
kitty @ focus-window --match "title:ah-task and state:focused_os_window"
```

**OS-level focus:**

- Focusing window/tab within kitty brings the OS window to front automatically
- Cross-display support depends on window manager
- On macOS, focus works reliably across spaces/desktops

## Programmatic Control Interfaces

**CLI remote control (`kitty @`):**

```bash
# From within Kitty (no --to needed, uses stdio)
kitty @ <command>

# From external script/terminal (requires socket)
kitty @ --to unix:/tmp/kitty-ah.sock" <command>

# Via KITTY_LISTEN_ON environment variable (for external scripts)
export KITTY_LISTEN_ON=unix:/tmp/kitty-ah.sock"
kitty @ <command>  # Now works without --to

# Over SSH (requires remote control enabled on remote host)
ssh host 'kitty @ <command>'
```

**How socket connection works:**

1. Kitty started with `listen_on unix:/tmp/kitty-ah.sock"` in config
2. Kitty calls `bind()` system call to create socket file (type `srwxr-xr-x`)
3. External `kitty @` commands use `--to` to `connect()` to this socket
4. Commands and responses flow over socket connection (like a pipe)
5. Socket file is automatically removed when Kitty exits

**Starting kitty with remote control:**

```bash
# Configure in kitty.conf (recommended):
allow_remote_control yes
listen_on unix:/tmp/kitty-ah.sock

# Or start with command-line options (temporary):
kitty --listen-on unix:/tmp/kitty-ah.sock -o allow_remote_control=yes
# This creates the socket file when Kitty starts

# Start detached (in background):
kitty --detach --listen-on unix:/tmp/kitty-ah.sock
```

**Socket lifecycle:**

- Socket file is created by Kitty automatically at startup via `bind()` system call
- File type becomes `srwxr-xr-x` (socket, not regular file)
- Socket is removed automatically when Kitty exits
- If you see `.rw-r--r--` permissions, the socket is stale (Kitty not running)

**Configuration details:**

- `allow_remote_control yes` - Enables remote control for all windows. See <https://sw.kovidgoyal.net/kitty/conf/#opt-kitty.allow_remote_control>
- `allow_remote_control socket-only` - Only allows control via socket, not from child processes
- `allow_remote_control password` - Requires authentication via `remote_control_password`
- `listen_on unix:/tmp/kitty-ah.sock"` - Path where Kitty creates the UNIX socket file for external control
- Per-window control: Use `launch --allow-remote-control` to enable for specific windows only

**Exit codes:**

- 0: success
- Non-zero: error (varies by command)
- Use `--no-response` to ignore errors and always return 0

**Security considerations:**

- `allow_remote_control yes` enables control for all windows in the Kitty instance
- `allow_remote_control socket-only` restricts control to socket connections only (not from child processes)
- Use `remote_control_password` for fine-grained permissions and authentication
- Per-window control via `launch --allow-remote-control` for selective enablement
- Socket permissions protect from unauthorized access (UNIX file permissions)
- See <https://sw.kovidgoyal.net/kitty/conf/#opt-kitty.allow_remote_control> for all security options

**IPC protocol:**

- JSON-based protocol over socket/stdio
- See <https://sw.kovidgoyal.net/kitty/rc_protocol/> for protocol spec
- Standalone `kitten` binary available for remote control from any system

## Detection and Version Compatibility

**Detection:**

```bash
# Check if kitty is available
command -v kitty >/dev/null 2>&1

# Get version
kitty --version  # Output: "kitty 0.43.1"

# Check if remote control is enabled (from within kitty)
if [ -n "$KITTY_LISTEN_ON" ]; then
  echo "Remote control available"
fi

# Test remote control connectivity
kitty @ ls >/dev/null 2>&1 && echo "Connected"
```

**Minimum version:**

- Remote control (`kitty @`): Available since very early versions, stable since 0.13.0
- `--location` for splits: Requires splits layout (kitty 0.17.0+)
- `send-key` command: kitty 0.25.0+
- Fine-grained permissions: kitty 0.26.0+
- Tab/window matching improvements: kitty 0.28.0+

**Feature gates:**

- Check command availability: `kitty @ <command> --help 2>&1 | grep -q "Usage"`
- Use `kitty @ ls` to test basic remote control
- Feature detection via `--version` parsing

**Recommended version:** kitty 0.28.0 or later for full feature set

## Cross‑Platform Notes

**macOS:**

- UNIX domain sockets work reliably
- Security/privacy settings do not block local socket communication
- App bundles and command-line kitty both support remote control
- Focus commands work across desktops/spaces
- Install via `brew install kitty` or official DMG

**Linux:**

- Supports X11 and Wayland
- Wayland may have focus restrictions depending on compositor
- Socket location should be in `/tmp` or `$XDG_RUNTIME_DIR`
- Works with all common desktop environments
- Install via package manager (apt, dnf, pacman) or official binaries

**Windows:**

- Not natively supported; kitty is UNIX-only
- Use alternative terminals (Windows Terminal, ConEmu) or WSL2 with kitty
- For Windows workflows, consider tmux/zellij within WSL2

## Example: TUI Follow Flow

Complete script to create or focus a task session:

```bash
#!/bin/bash
set -euo pipefail

TASK_ID="${1:?Task ID required}"
TITLE="ah-task-${TASK_ID}"
WORKDIR="${2:-$PWD}"

# For external control (running from different terminal)
# Uncomment and use if running outside Kitty:
# TO="--to unix:/tmp/kitty-ah.sock"
# Or set: export KITTY_LISTEN_ON=unix:/tmp/kitty-ah.sock

# For running within Kitty, no --to needed:
TO=""

# Check if kitty is running with remote control enabled
if ! kitty @ $TO ls >/dev/null 2>&1; then
  echo "Error: kitty remote control not available" >&2
  echo "Add to ~/.config/kitty/kitty.conf:" >&2
  echo "  allow_remote_control yes" >&2
  if [ -n "$TO" ]; then
    echo "  listen_on unix:/tmp/kitty-ah.sock" >&2
  fi
  echo "Then restart kitty or reload config (Ctrl+Shift+F5)" >&2
  exit 1
fi

# Check if tab already exists
if kitty @ $TO ls | grep -q "\"title\": \"$TITLE\""; then
  echo "Focusing existing tab: $TITLE"
  kitty @ $TO focus-tab --match title:"$TITLE"
else
  echo "Creating new tab: $TITLE"

  # Create tab with editor
  kitty @ $TO launch \
    --type=tab \
    --tab-title "$TITLE" \
    --cwd "$WORKDIR" \
    --title "Editor-$TITLE" \
    -- bash -lc 'nvim .'

  # Add TUI pane
  kitty @ $TO launch \
    --type=window \
    --match title:"Editor-$TITLE" \
    --location=hsplit \
    --cwd "$WORKDIR" \
    --title "TUI-$TITLE" \
    --keep-focus \
    -- bash -lc "ah tui --follow ${TASK_ID}"

  # Add logs pane
  kitty @ $TO launch \
    --type=window \
    --match title:"TUI-$TITLE" \
    --location=vsplit \
    --cwd "$WORKDIR" \
    --title "Logs-$TITLE" \
    --keep-focus \
    -- bash -lc "ah session logs ${TASK_ID} -f"

  # Focus the TUI pane
  kitty @ $TO focus-window --match title:"TUI-$TITLE"
fi
```

**Notes:**

- **Within Kitty:** Set `TO=""` - commands work without `--to` using stdio
- **External control:** Set `TO="--to unix:/tmp/kitty-ah.sock"` or export `KITTY_LISTEN_ON`
- Requires `allow_remote_control yes` in kitty.conf for all scenarios
- Requires `listen_on unix:/tmp/kitty-ah.sock` in kitty.conf for external control
- Socket file (`srwxr-xr-x`) is created by Kitty automatically when it starts
- Use unique titles per window for reliable `--match` targeting
- Store titles or use `kitty @ ls` JSON output to retrieve window/tab IDs
- `--keep-focus` prevents focus changes during pane creation
- Error handling checks remote control availability before operations

## Additional Examples

### Launch with Environment Variables

Set custom environment variables for a window:

```bash
kitty @ $TO launch \
  --type=window \
  --title "Custom Env" \
  --env TASK_ID=12345 \
  --env DEBUG=true \
  --env LOG_LEVEL=verbose \
  --cwd "$WORKDIR" \
  -- bash -lc 'echo "TASK_ID=$TASK_ID DEBUG=$DEBUG LOG_LEVEL=$LOG_LEVEL"; bash -l'
```

### Launch with Copied Environment

Copy all environment variables from current window:

```bash
export TEST_VAR="hello from parent"
export ANOTHER_VAR="copied successfully"

kitty @ $TO launch \
  --type=window \
  --title "Copied Env" \
  --copy-env \
  -- bash -lc 'echo "TEST_VAR=$TEST_VAR ANOTHER_VAR=$ANOTHER_VAR"; bash -l'
```

**Note:** `--copy-env` copies environment at creation time only, not dynamically.

### Send Text to Window (Simulating User Input)

Automate command execution in a window:

```bash
# Create a window
kitty @ $TO launch \
  --type=window \
  --title "Send-Test" \
  -- bash -l

sleep 1

# Send commands (note the \r for Enter key)
kitty @ $TO send-text --match title:"Send-Test" -- "echo 'Hello from automation'\r"
sleep 0.5
kitty @ $TO send-text --match title:"Send-Test" -- "date\r"
sleep 0.5
kitty @ $TO send-text --match title:"Send-Test" -- "pwd\r"
```

### Send Key Combinations (Vim Example)

Control applications with key sequences:

```bash
# Create window with vim
kitty @ $TO launch \
  --type=window \
  --title "Vim-Test" \
  -- bash -lc 'vim /tmp/test-kitty.txt'

sleep 1

# Enter insert mode and type text (note \e for Escape)
kitty @ $TO send-text --match title:"Vim-Test" -- "iHello from Kitty automation\e"
sleep 0.5

# Save and quit
kitty @ $TO send-text --match title:"Vim-Test" -- ":wq\r"
```

**Escape sequences:**

- `\r` - Carriage return (Enter key)
- `\n` - Line feed (newline)
- `\e` - Escape key
- `\t` - Tab

### Focus or Create Pattern (Idempotent)

Focus existing tab if it exists, otherwise create it:

```bash
focus_or_create_task() {
  local task_id="${1:-demo-task}"
  local title="ah-task-${task_id}"

  # Check if tab exists
  if kitty @ $TO ls | grep -q "\"title\": \"$title\""; then
    echo "Tab exists, focusing..."
    kitty @ $TO focus-tab --match title:"$title"
  else
    echo "Tab doesn't exist, creating..."
    # Create three-pane layout (editor, TUI, logs)
    # ... (use example from above)
  fi
}
```

**Use case:** Idempotent script that can be run multiple times safely.

### List All Windows and Tabs

Query Kitty structure programmatically:

```bash
# Get full JSON structure
kitty @ $TO ls

# Pretty-print with jq
kitty @ $TO ls | jq -r '
  .[] |
  "OS Window \(.id):" as $os |
  .tabs[] |
  "\($os)\n  Tab \(.id): \(.title // "Untitled")" as $tab |
  .windows[] |
  "\($tab)\n    Window \(.id): \(.title // "Untitled") [PID: \(.pid)]"
'

# List just window titles
kitty @ $TO ls | jq -r '.[] | .tabs[] | .windows[] | .title'

# Find window by title and get its ID
kitty @ $TO ls | jq -r '.[] | .tabs[] | .windows[] | select(.title == "Editor") | .id'
```

### Get Window ID and Use It

Create window and capture its ID for later use:

```bash
# Launch window and capture JSON output
output=$(kitty @ $TO launch \
  --type=window \
  --title "ID-Test" \
  -- bash -l)

# Extract window ID from JSON response
window_id=$(echo "$output" | jq -r '.id // empty')

if [ -n "$window_id" ]; then
  echo "Created window with ID: $window_id"

  # Use the ID directly (more reliable than title matching)
  kitty @ $TO send-text --match id:"$window_id" -- "echo 'Using window ID: $window_id'\r"

  # Close by ID
  kitty @ $TO close-window --match id:"$window_id"
fi
```

**Advantage:** Window IDs are unique and don't change, unlike titles which can be duplicated or modified.

### Focus by Recent Activity

Navigate through recently used windows:

```bash
# Focus current window (recent:0)
kitty @ $TO focus-window --match recent:0

# Focus previous window (recent:1)
kitty @ $TO focus-window --match recent:1

# Focus two windows ago (recent:2)
kitty @ $TO focus-window --match recent:2

# Cycle back and forth
for i in 1 0 1 0; do
  kitty @ $TO focus-window --match recent:$i
  sleep 1
done
```

**Use case:** Quick navigation without knowing window titles or IDs.

### Session Files for Reproducible Layouts

Create a session file for complex layouts:

```bash
cat > /tmp/kitty-ah-session.conf <<'EOF'
# Agent Harbor development session
new_tab Editor
cd ~/project
launch nvim .

new_tab TUI+Logs
cd ~/project
layout splits
launch bash -lc "ah tui --follow task-123"
launch --location=hsplit bash -lc "ah session logs task-123 -f"

new_tab Tests
cd ~/project
launch bash -lc "just test-rust"
EOF

# Start kitty with session
kitty --session /tmp/kitty-ah-session.conf
```

**Session file syntax:**

- `new_tab <title>` - Create new tab
- `cd <path>` - Set working directory for tab
- `layout <name>` - Set layout (splits, tall, etc.)
- `launch <command>` - Create window with command
- `--location` - Position new window (hsplit, vsplit)

### Close Windows by Title or Pattern

Clean up windows programmatically:

```bash
# Close single window by title
kitty @ $TO close-window --match title:"Send-Test"

# Close all windows in a tab
kitty @ $TO close-tab --match title:"ah-task-demo"

# Close all windows matching pattern
for id in $(kitty @ $TO ls | jq -r '.[] | .tabs[] | .windows[] | select(.title | startswith("Test-")) | .id'); do
  kitty @ $TO close-window --match id:"$id"
done
```

**Warning:** Be careful with close commands; they don't ask for confirmation.

## References

1. Kitty Remote Control: <https://sw.kovidgoyal.net/kitty/remote-control/>
2. Kitty Overview: <https://sw.kovidgoyal.net/kitty/overview/>
3. Kitty Launch Command: <https://sw.kovidgoyal.net/kitty/launch/>
4. Kitty Remote Control Protocol: <https://sw.kovidgoyal.net/kitty/rc_protocol/>
5. Kitty Configuration: <https://sw.kovidgoyal.net/kitty/conf/>
6. Kitty GitHub Releases: <https://github.com/kovidgoyal/kitty/releases>
