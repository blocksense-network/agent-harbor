# Zellij — Multiplexer Integration

Automate Zellij workspaces for agent-harbor tasks using its CLI and KDL-based configuration/layout files.

## Overview

- **Product and version(s)**: Zellij 0.43.1 (`zellij --version`) — terminal workspace / multiplexer.
- **Platforms**: Linux, macOS, BSD (Windows usually via WSL).
- **Install/config**: Open source (MIT); discovered on `PATH`. Per-user config under `~/.config/zellij` (KDL `config.kdl` plus `layouts/`).
- **Interfaces used**: `zellij` CLI (`--session`, `--layout`, `list-sessions`, `attach`), `zellij run`, and `zellij action` for pane/tab control.

## Capabilities Summary

- **Tabs/workspaces**: Multiple named tabs per session; addressable by index or name.
- **Splits**: Horizontal/vertical panes defined in KDL layouts or via `zellij action new-pane` and resize actions.
- **Addressability**: Sessions by name (`ah-<TASK_ID>`), tabs by index/name, panes primarily by focus (some actions accept pane ids).
- **Per-pane commands**: Declared in layouts (`pane { command "ah" args "tui" "--follow" "<TASK_ID>" }`) or launched with `zellij run -- ah tui --follow <TASK_ID>`.
- **Focus/activation**: Reattach with `zellij attach ah-<TASK_ID>`; within a session use `zellij action go-to-tab[‑name]` and `move-focus`.
- **Scripted input**: `zellij action write-chars` / `write` can send input to the focused pane, but non-interactive `ah` commands are preferred.
- **Startup layouts**: `--layout` / `--new-session-with-layout` load KDL layouts from files or named entries in `layouts/`.

## Creating a New Tab With Split Layout

Create an “agent coding session” layout per task with:

- Left: editor and logs split vertically.
- Right: `ah tui --follow <TASK_ID>` pane.

Example layout (`ah-task.kdl`):

```kdl
layout {
  cwd "{PROJECT_ROOT}"

  tab name="ah-{TASK_ID}" {
    pane size=1 {
      split_direction "vertical"
      pane { command "nvim" args "." }
      pane size=30% {
        command "ah"
        args "session" "logs" "{TASK_ID}" "-f"
      }
    }

    pane {
      command "ah"
      args "tui" "--follow" "{TASK_ID}"
    }
  }
}
```

Agent-harbor generates this from a template, substituting `{PROJECT_ROOT}` and `{TASK_ID}`.

To start or extend a session:

```bash
TASK_ID="abc123"
PROJECT_ROOT="/path/to/repo"

zellij --session "ah-${TASK_ID}" --layout ./ah-task.kdl
```

If the session exists, the layout adds tabs; otherwise, it creates a new session.

## Launching Commands in Each Pane

- **Layouts**: Each `pane` can set `command`, `args`, and optional `cwd`. Environment inherits from the `zellij` process (agent-harbor may inject `TASK_ID`, `PROJECT_ROOT`, etc.).
- **Runtime**: Use `zellij run` in an existing session:

```bash
TASK_ID="abc123"
zellij --session "ah-${TASK_ID}" run -- ah tui --follow "${TASK_ID}"
```

Use `--cwd` and `--direction` as needed; floating panes are available but optional for this integration.

## Scripting Interactive Answers (Send Keys)

- `zellij action write-chars <CHARS>` sends a UTF-8 string to the focused pane.
- `zellij action write <BYTES>...` sends raw bytes.

Example:

```bash
TASK_ID="abc123"
zellij --session "ah-${TASK_ID}" action write-chars "y\n"
```

**Guidance**:

- Prefer non-interactive `ah` commands (flags/config) over scripted keystrokes.
- If scripting is unavoidable, keep payloads short, escape shell-special characters carefully, and ensure focus is on the correct pane first.

## Focusing an Existing Task’s Pane/Window

