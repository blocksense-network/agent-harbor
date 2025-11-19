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

Agent Harbor uses GNU Screen's layout system to organize task workspaces. The implementation follows this pattern:

### Initial Setup (One-Time)

When Agent Harbor first interacts with a Screen session, it initializes a base layout:

```bash
# Get current session name from STY environment variable
SESSION="$STY"

# Create the agent-harbor base layout
screen -S "$SESSION" -X layout new agent-harbor

# Select window 0 (the original terminal)
screen -S "$SESSION" -X select 0
```

### Opening a Task Window

For each task, Agent Harbor creates a named layout:

```bash
TASK_NAME="ah-task-123"
SESSION="$STY"

# Create a new layout for this task
screen -S "$SESSION" -X layout new "$TASK_NAME"

# Create an initial window in the layout
screen -S "$SESSION" -X screen
```

### Splitting Panes Within a Task

To split a region and run commands:

```bash
SESSION="$STY"
TASK_NAME="ah-task-123"

# First, select the task's window if needed
screen -S "$SESSION" -X select "$TASK_NAME"

# Split the current region (horizontal: top/bottom)
screen -S "$SESSION" -X split

# Focus the newly created region
screen -S "$SESSION" -X focus down

# Create a new window in the focused region
screen -S "$SESSION" -X screen

# Send commands to the new window using stuff
# Commands are wrapped in bash -lc for proper environment loading
# Escaping rules: backslashes (\\), dollar signs (\$), single quotes ('\''）
screen -S "$SESSION" -X stuff "bash -lc 'cd /path/to/dir && command'\n"
```

For vertical splits (side-by-side):

```bash
# Split vertically (left/right)
screen -S "$SESSION" -X split -v

# Focus right
screen -S "$SESSION" -X focus right

# Create window in the new region
screen -S "$SESSION" -X screen

# Send command
screen -S "$SESSION" -X stuff "bash -lc 'command'\n"
```

### Notes on Implementation Patterns

- **Layout management**: Agent Harbor uses Screen's `layout` command to create named workspaces for each task
- **Consistent focus pattern**: After any split (horizontal or vertical), the implementation uses `focus down` to move to the new region
- **Command execution**: Commands are always sent via `stuff` with `bash -lc` wrapper for proper environment initialization
- **Escaping for stuff**: Characters that need escaping: `\\` → `\\\\`, `$` → `\\$`, `'` → `'\\''`
- **Newline termination**: Commands sent via `stuff` end with `\n` to execute immediately
- **Window selection**: When splitting within a specific task window, use `select` before the split commands

## Launching Commands in Each Pane

Agent Harbor uses a consistent pattern for running commands in Screen regions:

### Command Execution Pattern

All commands are sent to Screen windows using the `stuff` command with proper escaping:

```bash
SESSION="$STY"

# Basic command execution (no directory change)
screen -S "$SESSION" -X stuff "bash -lc 'command args'\n"
```

### Working Directory Control

Screen doesn't support direct CWD specification, so commands are prefixed with `cd`:

```bash
# With working directory
screen -S "$SESSION" -X stuff "bash -lc 'cd /path/to/dir && command'\n"

# With working directory and fallback shell
screen -S "$SESSION" -X stuff "bash -lc 'cd /path/to/dir && bash'\n"
```

### Escaping Rules for stuff Command

When sending commands through `stuff`, apply these escaping rules in order:

1. **Backslashes**: `\` → `\\\\` (escaped first to avoid interfering with other escapes)
2. **Dollar signs**: `$` → `\\$` (prevents variable expansion)
3. **Single quotes**: `'` → `'\\''` (end quote, escaped quote, start quote)

Example escaping:

```bash
# Original command: cd /path/with'quote && echo $VAR
# After escaping: cd /path/with'\''quote && echo \$VAR
screen -S "$SESSION" -X stuff "bash -lc 'cd /path/with'\\''quote && echo \\$VAR'\n"
```

### Environment Propagation

- New windows inherit the environment from the Screen server process
- Use `bash -lc` wrapper to ensure login shell initialization (`.bashrc`, `.bash_profile`)
- The `-l` flag loads the user's shell environment
- The `-c` flag executes the provided command string

## Scripting Interactive Answers (Send Keys)

Agent Harbor uses Screen's `stuff` command to send text and simulate keystrokes in windows.

### Basic Text Sending

```bash
SESSION="$STY"

# Send text with newline (executes Enter)
screen -S "$SESSION" -X stuff "y\n"

# Send text without newline (types but doesn't execute)
screen -S "$SESSION" -X stuff "some text"
```

### Command Formatting

The implementation uses a specific format for sending commands:

```bash
# Simple command (already formatted with newline)
COMMAND="ls -la\n"
screen -S "$SESSION" -X stuff "$COMMAND"
```

### Timing Considerations

- `stuff` queues characters to the target program immediately
- For interactive REPLs or prompts that need time to respond, consider adding delays between commands
- Commands ending with `\n` execute immediately; without `\n` they remain in the input buffer

### Security Considerations

- Never send secrets or credentials through `stuff` - they become visible in the window
- Characters sent via `stuff` are visible to any process attached to that window
- For automation involving sensitive data, prefer file-based or environment variable approaches

## Focusing an Existing Task's Pane/Window

### Listing Sessions

Agent Harbor lists Screen sessions using `screen -ls`:

```bash
# List all sessions
screen -ls

