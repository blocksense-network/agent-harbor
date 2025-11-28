# Agent Activity TUI PRD — Product Requirements and UI Specification

## Summary

The Agent Activity TUI provides a terminal-first interface for monitoring and interacting with AI agent execution in real-time. Built with **Ratatui**, it displays agent thoughts, tool calls, file edits, terminal output, and status updates using a unified activity stream design. The interface supports both live agent monitoring during execution and post-facto review of completed sessions.

The TUI leverages the same **Ratatui** ecosystem as the main dashboard:

- **ratatui**: Core TUI framework for rendering and layout
- **tui-textarea**: Advanced multi-line text editing with cursor management
- **tui-input**: Single-line input for modals and forms
- **crossterm**: Cross-platform terminal manipulation and event handling

See [`TUI-PRD.md`](TUI-PRD.md) for the main dashboard design and [`ah-agent-record.md`](ah-agent-record.md) for the SessionViewer UI that inspired this activity display.

## Terminal State Management

The Agent Activity TUI properly manages terminal state for smooth real-time updates:

- **Keyboard Enhancement Flags**: Uses Crossterm's keyboard enhancement flags for improved input handling
- **State Tracking**: Tracks raw mode, alternate screen, and keyboard flags for proper cleanup
- **Panic Safety**: Implements panic hooks and signal handlers to restore terminal state on crashes
- **Graceful Exit**: Ensures terminal returns to normal state regardless of exit method (ESC, Ctrl+C, panic)

## Activity Stream Layout

The Agent Activity interface displays a chronological stream of agent execution events with a focus on readability and real-time updates:

```
┌─ Agent Activity: Claude Code ──────────────────────────────────────────┐
│ ● Starting task execution...                                          │
│                                                                       │
│ 🤔 Analyzing codebase structure and identifying key files            │
│                                                                       │
│ 🔧 Running: find src -name "*.rs" -type f | head -10                  │
│    src/main.rs                                                        │
│    src/lib.rs                                                         │
│    src/config.rs                                                      │
│    src/agent.rs                                                       │
│    src/ui.rs                                                          │
│    [+6 more files...]                                                 │
│                                                                       │
│ 📝 Editing: src/main.rs (+5 -2 lines)                                 │
│    @@ -15,7 +15,10 @@                                                 │
│     fn main() {                                                       │
│    -    println!("Hello, world!");                                    │
│    +    println!("Hello, Agent Harbor!");                             │
│    +                                                                  │
│    +    // Initialize agent system                                    │
│    +    let agent = Agent::new();                                     │
│    +                                                                  │
│         agent.run().await;                                            │
│     }                                                                 │
│                                                                       │
│ 🤔 Now implementing the core agent loop with proper error handling   │
│                                                                       │
│ 🔧 Running: cargo check                                               │
│    Compiling agent-harbor v0.1.0 (/workspace)                         │
│    Finished dev [unoptimized + debuginfo] target(s) in 2.34s          │
│                                                                       │
└─────────────────────────────────────────────────────────────────────────┘
```

### Activity Event Types

The interface displays a unified stream of agent activity events, each with distinct visual styling and icons:

#### Thought Events (`🤔`)

- **Display**: Full-width thought content with thinking icon
- **Purpose**: Shows agent reasoning and planning
- **Styling**: Muted text color with subtle background highlight
- **Wrapping**: Multi-line with proper text wrapping

#### Tool Call Events (`🔧`)

- **Display**: Tool name and arguments with wrench icon
- **Purpose**: Shows when agent executes tools/commands
- **Styling**: Bold tool name, monospace arguments
- **Status**: Shows "Running...", "Completed", or "Failed" with appropriate colors

#### Tool Output Events

- **Display**: Indented terminal output with syntax highlighting
- **Purpose**: Shows command execution results
- **Styling**: Monospace font, color-coded by stream (stdout/stderr)
- **Truncation**: Auto-truncates long output with "[...X more lines...]" indicator

#### File Edit Events (`📝`)

- **Display**: File path, line changes, and unified diff preview
- **Purpose**: Shows when agent modifies files
- **Styling**: File path in bold, diff with syntax highlighting
- **Preview**: Shows context lines around changes (configurable, default 3)

#### Log Events (`📋`)

- **Display**: Timestamped log messages with appropriate icons
- **Purpose**: Shows agent status and diagnostic information
- **Styling**: Color-coded by log level (info=blue, warn=yellow, error=red)
- **Levels**: DEBUG, INFO, WARN, ERROR with distinct visual treatment

#### Status Events (`●`)

- **Display**: Session lifecycle status with progress dots
- **Purpose**: Shows overall agent execution state
- **Styling**: Color-coded status indicators
- **States**: Queued, Running, Paused, Completed, Failed

#### Terminal Events (`💻`)

- **Display**: Direct terminal output from agent processes
- **Purpose**: Shows interactive terminal sessions
- **Styling**: Full terminal emulation with ANSI color support
- **Interaction**: Supports live terminal following and input injection

### Real-Time Activity Display

