# iTerm2 (macOS) — Integration Guide

Automate iTerm2 using its AppleScript dictionary (built‑in) or the official Python API.

## Overview

- Product: iTerm2
- Platform: macOS
- Install: <https://iterm2.com>
- Interfaces: AppleScript/JXA automation; Python API (run inside iTerm2 environment) [1][2]

## Capabilities Summary

- Tabs/windows: AppleScript `create window`, `create tab with default profile`. [1]
- Splits: `split vertically with default profile`, `split horizontally with default profile`. [1]
- Addressability: current window/session; iterate windows/tabs/sessions. [1]
- Start commands per pane: `write text "<cmd>"`. [1]
- Focus/activate: AppleScript can `select` sessions and `activate` the app. [1]
- Send keys/text: `write text` sends a command; Python API offers richer control. [1][2]

## Creating a New Tab With Split Layout (AppleScript)

```
osascript <<'OSA'
on run argv
  set taskId to item 1 of argv
  set title to "ah-task-" & taskId
  tell application "iTerm"
    activate
    set newwin to (create window with default profile)
    tell current session of newwin
      write text "printf '\e]1;%s\a' " & quoted form of title
      write text "nvim ."
    end tell
    tell current tab of newwin
      set rightPane to (split vertically with default profile)
      tell rightPane
        write text "ah tui --follow " & taskId
      end tell
      set bottomPane to (split horizontally with default profile)
      tell bottomPane
        write text "ah session logs " & taskId & " -f"
      end tell
    end tell
  end tell
end run
OSA
```

## Launching Commands in Each Pane

- Use `write text` on the target session to run a shell command. [1]

## Scripting Interactive Answers (Send Keys)

- Use `write text` with the desired text and control sequences (e.g., `y` then Enter). For arbitrary keypresses, the Python API provides `session.send_text` and more granular control. [2]

## Focusing an Existing Task’s Pane/Window

- Maintain a window/tab title like `ah-task-<id>` and loop through windows/tabs to find it; call `activate`. [1]

## Programmatic Control Interfaces

- AppleScript/JXA: available by default; see the “Scripting” section. [1]
- Python API: install iTerm2 Python runtime; scripts are launched from iTerm2; supports sessions, splits, and keystrokes. [2]

## Detection and Version Compatibility

- AppleScript commands above are documented in the current iTerm2 “Scripting” docs. Some APIs may vary across major versions; test on the installed build. [1]

## Cross‑Platform Notes

- iTerm2 is macOS‑only.
- Automation requires Accessibility permissions for iTerm2 on macOS.

## Example: TUI Follow Flow

Use the AppleScript above to create/focus a titled window and run `ah tui --follow <id>` and logs in dedicated panes. Store the window title in the TUI control index for reuse.

## References

1. iTerm2 Scripting (AppleScript): [iTerm2 scripting docs][1]
2. iTerm2 Python API: [iTerm2 Python API tutorial][2]

[1]: https://iterm2.com/documentation-scripting.html
[2]: https://iterm2.com/python-api/tutorial/index.html
