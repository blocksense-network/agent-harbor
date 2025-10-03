# WezTerm — Integration Guide

This document describes automating WezTerm layouts and pane commands via the `wezterm cli`.

## Overview

- Product: WezTerm (GPU-accelerated terminal)
- Platforms: Linux, macOS, Windows
- Install: official releases; Homebrew/winget packages
- Interfaces: `wezterm cli` subcommands (`spawn`, `split-pane`, `send-text`, `list`, `activate`)

## Capabilities Summary

- Tabs/windows: `spawn --new-window`, tab titles, workspaces. [1]
- Splits/panes: `split-pane --right|--bottom --percent <int>`. [1]
- Addressability: window_id, pane_id, titles via `list --format json`. [1]
- Start commands per pane: append command after CLI; or `send-text`. [1]
- Focus/activate: `activate --window-id`, `activate-pane --pane-id`. [1]
- Send keys/text: `send-text --no-paste -- "cmd && exit"`. [1]

## Creating a New Tab With Split Layout

```
TASK_ID=$1
TITLE="ah-task-${TASK_ID}"

# New window running editor
wezterm cli spawn --new-window --cwd "$PWD" -- bash -lc "printf '\e]2;%s\a' '$TITLE'; nvim ."

# Right split for TUI (60%)
wezterm cli split-pane --right --percent 60 -- bash -lc "ah tui --follow ${TASK_ID}"

# Bottom split for logs (30%) in the left pane
wezterm cli activate-pane-direction left
wezterm cli split-pane --bottom --percent 30 -- bash -lc "ah session logs ${TASK_ID} -f"

# Optionally focus the TUI pane
wezterm cli activate-pane-direction right
```

Notes

- Title set via OSC 2 so later discovery by title is possible.
- You can also target by explicit `--window-id`/`--pane-id` from `wezterm cli list`. [1]

## Launching Commands in Each Pane

- Append `bash -lc '<command>'` after `spawn`/`split-pane` to run non-interactively. [1]
- Alternatively use `send-text --no-paste -- 'command'` followed by `send-text -- "\r"`. [1]

## Scripting Interactive Answers (Send Keys)

- `send-text` inserts literal text into a pane’s PTY. Use `--no-paste` to simulate typing; add `\r` to submit. [1]

## Focusing an Existing Task’s Pane/Window

```
TASK_ID=$1
WINDOW_ID=$(wezterm cli list --format json | jq -r '.[] | select(.title=="ah-task-'"${TASK_ID}"'") | .window_id' | head -n1)
[ -n "$WINDOW_ID" ] && wezterm cli activate --window-id "$WINDOW_ID"
```

## Programmatic Control Interfaces

- CLI only; returns non‑zero on failure. `list --format json` is script‑friendly. [1]

## Detection and Version Compatibility

- Detect via `wezterm --version`.
- The commands referenced are documented in current stable docs and `wezterm --help`. Some flags (e.g., `activate-pane`) require recent builds (2023+). Verify with your installed version. [1]

## Cross‑Platform Notes

- Windows: use `powershell -NoLogo -NoExit -Command` in place of `bash -lc`.
- macOS/Linux: `bash -lc` ensures shell init and PATH.

## Example: TUI Follow Flow

Use the layout recipe above; store `window_id`/`pane_id` in the TUI control index for reuse; otherwise compute from `list`.

## References

1. WezTerm — official site and documentation: [WezTerm homepage][1]

[1]: https://wezterm.org
