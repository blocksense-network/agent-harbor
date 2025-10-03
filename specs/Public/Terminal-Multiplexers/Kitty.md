# Kitty — Integration Guide

This document describes automating Kitty via its remote‑control interface `kitty @`.

## Overview

- Product: Kitty (fast, GPU‑based terminal)
- Platforms: Linux, macOS
- Install: official binary or package manager
- Interfaces: remote control via `kitty @` over a UNIX socket; controlled via `--to`/`KITTY_LISTEN_ON`. [1]

## Capabilities Summary

- Tabs/windows: `launch --type=tab|window`, naming via `--title`. [1]
- Splits: `--location=hsplit|vsplit`. [1]
- Addressability: match targets by title/id with `--match`. [1]
- Start commands per pane: provide command after `launch ... -- <cmd>`. [1]
- Focus/activate: `focus-window`, `focus-tab`, `focus-visible-window`. [1]
- Send text/keys: `send-text` to a window; `kitten send-text` is also available. [2]

## Creating a New Tab With Split Layout

```
TASK_ID=$1
TITLE="ah-task-${TASK_ID}"

# Ensure there is a controlling socket; if running from inside Kitty, $KITTY_LISTEN_ON is set.
# Otherwise start Kitty with: kitty --listen-on unix:/tmp/kitty-ah.sock &
TO=${KITTY_LISTEN_ON:-unix:/tmp/kitty-ah.sock}

kitty @ --to "$TO" launch --type=tab --cwd "$PWD" --title "$TITLE" -- bash -lc 'nvim .'
kitty @ --to "$TO" launch --type=window --location=hsplit --cwd "$PWD" -- bash -lc "ah tui --follow ${TASK_ID}"
kitty @ --to "$TO" launch --type=window --location=vsplit --cwd "$PWD" -- bash -lc "ah session logs ${TASK_ID} -f"
```

## Launching Commands in Each Pane

- Pass the command after `--` to `launch ...` and it runs in the new tab/split. [1]

## Scripting Interactive Answers (Send Keys)

- Use `kitty @ send-text --match title:"$TITLE" -- "y\r"` to answer prompts; `--no-newline` avoids appending CR. [2]

## Focusing an Existing Task’s Pane/Window

```
TASK_ID=$1
kitty @ focus-window --match title:"ah-task-${TASK_ID}"
```

## Programmatic Control Interfaces

- Remote control requires a listening socket. Start Kitty with `--listen-on <addr>` or set `KITTY_LISTEN_ON`; then use `kitty @ --to <addr> ...`. [1]

## Detection and Version Compatibility

- Detect with `kitty --version`.
- Remote control (`kitty @`) is stable in current Kitty releases (see docs). Ensure the socket is available when controlling from external processes. [1]

## Cross‑Platform Notes

- macOS: the UNIX domain socket works; security settings do not generally block it.
- Windows: Kitty is not native; use alternative terminals for Windows.

## Example: TUI Follow Flow

Use the layout above; store the `title` so that `--match title:` can focus later; otherwise derive window id with `kitty @ ls`. [1]

## References

1. Kitty Remote Control: [Kitty remote-control docs][1]
2. Kitty send-text: [Send text to kitty][2]

[1]: https://sw.kovidgoyal.net/kitty/remote-control/
[2]: https://sw.kovidgoyal.net/kitty/remote-control/#sending-text-to-kitty
