# WezTerm — Multiplexer Integration Spec

This specification documents how the agent-harbor workflows integrate with the WezTerm GPU‑accelerated terminal emulator and built‑in multiplexer. It follows the shared Multiplexer Description Template to enable reliable automated session layout creation, pane targeting, key injection and task focusing across platforms.

## Overview

- Product name & purpose: **WezTerm** – cross‑platform GPU accelerated terminal emulator with native tab/window management, panes (splits), workspaces and an embedded multiplexer reachable via CLI and Lua API.
- Versions tested: 2024+ nightly/stable (features: `cli split-pane`, `activate-pane-direction`, workspaces, unix domains). Minimum recommended: `20230408` or later for stable pane activation semantics.
- Platforms: Linux, macOS, Windows (also FreeBSD, NetBSD; tested focus logic on the first three).
- License: MIT; binary locations typically: macOS Homebrew `$(brew --prefix)/bin/wezterm`, Linux package manager or AppImage, Windows installer places `wezterm.exe` in `%LOCALAPPDATA%\wezterm\`. Discover via `command -v wezterm` (POSIX) / `where wezterm` (Windows).
- Primary control interfaces: `wezterm` top‑level subcommands (`start`, `cli`, `ssh`, `serial`, `connect`) and the experimental multiplexing CLI (`wezterm cli <subcommand>`). Secondary: Lua scripting in `wezterm.lua` exposing mux/window/pane objects.

**Configuration Required:**

WezTerm CLI operations work out-of-the-box without additional configuration, but for optimal agent-harbor integration, add these settings to `~/.config/wezterm/wezterm.lua` (or `%APPDATA%\wezterm\wezterm.lua` on Windows):

```lua
local wezterm = require 'wezterm'
local config = {}

-- Enable CLI access (default: true, but explicit for clarity)
config.enable_cli = true

-- Increase scrollback for log monitoring
config.scrollback_lines = 10000

-- Disable confirmation dialogs for automation
config.skip_close_confirmation_for_processes_named = {
  'bash', 'sh', 'zsh', 'fish', 'tmux', 'nvim', 'vim'
}

-- Set default workspace for task isolation
config.default_workspace = "main"

-- Optional: Enable multiplexer for remote connections
-- config.unix_domains = {
--   { name = 'unix' }
-- }
-- config.default_gui_startup_args = { 'connect', 'unix' }

return config
```

**How it works:**

- `enable_cli = true` - Ensures CLI commands work (default behavior, explicit for documentation)
- `scrollback_lines = 10000` - Larger scrollback for monitoring logs and command output
- `skip_close_confirmation_for_processes_named` - Prevents confirmation dialogs when closing panes programmatically
- `default_workspace = "main"` - Sets consistent workspace naming for task isolation
- Unix domains (commented) - Optional multiplexer support for remote/detached sessions

Unlike some terminals, WezTerm's CLI works immediately without socket setup - commands run directly against the GUI process. For security-sensitive environments, consider restricting CLI access via Lua event handlers or running in isolated workspaces.

## Capabilities Summary

- Tabs & Workspaces: Yes – spawn tabs in existing window or new window; workspaces namespace sets of tabs; switch via `wezterm cli list` + `activate-tab` or Lua `mux.set_active_workspace`.
- Horizontal/Vertical splits: Yes – `wezterm cli split-pane --right|--left|--bottom|--top --percent <int>` or keyboard actions mapped to `SplitPane`/`SplitHorizontal`/`SplitVertical`.
- Addressability: Window, Tab, Pane IDs exposed through `wezterm cli list --format json`; titles set via OSC 2 or `set-tab-title` / `set-window-title`; workspaces enumerated with `get_workspace_names` (Lua) or `list` output.
- Auto‑starting commands: Provide command after `spawn`/`split-pane` (it execs in the new PTY). Alternative: spawn empty then `send-text` + carriage return.
- Focus/activate existing pane: `activate-pane-direction`, `activate-pane --pane-id`, `activate-tab --tab-id`, `activate --window-id`; Lua: `pane:activate()`, `tab:activate()`, `window:focus()`.
- Send keys / scripted answers: `send-text` (literal bytes), `--no-paste` to simulate typing timing; OSC & Paste operations; Lua `pane:send_text()` for programmatic injection.
- Startup layout recipe: Script constructs window, editor pane (left), task TUI pane (right 60%), log pane (bottom left 30%). See detailed example below.

## Creating a New Tab With Split Layout

Goal: For task `<TASK_ID>` create/focus a workspace `task-<TASK_ID>` with:

1. Main editor (Neovim) pane left (remaining vertical space after bottom split)
2. TUI follow pane right (60% width)
3. Log tail pane bottom-left (30% height of left region)

Script (POSIX bash):

```bash
#!/usr/bin/env bash
set -euo pipefail

