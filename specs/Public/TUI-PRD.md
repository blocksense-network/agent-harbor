## TUI — Product Requirements and UI Specification

### Summary

The TUI provides a terminal-first dashboard for launching and monitoring agent tasks, integrated with terminal multiplexers (tmux, zellij, screen). It auto-attaches to the active multiplexer session and assumes all active tasks are already visible as multiplexer windows.

The TUI is built with **Ratatui**, a Rust library for building terminal user interfaces, along with specialized ecosystem crates:

- **ratatui**: Core TUI framework for rendering and layout
- **tui-textarea**: Advanced multi-line text editing with cursor management
- **tui-input**: Single-line input handling for modals and forms
- **crossterm**: Cross-platform terminal manipulation and event handling

See specs/Research/TUI for helpful information for developing with Ratatui.

Backends:

- REST: Connect to a remote REST service and mirror the WebUI experience for task creation, with windows created locally (or remotely via SSH) for launched tasks.
- Local: Operate in the current directory/repo using the SQLite state database for discovery and status.

## Terminal State Management

The TUI properly manages terminal state to ensure clean restoration:

- **Keyboard Enhancement Flags**: Uses Crossterm's keyboard enhancement flags for improved input handling
- **State Tracking**: Tracks raw mode, alternate screen, and keyboard flags for proper cleanup
- **Panic Safety**: Implements panic hooks and signal handlers to restore terminal state on crashes
- **Graceful Exit**: Ensures terminal returns to normal state regardless of exit method (ESC, Ctrl+C, panic). All calls to process:exit calls must be wrapped in a helper that performs the necessary state restoration.

### Auto-Attach and Window Model

- On start, `ah tui` auto-attaches to the configured multiplexer session (creating one if needed) and launches the TUI dashboard in a single window initially. Existing task windows are left intact.
- The TUI dashboard (`ah tui dashboard`) is the main interface for task management and runs inside a multiplexer window.
- Launching a new task from the dashboard creates a new multiplexer window with split panes:
  - Right pane = agent activity and logs, left pane = terminal or configured editor in the workspace.
  - Devcontainer and remote-server runs: panes are inside the container/remote context.
- The multiplexer provides the windowing environment; the TUI dashboard coordinates task creation and monitoring across windows.

### Simplified Task-Centric Layout

The dashboard screen has the following elements:

- **Header**: Agent Harbor branding with settings access
  - Displays image logo when terminal supports modern image protocols (e.g., Kitty, iTerm2)
  - Falls back to ASCII art logo for terminals without image support
  - **Settings Button**: Located in upper-right corner, accessible via Up arrow from the top draft card
  - Settings dialog provides configuration options

- **Tasks**: Chronological list of recent tasks (completed/merged, active, draft) displayed as bordered cards, with draft tasks always visible at the top, sorted newest first.
  - Uses 1 character of padding between screen edges and cards for clean visual spacing.
  - **Scrollable Viewport**: When the number of task cards exceeds available screen space, the task list becomes scrollable with a visible scrollbar indicator. Users can scroll through cards using mouse wheel, arrow keys, or Page Up/Page Down.
  - **Existing Tasks Section**: Below draft tasks, a horizontal line separator with "Existing Tasks" label and filter controls:
    - **Inline Selection Dialogs**: Filter controls use embedded dropdown/selection interface
      - **Filter Controls**: Inline filter buttons for task status (All, Active, Completed, Merged) and time range (Today, Week, Month, All Time)
      - **Search Box**: Inline text input for filtering tasks by title, repository, or description
      - **Sort Options**: Inline dropdown for sorting by date, status, repository, or agent
      - **Characteristics**: No overlay, expand in-place, arrow keys to navigate, Enter/Space to toggle

- **Footer**: Displays context-specific keyboard shortcuts.

#### Task States and Card Layouts

Tasks display in four different states with optimized heights and consistent layout principles:

- **Fixed height for completed/active cards**: Completed and active cards maintain constant height regardless of content to prevent UI jumping
- **Variable height for draft cards**: Draft cards expand/contract with the text area for better editing experience
- **Compact layout**: All metadata (repo, branch, agent, timestamp) fits on single lines
- **Status indicators**: Color-coded icons with symbols controlled by `tui-font-style` config
- **Visual separators** between cards
- **Keyboard navigation**: Arrow keys (↑↓) navigate through the hierarchical UI structure with wrapping. The navigation order is:
  - Settings button (top of screen)
  - Draft task cards (newest first, if any exist)
  - Filter bar separator line (between draft and existing tasks)
  - Existing task cards (active/completed/merged, newest first)
  - Wraps around to settings button when reaching the bottom
- **Visual selection state**: The currently selected element is visually highlighted
- **Index wrapping**: Navigation wraps around the complete navigation hierarchy

The initially focused element is the top draft task card.

##### Completed/Merged Cards (2 lines)

```
✓ Task title in card border
Repository • Branch • Agent • Timestamp • Delivery indicators • Summary of changes
```

The **summary of changes** shows the total impact across all modified files in VS Code-style format:

- Format: `{N} file(s) changed (+{lines_added} -{lines_removed})`
- Example: `3 files changed (+42 -18)`
- Shows net lines added and removed across all files modified during the task

**Delivery indicators** show delivery method outcome with ANSI color coding:

- **Unicode symbols** (default, `tui-font-style = "unicode"`):
  - Branch exists: `⎇` (branch glyph) in `color:branch`
  - PR exists: `⇄` (two-way arrows) in `color:pr`
  - PR merged: `✓` (checkmark) in `color:success`
