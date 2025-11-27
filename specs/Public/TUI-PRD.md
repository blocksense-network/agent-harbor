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

#### Handling Arrow Keys within text areas

Within text areas, the up and down arrow keys move the caret within the text area when this is possible. Only when they have exhausted the possible movements (i.e. the caret is already on the top line when moving up, or already on the bottom line when moving down), the focus should be moved to the next navigation item in the hierarchy (settings button, draft cards, filter bar, existing tasks, etc).

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
  - Branch exists: `⎇` (branch glyph) in **cyan**
  - PR exists: `⇄` (two-way arrows) in **yellow**
  - PR merged: `✓` (checkmark) in **green**
- **Nerd Font symbols** (`tui-font-style = "nerdfont"`):
  - Branch exists: `` (Powerline branch glyph) in **cyan**
  - PR exists: `` (nf-oct-git-pull-request) in **yellow**
  - PR merged: `` (nf-oct-git-merge) in **green**
- **ASCII fallback** (`tui-font-style = "ascii"`):
  - Branch exists: `br` in **cyan**
  - PR exists: `pr` in **yellow**
  - PR merged: `ok` in **green**

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

Variable height cards with auto-expandable text area and controls (keyboard navigable, Enter to submit):

- Shows placeholder text when empty: "Describe what you want the agent to do..."
- Always-visible text area for task description with expandable height
- Single line of compact controls below the text area:
  - Left side: Repository Selector, Branch Selector, Model Selector (horizontally laid out)
  - Right side: "⏎ Go" button and "⚙️" advanced options button (right-aligned, horizontally adjacent)
- **Model Selector Button Display**: Shows selected models as comma-separated list with instance counts
  - Format: "🤖 model1, model2 x2, model3 x3" (shows "xN" only when count > 1)
  - Falls back to "🤖 Models" when no models are selected
- **Agent Selection Dialogs**: When buttons are activated (Tab/Enter), display overlay dialog windows
  - Repository Selector: Fuzzy search through available repositories
  - Branch Selector: Fuzzy search through repository branches
  - Agent Multi-Selector: Multi-select interface with instance counts and +/- controls
  - **Characteristics**: Full-screen overlay, dedicated input box, ESC to cancel, Enter to confirm
- **Advanced Options Menu**: Gear button (⚙️) opens a modal displaying launch options with visible keyboard shortcuts. Options are stored in the draft card and persist for the session. Split mode selections (t/s/h/v/T/S/H/V) are remembered and applied as the default for subsequent launches via Enter or Go button.
- TAB navigation between controls (Repository → Branch → Model → Go → Advanced Options → wrap around)
- Multiple draft tasks supported - users can create several draft tasks in progress
- Auto-save drafts to local storage and restore across sessions (debounced, 500ms delay)
- Default values from last used selections
- **Auto-completion support** with popup menu:
  - `@filename` - Auto-completes file names within the repository
  - `/workflow` - Auto-completes available workflow commands from `.agents/workflows/`
  - **Immediate menu opening**: Menu opens immediately when trigger characters (`/` or `@`) are typed, showing cached results while background refresh occurs
  - **Caching and refresh**: File and workflow lists are cached for instant display. When trigger characters are typed, cached results are shown immediately, and a background refresh is triggered. When refresh completes, results are updated automatically, preserving the currently selected item if it exists in the refreshed results (matched by name)
  - **Fuzzy matching**: All autocomplete suggestions use fuzzy matching for efficient filtering as the user types
  - **Popup menu navigation**: Tab or arrow keys to navigate suggestions, mouse wheel to scroll, Mouse click or Enter to select
  - **Quick selection**: Right arrow key selects the currently active suggestion
  - **Ghost text**: Currently active suggestion appears as dimmed/ghost text in the text area
  - Inline completions now present **two dimmed segments**: the shared continuation (characters guaranteed across every match) and the shortest possible completion. The shared portion uses a lighter dim + muted color, while the full completion remainder uses a slightly brighter dim + normal text color so the two steps are visually distinguishable.
  - **Workspace terms menu**: The background `WorkspaceTermsEnumerator` continuously indexes repository tokens. When the `workspace_terms_menu` preference is enabled (default), typing a regular token (two or more characters without a trigger) opens a `Workspace Terms` popup that lists the ranked matches. The menu uses the same navigation semantics as the `/` and `@` menus and Right Arrow accepts the currently highlighted entry.
  - **Autocomplete active mode**: Whenever a ghost suggestion or popup menu entry is visible, the input system switches into an `Autocomplete Active` mode that exposes the `IndentOrComplete` operation (Tab by default). This mode is consulted before any of the standard navigation modes, so Tab inserts the suggested text. As soon as no suggestion is present, Tab reverts to its usual "next field" navigation role automatically.
  - While the menu is focused, the inline ghost text mirrors the currently selected term—users always see exactly what the next Tab/Right Arrow action will insert. Moving the highlight immediately updates the ghost text so that the “longer” completion segment reflects the candidate under the cursor.
  - Users who prefer ghost-only completions can disable the menu by setting `workspace_terms_menu = false` in their settings; inline completions (shared + shortest segments) remain available even when the popup is hidden.
  - Pressing `Tab` inside the textarea inserts the shared continuation when one exists. Pressing `Tab` again (without moving the caret) inserts the remainder needed to reach the shortest matching term. When the lookup returns a single match, both segments are identical so one press is enough. Tab completion takes precedence over focus navigation—only when no inline completion is available does `Tab` move the cursor to the next form control.
  - These completions are powered by the `WorkspaceTermsEnumerator::lookup` results, which expose both the shared suffix and the shortest completion suffix for any prefix. The inline ghost renderer consumes those values directly and never needs to re-scan the filesystem on every keystroke.
