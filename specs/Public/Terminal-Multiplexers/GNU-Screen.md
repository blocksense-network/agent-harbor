# GNU Screen — Integration Guide

Automate GNU Screen sessions, windows, and regions using its CLI and `-X` command interface.

## Overview

- Product: GNU Screen (terminal multiplexer)
- Platforms: Linux, macOS, BSD
- License and installation: GPL; packaged on most Unix‑like systems (`apt install screen`, `brew install screen`, `pkg install screen`)
- CLI(s) used: `screen` CLI (sessions, windows, regions), `-X` to send commands to a running session, configuration via `.screenrc`.

## Capabilities Summary

- Tabs/workspaces support: **Yes** — sessions (server processes) with multiple windows (akin to tabs) and regions (split views).
- Horizontal/vertical splits: **Yes** — regions created with `split` (horizontal) and `split -v` (vertical) in the current window.
- Addressability: windows by number or title; regions/“focus targets” via `focus`, `focus up/down/left/right`, and `select`. Sessions by name (`-S`, `-r`, `-ls`).
- Start commands per pane automatically: start a detached session with a command (`screen -dmS <name> bash -lc '<cmd>'`) or create new windows in an existing session using `screen -S <name> -X screen bash -lc '<cmd>'`.
- Focus/activate existing pane: attach to a session with `screen -r <name>` and use `select`, `focus`, and `only` to navigate windows/regions. `-p` can preselect a window at startup.
- Send keys / scripted answers: use `screen -S <name> -X stuff 'text\n'` to inject characters into the focused window, including newlines for Enter.
- Startup layout recipe: combine `-dmS` (detached session) with `split`, `focus`, and `screen` commands via `-X` to build a deterministic editor/TUI/logs layout per task id.

## Creating a New Tab With Split Layout

This example creates a session `ah-<id>`, uses a single Screen window as the “tab”, splits it into three regions, and starts editor/TUI/logs commands in each.

```
TASK_ID=$1
SESSION="ah-${TASK_ID}"

# Start detached session with editor in window 0
screen -dmS "$SESSION" bash -lc 'cd "$PWD" && nvim .'

# Split vertically (left/right) and create a new region on the right running the TUI follower
screen -S "$SESSION" -X split -v
screen -S "$SESSION" -X focus right
screen -S "$SESSION" -X screen bash -lc "cd \"$PWD\" && ah tui --follow ${TASK_ID}"

# Focus left, split horizontally for logs, run tailing logs in bottom‑left
screen -S "$SESSION" -X focus left
screen -S "$SESSION" -X split
screen -S "$SESSION" -X focus down
screen -S "$SESSION" -X screen bash -lc "cd \"$PWD\" && ah session logs ${TASK_ID} -f"

# Return focus to the TUI (right region)
screen -S "$SESSION" -X focus up
screen -S "$SESSION" -X focus right
```

Notes

- Vertical splits require Screen with vertical split support (available in modern releases).
- `bash -lc 'cd "$PWD" && ...'` ensures each window starts in the project directory even when launched from detached automation.

## Launching Commands in Each Pane

- **Per‑window command**: start a detached session running a command in its initial window:
  - `screen -dmS <session_id> bash -lc '<cmd>'`
- **Additional panes in an existing layout**: create windows in the currently focused region:
  - `screen -S <session_id> -X screen bash -lc '<cmd>'`
- **Working directory control**:
  - Screen itself does not have a `-c` flag like tmux; use `bash -lc 'cd "<dir>" && ...'` or `chdir` in `.screenrc` to set per‑window cwd.
- **Environment propagation**:
  - New windows inherit the environment of the Screen server process; ensure automation starts Screen with the correct `$PATH`, project env, and any task‑specific variables.

## Scripting Interactive Answers (Send Keys)

- Use `stuff` to inject characters into the focused window of a session:
  - `screen -S <session_id> -X stuff 'y\n'` — send `y` followed by Enter.
- Quote and escape carefully:
  - Backslashes and quotes should be escaped by the calling shell _and_ by Screen’s own interpretation rules; prefer simple ASCII where possible.
- Timing and retries:
  - `stuff` queues characters to the target program; for fragile REPLs or prompts, scripts may need small sleeps between commands and explicit newlines.
- Security considerations:
  - Avoid stuffing secrets into windows with untrusted programs; keystrokes are visible to whatever process is attached to that window.

## Focusing an Existing Task’s Pane/Window

- **Find and attach to a session**:
  - Sessions are listed with `screen -ls`; sessions named `ah-<id>` can be reattached with:
    - `screen -r "ah-${TASK_ID}"`