TASK_ID="${1:?Task ID required}"
TITLE="ah-task-${TASK_ID}"
WORKDIR="${2:-$PWD}"

# Ensure wezterm and jq present
command -v wezterm >/dev/null || { echo "Error: wezterm not found" >&2; exit 2; }
command -v jq >/dev/null || { echo "Error: jq required for JSON parsing" >&2; exit 3; }

# Test CLI connectivity
if ! wezterm cli list >/dev/null 2>&1; then
  echo "Error: WezTerm CLI not accessible" >&2
  echo "Ensure WezTerm is running and CLI is enabled" >&2
  exit 4
fi

echo "Creating layout for task: $TASK_ID in $WORKDIR"

# Spawn new window (tab 0) running editor; set window & tab title via OSC 2
echo "Creating editor pane..."
if ! WEZ_SPAWN_JSON=$(wezterm cli spawn --new-window --cwd "$WORKDIR" -- bash -lc "printf '\e]2;%s\a' '$TITLE'; nvim ."); then
  echo "Error: Failed to create editor pane" >&2
  exit 5
fi

WINDOW_ID=$(echo "$WEZ_SPAWN_JSON" | jq -r '.window_id // empty')
PANE_ID_EDITOR=$(echo "$WEZ_SPAWN_JSON" | jq -r '.pane_id // empty')

if [[ -z "$WINDOW_ID" || -z "$PANE_ID_EDITOR" ]]; then
  echo "Error: Failed to extract window/pane IDs from spawn response" >&2
  echo "Response: $WEZ_SPAWN_JSON" >&2
  exit 6
fi

echo "Created window $WINDOW_ID with editor pane $PANE_ID_EDITOR"

# Split editor pane to create right TUI pane (60%)
echo "Creating TUI pane..."
if ! WEZ_TUI_JSON=$(wezterm cli split-pane --right --percent 60 --pane-id "$PANE_ID_EDITOR" -- bash -lc "ah tui --follow ${TASK_ID}"); then
  echo "Error: Failed to create TUI pane" >&2
  exit 7
fi

PANE_ID_TUI=$(echo "$WEZ_TUI_JSON" | jq -r '.pane_id // empty')
[[ -n "$PANE_ID_TUI" ]] || { echo "Error: Failed to extract TUI pane ID" >&2; exit 8; }
echo "Created TUI pane $PANE_ID_TUI"

# Create bottom log pane off the editor (left) pane (30%)
echo "Creating logs pane..."
if ! WEZ_LOG_JSON=$(wezterm cli split-pane --bottom --percent 30 --pane-id "$PANE_ID_EDITOR" -- bash -lc "ah session logs ${TASK_ID} -f"); then
  echo "Error: Failed to create logs pane" >&2
  exit 9
fi

PANE_ID_LOG=$(echo "$WEZ_LOG_JSON" | jq -r '.pane_id // empty')
[[ -n "$PANE_ID_LOG" ]] || { echo "Error: Failed to extract logs pane ID" >&2; exit 10; }
echo "Created logs pane $PANE_ID_LOG"

# Focus TUI pane at end
echo "Focusing TUI pane..."
wezterm cli activate-pane --pane-id "$PANE_ID_TUI" || echo "Warning: Failed to focus TUI pane" >&2

echo "Layout created successfully:"
echo "  Window: $WINDOW_ID"
echo "  Editor: $PANE_ID_EDITOR"
echo "  TUI:    $PANE_ID_TUI"
echo "  Logs:   $PANE_ID_LOG"