# Output format (parsed by implementation):
# There are screens on:
#     12345.session-name    (Detached)
#     67890.another-session (Attached)
# 2 Sockets in /var/run/screen/S-user.
```

The implementation parses this output using the pattern: `^\s*\d+\.([^\s]+)\s+\(`

### Focusing a Window

To bring focus to a specific window (equivalent to attaching to a session):

```bash
WINDOW_ID="session-name"

# Reattach to the session
screen -r "$WINDOW_ID"
```

**Note**: The `screen -r` command is interactive and will block until the session is detached. This is suitable for end-user interaction but may not be appropriate for programmatic control that needs to return immediately.

### Selecting Windows Within a Session

To switch to a specific window by name or number:

```bash
SESSION="$STY"
WINDOW_NAME="ah-task-123"

# Select window by name
screen -S "$SESSION" -X select "$WINDOW_NAME"

# Or by number
screen -S "$SESSION" -X select 0
```

### Region Navigation

Direct pane/region focusing is not available through Screen's CLI. The implementation returns an error for `focus_pane` operations as this requires manual keyboard navigation by the user (`Ctrl-A Tab` to cycle through regions).

### OS-Level Focus

Screen itself only manages terminal multiplexing within a single terminal window. Bringing the terminal emulator to the foreground requires OS-level or terminal-specific automation (handled separately by terminal integration).

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

Automation can rely on environment variables that Screen **defines** for child processes, as well as variables that **influence** how Screen starts and where it stores state.

### Variables Set by Screen (Available to Child Processes)

These variables are set by Screen and are visible to programs running inside windows:

- **`STY`** — Name of the current Screen session (e.g., `ah-1234`). Useful to detect that a process is running inside Screen and to associate panes with a given Agent Harbor task. If `STY` is set when `screen` is invoked, it creates a new window in the running session rather than starting a new session.
- **`WINDOW`** — Numeric window id at the time the shell/program was created. Can be used for per-window logging, metrics, or layout bookkeeping.
- **`TERM`** — Terminal name that Screen exposes to windows (often `screen` or `screen-256color`). Agent Harbor tools must treat this as the ground truth for terminal capabilities instead of the outer terminal's `$TERM`.
- **`TERMCAP`** — Terminal description/capability database that Screen exposes to windows. Screen may customize this based on `SCREENCAP` (see below).
- **`COLUMNS`** — Number of columns on the terminal (overrides termcap entry). Current terminal geometry as seen by the window. These may be updated when the Screen layout or outer terminal size changes.
- **`LINES`** — Number of lines on the terminal (overrides termcap entry). Current terminal geometry as seen by the window. These may be updated when the Screen layout or outer terminal size changes.

### Variables That Influence Screen's Behavior

These environment variables affect how Screen starts, where it looks for configuration, and how it behaves:

- **`HOME`** — Directory in which Screen looks for `.screenrc` configuration file. Defaults to the user's home directory.
- **`SCREENRC`** — Alternate user screenrc file path. Overrides the default `~/.screenrc` location. Useful for automation that needs task-specific or isolated Screen configurations.
- **`SYSTEM_SCREENRC`** — Alternate system-wide screenrc file path. Used for system-level defaults before user configuration is loaded.
- **`SCREENDIR`** — Alternate socket directory where Screen stores session sockets. Defaults to a per-user directory (typically `/tmp/screens` or `/var/run/screen`). Automation may set this to isolate sessions or manage permissions.
- **`SHELL`** — Default shell program for opening windows (default `/bin/sh`). Screen uses this when creating new windows without an explicit command. Automation should ensure `SHELL` points to the desired shell (e.g., `/bin/bash` or `/bin/zsh`) for consistent behavior. Can also be configured via the `shell` command in `.screenrc`.
- **`PATH`** — Used by Screen for locating programs to run. New windows inherit the `PATH` from the Screen server process, so automation should ensure the correct `PATH` is set before starting Screen.
- **`SCREENCAP`** — For customizing a terminal's `TERMCAP` value. Screen uses this to generate the `TERMCAP` environment variable for child processes. Rarely needed in automation scenarios.
- **`LOCKPRG`** — Screen lock program. When Screen's lock feature is triggered, it executes the program specified by `LOCKPRG`. Defaults to a built-in lock mechanism if not set.

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

## Example: Agent Harbor Task Workflow

Agent Harbor's Screen integration follows this workflow for task management:

### Initial Setup

When Agent Harbor first runs in a Screen session:

```bash
SESSION="$STY"  # Automatically detected from environment

# Initialize the base agent-harbor layout (done once)
screen -S "$SESSION" -X layout new agent-harbor
screen -S "$SESSION" -X select 0
```

### Opening a Task Window

When a user starts working on a task:

```bash
TASK_NAME="ah-task-123"
SESSION="$STY"

# Check if window already exists (reuse if present)
# Implementation: list_windows() and check for matching name

# If not exists, create new task layout
screen -S "$SESSION" -X layout new "$TASK_NAME"
screen -S "$SESSION" -X screen
```

### Creating Split Layout for Task

To create a split pane layout within the task window:

```bash
# Select the task window
screen -S "$SESSION" -X select "$TASK_NAME"

# Create first split (horizontal)
screen -S "$SESSION" -X split
screen -S "$SESSION" -X focus down
screen -S "$SESSION" -X screen

# Send command to bottom pane
screen -S "$SESSION" -X stuff "bash -lc 'cd /project && tail -f logs.txt'\n"

# Create second split (vertical) in top region
screen -S "$SESSION" -X focus up
screen -S "$SESSION" -X split -v
screen -S "$SESSION" -X focus down
screen -S "$SESSION" -X screen

# Send command to right pane
screen -S "$SESSION" -X stuff "bash -lc 'cd /project && nvim'\n"
```

### Detecting Current Context

The implementation uses environment variables to detect the current Screen context:

- `$STY` - Contains the session name (e.g., `12345.session-name`)
- `$WINDOW` - Contains the current window number

This allows Agent Harbor to automatically operate within the correct Screen session without requiring explicit session names.
