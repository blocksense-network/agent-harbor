# Windows Terminal — Integration Guide

Automate Windows Terminal via its `wt.exe` command‑line arguments.

## Overview

- Product: Windows Terminal
- Platform: Windows 10/11
- Install: Microsoft Store / winget
- Interface: `wt` command line arguments (`new-tab`, `split-pane`, `focus-tab`, `move-focus`, `pane` sizing, targeting windows) [1]

## Capabilities Summary

- Tabs/panes: create tabs and split panes horizontally/vertically (`split-pane -H|-V`). [1]
- Addressability: target a window by ID with `-w <id>`; list windows via `wt -w 0` etc.; titles per tab via `--title`. [1]
- Start commands per pane: specify command after the subcommand, e.g., `powershell -NoExit -Command "..."`. [1]
- Focus/activate: `focus-tab -t <index>`, `move-focus` to a direction. [1]
- Send keys/text: no official send‑keys; interact via the command passed to the shell.

## Creating a New Tab With Split Layout

```
@echo off
set TASK_ID=%1
set TITLE=ah-task-%TASK_ID%

rem New tab with editor
wt -w 0 new-tab --title "%TITLE%" powershell -NoExit -Command "nvim ." ^&^& exit ; ^

rem Right split for TUI
split-pane -H powershell -NoExit -Command "ah tui --follow %TASK_ID%" ; ^

rem Focus left and split bottom for logs
move-focus left ; split-pane -V powershell -NoExit -Command "ah session logs %TASK_ID% -f"
```

Notes

- Use the caret (`^`) to escape line continuations in batch; PowerShell uses `;` to separate `wt` subcommands. [1]
- To reuse a specific window, supply `-w <window-id>` from a prior launch.

## Launching Commands in Each Pane

- Pass the shell and command after each `new-tab`/`split-pane`.
- Use `-NoExit` to keep the pane open after the command runs. [1]

## Scripting Interactive Answers (Send Keys)

- Not supported natively; use the program’s own CLI flags or PowerShell to feed input where possible.

## Focusing an Existing Task’s Pane/Window

- If you track window IDs, use `-w <id>` to target that window; otherwise rely on OS‑level window activation mechanisms outside `wt`.

## Programmatic Control Interfaces

- Command‑line only; no JSON‑RPC. Exit codes are propagated from `wt`.

## Detection and Version Compatibility

- Detect via `wt --version` or package version. Subcommands used above are documented in current Windows Terminal releases. [1]

## Cross‑Platform Notes

- Windows‑only.

## Example: TUI Follow Flow

Use the script above to create a titled tab, split panes, and run the TUI and logs. Store the `window-id` if you need to reuse the same window on subsequent invocations.

## References

1. Windows Terminal command line arguments: [Microsoft Docs][1]

[1]: https://learn.microsoft.com/windows/terminal/command-line-arguments