# Optional: Save pane IDs for later reference
PANE_STATE_DIR="${XDG_RUNTIME_DIR:-/tmp}/ah/task/${TASK_ID}"
mkdir -p "$PANE_STATE_DIR"
cat > "$PANE_STATE_DIR/wezterm.json" <<EOF
{
  "task_id": "$TASK_ID",
  "window_id": "$WINDOW_ID",
  "panes": {
    "editor": "$PANE_ID_EDITOR",
    "tui": "$PANE_ID_TUI",
    "logs": "$PANE_ID_LOG"
  },
  "created": "$(date -Iseconds)"
}
EOF
echo "Pane state saved to: $PANE_STATE_DIR/wezterm.json"
```

Notes:

- We capture JSON responses from `spawn` and `split-pane` (they return object with `pane_id`, `window_id`, `tab_id`).
- Titles: Use OSC 2 or later adjust with `set-tab-title`/`set-window-title`.
- jq dependency required for JSON parsing; if absent fall back to `list --format json` filtering.
- For repeated invocations, first attempt to focus existing session (`activate --window-id`). See focusing section.

## Launching Commands in Each Pane

- Inline Exec: Append the command after `spawn`/`split-pane`; WezTerm runs it as the child process of that PTY. Use a login shell wrapper (`bash -lc`, `zsh -l -c`) to load environment including Nix dev shell and `PATH`.
- Environment propagation: Shell inherits parent environment; modify via config `set_environment_variables` or use wrappers.
- CWD control: `--cwd <path>` on `spawn`; for splits, current pane’s CWD is inherited unless overridden by preceding `--cwd` and `bash -lc 'cd ... && ...'`.
- Non‑interactive injection: `wezterm cli send-text --no-paste --pane-id <id> 'command && echo done' && wezterm cli send-text --pane-id <id> "\r"`.

## Scripting Interactive Answers (Send Keys)

- Primary method: `send-text` – sends raw bytes; `--no-paste` simulates typing (may trigger shell keybindings sensitive to input rate).
- Carriage return: always append a separate `\r` call or include `\r` within string if not ambiguous.
- Escaping: Wrap in single quotes; for complex quoting use Lua `shell_join_args` to produce safe command lines.
- Timing: For prompts requiring processing time, use `sleep 0.2` between injections or `wezterm.gui.sleep_ms()` in Lua event handlers.
- Copy/Paste alternative: `--pane-id <id> -- "long multiline script\nsecond line\n"` (paste mode). Avoid for password prompts; rely on simulated typing to preserve bracketed paste disable semantics.

## Focusing an Existing Task’s Pane/Window

Workflow:

1. Enumerate sessions: `wezterm cli list --format json` → objects with `tabs[]`, `panes[]`, `window_id`, `pane_id`, titles.
2. Filter by window or tab title (OSC 2 set earlier) or workspace name.
3. Activate window then optionally specific pane.

Example (focus by title):

```bash
TASK_ID="$1"
WINDOW_ID=$(wezterm cli list --format json | jq -r '.[] | select(.title=="ah-task-'"${TASK_ID}"'") | .window_id' | head -n1)
if [ -n "$WINDOW_ID" ]; then
 wezterm cli activate --window-id "$WINDOW_ID" || true
 # Optionally zoom active pane
 ACTIVE_PANE=$(wezterm cli list --format json | jq -r '.[] | select(.window_id=="'$WINDOW_ID'") | .pane_id' | head -n1)
 [ -n "$ACTIVE_PANE" ] && wezterm cli zoom-pane --pane-id "$ACTIVE_PANE" --toggle
