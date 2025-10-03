# Multiplexer Integration — Description Template

Use this template to document how agents‑workflow integrates with a specific terminal multiplexer or terminal emulator/editor that offers comparable features. Replace bracketed placeholders and remove non‑applicable items with rationale.

## Overview

- Product name and version(s) tested:
- Platforms: [Linux/macOS/Windows]
- License and install path(s):
- CLI(s) or RPC used:

## Capabilities Summary

- Tabs/workspaces support: [Yes/No]
- Horizontal/vertical splits: [Yes/No]
- Addressability: [window id, pane id, titles, labels]
- Start commands per pane automatically: [How]
- Focus/activate existing pane: [How]
- Send keys / scripted answers: [How]
- Startup layout recipe: [Provide]

## Creating a New Tab With Split Layout

Reference TUI PRD requirements for the “agent coding session” layout (editor/logs/shell splits). Provide exact commands.

Example

```
# commands to create new tab and two splits, with sizes
# commands to run in each pane
```

Notes

- Headless vs GUI behavior
- Session naming conventions

## Launching Commands in Each Pane

- How to specify per‑pane commands non‑interactively.
- Environment propagation (PATH, project env).
- Working directory control.

## Scripting Interactive Answers (Send Keys)

- How to send keystrokes/strings.
- Quoting/escaping details.
- Timing/retry guidance.

## Focusing an Existing Task’s Pane/Window

- How to find the right pane/tab by id/title.
- How to bring the window/app to foreground on each OS.

## Programmatic Control Interfaces

- CLI examples and exit codes.
- IPC/daemon APIs if any (e.g., sockets).
- Security considerations.

## Detection and Version Compatibility

- How the `ah` CLI detects availability.
- Minimum version required and feature gates.

## Cross‑Platform Notes

- macOS: quirks and focus rules.
- Linux: X11/Wayland differences; DBus portals.
- Windows: ConPTY and Windows Terminal specifics.

## Example: TUI Follow Flow

Provide a realistic end‑to‑end example script that creates or focuses the session layout for a task id and starts `ah tui --follow <id>` within the appropriate pane.

## References

- Official docs links.
- Any community‑maintained guides/scripts used.
