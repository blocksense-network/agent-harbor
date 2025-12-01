# TUI Keyboard Shortcuts Specification

This document defines the standard keyboard shortcuts and operation mappings used across all Agent Harbor TUI components.

## Notation

Throughout the documentation, we use the notation `keys:operation-name` to refer to keyboard shortcuts. This emphasizes the _semantic action_ rather than the specific key binding, which can be customized by the user.

For example:

- `keys:move-to-next-line` refers to the action of moving the cursor or focus down (Default: `Down`).
- `keys:draft-new-task` refers to creating a new draft (Default: `Ctrl+N`).

## Core Concepts & Minor Modes

The TUI input system is built around **Keyboard Operations** and **Input Minor Modes**.

- **Keyboard Operations**: Abstract semantic actions (e.g., `DeleteCharacterForward`) defined in `settings.rs`.
- **Input Minor Modes**: Named collections of operations active in specific UI contexts (e.g., `Mode:TextEditing`, `Mode:Selection`).

This system allows the same physical key to trigger different operations depending on the active mode. For example, `Enter` might trigger `keys:open-new-line` in a text area but `keys:activate-current-item` in a list.

In the Rust implementation, these kebab-case configuration names are translated to PascalCase enum variants (e.g., `move-to-next-line` -> `MoveToNextLine`).

For technical details, see `crates/ah-tui/src/view_model/input.rs`.

### Text Area Shortcuts

All inputs should have appropriate placeholder text.
Text inputs should support a combination of CUA, macOS and Emacs key bindings.
The user can override any of the default key bindings through configuration variables listed below.
All such variables are in under the "[tui.keymap]" section.

## Configuration Variable Mapping