fi
```

Foreground (OS focus):

- macOS: `wezterm` window focus is native; nothing extra required; AppleScript fallback: `osascript -e 'tell application "WezTerm" to activate'` if CLI activation insufficient.
- Linux: When Wayland focus inhibitors present, CLI activation works; to raise window in X11 use `wmctrl -ia <X11_WINDOW_ID>` if integrated (WezTerm returns mux IDs not X11 IDs – optional advanced integration).
- Windows: CLI activates mux window; to ensure foreground use `powershell -Command "(New-Object -ComObject WScript.Shell).AppActivate('WezTerm')"` if needed.

## Programmatic Control Interfaces

CLI Subcommands (core for automation):

- `spawn` (create window/tab/pane with command, returns JSON)
- `split-pane` (split existing pane specifying direction & percent)
- `activate-pane-direction` / `activate-pane` / `activate-tab` / `activate` (focus operations)
- `list` / `list-clients` (enumeration)
- `send-text` (inject text)
- `adjust-pane-size`, `zoom-pane`, `kill-pane`, `move-pane-to-new-tab`
- `set-tab-title`, `set-window-title`, `rename-workspace`

Exit codes: Non‑zero indicates failure (e.g., invalid pane id). Always check and retry enumeration if object not found.

Lua API (when customizing `wezterm.lua`): `wezterm.mux` for enumerating domains/windows/tabs/panes; event hooks (`mux-startup`, `gui-startup`) can pre‑create canonical layout or register command palette entries.

Security considerations:

- Configuration file (`wezterm.lua`) is executable Lua; treat repository‑sourced configs as code (review before enabling, avoid executing untrusted code).
- Remote domains via SSH/TLS rely on OpenSSL/Schannel; enforce host key verification; avoid embedding plaintext secrets in `ssh_domains` definitions.
- `send-text` can inject arbitrary commands into panes – ensure targeted pane belongs to the expected task workspace; validate by checking foreground process name if needed (`pane:get_foreground_process_name()`).

## Detection and Version Compatibility

Detection algorithm for agent-harbor:

1. `command -v wezterm` – if absent mark multiplexer unsupported.
2. `wezterm --version` – parse semantic/build id.
3. Feature gates:

- Pane operations require `cli` subcommand present (verify `wezterm --help | grep cli`).
- Workspaces (default workspace naming) stable after mid‑2023 – if earlier, degrade to tab layout without workspace isolation.

4. JSON output presence: ensure `wezterm cli list --format json` returns valid JSON before relying on ID extraction.

## Cross‑Platform Notes

- macOS: Use `native_macos_fullscreen_mode=false` if needing fast splits in fullscreen; app bundle path may differ – favor `command -v` result. Pasteboard integration stable; for security sensitive tasks disable automatic paste mode by always using `--no-paste`.
- Linux: Wayland vs X11 – pane/key operations unaffected; window raise may need WM tools (optional). Ensure `ulimit_nofile` sufficient for large task sets (WezTerm config option).
- Windows: ConPTY backend; prefer `bash -lc` inside WSL domain or use `default_prog = { "powershell" }`. Use `SplitPane` key assignments if CLI percent granularity differs; quoting commands uses Windows rules – inside WSL treat as Linux.
- Remote Domains: unix socket multiplexing across WSL and Windows native GUI requires `unix_domains` config and potential `default_gui_startup_args = {'connect','unix'}`.

## Example: TUI Follow Flow (Create or Focus)

```bash
#!/usr/bin/env bash
TASK_ID="$1"; [ -n "$TASK_ID" ] || { echo "Need TASK_ID"; exit 1; }
TITLE="ah-task-${TASK_ID}"

if wezterm cli list --format json | jq -e '.[] | select(.title=="'$TITLE'")' >/dev/null 2>&1; then
 # Focus existing window
 WIN=$(wezterm cli list --format json | jq -r '.[] | select(.title=="'$TITLE'") | .window_id' | head -n1)
 wezterm cli activate --window-id "$WIN"
else
 # Create layout fresh
 ah-wez-layout "$TASK_ID"  # wrapper for layout script from earlier section
fi
```

Extend with logging: record pane IDs to a state file (`$XDG_RUNTIME_DIR/ah/task/<TASK_ID>/wezterm.json`) for faster targeting.

## Additional Examples

### Launch with Environment Variables

Set custom environment variables for a pane:

```bash
# Method 1: Via shell wrapper
wezterm cli spawn --new-tab --cwd "$PWD" -- bash -lc '
  export TASK_ID=12345
  export DEBUG=true
  export LOG_LEVEL=verbose
  echo "TASK_ID=$TASK_ID DEBUG=$DEBUG LOG_LEVEL=$LOG_LEVEL"
  bash -l
'

# Method 2: Via wezterm.lua config (set_environment_variables)
# Add to wezterm.lua:
# config.set_environment_variables = {
#   TASK_ID = "12345",
#   DEBUG = "true"
# }
```

### Send Text to Pane (Simulating User Input)

Automate command execution in a pane:

```bash
# Create a pane
PANE_JSON=$(wezterm cli spawn --new-tab -- bash -l)
PANE_ID=$(echo "$PANE_JSON" | jq -r '.pane_id')

sleep 1

# Send commands (note the \r for Enter key)
wezterm cli send-text --pane-id "$PANE_ID" "echo 'Hello from automation'"
wezterm cli send-text --pane-id "$PANE_ID" "\r"
sleep 0.5
wezterm cli send-text --pane-id "$PANE_ID" "date\r"
sleep 0.5
wezterm cli send-text --pane-id "$PANE_ID" "pwd\r"
```

### Send Key Combinations (Vim Example)

Control applications with key sequences:

```bash
# Create pane with vim
PANE_JSON=$(wezterm cli spawn --new-tab --cwd /tmp -- vim test-wezterm.txt)
PANE_ID=$(echo "$PANE_JSON" | jq -r '.pane_id')

sleep 1

