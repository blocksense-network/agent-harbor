## TUI — Product Requirements and UI Specification

### Summary

The TUI provides a terminal-first dashboard for launching and monitoring agent tasks, integrated with terminal multiplexers (tmux, zellij, screen). It auto-attaches to the active multiplexer session and assumes all active tasks are already visible as multiplexer windows.

The TUI is built with **Ratatui**, a Rust library for building terminal user interfaces. See specs/Research/TUI for helpful information for developing with Ratatui.

Backends:

- REST: Connect to a remote REST service and mirror the WebUI experience for task creation, with windows created locally (or remotely via SSH) for launched tasks.
- Local: Operate in the current directory/repo using the SQLite state database for discovery and status.

### Auto-Attach and Window Model

- On start, `aw tui` auto-attaches to the configured multiplexer session (creating one if needed) and launches the TUI dashboard in a single window initially. Existing task windows are left intact.
- The TUI dashboard (`aw tui dashboard`) is the main interface for task management and runs inside a multiplexer window.
- Launching a new task from the dashboard creates a new multiplexer window with split panes:
  - Right pane = agent activity and logs, left pane = terminal or configured editor in the workspace.
  - Devcontainer and remote-server runs: panes are inside the container/remote context.
- The multiplexer provides the windowing environment; the TUI dashboard coordinates task creation and monitoring across windows.

### Simplified Task-Centric Layout

The dashboard screen has the following elements:

- **Header**: Agent Harbor branding
  - Displays image logo when terminal supports modern image protocols (e.g., Kitty, iTerm2)
  - Falls back to ASCII art logo for terminals without image support

- **Tasks**: Chronological list of recent tasks (completed/merged, active, draft) displayed as bordered cards, with draft tasks always visible at the top, sorted newest first.
  - Uses 1 character of padding between screen edges and cards for clean visual spacing.

- **Footer**: Displays context-specific keyboard shortcuts.

#### Task States and Card Layouts

Tasks display in four different states with optimized heights and consistent layout principles:

- **Fixed height for completed/active cards**: Completed and active cards maintain constant height regardless of content to prevent UI jumping
- **Variable height for draft cards**: Draft cards expand/contract with the text area for better editing experience
- **Compact layout**: All metadata (repo, branch, agent, timestamp) fits on single lines
- **Status indicators**: Color-coded icons with symbols controlled by `tui-font-style` config
- **Visual separators** between cards
- **Keyboard navigation**: Arrow keys (↑↓) navigate between ALL cards (draft tasks first, then sessions newest first) with visual selection state. The index of the selected element wraps around the list edges.

##### Completed/Merged Cards (2 lines)
```
✓ Task title • Delivery indicators
Repository • Branch • Agent • Timestamp
```

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
Repository • Branch • Agent • Timestamp
[Activity Row 1 - fixed height]
[Activity Row 2 - fixed height]
[Activity Row 3 - fixed height]
```

##### Draft Cards (Variable height)

Variable height cards with text area and controls (keyboard navigable, Enter to submit):

- Shows placeholder text when empty: "Describe what you want the agent to do..."
- Always-visible text area for task description with expandable height
- Single line of compact controls below the text area:
  - Left side: Repository Selector, Branch Selector, Model Selector (horizontally laid out)
  - Right side: "⏎ Go" button (right-aligned)
- Telescope-style modals: When buttons are activated (Tab/Enter), display fuzzy search dialogs
  - Repository Selector: Fuzzy search through available repositories
  - Branch Selector: Fuzzy search through repository branches
  - Model Multi-Selector: Multi-select interface with instance counts and +/- controls
- TAB navigation between controls
- Multiple draft tasks supported - users can create several draft tasks in progress
- Auto-save drafts to local storage and restore across sessions (debounced, 500ms delay)
- Default values from last used selections
- **Auto-completion support** with popup menu:
  - `@filename` - Auto-completes file names within the repository
  - `/workflow` - Auto-completes available workflow commands from `.agents/workflows/`
  - **Popup menu navigation**: Tab or arrow keys to navigate suggestions, Enter to select
  - **Quick selection**: Right arrow key selects the currently active suggestion
  - **Ghost text**: Currently active suggestion appears as dimmed/ghost text in the text area
- **Auto-save status indicators** in text area corners (low-contrast/dimmed text):
  - **Unsaved** (gray): User has typed but no save request is in flight OR current in-flight request is invalidated
  - **Saving...** (yellow): There is a valid (non-invalidated) save request currently in flight
  - **Saved** (green): No pending changes AND most recent save request completed successfully
  - **Error** (red): Most recent save request failed and no new typing has occurred
- Context-sensitive keyboard shortcuts:
  - While focus is inside a draft text area, footer shows: "Enter Launch Agent(s) • Shift+Enter New Line • Tab Next Field"
  - "Agent(s)" is plural if multiple agents are selected
  - Enter key launches the task (calls Go button action)
  - Shift+Enter creates a new line in the text area

##### Activity Display for Active Tasks

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
- Maximum 3 rows displayed at all times
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
- **Draft textarea focused**: "Enter Launch Agent(s) • Shift+Enter New Line • Tab Next Field"
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

### Commands and Hotkeys

#### Global Navigation
- **↑↓**: Navigate between ALL cards (draft tasks first, then sessions newest first)
- **Ctrl+C** (twice): Quit the TUI

#### Task Selection and Navigation
- **↑↓**: Navigate between cards with visual selection state
- **Enter**:
  - When on draft card: Focus the textarea for editing
  - When on session card: Navigate to task details page

#### Draft Task Editing
- **Tab/Shift+Tab**: Cycle between buttons (Repository, Branch, Models, Go) when not in textarea
- **Enter**: Activate focused button or select item in modal (when in textarea: launch task)
- **Esc**: Close modal or go back to navigation mode
- **Shift+Enter**: Create new line in textarea (when focused)
- **Any key**: Type in description area when focused
- **Backspace**: Delete characters
- **Auto-complete menu**: When certain characters like / or @ are entered in the text area, show auto-completion menu with dynamically populated choices (@ for citing files, / for selecting workflows, etc)

#### Modal Navigation (Telescope-style)
- **↑↓**: Navigate through options in fuzzy search
- **Enter**: Select current item
- **Esc**: Close modal
- **Left/Right** or **+/-**: Adjust model instance counts in model selection

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

- Last selections (project, agent, branch) are remembered per repo/user scope via the configuration layer.
- Selected theme preference is persisted across sessions.

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

### Accessibility

- **Theme Selection**: Multiple themes including high-contrast accessibility theme
- **High-contrast theme option**: Enhanced contrast ratios for better visibility
- **Full keyboard operation**: All features accessible without mouse
- **Predictable focus order**: Logical tab navigation through all interactive elements
- **Charm theming**: Provides excellent contrast ratios and visual hierarchy