1. **Attach session** (session naming convention: `ah-<TASK_ID>`):

```bash
TASK_ID="abc123"
zellij attach "ah-${TASK_ID}"
```

2. **Focus inside the session** via `zellij action`:

- Tabs: `zellij action go-to-tab <INDEX>` or `zellij action go-to-tab-name <NAME>`.
- Panes: `zellij action move-focus <left|right|up|down>` or `focus-next-pane` / `focus-previous-pane`.

Layouts should give task-related tabs stable names (e.g. `name="ah-{TASK_ID}"`) so `go-to-tab-name` can be used reliably.

Bringing the terminal window to the foreground is handled by agent-harbor’s OS-specific mechanisms, not Zellij.

## Programmatic Control Interfaces

- **Session lifecycle**: `zellij list-sessions`, `attach`, `delete-session`, `kill-session`, `delete-all-sessions`, `kill-all-sessions`.
- **Layouts**: `--layout`, `--new-session-with-layout` for applying layouts.
- **Panes/tabs**: `zellij run`, `zellij action new-pane`, `close-pane`, `resize`, `new-tab`, `close-tab`, `go-to-next-tab`, `go-to-previous-tab`.
- **Debugging**: `zellij action dump-screen` and `dump-layout` for diagnostics.

This is sufficient for agent-harbor; advanced features like `zellij pipe`, plugins, and `zellij web` are optional and can be considered separately.

## Detection and Version Compatibility

- **Detection**: Run `zellij --version`; optionally `zellij list-sessions` to confirm it works.
- **Minimum version**: Target Zellij 0.43.x (validated on 0.43.1). Required features:
  - KDL config/layouts.
  - `zellij run` with `--cwd` / `--direction`.
  - `zellij action` with `write`, `write-chars`, `new-tab`, `new-pane`, `go-to-tab-name`.
- **If unsupported**: Disable Zellij integration and show a message indicating the required minimum version.

## Cross-Platform Notes

- **macOS**: Runs in standard terminals; bringing windows to front requires external automation (AppleScript, etc.).
- **Linux/BSD**: Terminal-agnostic; window focusing is WM/Wayland/X11 specific and handled outside Zellij.
- **Windows**: Typically used via WSL; treat as unsupported for direct Zellij control unless explicitly configured.

## Example: TUI Follow Flow

Minimal end-to-end flow to create or focus a task session and start `ah tui --follow <TASK_ID>`:

```bash
TASK_ID="abc123"
PROJECT_ROOT="/path/to/repo"
SESSION_NAME="ah-${TASK_ID}"

if zellij list-sessions | grep -q "^${SESSION_NAME}\b"; then
  zellij attach "${SESSION_NAME}"
else
  cat > /tmp/ah-task.kdl <<'EOF'
layout {
  cwd "{PROJECT_ROOT}"
  tab name="ah-{TASK_ID}" {
    pane size=1 {
      split_direction "vertical"
      pane { command "nvim" args "." }
      pane { size=30% command "ah" args "session" "logs" "{TASK_ID}" "-f" }
    }
    pane { command "ah" args "tui" "--follow" "{TASK_ID}" }
  }
}
EOF

  sed -i "s|{PROJECT_ROOT}|${PROJECT_ROOT}|g" /tmp/ah-task.kdl
  sed -i "s|{TASK_ID}|${TASK_ID}|g" /tmp/ah-task.kdl

  zellij --session "${SESSION_NAME}" --layout /tmp/ah-task.kdl
fi
```

In real code, agent-harbor should use a proper templating mechanism rather than inline `sed`.

## References

- Zellij documentation — commands: `https://zellij.dev/documentation/commands.html`
- Zellij documentation — creating a layout (KDL): `https://zellij.dev/documentation/creating-a-layout.html`
- Zellij site and documentation: `https://zellij.dev`
- Zellij manual (actions, modes, etc.): `https://man.archlinux.org/man/zellij.1.en`