# Enter insert mode and type text
wezterm cli send-text --pane-id "$PANE_ID" "iHello from WezTerm automation"
# Send Escape key (exit insert mode)
wezterm cli send-text --pane-id "$PANE_ID" "\e"
sleep 0.5

# Save and quit
wezterm cli send-text --pane-id "$PANE_ID" ":wq\r"
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

  # Check if window/tab exists by title
  if wezterm cli list --format json | jq -e '.[] | select(.title=="'$title'")' >/dev/null 2>&1; then
    echo "Task exists, focusing..."
    local window_id=$(wezterm cli list --format json | jq -r '.[] | select(.title=="'$title'") | .window_id' | head -n1)
    wezterm cli activate --window-id "$window_id"
  else
    echo "Task doesn't exist, creating..."
    # Use the enhanced layout script from above
    ah-wez-layout "$task_id"
  fi
}

# Usage
focus_or_create_task "project-123"
```

### List All Windows and Panes

Query WezTerm structure programmatically:

```bash
# Get full JSON structure
wezterm cli list --format json

# Pretty-print with jq
wezterm cli list --format json | jq -r '
  .[] |
  "Window \(.window_id): \(.title // "Untitled")" as $win |
  .tabs[] |
  "\($win)\n  Tab \(.tab_id): \(.title // "Untitled")" as $tab |
  .panes[] |
  "\($tab)\n    Pane \(.pane_id): [\(.width)x\(.height)] \(.title // "Untitled")"
'

# List just pane titles
wezterm cli list --format json | jq -r '.[] | .tabs[] | .panes[] | .title // "Untitled"'

# Find pane by title and get its ID
wezterm cli list --format json | jq -r '.[] | .tabs[] | .panes[] | select(.title == "Editor") | .pane_id'
```

### Get Pane ID and Use It

Create pane and capture its ID for later use:

```bash
# Launch pane and capture JSON output
output=$(wezterm cli spawn --new-tab -- bash -l)

# Extract pane ID from JSON response
pane_id=$(echo "$output" | jq -r '.pane_id // empty')

if [ -n "$pane_id" ]; then
  echo "Created pane with ID: $pane_id"

  # Use the ID directly (more reliable than title matching)
  wezterm cli send-text --pane-id "$pane_id" "echo 'Using pane ID: $pane_id'\r"

  # Get pane dimensions
  dimensions=$(wezterm cli list --format json | jq -r '.[] | .tabs[] | .panes[] | select(.pane_id == "'$pane_id'") | "\(.width)x\(.height)"')
  echo "Pane dimensions: $dimensions"
fi
```

### Working with Workspaces

Manage multiple isolated task environments:

```bash
# Create workspace for specific project
wezterm cli spawn --new-window --workspace "project-alpha" -- bash -l

# Switch to different workspace
wezterm cli spawn --new-window --workspace "project-beta" -- bash -l

# List workspaces (via Lua - requires wezterm.lua config)
# In wezterm.lua, add keybinding:
# config.keys = {
#   { key = 'w', mods = 'CTRL|SHIFT', action = wezterm.action.ShowLauncherArgs { flags = 'WORKSPACES' } }
# }

# Programmatically switch workspace (requires Lua integration)
# Use send-text to trigger workspace switcher or implement in Lua events
```

### Close Panes by ID or Pattern

Clean up panes programmatically:

```bash
# Close single pane by ID
wezterm cli kill-pane --pane-id "$PANE_ID"

# Close all panes matching pattern (careful!)
for id in $(wezterm cli list --format json | jq -r '.[] | .tabs[] | .panes[] | select(.title | test("^Test-")) | .pane_id'); do
  echo "Closing pane: $id"
  wezterm cli kill-pane --pane-id "$id"
done
```

**Warning:** Kill commands don't ask for confirmation; ensure correct targeting.

### Monitor Pane Status

Check if processes are running and healthy:

```bash
monitor_pane() {
  local pane_id="$1"
  local expected_process="$2"

  # Get current process info (requires recent WezTerm version)
  local pane_info=$(wezterm cli list --format json | jq -r '.[] | .tabs[] | .panes[] | select(.pane_id == "'$pane_id'")')

  if [ -n "$pane_info" ]; then
    local title=$(echo "$pane_info" | jq -r '.title // "Untitled"')
    echo "Pane $pane_id ($title) is active"

    # Check if expected process is running (basic check)
    if wezterm cli send-text --pane-id "$pane_id" "\r" 2>/dev/null; then
      echo "Pane is responsive"
    else
      echo "Warning: Pane may be unresponsive"
    fi
  else
    echo "Error: Pane $pane_id not found"
    return 1
  fi
}