- **Nerd Font symbols** (`tui-font-style = "nerdfont"`):
  - Branch exists: `` (Powerline branch glyph) in `color:branch`
  - PR exists: `` (nf-oct-git-pull-request) in `color:pr`
  - PR merged: `` (nf-oct-git-merge) in `color:success`
- **ASCII fallback** (`tui-font-style = "ascii"`):
  - Branch exists: `br` in `color:branch`
  - PR exists: `pr` in `color:pr`
  - PR merged: `ok` in `color:success`

**Example output with ANSI color coding:**

```
\033[36m⎇\033[0m feature/payments
\033[33m⇄\033[0m PR #128 — "Add retry logic"
\033[32m✓\033[0m PR #128 merged to main
```

##### Active Cards (5 lines)

```
● Task title • Action buttons
Repository • Branch • Agent • Timestamp • Pause Button • Delete Button
[Activity Row 1 - fixed height]
[Activity Row 2 - fixed height]
[Activity Row 3 - fixed height]
```

**Pause/Delete Buttons Placement**: In the right-most position of the task metadata line. Reachable by pressing the right arrow key when an active task is focused.

##### Draft Cards (Variable height)

Draft cards follow the shared **Task Entry TUI** specification.

> **Reference**: See [`Task-Entry-TUI-PRD.md`](./Task-Entry-TUI-PRD.md) for detailed behavior, layout, and interaction rules.

- **Context**: In the Dashboard, draft cards appear at the top of the task list.
- **Multiple Drafts**: Users can create multiple draft cards (`keys:draft-new-task`).
- **Navigation**: Arrow keys navigate between draft cards and other dashboard elements.

#### Agent Multi-Selector Modal

The agent selection dialog provides advanced agent configuration:

- **Multi-Select Interface**: Select multiple AI agent/model pairs for a single task
- **Instance Counts**: Configure instance counts for each selected model
- **Visual Layout**:
  - At the top of the dialog, there is the same tui-input box as in the repository and branch selection dialogs
  - Separator line (as in the other dialogs)
  - The selection menu is enhanced with right-aligned counts (x1, x2, etc)
  - The count editing buttons (described below) are visible in the status bar while the menu is opened
  - When text is typed into the input box, the models are filtered normally. If there are models that don't match the filder, but have non-zero counts, they are displayed below the models that match the filter.
- **Keyboard Controls**:
  - `keys:navigate-up`/`keys:navigate-down`: Navigate between sections and items
  - `Mouse Wheel`: Scroll through model selection menu
  - `keys:increment-value` or `keys:navigate-right`: Increment instance count
  - `keys:decrement-value` or `keys:navigate-left`: Decrement instance count
  - `keys:activate-current-item`: Close the dialog with the current model and count selections:
    - If the currently selected model has count zero: select ONLY this model with count 1, remove all other model selections
    - If the currently selected model has non-zero count: keep all current non-zero count models with their counts
  - `keys:dismiss-overlay`: Close without applying the special logic for the Enter key. Any changes to counts made while the dialog was opened remain in place. Focus returns to the model picker button.

#### Modal Focus Restoration

When modals are dismissed (via ESC, Enter, or apply actions), focus is **always restored to the task description textarea** to optimize the user workflow. This consistent behavior ensures that after making any selection or adjustment via a modal, the user is immediately returned to the primary text editing context where they can:

- Continue editing the task description
- Quickly launch the task with keyboard shortcuts
- Make further adjustments without additional navigation

This unified focus restoration applies to all modal types:

- **Model Selection Modal**: After selecting models, returns to task description
- **Repository/Branch Selection Modals**: After choosing repo/branch, returns to task description
- **Launch Options Modal**: After reviewing or adjusting launch options, returns to task description
- **Settings Modal**: After configuring settings, returns to task description

This consistent focus restoration ensures a smooth editing workflow where dismissing a modal always returns the user to the primary interaction point for task creation.

### Multi-Agent Task Launching

When a user selects multiple agents with instance counts in the draft task card, the system creates separate multiplexer windows/panes for each agent instance:

- **Local Mode**: Each agent instance gets a unique session ID (e.g., `task-$agent-1`, `task-$agent-2` for two instances of the same agent, or `task-$agent` for different agents)
- **Remote Mode**: Multiple sessions are created via the REST API, with each session corresponding to one agent instance
- **Session ID Generation**: Uses global instance indexing to ensure unique IDs across all launched agent instances
- **Persistence**: Draft tasks store the complete `SelectedModel` vector with counts, allowing restoration of multi-agent selections across sessions

### Settings Dialog

The settings dialog provides comprehensive configuration management through a tabbed interface, allowing users to modify all Agent Harbor preferences. Changes are written to the user-home configuration file and take effect immediately.

#### Settings Dialog Activation

- **Access**: Click settings button in header (upper-right corner) or press `keys:navigate-up` from top draft card
- **Modal Layout**: Full-screen overlay with tabbed interface
- **Footer Scope Selection**: Bottom footer allows choosing configuration scope (user, project, global/system)
- **Persistence**: Changes written to the selected scope's configuration file
- **Real-time Updates**: Changes take effect immediately in the current session

#### Tabbed Interface Structure

The settings dialog uses tabs to organize configuration options:

##### 1. General Options Tab

**Purpose**: Customize terminal user interface appearance and behavior