- **Auto-save status indicators** in text area corners (low-contrast/dimmed text):
  - **Unsaved** (gray): User has typed but no save request is in flight OR current in-flight request is invalidated
  - **Saving...** (yellow): There is a valid (non-invalidated) save request currently in flight
  - **Saved** (green): No pending changes AND most recent save request completed successfully
  - **Error** (red): Most recent save request failed and no new typing has occurred
- Context-sensitive keyboard shortcuts:
  - While focus is inside a draft text area, footer shows: "Enter Launch Agent(s) • Shift+Enter New Line • Tab Complete/Next Field" to reflect the two-step completion behavior described above. When no inline completion is available, `Tab` falls back to navigating to the next field.
  - "Agent(s)" is plural if multiple agents are selected
  - Enter key launches the task (calls Go button action)
  - Shift+Enter creates a new line in the text area

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
  - `↑↓`: Navigate between sections and items
  - `Mouse Wheel`: Scroll through model selection menu
  - `Shift+=` or `Right`: Increment instance count
  - `-` or `Left`: Decrement instance count
  - `Enter`: Close the dialog with the current model and count selections:
    - If the currently selected model has count zero: select ONLY this model with count 1, remove all other model selections
    - If the currently selected model has non-zero count: keep all current non-zero count models with their counts
  - `Esc`: Close without applying the special logic for the Enter key. Any changes to counts made while the dialog was opened remain in place. Focus returns to the model picker button in both cases.

### Multi-Agent Task Launching

When a user selects multiple agents with instance counts in the draft task card, the system creates separate multiplexer windows/panes for each agent instance:

- **Local Mode**: Each agent instance gets a unique session ID (e.g., `task-$agent-1`, `task-$agent-2` for two instances of the same agent, or `task-$agent` for different agents)
- **Remote Mode**: Multiple sessions are created via the REST API, with each session corresponding to one agent instance
- **Session ID Generation**: Uses global instance indexing to ensure unique IDs across all launched agent instances
- **Persistence**: Draft tasks store the complete `SelectedModel` vector with counts, allowing restoration of multi-agent selections across sessions

### Settings Dialog

The settings dialog provides comprehensive configuration management through a tabbed interface, allowing users to modify all Agent Harbor preferences. Changes are written to the user-home configuration file and take effect immediately.

#### Settings Dialog Activation

- **Access**: Click settings button in header (upper-right corner) or press Up arrow from top draft card
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

- **Tab Navigation**: Left/Right arrows or mouse clicks to switch tabs
- **Within Tab**: Up/Down arrows navigate options, Left/Right modify values
- **Modal Controls**: ESC to cancel changes, Enter to save, Tab to cycle focus
- **Search**: Global search across all settings (Ctrl+F)
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

- **Task feed focused**: "↑↓ Navigate • Enter Select Task • Ctrl+C x2 Quit"
- **Draft card selected**: "↑↓ Navigate • Enter Edit Draft • Ctrl+C x2 Quit"
- **Draft textarea focused**: "Ctrl+Enter Advanced Options • Enter Launch Agent(s) • Shift+Enter New Line • Tab Complete/Next Field"
- **Active task focused**: "↑↓ Navigate • Enter Show Task Progress • Ctrl+C x2 Quit"
- **Completed/merged task focused**: "↑↓ Navigate • Enter Show Task Details • Ctrl+C x2 Quit"
- **Modal active**: "↑↓ Navigate • Enter Select • Esc Back"

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

