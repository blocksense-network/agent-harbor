# Tilix (Linux) — Integration Guide

Automate Tilix using command‑line actions and session files; D‑Bus is also available.

## Overview

- Product name and version(s) tested: Tilix 1.9.6 (tiling terminal emulator)
- Platforms: Linux (GTK)
- License and install path(s): Open source, available via distro package managers
- CLI(s) or RPC used: `tilix` CLI with `--action`, `--session`; D‑Bus methods. [1]

## Capabilities Summary

- Tabs/workspaces support: Yes (sessions and windows)
- Horizontal/vertical splits: Yes (via `session-add-right`, `session-add-down` actions)
- Addressability: Via session layout definitions; runtime targeting via D‑Bus/window focus is limited
- Start commands per pane automatically: Yes, via `--command` and `--working-directory` or session layouts
- Focus/activate existing pane: Window manager focus; Tilix can `--maximize` etc. [1]
- Send keys / scripted answers: No built‑in send‑keys; use the shell/app CLI
- Startup layout recipe: Command-line actions or saved session layouts

## Creating a New Tab With Split Layout

Using actions (left/right/down) and `--command`:

```
TASK_ID=$1
tilix --title="ah-task-${TASK_ID}" \
  --command="bash -lc 'nvim .'" &
sleep 0.3
tilix --action=session-add-right --command="bash -lc 'ah tui --follow ${TASK_ID}'"
tilix --action=session-add-down --command="bash -lc 'ah session logs ${TASK_ID} -f'"
```

Alternatively, define a reusable session JSON and load it with `--session FILE`. [1]

## Launching Commands in Each Pane

- Use `--command` and `--working-directory` with each action or encode commands in a session JSON. [1]

## Scripting Interactive Answers (Send Keys)

- Not supported natively; rely on the program’s non‑interactive flags.

## Focusing an Existing Task’s Pane/Window

- Use window titles and the window manager; Tilix itself does not provide a robust CLI for “focus by title”.

## Programmatic Control Interfaces

- CLI actions: `session-add-right`, `session-add-down`, `app-new-session`, `app-new-window`. [1]
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
