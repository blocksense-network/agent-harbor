# Emacs — Terminal Integration

Create/focus Emacs window layouts and run interactive commands using `vterm` (recommended) or `ansi-term`.

## Overview

- Product: GNU Emacs with `vterm` package (libvterm backend)
- Platforms: Linux, macOS, Windows
- Interfaces: Elisp APIs (`split-window`, `other-window`), `vterm` functions `vterm-send-string`, `vterm-send-key`. [1]

## Capabilities Summary

- Splits: `split-window-right`, `split-window-below`; tabs via tab-bar or perspective packages.
- Terminal: `vterm` (PTY with libvterm) or built‑in `ansi-term`. `vterm` recommended for robust input. [2]
- Start commands per pane: call `vterm`, then `vterm-send-string` and `vterm-send-return`. [2]
- Send keys/text: `vterm-send-string`, `vterm-send-key`. [2]
- Focus/activate: standard Emacs window focus; external OS focus handled by the terminal emulator/WM.

## Creating a New Tab With Split Layout (Elisp)

```
(defun ah-open-task (task-id)
  "Open AH layout for TASK-ID with TUI and logs in vterms."
  (interactive "sTask ID: ")
  (let ((title (format "ah-task-%s" task-id)))
    (tab-new)
    (rename-buffer title)
    (delete-other-windows)
    ;; Right split: TUI follower
    (split-window-right)
    (other-window 1)
    (vterm)
    (vterm-send-string (format "ah tui --follow %s" task-id))
    (vterm-send-return)
    ;; Left-bottom split: logs
    (other-window -1)
    (split-window-below)
    (other-window 1)
    (vterm)
    (vterm-send-string (format "ah session logs %s -f" task-id))
    (vterm-send-return)))
```

## Launching Commands in Each Pane

- After `(vterm)`, use `(vterm-send-string "your command")` and `(vterm-send-return)` to execute. [2]

## Scripting Interactive Answers (Send Keys)

- `(vterm-send-string "y")` then `(vterm-send-return)`; for special keys use `(vterm-send-key "escape")` etc. [2]

## Focusing an Existing Task’s Pane/Window

- Name tabs/buffers with `ah-task-<id>`; switch via `tab-bar-switch-to-tab` or buffer selection.

## Programmatic Control Interfaces

- Emacs daemon: start with `emacs --daemon`; attach with `emacsclient -c` from scripts. [4]

## Detection and Version Compatibility

- Ensure `vterm` package installed and libvterm available; functions referenced are documented in `vterm` README. [2]

## Cross‑Platform Notes

- macOS requires granting Accessibility to allow external key injection only if using OS automation (not needed for internal `vterm`).

## Example: TUI Follow Flow

Call `(ah-open-task "ABC")` (or via `emacsclient -e`) to create splits and run TUI/logs.

## References

1. vterm (official README): [emacs-libvterm][1]

[1]: https://github.com/akermu/emacs-libvterm