- **↑↓**: Navigate between ALL cards (draft tasks first, then sessions newest first)
- **Ctrl+C** (twice): Quit the TUI

#### Task Selection and Navigation

- **↑↓**: Navigate between cards with visual selection state
- **Enter**:
  - When on draft card: Focus the textarea for editing
  - When on session card: Navigate to task details page

#### Advanced Keyboard Navigation

**Button Navigation in Draft Cards:**

- `Tab` or `Right`: Repository → Branch → Model → Go → Advanced Options → (wrap to Repository)
- `Shift+Tab` or `Left`: Advanced Options → Go → Model → Branch → Repository → (wrap to Go)
- `Esc` on buttons: Return focus to text area (don't exit application)

**Text Area Focus:**

- `Enter`: Launch task (same as Go button)
- `Shift+Enter`: Insert new line
- `Tab`: Move to next button
- `Esc`: Remove current focus. If none was focused, exit the application
- `↑` inside textarea:
  - If the caret is not already at column 0 of the first line, the first press moves it to the start of that line
  - When the caret is already at the first column of the first line, the operation bubbles to dashboard navigation (focus shifts to the previous UI element)
- `↓` inside textarea:
  - If the caret is not at the end of the last line, the first press moves it to the end of that line
  - When the caret is already at the end of the last line, the operation bubbles to dashboard navigation (focus shifts to the next UI element)

#### Draft Task Editing

- **Tab/Shift+Tab**: Cycle between buttons (Repository, Branch, Models, Advanced Options, Go) when not in textarea
- **Enter**: Activate focused button or select item in modal (when in textarea: launch task)
- **Esc**: Close modal or go back to navigation mode
- **Shift+Enter**: Create new line in textarea (when focused)
- **Ctrl+Enter**: Show advanced launch options menu (when in textarea)
- **Any key**: Type in description area when focused
- **Backspace**: Delete characters
- **Auto-complete menu**: When certain characters like / or @ are entered in the text area, show auto-completion menu with dynamically populated choices (@ for citing files, / for selecting workflows, etc)

#### Advanced Task Launch Options Modal

The advanced task launch options modal provides comprehensive control over task execution parameters, exposing the full range of options available in the `ah task` and `ah agent start` commands. These options are stored in the draft card and persist for the current TUI session. When you launch a task, the options are preserved for the next draft card, making it easy to launch multiple tasks with the same configuration. Split mode preferences are also remembered for the session. Neither advanced options nor split mode preferences are persisted to disk - they reset when the TUI restarts.

- **Modal Activation**: Click gear button (⚙️) or press Ctrl+Enter when in draft textarea
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

**Option Navigation**: Left/Right arrow keys move between left and right columns. Up/Down arrows navigate within each column. Tab cycles through all controls in left-to-right, top-to-bottom order.

##### Right Column: Launch Shortcuts & Menu

The right column provides launch action selection with keyboard shortcuts:

- **Launch in new tab** - Type `t` when menu is visible (launches the task in a new tab/window in the multiplexer)
- **Launch in split view** - Type `s` when menu is visible (auto-detects vertical/horizontal split based on longer edge)
- **Launch in horizontal split** - Type `h` when menu is visible (creates horizontal split pane)
- **Launch in vertical split** - Type `v` when menu is visible (creates vertical split pane)
- **Focus variants**: Capital letters `T`, `S`, `H`, `V` launch and automatically focus the new task window/pane

**Launch Menu Navigation**: Arrow keys navigate between launch options. Enter selects the highlighted option. Single letters (t/s/h/v) or capitals (T/S/H/V) can be typed directly to select when modal is visible.

- **Modal dismissal**: ESC closes without launching
- **Default focus**: Left column options when modal opens
- **Visual feedback**: Highlighted selection in both columns, clear keyboard shortcuts displayed

##### Session Persistence Behavior

- **Advanced Options Preservation**: When you configure advanced options and launch a task, those options are automatically applied to the next draft card. This allows you to quickly launch multiple tasks with the same configuration without repeatedly opening the modal. Each draft card maintains its own advanced options, so if you create multiple draft cards (Ctrl+N), each can have different options configured.

- **Split Mode Memory**: The TUI remembers your last selected split mode (t/s/h/v/T/S/H/V) for the current session. When you press Enter or click "Go" without opening the advanced options modal, the task launches using your last selected split mode. This provides a convenient workflow where you can select your preferred split mode once and then launch multiple tasks with the same layout.

- **Session-Only Storage**: Both advanced options and split mode preferences are stored only in memory during the TUI session. They are not written to configuration files and will reset to defaults when you restart the TUI. This design prevents temporary launch preferences from polluting persistent configuration while still providing convenience within a session.

- **Default Behavior**: If no split mode has been selected in the current session, pressing Enter or clicking "Go" uses the configured default split mode from your settings. The first time you use a split mode shortcut (t/s/h/v/T/S/H/V), that becomes the session default.

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

- **↑↓**: Navigate through options in fuzzy search
- **Enter**: Select current item
- **Esc**: Close modal
- **Left/Right** or **Shift+= / -**: Adjust model instance counts in model selection

#### Advanced Launch Options Modal Navigation

The advanced launch options modal uses a two-column navigation system:

- **Left/Right Arrows**: Move between left column (options) and right column (launch menu)
- **Up/Down Arrows**: Navigate within the current column
- **Tab**: Cycle through all controls in left-to-right, top-to-bottom order
- **Enter**: Activate selected option or launch with selected method
- **Esc**: Close modal without launching
- **Shortcut Keys**: Single letters (b/s/h/v) or capitals (B/S/H/V) directly select launch options
- **Focus Behavior**: Modal opens with focus in left column; maintains focus position when switching columns

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
  - Background: `#11111B`
  - Surface/Card backgrounds: `#242437`
  - Text: `#CDD6F4`
  - Primary: `#89B4FA` (blue for actions)
  - Accent: `#A6E3A1` (green for success)
  - Muted: `#7F849C` (secondary text)
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

#### Text Area Shortcuts

All inputs should have appropriate placeholder text.
Text inputs should support a combination of CUA, macOS and Emacs key bindings.
The user can override any of the default key bindings through configuration variables listed below.
All such variables are in under the "[tui.keymap]" section.

## Configuration Variable Mapping

| Category                        | Operation                                     | Config Variable                  | Key Bindings                                                                    |
| ------------------------------- | --------------------------------------------- | -------------------------------- | ------------------------------------------------------------------------------- |
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
|                                 | Recenter screen on cursor                     | `recenter-screen-on-cursor`      | C-l (Emacs)                                                                     |
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
|                                 | Launch and focus                              | `launch-and-focus`               | No Default Shortcut                                                             |
|                                 | Launch in split view                          | `launch-in-split-view`           | No Default Shortcut                                                             |
|                                 | Launch in split view and focus                | `launch-in-split-view-and-focus` | No Default Shortcut                                                             |
|                                 | Launch in horizontal split                    | `launch-in-horizontal-split`     | No Default Shortcut                                                             |
|                                 | Launch in vertical split                      | `launch-in-vertical-split`       | No Default Shortcut                                                             |
|                                 | Activate current item                         | `activate-current-item`          | Enter                                                                           |
|                                 | Delete current task                           | `delete-current-task`            | Ctrl+W (CUA/PC), Cmd+W (macOS), C-x k (Emacs)                                   |
| **Session Viewer Task Entry**   | Move to next snapshot                         | `move-to-next-snapshot`          | Ctrl+Shift+Down                                                                 |
|                                 | Move to previous snapshot                     | `move-to-previous-snapshot`      | Ctrl+Shift+Up                                                                   |

Note: In the table, "C-" means Control, "M-" means Meta (often Alt/Option), and combinations like "C-M-" use both. Please note that the Meta key should be the Option key on macOS and the Alt key otherwise. This can be overridden with the configuration option `tui.keymap.meta-key`.

### Emacs Key Binding Conflicts

Several traditional Emacs key bindings (C-b, C-f, C-n) for cursor movement have been simplified to use only arrow keys in the current implementation due to conflicts with other application shortcuts (C-b conflicts with "Bold", C-f conflicts with "Incremental search forward", C-n conflicts with "Draft new task"). Users who prefer full Emacs-style navigation can configure custom key bindings using the `tui.keymap` configuration section.

### Card List Keyboard Shortcuts

While the focus is on a task card, the user can press Ctrl+W (CUA/PC), Cmd+W (macOS), C-x k (Emacs) to delete the task.

Draft and active cards are deleted without leaving a trace. Deleting an active cards aborts any running agents.

The delete operation is mapped to archiving the card for completed/merged task. Archived tasks are removed from listings and search results by default.

Both Ctrl+N (CUA/PC), Cmd+N (macOS) create a new draft task card.

### Accessibility

- **Theme Selection**: Multiple themes including high-contrast accessibility theme
- **High-contrast theme option**: Enhanced contrast ratios for better visibility
- **Full keyboard operation**: All features accessible without mouse
- **Predictable focus order**: Logical tab navigation through all interactive elements
- **Charm theming**: Provides excellent contrast ratios and visual hierarchy