# Usage
monitor_pane "$PANE_ID_TUI" "ah"
```

## Additional Operational Guidance

- **Resilience:** If a pane command fails (non‑zero exit) the PTY stays – consider monitoring foreground process; if dead, re‑run command via `send-text`.
- **Zoom toggling:** `wezterm cli zoom-pane --pane-id <id> --toggle` for temporarily enlarging TUI.
- **Workspace naming:** prefer `task-<id>` over raw `<id>` to avoid collisions with other project domains.
- **Metrics/log scraping:** Use `get-text` for capturing last N lines but beware performance for large scrollback; alternative is direct log files.
- **State management:** Save pane/window IDs to JSON files for quick retrieval across script invocations.
- **Error handling:** Always check JSON parsing results; WezTerm CLI returns structured error information.
- **Performance:** For high-frequency operations, batch commands or use Lua event handlers instead of repeated CLI calls.
- **Debugging:** Use `wezterm cli list --format json | jq` to inspect current state when automation fails.

## Troubleshooting Common Issues

### "CLI not accessible" errors

```bash
# Check if WezTerm is running
ps aux | grep wezterm

# Test basic CLI connectivity
wezterm cli list 2>&1

# If CLI fails, check WezTerm version and restart
wezterm --version
# Kill and restart WezTerm, then retry
```

### JSON parsing failures

```bash
# Check if jq is installed
command -v jq || echo "Install jq: brew install jq (macOS) or apt install jq (Linux)"

# Validate JSON output manually
wezterm cli list --format json | jq . || echo "Invalid JSON output"

# Fallback parsing without jq (basic extraction)
wezterm cli list --format json | grep -o '"pane_id":[0-9]*' | cut -d: -f2
```

### Pane creation failures

```bash
# Check current working directory permissions
ls -la "$PWD"

# Test with explicit absolute path
wezterm cli spawn --new-tab --cwd /tmp -- bash -l

# Check available resources
wezterm cli list --format json | jq length  # Count current panes
```

### Focus/activation not working

```bash
# Verify window/pane exists
wezterm cli list --format json | jq '.[] | {window_id, panes: [.tabs[].panes[].pane_id]}'

# Try different focus methods
wezterm cli activate --window-id "$WINDOW_ID"  # Window level
wezterm cli activate-pane --pane-id "$PANE_ID"  # Pane level

# Check if WezTerm window is minimized/hidden
# On macOS: osascript -e 'tell application "WezTerm" to activate'
```

### Performance issues with large layouts

```bash
# Reduce scrollback in config
# config.scrollback_lines = 1000  -- instead of 10000+

# Use targeted operations instead of full list scans
wezterm cli list --format json | jq '.[] | select(.window_id == "'$WINDOW_ID'")'

# Consider workspace isolation for large task counts
wezterm cli spawn --workspace "task-$TASK_ID" --new-window
```

## References

### Official Documentation

- Homepage & feature overview: <https://wezterm.org/>
- Installation guide: <https://wezterm.org/installation.html>
- Configuration files & Lua API: <https://wezterm.org/config/files.html>
- CLI Reference (complete): <https://wezterm.org/cli/cli/index.html>
- Multiplexing & domains: <https://wezterm.org/multiplexing.html>
- Key assignments (Lua): <https://wezterm.org/config/lua/keyassignment/>
- Mux API documentation: <https://wezterm.org/config/lua/wezterm.mux/>
- Pane object methods: <https://wezterm.org/config/lua/pane/>
- Window & GUI APIs: <https://wezterm.org/config/lua/window/>

### CLI Command Reference

- spawn: <https://wezterm.org/cli/cli/spawn.html>
- split-pane: <https://wezterm.org/cli/cli/split-pane.html>
- send-text: <https://wezterm.org/cli/cli/send-text.html>
- list: <https://wezterm.org/cli/cli/list.html>
- activate commands: <https://wezterm.org/cli/cli/activate-pane.html>

### Community & Advanced Usage

- Workspaces recipe: <https://wezterm.org/recipes/workspaces.html>
- SSH domains setup: <https://wezterm.org/ssh.html>
- Color schemes: <https://wezterm.org/colorschemes/index.html>
- Font configuration: <https://wezterm.org/config/fonts.html>
- GitHub repository: <https://github.com/wez/wezterm>
- Community discussions: <https://github.com/wez/wezterm/discussions>