- **UI Mode**: Default interface (tui/webui)
- **Logging**: Log level, output format
- **Remote Server Aliases**: Default server configuration
- **TUI Appearance**:
  - Theme selection (Catppuccin variants, Nord, Dracula, etc.)
  - Font style (unicode/nerdfont/ascii)
  - High contrast mode toggle
  - Activity lines count per card
  - Card height preferences
  - Selection dialog style (modal/inline)
  - Word wrap settings
  - Native vs normalized output mode
- **TUI Settings**:
  - Autocomplete behavior
  - Scroll behavior
  - Mouse interaction preferences
  - Default multiplexer selection (tmux/zellij/screen/auto)

##### 2. Keyboard Shortcuts Tab

**Purpose**: Customize keyboard bindings for TUI interactions

- **Key Binding Categories**: Organized by functional groups with human-friendly display names:
  - **Cursor Movement**: Arrow keys, Home/End, word navigation ("Move to Beginning of Line", "Move Forward One Character", etc.)
  - **Editing & Deletion**: Backspace, Delete, kill operations ("Delete Character Forward", "Delete Word Backward", etc.)
  - **Text Selection**: Mark, region, word/line selection ("Select All", "Select Word Under Cursor", etc.)
  - **Application Actions**: Task management, navigation ("Create New Draft Task", "Show Launch Options", etc.)
  - **Search & Replace**: Find operations ("Find Next", "Incremental Search Forward", etc.)
  - **Code Editing**: Comment, duplicate, formatting ("Toggle Comment", "Indent Region", etc.)
  - **Formatting**: Text styling ("Bold", "Italic", "Underline")
  - **Mark & Region**: Selection management ("Set Mark", "Transpose Characters", etc.)

- **Key Binding Input Method**: Settings grab keyboard input directly from the user
  - **Replace Current Key**: Press ENTER on a keyboard operation row to replace the current binding
  - **Add Additional Key**: Press SHIFT+ENTER to add an extra shortcut without removing existing ones
  - **Mouse Alternatives**: Left click to replace, SHIFT+left click to add additional shortcut
  - **Visual Feedback**: Clear indication when key grabbing is active ("Press key combination...")

- **Key Binding Display**: Shows current bindings with human-readable operation names
- **Validation**: Prevent conflicting bindings, warn about overrides
- **Reset Options**: Restore defaults, restore factory defaults
- **Key Binding Examples**:
  ```
  Move to Beginning of Line: Ctrl+A, Home
  Move to End of Line: Ctrl+E, End
  Delete Character Forward: Delete, Ctrl+D
  Activate Current Item: Enter
  Create New Draft Task: Ctrl+N
  ```

##### 2. Agent Settings Tab

**Purpose**: Configure agent execution environment and MCP tools

- **Sandbox Environment** (from `ah agent start` options):
  - **Sandbox Profile**: Default isolation level (local/devcontainer/vm/disabled)
  - **Working Copy Mode**: Filesystem isolation strategy (auto/cow-overlay/worktree/in-place)
  - **FS Snapshots Provider**: Snapshot backend selection (auto/zfs/btrfs/agentfs/git/disable)
  - **Network Permissions**: Allow egress, container nesting, VM access
  - **Resource Limits**: Timeout, memory/CPU constraints