| Category                        | Operation                                     | Config Variable                  | Key Bindings                                                                    |
| ------------------------------- | --------------------------------------------- | -------------------------------- | ------------------------------------------------------------------------------- | --- | --- | ------------------------- | --------------------------- | ----------- |
| **Cursor Movement**             | Move to beginning of line                     | `move-to-beginning-of-line`      | C-a (Emacs), Home (CUA/PC), Cmd+Left (macOS)                                    |
|                                 | Move to end of line                           | `move-to-end-of-line`            | C-e (Emacs), End (CUA/PC), Cmd+Right (macOS)                                    |
|                                 | Move forward one character                    | `move-forward-one-character`     | C-f (Emacs), Right                                                              |
|                                 | Move backward one character                   | `move-backward-one-character`    | Left                                                                            |
|                                 | Move to next line                             | `move-to-next-line`              | Down                                                                            |
|                                 | Move to previous line                         | `move-to-previous-line`          | Up, C-p (Emacs)                                                                 |
|                                 | Move forward one word                         | `move-forward-one-word`          | M-f (Emacs), Ctrl+Right (CUA/PC), Opt+Right (macOS)                             |
|                                 | Move backward one word                        | `move-backward-one-word`         | M-b (Emacs), Ctrl+Left (CUA/PC), Opt+Left (macOS)                               |
|                                 | Move to beginning of sentence                 | `move-to-beginning-of-sentence`  | M-a (Emacs)                                                                     |
|                                 | Move to end of sentence                       | `move-to-end-of-sentence`        | M-e (Emacs)                                                                     |
|                                 | Scroll down one screen                        | `scroll-down-one-screen`         | C-v (Emacs), PgDn (CUA/PC), Fn+Down (macOS)                                     |
|                                 | Scroll up one screen                          | `scroll-up-one-screen`           | M-v (Emacs), PgUp (CUA/PC), Fn+Up (macOS)                                       |
|                                 | Scroll timeline down card by card             | `scroll-down-one-item`           | `Shift+Down`                                                                    |
|                                 | Scroll timeline up card by card               | `scroll-up-one-item`             | `Shift+Up`                                                                      |     |     | Recenter screen on cursor | `recenter-screen-on-cursor` | C-l (Emacs) |
|                                 | Move to beginning of document                 | `move-to-beginning-of-document`  | Ctrl+Home (CUA/PC), Cmd+Up (macOS)                                              |
|                                 | Move to end of document                       | `move-to-end-of-document`        | Ctrl+End (CUA/PC), Cmd+Down (macOS)                                             |
|                                 | Move to beginning of paragraph                | `move-to-beginning-of-paragraph` | Opt+Up (macOS)                                                                  |
|                                 | Move to end of paragraph                      | `move-to-end-of-paragraph`       | Opt+Down (macOS)                                                                |
|                                 | Go to line number                             | `go-to-line-number`              | Ctrl+G (CUA/PC in some), Cmd+L (macOS in some), M-g g (Emacs)                   |
|                                 | Move to matching parenthesis                  | `move-to-matching-parenthesis`   | C-M-f (Emacs forward), C-M-b (Emacs backward)                                   |
| **Editing and Deletion**        | Delete character forward                      | `delete-character-forward`       | C-d (Emacs), Delete (CUA/PC and macOS; Fn+Delete on macOS laptops)              |
|                                 | Delete character backward                     | `delete-character-backward`      | DEL or C-h (Emacs), Backspace (CUA/PC and macOS)                                |
|                                 | Delete word forward                           | `delete-word-forward`            | M-d (Emacs), Ctrl+Delete (CUA/PC), Opt+Delete (macOS; Opt+Fn+Delete on laptops) |
|                                 | Delete word backward                          | `delete-word-backward`           | M-DEL (Emacs), Ctrl+Backspace (CUA/PC), Opt+Backspace (macOS)                   |
|                                 | Kill (cut) to end of line                     | `delete-to-end-of-line`          | C-k (Emacs), Ctrl+K (macOS in some text fields)                                 |
|                                 | Kill region (cut selected text)               | `cut`                            | C-w (Emacs), Ctrl+X (CUA/PC), Cmd+X (macOS)                                     |
|                                 | Copy region to kill ring (copy selected text) | `copy`                           | M-w (Emacs), Ctrl+C (CUA/PC), Cmd+C (macOS)                                     |
|                                 | Yank (paste) from kill ring                   | `paste`                          | C-y (Emacs), Ctrl+V (CUA/PC), Cmd+V (macOS)                                     |
|                                 | Cycle through kill ring (after yank)          | `cycle-through-clipboard`        | M-y (Emacs)                                                                     |
|                                 | Transpose characters                          | `transpose-characters`           | C-t (Emacs)                                                                     |
|                                 | Transpose words                               | `transpose-words`                | M-t (Emacs)                                                                     |
|                                 | Undo                                          | `undo`                           | C-\_ or C-/ (Emacs), Ctrl+Z (CUA/PC), Cmd+Z (macOS)                             |
|                                 | Redo                                          | `redo`                           | C-? (Emacs, Ctrl+Shift+/), Ctrl+Y (CUA/PC), Cmd+Shift+Z (macOS)                 |
|                                 | Open (insert) new line                        | `open-new-line`                  | C-o (Emacs), Enter (CUA/PC and macOS), Shift+Enter (TUI)                        |
|                                 | Indent or complete                            | `indent-or-complete`             | TAB (Emacs)                                                                     |
|                                 | Move to next field                            | `move-to-next-field`             | Tab                                                                             |
|                                 | Move to previous field                        | `move-to-previous-field`         | Shift+Tab                                                                       |
|                                 | Dismiss overlay                               | `dismiss-overlay`                | Esc                                                                             |
|                                 | Increment value                               | `increment-value`                | Shift+=, Right                                                                  |
|                                 | Decrement value                               | `decrement-value`                | -, Left                                                                         |
|                                 | Delete to beginning of line                   | `delete-to-beginning-of-line`    | Cmd+Backspace (macOS)                                                           |
|                                 | Toggle insert mode                            | `toggle-insert-mode`             | Insert                                                                          |
| **Text Transformation**         | Uppercase word                                | `uppercase-word`                 | M-u (Emacs)                                                                     |
|                                 | Lowercase word                                | `lowercase-word`                 | M-l (Emacs)                                                                     |
|                                 | Capitalize word                               | `capitalize-word`                | M-c (Emacs)                                                                     |
|                                 | Fill/justify paragraph                        | `justify-paragraph`              | M-q (Emacs)                                                                     |
|                                 | Join lines                                    | `join-lines`                     | M-^ (Emacs)                                                                     |
| **Formatting (Markdown Style)** | Bold                                          | `bold`                           | Ctrl+B (CUA/PC), Cmd+B (macOS)                                                  |
|                                 | Italic                                        | `italic`                         | Ctrl+I (CUA/PC), Cmd+I (macOS)                                                  |
|                                 | Underline                                     | `underline`                      | Ctrl+U (CUA/PC), Cmd+U (macOS)                                                  |
| **Code Editing**                | Toggle comment                                | `toggle-comment`                 | M-; (Emacs), Ctrl+/ (CUA/PC), Cmd+/ (macOS)                                     |
|                                 | Duplicate line/selection                      | `duplicate-line-selection`       | Ctrl+Shift+D (CUA/PC), Cmd+Shift+D (macOS)                                      |
|                                 | Move line up                                  | `move-line-up`                   | Alt+Up (CUA/PC), Opt+Up (macOS)                                                 |
|                                 | Move line down                                | `move-line-down`                 | Alt+Down (CUA/PC), Opt+Down (macOS)                                             |
|                                 | Indent region                                 | `indent-region`                  | C-M-\ (Emacs), Ctrl+] (CUA/PC), Cmd+] (macOS)                                   |
|                                 | Dedent region                                 | `dedent-region`                  | Ctrl+[ (CUA/PC), Cmd+[ (macOS)                                                  |
| **Search and Replace**          | Incremental search forward                    | `incremental-search-forward`     | C-s (Emacs), Ctrl+F (CUA/PC), Cmd+F (macOS)                                     |
|                                 | Incremental search backward                   | `incremental-search-backward`    | C-r (Emacs)                                                                     |
|                                 | Query replace                                 | `find-and-replace`               | M-% (Emacs), Ctrl+H (CUA/PC in some apps)                                       |
|                                 | Query replace with regex                      | `find-and-replace-with-regex`    | C-M-% (Emacs)                                                                   |
|                                 | Find next                                     | `find-next`                      | F3 (CUA/PC), Cmd+G (macOS)                                                      |
|                                 | Find previous                                 | `find-previous`                  | Shift+F3 (CUA/PC), Cmd+Shift+G (macOS)                                          |
| **Mark and Region**             | Set mark (start selection)                    | `set-mark`                       | C-SPC or C-@ (Emacs)                                                            |
|                                 | Select all (mark whole text area)             | `select-all`                     | C-x h (Emacs), Ctrl+A (CUA/PC), Cmd+A (macOS)                                   |
|                                 | Select word under cursor                      | `select-word-under-cursor`       | Alt+@                                                                           |
|                                 | Extend selection                              | no config variable               | Shift+movement key (CUA/PC and macOS)                                           |
| **Application Actions**         | Draft new task                                | `draft-new-task`                 | Ctrl+N                                                                          |
|                                 | Show Advanced Launch Options                  | `show-launch-options`            | Ctrl+Enter                                                                      |
|                                 | Apply modal changes                           | `apply-modal-changes`            | A (in modal context)                                                            |
|                                 | Launch and focus                              | `launch-and-focus`               | No Default Shortcut                                                             |
|                                 | Launch in split view                          | `launch-in-split-view`           | No Default Shortcut                                                             |
|                                 | Launch in split view and focus                | `launch-in-split-view-and-focus` | No Default Shortcut                                                             |
|                                 | Launch in horizontal split                    | `launch-in-horizontal-split`     | No Default Shortcut                                                             |
|                                 | Launch in vertical split                      | `launch-in-vertical-split`       | No Default Shortcut                                                             |
|                                 | Activate current item                         | `activate-current-item`          | Enter                                                                           |
|                                 | Load previous prompt                          | `history-prev`                   | Ctrl+Alt+Up                                                                     |
|                                 | Load next prompt                              | `history-next`                   | Ctrl+Alt+Down                                                                   |
|                                 | Stop/Pause                                    | `stop`                           | Pause, Cmd+. (macOS)                                                            |
|                                 | Show Help                                     | `show-help`                      | ?, Ctrl+?                                                                       |
|                                 | Delete current task                           | `delete-current-task`            | Ctrl+W (CUA/PC), Cmd+W (macOS), C-x k (Emacs)                                   |
| **Session Viewer Task Entry**   | Move to next snapshot                         | `move-to-next-snapshot`          | Ctrl+Shift+Down                                                                 |
|                                 | Move to previous snapshot                     | `move-to-previous-snapshot`      | Ctrl+Shift+Up                                                                   |
| **General Navigation**          | Navigate Down                                 | `navigate-down`                  | `Down`, `Alt+Down`                                                              |
|                                 | Navigate Up                                   | `navigate-up`                    | `Up`, `Alt+Up`                                                                  |
|                                 | Navigate Left                                 | `navigate-left`                  | `Left`, `Alt+Left`                                                              |
|                                 | Navigate Right                                | `navigate-right`                 | `Right`, `Alt+Right`                                                            |

Note: In the table, "C-" means Control, "M-" means Meta (often Alt/Option), and combinations like "C-M-" use both. Please note that the Meta key should be the Option key on macOS and the Alt key otherwise. This can be overridden with the configuration option `tui.keymap.meta-key`.

### Emacs Key Binding Conflicts

Several traditional Emacs key bindings (C-b, C-f, C-n) for cursor movement have been simplified to use only arrow keys in the current implementation due to conflicts with other application shortcuts (C-b conflicts with "Bold", C-f conflicts with "Incremental search forward", C-n conflicts with "Draft new task"). Users who prefer full Emacs-style navigation can configure custom key bindings using the `tui.keymap` configuration section.