The activity stream maintains exactly 3 visible activity rows that scroll smoothly as new events arrive:

**Activity Row Requirements:**

- Fixed height rows: Each of the 3 rows has fixed height (prevents UI "dancing")
- Scrolling effect: New events cause rows to scroll upward (newest at bottom)
- Always 3 rows visible: Shows the 3 most recent activity items at all times
- Never empty: Always displays events, never shows "waiting" state

**Event Types and Display Rules:**

1. **Thought Event** (`thought` property):
   - Format: `"🤔 {thought text}"`
   - Behavior: Scrolls existing rows up, appears as new bottom row
   - Single line display with ellipsis for overflow

2. **Tool Use Start** (`tool_name` property):
   - Format: `"🔧 Running: {tool_name} {args}"`
   - Behavior: Scrolls existing rows up, appears as new bottom row

3. **Tool Last Line** (`tool_name` + `last_line` properties):
   - Format: `"  {last_line}"` (indented, showing command output)
   - **Special behavior**: Updates the existing tool row IN PLACE without scrolling
   - Does NOT create a new row - modifies the current tool execution row

4. **Tool Complete** (`tool_name` + `tool_output` + `tool_status` properties):
   - Format: `"🔧 {tool_name}: {tool_output}"` (single line with status indicator)
   - Behavior: Sent immediately after last_line event
   - The last_line row is removed and replaced by this completion row

5. **File Edit Event** (`file_path` property):
   - Format: `"📝 {file_path} (+{lines_added} -{lines_removed})"`
   - Behavior: Scrolls existing rows up, appears as new bottom row

**Visual Behavior Example:**

```
Initial state (empty):
  [Waiting for agent activity...]

After "thought" event:
  🤔 Analyzing codebase structure

After "tool_name" event (scrolls up):
  🤔 Analyzing codebase structure
  🔧 Running: find src -name "*.rs"

After "last_line" event (updates in place - NO scroll):
  🤔 Analyzing codebase structure
  🔧 Running: find src -name "*.rs"
    Found 42 .rs files in src/

After "tool_output" event (replaces last_line row):
  🤔 Analyzing codebase structure
  🔧 find: Found 42 .rs files

After new "thought" event (scrolls up, oldest row disappears):
  🔧 find: Found 42 .rs files
  🤔 Now examining the main.rs file
```

## Theme and Visual Design

The Agent Activity TUI follows the same Charm-inspired design principles as the main dashboard:

### Charm-Inspired Aesthetics

- **Default Theme**: Catppuccin Mocha - Dark theme with cohesive colors
  - Background: `#11111B`
  - Surface/Card backgrounds: `#242437`
  - Text: `#CDD6F4`
  - Primary: `#89B4FA` (blue for actions)
  - Accent: `#A6E3A1` (green for success)
  - Muted: `#7F849C` (secondary text)
- **Multiple Theme Support**: Users can choose from various themes including Catppuccin variants, Nord, Dracula, Solarized Dark, etc.
- **Rounded borders**: `BorderType::Rounded` on all components
- **Proper padding**: Generous spacing with `Padding::new()` for breathing room
- **Truecolor support**: 24-bit RGB colors for rich visual experience

### Component Styling

- **Activity Stream**: Rounded border container with proper padding
- **Event Icons**: Unicode symbols with consistent sizing and spacing
- **Syntax Highlighting**: Color-coded syntax for diffs, code, and terminal output
- **Status Indicators**: Color-coded dots and badges for different event types
- **Scrollable Content**: Smooth scrolling with visual scroll indicators

### Theme Colors (Catppuccin Mocha)

```rust
pub struct ActivityTheme {
    pub bg: Color,                    // #11111B - Base background
    pub surface: Color,              // #242437 - Card/surface background
    pub text: Color,                 // #CDD6F4 - Main text
    pub muted: Color,                // #7F849C - Secondary text
    pub primary: Color,              // #89B4FA - Blue for actions
    pub accent: Color,               // #A6E3A1 - Green for success
    pub success: Color,              // #A6E3A1 - Green
    pub warning: Color,              // #FAB387 - Yellow/Orange
    pub error: Color,                // #F38BA8 - Red/Pink
    pub border: Color,               // #45475A - Border color
    pub border_focused: Color,       // #89B4FA - Focused border color
}
```

## Interactive Features

### Live Terminal Following

When agents execute commands, users can follow the terminal output in real-time:

- **Modal Terminal View**: Full-screen modal showing live terminal output
- **Input Injection**: Type commands that get sent to the running agent process
- **Resize Handling**: Proper terminal resizing and dimension negotiation
- **ANSI Support**: Full ANSI escape sequence support for colors and formatting

### Activity Filtering and Search

- **Event Type Filters**: Show/hide specific event types (thoughts, tools, files, logs)
- **Text Search**: Search through activity content with highlighting
- **Time Range Filtering**: Focus on specific time periods
- **Export Capabilities**: Save filtered activity to file

### Session Control

- **Pause/Resume**: Pause agent execution and resume later
- **Cancel Operation**: Cancel current agent operation
- **Step Through**: Execute one step at a time for debugging
- **Breakpoint Setting**: Set breakpoints on specific events or file changes