- **Focus the right window/region after attach**:
  - Use `screen -S "ah-${TASK_ID}" -X select <window_number>` to choose a window.
  - Use `screen -S "ah-${TASK_ID}" -X focus [up|down|left|right]` to move between regions.
  - `only` removes all regions except the current one if you need to collapse to a single view.

On macOS and Linux, bringing the terminal emulator itself to the foreground is handled by the outer terminal application or OS (Screen does not control GUI focus directly).

## Programmatic Control Interfaces

- **CLI commands and exit codes**:
  - `screen -dmS <name> ...` — create a detached session, non‑interactively. Returns non‑zero on failure.
  - `screen -S <name> -X <cmd> [args...]` — send a Screen command (e.g., `split`, `focus`, `screen`, `select`, `remove`, `only`, `stuff`) to an existing session.
  - `screen -r <name>` / `-R` / `-D -R` — attach/reattach according to GNU Screen’s attach semantics.
  - `screen -ls` / `-wipe` — list/clean up sessions.
  - `-L` / `-Logfile` — enable per‑window logging and configure log file names.
- **Configuration and scripts**:
  - `.screenrc` can predefine key bindings, defaults (`defscrollback`, `defscrollback`, `defescape`), and layouts using `screen`, `split`, and `select` commands run at session startup.
  - Automation can rely on these commands for deterministic layouts instead of interactive key bindings.
- **Security**:
  - Sessions are identified by names in a per‑user socket directory; avoid using attacker‑controlled names, and ensure `screen` runs under a dedicated user where appropriate.

## Environment Variables

Automation can rely on a small set of environment variables that Screen **defines** for child processes, as well as variables that **influence** how Screen starts and where it stores state.

- **Variables set by Screen inside windows (child processes see these):**
  - `STY` — name of the current Screen session (e.g., `ah-1234`). Useful to detect that a process is running inside Screen and to associate panes with a given Agent Harbor task.
  - `WINDOW` — numeric window id at the time the shell/program was created. Can be used for per-window logging, metrics, or layout bookkeeping.
  - `TERM` / `TERMCAP` — terminal type and capability database that Screen exposes to windows (often `screen` or `screen-256color`). Agent Harbor tools must treat these as the ground truth for terminal capabilities instead of the outer terminal’s `$TERM`.
  - `COLUMNS` / `LINES` — current terminal geometry as seen by the window. These may be updated when the Screen layout or outer terminal size changes.

## Detection and Version Compatibility

- **Detect availability**:
  - `command -v screen >/dev/null 2>&1` to check for presence.
  - `screen --version` (or `screen -v`) to obtain the version string (e.g., `Screen version 5.0.1 ...`).
- **Minimum features**:
  - This integration assumes GNU Screen with support for regions (`split`) and `stuff`, available in widely‑deployed 4.x+ versions.
  - Vertical splits (`split -v`) may not be present in very old builds; scripts should be prepared to fall back to horizontal‑only layouts or detect failure.

## Cross‑Platform Notes

- **macOS**:
  - Homebrew/MacPorts provide up‑to‑date GNU Screen; the Apple‑supplied version may lag behind upstream.
  - Focus is managed by Terminal/iTerm2; Screen only controls virtual terminals within the PTY.
- **Linux/BSD**:
  - Screen is often installed by default; behavior is consistent across major distributions.
  - When using `sudo` or different users, sessions live in per‑user socket dirs; `screen -ls` only shows sessions for the current user.

## Example: TUI Follow Flow

To implement an “agent coding session” layout for a given task id:

- Create (or reuse) a session named `ah-<id>` using `screen -dmS "ah-${TASK_ID}"` with an editor in the first window.
- Use `screen -S "ah-${TASK_ID}" -X split`, `split -v`, `focus`, and `screen bash -lc '<cmd>'` to create regions for editor/TUI/logs and start `ah tui --follow <id>` and `ah session logs <id> -f` in their respective regions.
- When the user wants to resume, run `screen -r "ah-${TASK_ID}"` to attach and optionally send `select`/`focus` commands via `-X` to ensure the TUI region is focused.

## References

1. GNU Screen User’s Manual — overview, sessions, windows, and command reference. [GNU Screen manual](https://www.gnu.org/software/screen/manual/screen.html)
2. `screen(1)` man page — command‑line options and usage examples. [man7 screen(1)](https://man7.org/linux/man-pages/man1/screen.1.html)
3. GNU Screen Manual — regions and split commands. [GNU Screen regions/split](https://man7.org/linux/man-pages/man1/screen.1.html#WINDOW_TYPES)
4. GNU Screen Manual — `stuff` command for sending input. [GNU Screen stuff](https://man7.org/linux/man-pages/man1/screen.1.html#STRING_ESCAPES)
