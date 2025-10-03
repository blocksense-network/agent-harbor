# Tilix (Linux) — Integration Guide

Automate Tilix using command‑line actions and session files; D‑Bus is also available.

## Overview

- Product: Tilix (tiling terminal emulator)
- Platform: Linux (GTK)
- Install: distro package manager
- Interfaces: `tilix` CLI with `--action`, `--session` JSON; D‑Bus methods. [1]

## Capabilities Summary

- Tabs (terminals) and splits supported; actions to add terminals down/right. [1]
- Start commands per terminal via `--command`/`--working-directory` or session JSON. [1]
- Addressability: via session layout definitions; runtime targeting via D‑Bus/window focus is limited.
- Focus/activate: window manager focus; Tilix can `--maximize` etc. [1]
- Send keys: no built‑in send‑keys; use the shell/app CLI.

## Creating a New Tab With Split Layout

Using actions (left/right/down) and `--command`:

```
TASK_ID=$1
tilix --new-window --title="ah-task-${TASK_ID}" \
  --command="bash -lc 'nvim .'" &
sleep 0.3
tilix --action=app-new-session-right --command="bash -lc 'ah tui --follow ${TASK_ID}'"
tilix --action=app-new-session-down --command="bash -lc 'ah session logs ${TASK_ID} -f'"
```

Alternatively, define a reusable session JSON and load it with `--session FILE`. [1]

## Launching Commands in Each Pane

- Use `--command` and `--working-directory` with each action or encode commands in a session JSON. [1]

## Scripting Interactive Answers (Send Keys)

- Not supported natively; rely on the program’s non‑interactive flags.

## Focusing an Existing Task’s Pane/Window

- Use window titles and the window manager; Tilix itself does not provide a robust CLI for “focus by title”.

## Programmatic Control Interfaces

- CLI actions: `app-new-session-right`, `app-new-session-down`, etc. [1]
- Session files: JSON defining terminals, commands, titles (`tilix --session layout.json`). [1]

## Detection and Version Compatibility

- Detect via `tilix --version`. Features referenced are in current Tilix documentation/man page. [1]

## Cross‑Platform Notes

- Linux‑only; Wayland vs X11 affects global hotkeys; CLI works on both.

## Example: TUI Follow Flow

Use the action commands above or a session JSON to create the layout and run `ah tui --follow <id>` and logs.

## References

1. Tilix documentation (CLI, actions, session files): [Tilix manual][1]

[1]: https://gnunn1.github.io/tilix-web/manual/