- **Execution Options**:
  - **Output Format**: Text vs normalized output modes
  - **Record Sessions**: Persist session recordings toggle
  - **LLM Provider Configuration**: Enabled providers and API keys
  - **Provider Mappings**: Mapping editor for adding tuples (in agent software X, map model X to provider Y's model Z)

- **MCP Tools List**: Available Model Context Protocol tools for agents
  - **Filesystem Access**: File reading, writing, directory operations
  - **Git Operations**: Repository management, commit history
  - **Terminal Commands**: Safe command execution with restrictions
  - **Web Access**: HTTP requests, API calls (when permitted)
  - **Database Access**: SQL query execution for supported databases
  - **Tool Enable/Disable**: Toggle individual MCP tools on/off

##### 3. Active Agents Tab

**Purpose**: Select active agent/model pairs available in task creation

- **Agent/Model Selection**: Multi-select interface for available agent types and models
  - **Core Agents**: Claude, Codex, OpenHands, Cursor, Windsurf, Zed, Copilot
  - **Experimental Agents**: Checkbox toggles for experimental features
    - "Enable Gemini (experimental)" - Google Gemini agent
    - "Enable Goose (experimental)" - Block's Goose agent
    - "Enable Jules (experimental)" - Google Jules agent
    - Other experimental agents as they become available

- **Model Availability**: For each agent type, select available models
  - **Claude**: sonnet, haiku, opus (with version selection)
  - **Codex**: gpt-5.1, gpt-5.1-codex, etc

#### Settings Dialog Footer - Configuration Scope Selection

- **Scope Options**: Dropdown or button row at bottom of dialog for scope selection
  - **User**: `~/.config/agent-harbor/config.toml` (personal preferences)
  - **Project**: `<repo>/.agents/config.toml` (project-specific settings)
  - **Repo-User**: `<repo>/.agents/config.user.toml` (project user overrides, VCS-ignored)
  - **Global/System**: `/etc/agent-harbor/config.toml` (admin-enforced, read-only for users)
- **Scope Inheritance**: Changes in higher scopes override lower scopes
- **Visual Indicators**: Current scope highlighted, read-only scopes clearly marked
- **Scope Warnings**: Clear warnings when modifying shared configurations

#### Settings Dialog Navigation

- **Tab Navigation**: `keys:navigate-left`/`keys:navigate-right` or mouse clicks to switch tabs
- **Within Tab**: `keys:navigate-up`/`keys:navigate-down` navigate options, `keys:navigate-left`/`keys:navigate-right` modify values
- **Modal Controls**: `keys:dismiss-overlay` to cancel changes, `keys:activate-current-item` to save, `keys:move-to-next-field` to cycle focus
- **Search**: Global search across all settings (`keys:incremental-search-forward`)
- **Reset**: Per-setting or tab-wide reset to defaults
- **Help**: Context-sensitive help for each setting

#### Settings Persistence

- **File Location**: Follows standard configuration hierarchy (user scope)
- **Atomic Writes**: Changes written atomically to prevent corruption
- **Backup**: Automatic backup of previous configuration before changes
- **Validation**: Schema validation before writing to disk
- **Error Handling**: Clear error messages for invalid configurations
- **Reload**: Automatic reload of configuration without restart

### Configuration

Default agent selections are loaded from configuration files following the standard [Agent Harbor configuration hierarchy](./Configuration.md):

```toml
default-agents = [
  { software = "claude", model = "sonnet", count = 1 },
  { software = "codex", model = "gpt-5", count = 2 }
]
```

Configuration supports:

- **Global defaults**: Agents applied to all repositories
- **Instance counts**: Number of instances to launch for each agent

### Activity Display for Active Tasks

Active task cards show live streaming of agent activity with exactly 3 fixed-height rows displaying the most recent events:

**Activity Row Requirements:**

- Fixed height rows: Each of the 3 rows has fixed height (prevents UI "dancing")
- Scrolling effect: New events cause rows to scroll upward (newest at bottom)
- Always 3 rows visible: Shows the 3 most recent activity items at all times
- Never empty: Always displays events, never shows "waiting" state

**Event Types and Display Rules:**

1. **Thinking Event** (`thought` property):
   - Format: `"Thoughts: {thought text}"`
   - Behavior: Scrolls existing rows up, appears as new bottom row
   - Single line display

2. **Tool Use Start** (`tool_name` property):
   - Format: `"Tool usage: {tool_name}"`
   - Behavior: Scrolls existing rows up, appears as new bottom row

3. **Tool Last Line** (`tool_name` + `last_line` properties):
   - Format: `"  {last_line}"` (indented, showing command output)
   - **Special behavior**: Updates the existing tool row IN PLACE without scrolling
   - Does NOT create a new row - modifies the current tool execution row

4. **Tool Complete** (`tool_name` + `tool_output` + `tool_status` properties):
   - Format: `"Tool usage: {tool_name}: {tool_output}"` (single line with status indicator)
   - Behavior: Sent immediately after last_line event
   - The last_line row is removed and replaced by this completion row

5. **File Edit Event** (`file_path` property):
   - Format: `"File edits: {file_path} (+{lines_added} -{lines_removed})"`
   - Behavior: Scrolls existing rows up, appears as new bottom row

**Visual Behavior Example:**

```
Initial state (empty):
  [Waiting for agent activity...]

After "thought" event:
  Thoughts: Analyzing codebase structure

After "tool_name" event (scrolls up):
  Thoughts: Analyzing codebase structure
  Tool usage: search_codebase

After "last_line" event (updates in place - NO scroll):
  Thoughts: Analyzing codebase structure
  Tool usage: search_codebase
    Found 42 matches in 12 files

After "tool_output" event (replaces last_line row):
  Thoughts: Analyzing codebase structure
  Tool usage: search_codebase: Found 3 matches

After new "thought" event (scrolls up, oldest row disappears):
  Tool usage: search_codebase: Found 3 matches
  Thoughts: Now examining the authentication flow
```

**Implementation Requirements:**

- The number of activity rows is fixed through the configuration variable `tui.active-sessions-activity-rows` (defaults to 3)
- Fixed row height (no dynamic height based on content)
- Smooth scroll-up animation when new events arrive (except last_line)
- Text truncation with ellipsis if content exceeds row width
- Visual distinction between different event types (icons, indentation)

**Symbol selection logic:**

- Auto-detect terminal capabilities (check `$TERM_PROGRAM`, test glyph width)
- Default to Unicode symbols, fall back to ASCII if Unicode support is limited
- Users can override with `tui-font-style` config option
- Always pair symbols with descriptive text for accessibility and grep-ability

#### Footer Shortcuts (Lazygit-style)

Single-line footer without borders showing context-sensitive shortcuts that change dynamically based on application state:

- **Task feed focused**: "keys:navigate-up/keys:navigate-down Navigate • keys:activate-current-item Select Task • keys:quit x2 Quit"
- **Draft card selected**: "keys:navigate-up/keys:navigate-down Navigate • keys:activate-current-item Edit Draft • keys:quit x2 Quit"
- **Draft textarea focused**: "keys:show-launch-options Advanced Options • keys:activate-current-item Launch Agent(s) • keys:open-new-line New Line • keys:indent-or-complete Complete/Next Field"
- **Active task focused**: "keys:navigate-up/keys:navigate-down Navigate • keys:activate-current-item Show Task Progress • keys:quit x2 Quit"
- **Completed/merged task focused**: "keys:navigate-up/keys:navigate-down Navigate • keys:activate-current-item Show Task Details • keys:quit x2 Quit"
- **Modal active**: "keys:navigate-up/keys:navigate-down Navigate • keys:activate-current-item Select • keys:dismiss-overlay Back"

**Shortcut behavior notes:**

- "Agent(s)" adjusts to singular/plural based on number of selected agents
- Enter key launches the task when in draft textarea (calls Go button action)
- Shift+Enter creates a new line in the text area

#### Draft Auto-Save Behavior

- **Request Tracking**: Each save attempt is assigned a unique request ID to track validity
- **Request Invalidation**: When user types while a save request is pending, that request becomes "invalidated"
- **Save Timing**: Save requests are sent only after 500ms of continuous inactivity
- **Concurrent Typing Protection**: Ongoing typing invalidates previous save requests
- **Response Handling**: Save confirmations for invalidated requests are ignored if newer changes exist
- **Local Storage**: Drafts are persisted to local storage with automatic restoration across sessions

### Task Management

- Task list shows draft tasks at the top, then recent completed/merged and active tasks ordered by recency (newest first)
- Each task displays with appropriate visual indicators for its state
- Draft tasks are saved locally and can be resumed later
- New task input supports multiline editing with Shift+Enter for line breaks
- Default values for repository/branch/agent are the last ones used

#### Task Creation Workflow

When a user launches a task from the dashboard, the workflow depends on the backend mode:

##### Local Mode (SQLite Database)

When running in local mode with SQLite database:

1. **Task Creation**: Dashboard collects repository, branch, agent, and task description from draft card
2. **Local Command Execution**: Issues the equivalent of the `ah task` command locally with collected parameters by directly leveraging the `ah-core` crate.
3. **Multiplexer Integration**: Upon successful task creation, TUI creates new multiplexer window with split panes:
   - **Left Pane**: Terminal/editor attached to workspace (may run shell or configured editor)
   - **Right Pane**: Executes `ah agent record` wrapping `ah agent start <task_id>` to launch and record the agent (see [ah-agent-record.md](ah-agent-record.md) for recording details)
4. **Session Monitoring**: Task card in dashboard shows real-time updates via local state and SSE streams
5. **Window Management**: Multiplexer provides windowing environment; TUI coordinates task creation and monitoring across windows

##### Remote Mode (REST API)

When running in remote mode with REST service:

1. **Task Creation**: Dashboard collects repository, branch, agent, and task description from draft card
2. **REST API Call**: Creates task via `POST /api/v1/tasks` with collected parameters
3. **Multiplexer Integration**: Upon successful task creation, TUI may create local multiplexer windows or attach to remote sessions:
   - For local execution: Creates split-pane windows as in local mode
   - For remote execution: May attach to remote multiplexer sessions via SSH
4. **Session Monitoring**: Task card in dashboard shows real-time updates via SSE streams from remote server
5. **Window Management**: Multiplexer provides windowing environment; TUI coordinates task creation and monitoring across local/remote windows

This dual-mode architecture enables the TUI to work seamlessly with both local SQLite-based workflows and remote REST service deployments, while providing a unified dashboard experience that leverages the existing `ah agent start` command infrastructure.

### Commands and Hotkeys

The Agent Activity TUI follows the standard keyboard operations defined in [`TUI-Keyboard-Shortcuts.md`](./TUI-Keyboard-Shortcuts.md).

#### Global Help Dialog

A global help dialog is available in all TUI contexts (Dashboard, Agent Activity, Session Viewer) to provide quick access to keyboard shortcuts.

- **Activation**: Press `keys:show-help` (Default: `?` or `Ctrl+?`).
  - **Availability**:
    - `?`: Active whenever input is not being consumed by a focused text editing component.
    - `Ctrl+?`: Active globally, even within text areas.
- **Content**: Displays a well-formatted, scrollable list of all currently active keyboard shortcuts, grouped by category (e.g., Navigation, Editing, Application Actions).
- **Interaction**:
  - `keys:dismiss-overlay` (Esc) or `keys:show-help` (?) closes the dialog.
  - Arrow keys or Page Up/Down scroll the list.

#### Card List Keyboard Shortcuts

While the focus is on a task card, the user can press Ctrl+W (CUA/PC), Cmd+W (macOS), C-x k (Emacs) to delete the task.

Draft and active cards are deleted without leaving a trace. Deleting an active cards aborts any running agents.

Pressing `keys:stop` on an active card pauses/stops the agent execution.

The delete operation is mapped to archiving the card for completed/merged task. Archived tasks are removed from listings and search results by default.

Both Ctrl+N (CUA/PC), Cmd+N (macOS) create a new draft task card.

#### Handling Arrow Keys within text areas

Within text areas, the up and down arrow keys move the caret within the text area.

- **If the text area is empty**: `Up` and `Down` cycle through the prompt history (equivalent to pressing `keys:history-prev` / `keys:history-next`).
- **If the text area is NOT empty**:
  - `Up` moves the caret up one line. If at the top line, it moves to the start of the line.
  - `Down` moves the caret down one line. If at the bottom line, it moves to the end of the line.
  - These keys do **not** bubble focus to the parent context (e.g., they will not move focus to the dashboard list).

The user can navigate away from the text area by pressing `keys:navigate-up` or `keys:navigate-down`.

#### Mouse Support

The TUI provides comprehensive mouse support alongside keyboard navigation:

- **Mouse Wheel Scrolling**:
  - **Task List**: Scroll through task cards when list exceeds screen height
  - **Popup Menus**: Scroll through options in auto-complete menus, repository/branch/model selectors
  - **Text Areas**: Scroll within long text content in draft task descriptions
  - **Activity Display**: Scroll through agent activity history in active task cards

- **Mouse Clicking**:
  - **Card Selection**: Click on any task card to select it
  - **Button Activation**: Click on buttons (Repository, Branch, Model, Go) to activate them
  - **Text Area Focus**: Click in draft text areas to focus and position cursor
- **Single Click Inside Text Area**: Moves the caret to the precise character closest to the pointer location, honoring horizontal padding and wide glyph widths
- **Click and Drag Inside Text Area**: Click at a selection start point, move the mouse while holding the button to see live visual feedback, then release the button to complete the selection. The selection expands from the click position to the current mouse position as the mouse moves
- **Double Click Inside Text Area**: Selects the word under the caret using the same token boundaries as keyboard shortcuts
- **Triple Click Inside Text Area**: Selects the entire logical line containing the caret
- **Quadruple Click Inside Text Area**: Selects the entire textarea contents; timing thresholds ensure slow sequential clicks fall back to single-click behavior
- **Menu Selection**: Click on items in popup menus to select them
- **Scrollbar Interaction**: Click and drag scrollbars when visible

- **Mouse Hover**:
  - **Visual Feedback**: Hover effects on interactive elements (buttons, menu items, cards)
  - **Tooltips**: Context-sensitive tooltips for complex UI elements when applicable

Right click is left for the native terminal UI to handle in order to preserve its native context manus.

#### Global Navigation

- **keys:navigate-up/keys:navigate-down**: Navigate between ALL cards (draft tasks first, then sessions newest first)
- **keys:quit** (twice): Quit the TUI

#### Task Selection and Navigation

- **keys:navigate-up/keys:navigate-down**: Navigate between cards with visual selection state
- **keys:activate-current-item**:
  - When on draft card: Focus the textarea for editing
  - When on session card: Navigate to task details page

#### Advanced Keyboard Navigation

**Draft Cards:**

See [`Task-Entry-TUI-PRD.md`](./Task-Entry-TUI-PRD.md) for detailed keyboard navigation within draft cards (text editing and button navigation).

#### Advanced Task Launch Options Modal

The advanced task launch options modal provides comprehensive control over task execution parameters, exposing the full range of options available in the `ah task` and `ah agent start` commands. These options are stored in the draft card and persist for the current TUI lifetime. When you launch a task, the options are preserved for the next draft card, making it easy to launch multiple tasks with the same configuration. Split mode preferences are also remembered for the TUI lifetime. Neither advanced options nor split mode preferences are persisted to disk - they reset when the TUI restarts.

- **Modal Activation**: Click gear button (⚙️) or press `keys:show-launch-options` when in draft textarea
- **Modal Layout**: Two-column layout with options on the left and launch shortcuts/menu on the right

##### Left Column: Task Options

The left column contains grouped configuration options organized by category and is **scrollable** to accommodate all available options:

###### Sandbox & Environment

- **Sandbox Profile**: `local` (default), `devcontainer`, `vm`, `disabled` - Controls isolation level
- **Working Copy Mode**: `auto` (default), `cow-overlay`, `worktree`, `in-place` - Filesystem isolation strategy
- **FS Snapshots**: `auto` (default), `zfs`, `btrfs`, `agentfs`, `git`, `disable` - Snapshot provider selection
- **Devcontainer Path/Tag**: Path to devcontainer configuration or image tag
- **Allow Egress**: `yes`/`no` - Permit network access from sandbox (default: `no`)
- **Allow Containers**: `yes`/`no` - Permit nested container execution (default: `no`)
- **Allow VMs**: `yes`/`no` - Permit nested virtualization (default: `no`)
- **Allow Web Search**: Enable web search capabilities for supported agents

###### Agent Configuration

- **Interactive Mode**: `yes`/`no` - Launch agent in interactive mode (default: `no`)
- **Output Format**: `text`, `text-normalized` - Control output formatting
- **Record Session**: `yes`/`no` - Enable session recording (default: `yes`)
- **Timeout**: Duration limit for agent execution
- **LLM Provider**: A pre-configured LLM provider to use for this session (e.g. OpenRouter)
- **Environment Variables**: Key-value pairs for agent environment

###### Task Management

- **Delivery Method**: `pr`, `branch`, `patch` - How results should be delivered
- **Target Branch**: Branch where results should be delivered
- **Create Task Files**: `yes`/`no` - Control local task file creation (default: `yes`)
- **Create Metadata Commits**: `yes`/`no` - Control metadata-only commits (default: `yes`)
- **Notifications**: `yes`/`no` - Enable OS notifications on completion (default: `yes`)
- **Labels**: Key-value labels for task organization
- **Push to Remote**: `true`/`false` - Automatically push created branches (default: `false`) (PLANNED - DON'T IMPLEMENT YET)
- **Fleet**: Named fleet configuration for multi-OS execution

###### Browser Automation (Cloud Agents) (PLANNED - DON'T IMPLEMENT YET)

- **Browser Automation**: `true`/`false` - Enable/disable browser automation (default: `true`)
- **Browser Profile**: Named browser profile for automation
- **ChatGPT Username**: Username for ChatGPT profile discovery
- **Codex Workspace**: Cloud workspace identifier for Codex
- **Workspace**: Generic workspace identifier for cloud agents

**Option Navigation**: `keys:navigate-left`/`keys:navigate-right` move between left and right columns. `keys:navigate-up`/`keys:navigate-down` navigate within each column. `keys:move-to-next-field` cycles through all controls in left-to-right, top-to-bottom order.

##### Right Column: Launch Shortcuts & Menu

The right column provides launch action selection with keyboard shortcuts:

- **Launch in new tab** - Type `t` when menu is visible (launches the task in a new tab/window in the multiplexer)
- **Launch in split view** - Type `s` when menu is visible (auto-detects vertical/horizontal split based on longer edge)
- **Launch in horizontal split** - Type `h` when menu is visible (creates horizontal split pane)
- **Launch in vertical split** - Type `v` when menu is visible (creates vertical split pane)
- **Focus variants**: Capital letters `T`, `S`, `H`, `V` launch and automatically focus the new task window/pane

**Launch Menu Navigation**: `keys:navigate-up`/`keys:navigate-down` navigate between launch options. `keys:activate-current-item` selects the highlighted option. Single letters (t/s/h/v) or capitals (T/S/H/V) can be typed directly to select when modal is visible.

- **Modal dismissal and changes**:
  - **'Space' key**: Toggles boolean values and opens enum selection popups for editing option values
  - **'Enter' key**: Applies changes and closes the modal, preserving any modifications made to the launch options (when inside an enum popup, Enter selects the value instead)
  - **'Esc' key**: Discards all changes and restores the original configuration from before the modal was opened
  - **Split launch shortcuts** (t/s/h/v/T/S/H/V): Apply changes, launch the task with the selected split mode, and close the modal
  - **Mouse interactions**: Clickable hint text at the bottom of the modal ("**ENTER** Apply • **Esc** Cancel" on first line, "**SPACE** Edit Value" on second line) provides visual cues and mouse click support for actions
  - **Focus restoration**: After applying changes, canceling, or using split launch shortcuts, focus returns to the task description textarea, allowing the user to immediately continue editing their prompt
- **Default focus**: Left column options when modal opens
- **Visual feedback**: Highlighted selection in both columns, clear keyboard shortcuts displayed, and interactive hint text at bottom with bold key indicators

##### TUI lifetime Persistence Behavior

- **Advanced Options Preservation**: When you configure advanced options and launch a task, those options are automatically applied to the next draft card. This allows you to quickly launch multiple tasks with the same configuration without repeatedly opening the modal. Each draft card maintains its own advanced options, so if you create multiple draft cards (Ctrl+N), each can have different options configured.

- **Split Mode Memory**: The TUI remembers your last selected split mode (t/s/h/v/T/S/H/V) for the current TUI lifetime. When you press Enter or click "Go" without opening the advanced options modal, the task launches using your last selected split mode. This provides a convenient workflow where you can select your preferred split mode once and then launch multiple tasks with the same layout.

- **TUI-Lifetime-Only Storage**: Both advanced options and split mode preferences are stored only in memory during the TUI lifetime. They are not written to configuration files and will reset to defaults when you restart the TUI. This design prevents temporary launch preferences from polluting persistent configuration while still providing convenience within a session.

- **Default Behavior**: If no split mode has been selected in the current TUI lifetime, pressing Enter or clicking "Go" uses the configured default split mode from your settings. The first time you use a split mode shortcut (t/s/h/v/T/S/H/V), that becomes the TUI lifetime default.

##### Configuration Policies as Top-Level Options

To determine its defaults, the advanced launch options modal respects the following configuration policies that can be set as top-level configuration options:

```toml
# Sandbox defaults (CLI.md section reference)
sandbox = "local"
working-copy = "auto"
fs-snapshots = "auto"
allow-egress = false
allow-containers = false
allow-vms = false

# Agent defaults
non-interactive = false
record-output = true
notifications = true
browser-automation = true

# Task management defaults
create-task-files = true
create-metadata-commits = true
delivery = "pr"
push-to-remote = false

# Browser automation defaults
browser-profile = "default"
chatgpt-username = ""
codex-workspace = ""
workspace = ""

# Fleet defaults
fleet = "default"
```

These policies serve as defaults in the advanced launch options modal but can be overridden per-task. They are stored in the standard Agent Harbor configuration hierarchy (user/system/repo/repo-user scopes).

#### Modal Navigation (Telescope-style)

- **keys:navigate-up`/`keys:navigate-down**: Navigate through options in fuzzy search
- **keys:activate-current-item**: Select current item
- **keys:dismiss-overlay**: Close modal
- **keys:navigate-left`/`keys:navigate-right** or **keys:increment-value`/`keys:decrement-value**: Adjust model instance counts in model selection

#### Advanced Launch Options Modal Navigation

The advanced launch options modal uses a two-column navigation system:

- **Left/Right Arrows**: Move between left column (options) and right column (launch menu)
- **Up/Down Arrows**: Navigate within the current column
- **Tab**: Cycle through all controls in left-to-right, top-to-bottom order
- **Space key**: Toggle boolean values and open enum selection popups in the options column
- **Enter key**: Apply changes and close modal (or select value when inside enum popup), preserving any modifications made to launch options
- **'Esc' key**: Discard changes and restore original configuration, then close modal
- **Mouse clicks**: Clickable hint text at bottom of modal provides visual feedback and mouse interaction support for Apply and Cancel actions
- **Shortcut Keys**: Single letters (t/s/h/v) or capitals (T/S/H/V) directly select launch options and immediately launch the task
- **Focus Behavior**: Modal opens with focus in left column; maintains focus position when switching columns; returns focus to task description textarea after closing
- **Visual Hints**: Single-line hint display at bottom: "ENTER Apply • Esc Cancel • SPACE Edit Value" with semantic color coding (success/error/primary colors for keys, bold styling for emphasis)

### Real-Time Behavior

#### Live Event Streaming

- Active task cards continuously update with agent activity events
- Events sent and processed one at a time for smooth UI updates
- Reconnect logic with exponential backoff for network interruptions
- Buffer events during connection blips to prevent data loss

### Error Handling and Status

- Inline validation messages under selectors (e.g., branch not found, agent unsupported).
- Status bar shows backend (`local`/`<remote-server-hostname>`), and last operation result.
- **Non-intrusive error notifications**: Temporary status messages for failed operations that don't interrupt workflow

### Remote Sessions

- If the REST service indicates the task will run on another machine, the TUI uses provided SSH details to create/attach a remote multiplexer window.

### Persistence

The TUI maintains user selections and preferences across sessions using multiple storage mechanisms:

#### Database-Persisted State

The local database stores session-specific selections that change frequently:

- **Agent Selector State**: Last selected agents with counts, used as defaults for new draft task cards
- **Repository and Branch Selections**: Per-repository preferences for branch and repository choices
- **Draft Task Persistence**: Complete draft states with auto-save and restoration across sessions

For detailed technical information about storage mechanisms, database schemas, and persistence implementation, see [State-Persistence.md](./State-Persistence.md).

#### Configuration-Persisted State

User configuration files store preferences that are set less frequently:

- **Theme Preferences**: Selected theme preference is persisted in the user home configuration.

For detailed information about configuration file locations, precedence rules, and available configuration options, see [Configuration.md](./Configuration.md).

### Visual Design & Theming

#### Charm-Inspired Aesthetics

The TUI follows Charm (Bubble Tea/Lip Gloss) design principles with multiple theme options:

- **Default Theme**: Catppuccin Mocha - Dark theme with cohesive colors
  - Background: `color:bg`
  - Surface/Card backgrounds: `color:surface`
  - Text: `color:text`
  - Primary: `color:primary` (blue for actions)
  - Accent: `color:accent` (green for success)
  - Muted: `color:muted` (secondary text)
- **Multiple Theme Support**: Users can choose from various themes including:
  - Catppuccin variants (Latte, Frappe, Macchiato, Mocha)
  - Other popular dark themes (Nord, Dracula, Solarized Dark, etc.)
  - High contrast accessibility theme
- **Rounded borders**: `BorderType::Rounded` on all cards and components
- **Proper padding**: Generous spacing with `Padding::new()` for breathing room
- **Powerline-style titles**: ` Title ` glyphs for card headers
- **Truecolor support**: 24-bit RGB colors for rich visual experience

#### Component Styling

- **Cards**: Rounded borders, themed backgrounds, proper padding
- **Buttons**: Background color changes on focus, bold text
- **Modals**: Shadow effects, centered positioning, fuzzy search interface
- **Status indicators**: Color-coded icons (✓ completed, ● active, 📝 draft)

#### Selection Dialog Styles

The TUI supports two distinct styles for selection interfaces:

#### Modal Dialog Styling

Modal dialogs use a clean, minimal design:

- **Single Border**: Outer rounded border provides sufficient visual containment
- **Separator Lines**: Horizontal separator lines divide content sections instead of nested rectangles
- **Contextual Instructions**: Clear labels and instructions for user actions
- **Consistent Theming**: Follows Charm-inspired design with proper color usage
- **Shadows**: The dialog drops shadow over the underlying content

#### Input Handling Libraries

The TUI uses specialized Ratatui ecosystem crates for enhanced input handling:

- **tui-textarea**: Multi-line text editing with advanced features
- **tui-input**: Single-line input for modals

##### Modal Selection Dialogs

Used by draft task controls (Repository, Branch, Model selectors):

- **Full-screen overlay**: Dialog appears over existing content with background dimming
- **Dedicated input box**: Separate fuzzy search input at top of dialog
- **Focused interaction**: All input goes to the modal until dismissed
- **Clear boundaries**: Distinct visual separation from underlying interface
- **Navigation**: ESC to cancel, Enter to confirm selection, arrow keys for options
- **Use cases**: Complex selections requiring focus and search functionality

##### Inline Selection Dialogs

Used by existing tasks filter controls:

- **In-place expansion**: Options are displayed immediately after selecting the filter control.
- **Relative dialog position**: The dialog is placed on top of the filter control in such a way that the previously displayed filter value falls precisely in the same place as the now editable input box of the selection dialog.
- **Consistent style**: The style of the dialog resembles the style used for the auto-completion menu or the modal dialogs. Use uses rounded corners and a thin separator line between the input box and the selection choices.
- **Interactive results**: As the user types within the input box, the list of possible selections is filtered immediately with fuzzy search.
- **Rendered last**: In order for the dialogs to be displayed on top of all other screen content, they are rendered last.

##### Configuration Option

The dialog style preference can be configured via `tui.selection-dialog-style`:

- `modal` (default): Use modal dialogs for all selection interfaces
- `inline`: Use inline dialogs for all selection interfaces where possible
- `default`: Each dialog uses the style prescribed by the designers of the Agent Harbor interface.

### Accessibility

- **Theme Selection**: Multiple themes including high-contrast accessibility theme
- **High-contrast theme option**: Enhanced contrast ratios for better visibility
- **Full keyboard operation**: All features accessible without mouse
- **Predictable focus order**: Logical tab navigation through all interactive elements
- **Charm theming**: Provides excellent contrast ratios and visual hierarchy
