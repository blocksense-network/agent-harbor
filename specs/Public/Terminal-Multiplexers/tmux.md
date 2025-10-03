# tmux — Integration Guide

This document describes how agents‑workflow integrates with tmux to create/focus multi‑pane layouts and drive interactive sessions non‑interactively.

## Overview

- Product: tmux (terminal multiplexer)
- Platforms: Linux, macOS, BSD; Windows via WSL
- Installation: system package manager (`apt install tmux`, `brew install tmux`)
- Primary interfaces: `tmux` CLI; server auto‑starts on first command

## Capabilities Summary

- Tabs/windows: `new-window`, `list-windows`, `select-window`.
- Splits/panes: `split-window -h|-v`, `select-pane`, `swap-pane`, `resize-pane`.
- Addressability: window and pane IDs, names, and titles (`rename-window`, `display -p`).
- Start commands per pane: `new-window -c <cwd> 'cmd'`, `split-window -c <cwd> 'cmd'`, or `send-keys` + Enter.
- Focus/activate: `select-window`, `select-pane`.
- Send keys / scripted answers: `send-keys` supports key names and strings.
- Startup layout: combine new-window + split-window to match TUI PRD.

References: tmux(1) manual: windows/panes, `split-window`, `send-keys`, `-c` for working directory. See man7.org and OpenBSD man pages. [1][2]

## Creating a New Tab With Split Layout

Example creates a window titled `ah-task-<id>` with editor (top‑left), TUI follower (top‑right), and logs (bottom):

```
SESSION=${SESSION:-ah}
TASK_ID=$1
TITLE="ah-task-${TASK_ID}"

# Ensure session exists (detached)
(tmux has-session -t "$SESSION" 2>/dev/null) || tmux new-session -d -s "$SESSION" -c "$PWD"

# Create window and layout
WIN=$(tmux new-window -P -t "$SESSION" -n "$TITLE" -c "$PWD")
tmux split-window -h -t "$WIN" -c "$PWD"            # right split
tmux select-pane -t "$WIN".1 && tmux split-window -v -t "$WIN" -c "$PWD"  # bottom split on left

# Launch commands
tmux send-keys -t "$WIN".1 "nvim ." C-m
tmux send-keys -t "$WIN".2 "ah tui --follow ${TASK_ID}" C-m
tmux send-keys -t "$WIN".3 "ah session logs ${TASK_ID} -f" C-m

# Focus TUI pane
tmux select-window -t "$WIN" && tmux select-pane -t "$WIN".2
```

Notes

- `-c <cwd>` sets per‑pane working directory. [1]
- You can start programs directly in `new-window`/`split-window` by passing a command after options. [2]

## Launching Commands in Each Pane

- Prefer starting the command as part of `new-window`/`split-window`:
  - `tmux split-window -v -c "$PWD" "ah tui --follow ${TASK_ID}"`
- Alternatively use `send-keys 'cmd' C-m` to simulate Enter. [2]

## Scripting Interactive Answers (Send Keys)

- Use `send-keys -t <target-pane> 'y' C-m` to confirm prompts, or named keys like `Escape`, `C-c`. [2]
- For complex sequences, send one token per call with small delays if needed.

## Focusing an Existing Task’s Pane/Window

```
TASK_ID=$1
WIN=$(tmux list-windows -a -F '#S:#I:#W' | awk -F: -v t="ah-task-${TASK_ID}" '$3==t{print $1":"$2; exit}')
[ -n "$WIN" ] && tmux select-window -t "$WIN"
```

## Programmatic Control Interfaces

- CLI only; tmux server listens on a UNIX socket selected via `TMUX`/`-L`/`-S`. [2]
- Exit codes reflect success; parse output with `display -p` for IDs. [2]

## Detection and Version Compatibility

- Detect via `command -v tmux`; verify `tmux -V`.
- The commands used here are stable across tmux 2.x+. Consult the man page on target systems if older. [1][2]

## Cross‑Platform Notes

- macOS: Homebrew tmux uses system `login` shell; `-c` sets cwd per pane. [1]
- Windows: Use within WSL; native Windows console not supported.

## Example: TUI Follow Flow

Combine the creation script above with the TUI control index to either focus an existing `ah-task-<id>` window or create it and then run `ah tui --follow <id>`.

## References

1. tmux(1) manual — panes/windows, `split-window`, `-c <start-directory>`: [man7 tmux(1)][1]
2. OpenBSD tmux man page — `send-keys`, `new-window`, `split-window`: [OpenBSD tmux(1)][2]

[1]: https://man7.org/linux/man-pages/man1/tmux.1.html
[2]: https://man.openbsd.org/tmux