## Keyboard Shortcuts

### Global Navigation

- **↑↓**: Navigate through activity events
- **Page Up/Page Down**: Scroll through activity history
- **Home/End**: Jump to first/last activity event
- **Ctrl+C**: Exit the activity viewer

### Interactive Controls

- **Enter**: Follow terminal output for selected tool call
- **F**: Toggle follow mode for live terminal updates
- **S**: Show/hide activity stream (focus on terminal)
- **I**: Inject input to running terminal
- **C**: Copy selected text or command output

### Search and Filtering

- **/**: Open search mode for activity content
- **T**: Toggle thought events visibility
- **L**: Toggle log events visibility
- **E**: Toggle tool/file edit events visibility

## Footer Shortcuts (Lazygit-style)

Single-line footer without borders showing context-sensitive shortcuts:

- **Activity Stream**: "↑↓ Navigate • Enter Follow Terminal • F Toggle Follow • / Search • C Copy • Ctrl+C Exit"
- **Terminal Following**: "Ctrl+C Stop Following • I Inject Input • Ctrl+C Exit"
- **Search Mode**: "↑↓ Navigate Results • Enter Select • Esc Cancel"

## Configuration

The Agent Activity TUI is configured through the same settings system as the main dashboard:

### Activity Display Settings

```toml
[tui.activity]
# Number of visible activity rows
visible_rows = 3

# Maximum lines per activity item
max_lines_per_item = 10

# Auto-scroll to new events
auto_scroll = true

# Show timestamps for events
show_timestamps = true

# Syntax highlighting for code/diffs
syntax_highlighting = true

# Theme for activity display
theme = "catppuccin-mocha"

# Font style for symbols
font_style = "unicode"  # unicode, nerdfont, ascii
```

### Event Type Visibility

```toml
[tui.activity.events]
# Show thought/reasoning events
show_thoughts = true

# Show tool call events
show_tools = true

# Show file edit events
show_file_edits = true

# Show log events
show_logs = true

# Show status events
show_status = true

# Show terminal output events
show_terminal = true
```

### Terminal Following Settings

```toml
[tui.activity.terminal]
# Enable live terminal following
enable_following = true

# Terminal dimensions for following
follow_cols = 120
follow_rows = 30

# Allow input injection
allow_input_injection = true

# Show ANSI colors in terminal
ansi_colors = true
```

## Integration Points

### ACP Protocol Events

The activity display consumes ACP protocol events and translates them to the unified activity stream:

- `session/update` notifications with `thought`, `tool`, `log`, `file`, `terminal` content
- Tool call lifecycle events (`tool_use`, `tool_result`)
- Status change notifications
- Error and completion events

### SessionViewer Integration

The Agent Activity TUI is nested within the standard SessionViewer UI (described in [`ah-agent-record.md`](ah-agent-record.md)), replacing only the terminal rendering area that is used for third-party agents. The SessionViewer UI continues to handle snapshot indicators, task entry UI, pipeline explorers, and all other standard functionality.

The Agent Activity TUI reuses SessionViewer UI components:

- **Terminal State Management**: Unified terminal state tracking
- **Gutter System**: Snapshot indicators and activity markers
- **Pipeline Explorer**: Tool execution step visualization
- **Input Injection**: Live terminal interaction capabilities

### Filesystem Integration

- **Snapshot Creation**: Automatic snapshots on file edits (when enabled)
- **Workspace Navigation**: Quick file opening from activity events
- **Diff Viewing**: Inline diff display for file changes
- **Branch Operations**: Create branches from activity points

## Performance Considerations

- **Event Buffering**: Efficient buffering of high-frequency events
- **Lazy Rendering**: Only render visible activity items
- **Memory Management**: Bounded history with configurable limits
- **Smooth Scrolling**: Optimized scrolling animations
- **Background Processing**: Non-blocking event processing

## Accessibility

- **High Contrast Theme**: Enhanced contrast ratios for better visibility
- **Keyboard Navigation**: Full keyboard accessibility
- **Screen Reader Support**: Semantic markup for screen readers
- **Color Blind Friendly**: Multiple color schemes and symbol fallbacks
- **Font Size Options**: Configurable text sizing

## Error Handling and Status

- **Connection Status**: Clear indicators for ACP connection state
- **Event Processing Errors**: Graceful handling of malformed events
- **Terminal Resize Handling**: Proper layout adaptation
- **Memory Pressure**: Automatic cleanup under memory constraints

## Future Extensions

- **Collaborative Sessions**: Multiple users viewing the same activity stream
- **Activity Recording**: Save and replay activity sessions
- **Advanced Filtering**: Complex query-based event filtering
- **Plugin System**: Extensible activity display with custom event types
- **Real-time Collaboration**: Live cursor and annotation sharing

This Agent Activity TUI PRD provides the foundation for a rich, real-time agent monitoring experience that integrates seamlessly with Agent Harbor's existing UI patterns and the ACP protocol requirements.
