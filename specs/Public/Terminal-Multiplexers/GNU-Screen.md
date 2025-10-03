# GNU Screen — Integration Guide

Automate GNU Screen sessions, windows, and regions using its CLI and `-X` command interface.

## Overview

- Product: GNU Screen (terminal multiplexer)
- Platforms: Linux, macOS, BSD
- Install: system packages
- Interfaces: `screen` CLI (sessions, windows), `-X` to send commands to a running session, configuration. [1][2]

## Capabilities Summary

- Sessions/windows/regions: create detached sessions, multiple windows (akin to tabs), split regions vertical/horizontal. [1][3]
- Start commands per window: start detached with a command or create a new window running a command. [2]
- Focus/activate: select windows/regions with commands. [2][3]
- Send keys/text: `screen -X stuff 'text\n'` to inject input into the focused window. [4]

## Creating a New Tab With Split Layout

Example creates a session `ah-<id>`, splits the screen vertically, then opens a bottom region in the left side for logs.

```
TASK_ID=ABC
SESSION="ah-${TASK_ID}"

# Start detached session with editor in window 0
screen -dmS "$SESSION" bash -lc 'nvim .'

# Split vertically (left/right) and create a new region on the right running the TUI follower
screen -S "$SESSION" -X split -v
screen -S "$SESSION" -X focus right
screen -S "$SESSION" -X screen bash -lc "ah tui --follow ${TASK_ID}"

# Focus left, split horizontally for logs, run tailing logs
screen -S "$SESSION" -X focus left
screen -S "$SESSION" -X split
screen -S "$SESSION" -X focus down
screen -S "$SESSION" -X screen bash -lc "ah session logs ${TASK_ID} -f"

# Return focus to the TUI (right region)
screen -S "$SESSION" -X focus up
screen -S "$SESSION" -X focus right
```

Notes

- Vertical splits require Screen with vertical split support (common in modern builds). [3]

## Launching Commands in Each Pane

- Create a new window in the current region: `screen -S <session_id> -X screen bash -lc '<cmd>'`. [2]
- Or start the session detached with a command: `screen -dmS <session_id> bash -lc '<cmd>'`. [2]

## Scripting Interactive Answers (Send Keys)

- Use `screen -S <session_id> -X stuff 'y\n'` to send keystrokes to the focused window. Quote and escape carefully. [4]

## Focusing an Existing Task’s Pane/Window

- Reattach with `screen -r ah-<id>`; switch windows with `select` or cycle focus between regions with `focus`. [2]

## Programmatic Control Interfaces

- `-X <command>` to issue Screen commands to a session (e.g., `split`, `focus`, `screen`, `select`, `remove`, `only`). [2][3]
- Configuration can predefine key bindings and layouts; scripts can rely on commands for deterministic control. [1][2]

## Detection and Version Compatibility

- Detect via `screen --version`.
- Commands used above are from the official GNU Screen manual (`screen`, `select`, `split`, `focus`, `stuff`). [1][2][3][4]

## Cross‑Platform Notes

- Works across Unix-like systems. Behavior of shells and path quoting may differ slightly by platform.

## Example: TUI Follow Flow

Create or reattach the `ah-<id>` session, set up regions with `split`/`focus`, then create windows with editor/TUI/logs via `screen -X screen bash -lc '<cmd>'`.

## References

1. GNU Screen Manual — Introduction and Overview: [GNU Screen manual index][1]
2. GNU Screen Manual — Screen Command, Select, Screen (create window), Reattach: [GNU Screen commands][2]
3. GNU Screen Manual — Split command and regions: [GNU Screen split/regions][3]
4. GNU Screen Manual — stuff (send input to window): [GNU Screen stuff][4]

[1]: https://man7.org/linux/man-pages/man1/screen.1.html#DESCRIPTION
[2]: https://man7.org/linux/man-pages/man1/screen.1.html#COMMAND-LINE_OPTIONS
[3]: https://man7.org/linux/man-pages/man1/screen.1.html#WINDOW_TYPES
[4]: https://man7.org/linux/man-pages/man1/screen.1.html#STRING_ESCAPES
