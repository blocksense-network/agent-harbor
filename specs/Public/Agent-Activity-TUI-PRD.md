# Agent Activity TUI PRD — Product Requirements and UI Specification

## Summary

The Agent Activity TUI provides a terminal-first interface for monitoring and interacting with AI agent execution in real-time. Built with **Ratatui**, it displays agent thoughts, tool calls, file edits, terminal output, and status updates using a unified activity stream design. The interface supports both live agent monitoring during execution and post-facto review of completed sessions.

The TUI leverages the same **Ratatui** ecosystem as the main dashboard:

- **ratatui**: Core TUI framework for rendering and layout
- **tui-textarea**: Advanced multi-line text editing with cursor management
- **tui-input**: Single-line input for modals and forms
- **crossterm**: Cross-platform terminal manipulation and event handling

See [`TUI-PRD.md`](TUI-PRD.md) for the main dashboard design, [`ah-agent-record.md`](ah-agent-record.md) for the SessionViewer UI, and [`TUI-Color-Theme.md`](TUI-Color-Theme.md) for the shared color theme and semantic definitions.

## Timeline Activity Layout

The Agent Activity interface moves beyond a simple log stream to a structured **Timeline View**. This design uses a vertical line metaphor to connect related events, providing a clear visual narrative of the agent's execution path.

```
    │ │  error: something went wrong                                      │ │
    │ │  error: another failure                                           │ │
    │ ╰───────────────────────────────────────────────────────────────────╯ │
    │   ╭────────────────╮         [click here to fork] ╭───┬───┬───────╮   │
    │ ╭─┤ TASK COMPLETED ├──────────────────────────────┤ ❐ │ ▼ │ 14:24 ├─╮ │
    │ │ ╰────────────────╯                              ╰───┴───┴───────╯ │ │
    │ │  Summary                                                          │ │
    │ │  I have fixed the issue.                                          │ │
    │ │  • Updated permissions                                            │ │
    │ │                                                                   │ │
    │ │   rust                                                       ❐    │ │
    │ │                                                                   │ │
    │ │   fn fix() {                                                      │ │
    │ │       // code...                                                  │ │
    │ │   }                                                               │ │
    │ │                                                                   │ │
    │ ╰───────────────────────────────────────────────────────────────────╯ │
    │ ╭───────────────────────────────────────────────────────────────────╮ │
    │ │ Describe your task...                                             │ │
    │ │                                                                   │ │
    │ │                                                                   │ │
    │ │ ╭────────╮                                   ╭──────┬───────────╮ │ │
    │ ╰─┤ AGENTS ├───────────────────────────────────┤ ⏎ GO │ ≡ OPTIONS ├─╯ │
    │   ╰────────╯                                   ╰──────┴───────────╯   │
    │   ╭──────╮                                        ╭───┬───┬───────╮   │
    │ ╭─┤ READ ├────────────────────────────────────────┤ ❐ │ ▼ │ 14:22 ├─╮ │
    │ │ ╰──────╯                                        ╰───┴───┴───────╯ │ │
    │ │  src/interpose.rs (lines 40-50)                                   │ │
    │ │  src/main.rs (lines 10-20)                                        │ │
    │ ╰───────────────────────────────────────────────────────────────────╯ │
    │   ╭───────────╮                                   ╭───┬───┬───────╮   │
    │ ╭─┤ RAN cargo ├───────────────────────────────────┤ ❐ │ ▼ │ 14:25 ├─╮ │
    │ │ ╰───────────╯                                   ╰───┴───┴───────╯ │ │
    │ │ $ cargo test                                                      │ │
    │ ╰─... (Dimmed Future Event)                                     ...─╯ │
    │                                                                       │
    │    Ctrl+C Stop   Ctrl+D Detach   Ctrl+L Clear   Shift+Enter New Line  │
    └───────────────────────────────────────────────────────────────────────┘
```

### Visual Structure

1. **Layout**: The content is centered with margins (e.g., 2-4 spaces) from the container edges.
   - **Purpose**: Reduces visual clutter and focuses attention on the content.

2. **Hero Card (Active State)**: The active state is highlighted as a "Hero Card".
   - **Purpose**: Displays the **current, ongoing activity** of the agent (e.g. "Running tests...", "Thinking..."). It acts as a "Now Playing" indicator, ensuring the user always knows what the agent is doing right now, regardless of where they are in the history.
   - **Position**: Normally positioned chronologically in the timeline. When scrolling would hide it from view, it docks/sticks to the bottom of the view, typically immediately above the Instructions Card.
   - **Forking Behavior**: When the Instructions Card is moved up into the timeline to fork the session, the Hero Card takes the position at the **very bottom** of the view. It remains docked there even as the timeline is scrolled to track the position of the Instructions Card.
   - **Behavior**: It remains visible by docking to the bottom only when scrolling would otherwise hide it from view, ensuring the current action is always visible while preserving the timeline's chronological order.
   - **Style**: Uses color and bold text to draw attention.
   - **Border**: `color:accent` on the border for active states.
   - **Time Indicator**: While active, the time display shows the **elapsed duration** (e.g. `00:45`) since the action started. Upon completion, it freezes to show the static **end time** (e.g. `14:22`).

3. **Activity Cards**: Events are encapsulated in "Cards" (using box drawing characters).
   - **Borders**: Borders use a neutral/dim color (e.g., `color:border` or `color:muted`) to reduce visual noise.
   - **Background**: The entire screen uses a controlled background color (e.g., `color:base`). This background is applied to **entire rows**, filling margins and empty spaces, ensuring a seamless, non-boxy appearance.
   - **Header**: The top line of the card shows the Event Type (RAN, EDITED, THOUGHT, READ, DELETED, TASK COMPLETED) using a "Floating Box" style.
     - **Title Box**: The title is enclosed in a box that straddles the border line:
       ```
          ╭─────────╮
        ╭─┤ THOUGHT ├─
        │ ╰─────────╯
       ```
     - **Categories**: Uses specialized category labels.
     - **Tool Execution**:
       - **Title**: Constructed dynamically to show the command status and pipeline details.
         - **Format**: `RAN <command_1> <size_1> | <command_2> <size_2> ...`
         - **"RAN" Label**:
           - **Success**: Displayed in `color:success` if the final exit code is 0.
           - **Failure**: Displayed in `color:failure` if the final exit code is non-zero.
         - **Command Names**:
           - **Pipelines**: For pipelines (e.g., `cmd1 | cmd2`), each command name is **individually colored** based on its specific exit code (Success/Failure).
           - **Skipped Commands**: If a command fails, subsequent commands in the pipeline that are not executed are displayed in `color:muted` (Gray).
         - **Output Size Indicator**: Displayed in a distinct dimmer color (`color:muted`) next to the command name (or the last command in a pipeline).
           - **Content**: Shows the output size in **bytes** (e.g., `213B`, `12K`) or optionally **LLM tokens** (configurable via policy). Skipped commands do **not** show this indicator.
           - **Tooltip**: On mouse hover, displays detailed counts for **bytes**, **tokens**, and **lines**.
           - **Interaction**: Clicking the indicator opens the **Output Inspection Modal** (see [Output Inspection Modal](#output-inspection-modal)).
         - **Stop Button**: A stop button `■` is displayed for running commands or commands in a pipeline.
           - **Style**: `color:dim-error` (idle/completed) or `color:error` (active/hover).
           - **Interaction**: Clicking the button terminates the designated command.
     - **File Edits**:
       - **Title**: "EDITED"
       - **Color**: `color:success` to indicate successful modification.
       - **Content**: Filename (optionally prefixed with a **Nerd Font icon**) and a git-style summary `+A -D`.
     - **File Reads**:
       - **Title**: "READ"
       - **Color**: `color:success` to indicate information retrieval.
       - **Content**: Filename (optionally prefixed with a **Nerd Font icon**) and line ranges.
     - **Deleted**:
       - **Title**: "DELETED"
       - **Color**: `color:success` to indicate successful deletion.
       - **Content**: Filenames (optionally prefixed with a **Nerd Font icon**) and a git-style summary `-D`.
     - **Thought**:
       - **Title**: "THOUGHT"
       - **Color**: `color:success`.
       - **Content**: Markdown content, including bullet lists and code blocks.
     - **User Instructions**:
       - **Title**: "YOU WROTE" for text prompts, "YOU SAID" for audio messages.
       - **Collaborative Mode**: In collaborative sessions, "YOU" is replaced by the developer's name (e.g., "JOHN WROTE", "ALICE PARKER SAID"). The display format (full name, first name, handle) is controlled by configuration.
       - **Collaborative Mode**: In collaborative sessions, "YOU" is replaced by the developer's name (e.g., "JOHN WROTE", "ALICE PARKER SAID"). The display format (full name, first name, handle) is controlled by configuration.
       - **Color**: `color:primary` for the current user. Third-party messages (teammates) use a distinct color (e.g., `color:teammate`) to distinguish them from the user's own actions.
       - **Optimistic State**:
         - When the user submits instructions, a card is immediately created in an **Unconfirmed** state.
         - **Indicator**: A small spinner indicator appears in the lower-right corner of the card.
         - **Confirmation**: Once the server acknowledges the input (via a `TaskEvent`), the card transitions to the **Confirmed** state, and the indicator is removed.
   - **Control Box**: The right side of the header features a tightly segmented control box that also straddles the border line:

   ```
     ╭───┬───┬───────╮
   ──┤ ❐ │ ▼ │ 14:22 ├──
     ╰───┴───┴───────╯
   ```

   - **Structure**: It is divided into segments by vertical lines `│` without extra padding.
     - The vertical delimiters connect to the top and bottom caps using `┬` and `┴` characters.
   - **Segments**:
     1. **Copy Icon**: `❐` (with padding: `❐`).
     2. **Expand Icon**: `▼` (with padding: `▼`).
     3. **Timestamp**: `HH:MM` (dimmed, with padding: `HH:MM`).
   - **Interaction**:
     - **Mouse**: Icons are clickable to trigger their respective actions (Copy, Expand/Collapse).
     - **Keyboard**: When a card is selected, pressing `keys:navigate-right` moves focus to the Control Box buttons. `keys:activate-current-item` triggers the focused button.
   - **Body**: Content is nested within the card.
     - **Commands**: For tools, the full command is displayed on the first line, prefixed with `$`.
       - **Syntax Highlighting**:
         - **Command**: `color:primary`.
         - **Operators** (`|`, `&`, etc.): `color:command-operator`.
         - **Flags** (`-help`): `color:command-flag`.
         - **Arguments**: `color:text`.
       - **Background**: The command line does not use a distinct background color, keeping it clean.
     - **Output**: Command output follows, using the same background color as the card/app to maintain the seamless look.
       - **Colors**: Standard output lines use consistent text color (`color:terminal:stdout` or `color:dim-text`) across all card types. Errors use `color:terminal:stderr`.
     - **Code Blocks** (in markdown cards):
       - **Header**: Header displaying the Language name (left) and a **Copy Button** `❐` (right).
         - **Background**: `color:code-header-bg`.
       - **Background**: Uses distinct, slightly lighter background color (`color:code-bg`) to separate code from text.
       - **Padding**: 1 line of vertical padding (top/bottom) and 1 column of horizontal padding (left/right) around the code content.
       - **Highlighting**: Code content is syntax highlighted.

### Instructions Card (Task Entry)

The Instructions Card allows the user to provide feedback, new instructions, or branch the task. It follows the shared **Task Entry TUI** specification.

> **Reference**: See [`Task-Entry-TUI-PRD.md`](./Task-Entry-TUI-PRD.md) for detailed behavior, layout, and interaction rules.

- **Context**: In the Agent Activity TUI, this card is inserted into the timeline.
- **Forking**: Moving the card up/down (`keys:move-to-previous-snapshot`/`keys:move-to-next-snapshot`) changes the fork point for the new instructions.
  - **Forking Tooltip**:
    - **Trigger**: Displayed immediately when the mouse hovers over the whitespace between two activity cards.
    - **Content**: "Click here to fork".
    - **Style**:
      - **Text**: `color:tooltip`
      - **Background**: `color:tooltip-bg`
      - **Position**: Appears on top of other content, positioned relative to the card's right-side buttons as shown in the `Terminal Activity Layout` example above.
    - **Behavior**:
      - **Expiration**: The tooltip automatically hides after **5 seconds** of mouse inactivity.
      - **Interaction**: Clicking the tooltip moves the **Instructions Card** to the insertion point between the two cards, enabling the user to fork the session from that specific point (equivalent to keyboard positioning).

### Footer

- **Position**: Fixed at the very bottom of the screen, spanning the full width.
- **Content**:
  - **Left**: Displays context-sensitive shortcuts (e.g., `Alt+↑↓ Select Card`, `Ctrl+↑↓ Fork`).
  - **Right**: Displays session status information:
    - **Target Branch**: The branch where results will be delivered (e.g., `Target: feature/login`).
    - **Context Usage**: Percentage of the agent's context window consumed (e.g., `Context: 45%`).
- **Style**:
  - **Shortcuts**: Keys are highlighted in `color:keyboard-shortcut-key`, descriptions use `color:keyboard-shortcut-action`.
  - **Status Info**: Uses `color:muted` for labels and `color:text` for values. High context usage (>80%) should be highlighted in `color:warning` or `color:error`.
  - **Background**: Matches the app surface. Uses a configurable margin (default 4 chars) from the screen edge.

### Real-Time Behavior

- Cards slide in from the bottom.
- The view automatically tracks the newest event when the user hasn't scrolled up.
- Scrolling to the bottom of the activity stream automatically activates the auto-follow behavior.
- "Hero Status" (current action) can stick to the bottom of the view if the user scrolls up to review history.

### Dimmed / Discarded Events

When a session is forked (by moving the Instructions Card up), events below the fork point are rendered in a "Dimmed" state:

- **Border**: `color:dim-border`.
- **Text/Icons**: `color:dim-text`.
- **Background**: Standard background, but content lacks vibrant coloring.
- **Purpose**: Visualizes that these events are part of a discarded timeline branch.

### Theme Colors

The Agent Activity TUI uses the shared color theme defined in [`TUI-Color-Theme.md`](TUI-Color-Theme.md). The following mappings define how specific UI elements use these semantic roles:

#### Syntax Highlighting (TUI Specific)

- **Commands**: `color:primary`
- **Arguments**: `color:text`
- **Flags**: `color:accent`
- **Operators**: `color:warning`

#### Standard Syntax Highlighting

- **Keyword**: `color:syntax:keyword`
- **String**: `color:syntax:string`
- **Function**: `color:syntax:function`
- **Type**: `color:syntax:type`
- **Variable**: `color:syntax:variable`
- **Constant**: `color:syntax:constant`
- **Comment**: `color:syntax:comment`

#### Terminal Output

- **Stdout**: `color:terminal:stdout`
- **Stderr**: `color:terminal:stderr`
- **Command**: `color:terminal:command`
- **Success**: `color:terminal:success`
- **Failure**: `color:terminal:failure`
- **Warning**: `color:terminal:warning`
- **Info**: `color:terminal:info`

#### Specific Functional Roles

- **Keyboard Shortcuts**:
  - **Key**: `color:primary`
  - **Action Name**: `color:muted`
- **File Operations**:
  - **Added/Modified**: `color:accent`
  - **Deleted**: `color:error`
  - **Read**: `color:accent`
- **Command Elements**:
  - **Flag**: `color:accent`
  - **Operator**: `color:warning`
  - **Confirm Action**: `color:accent`
- **UI Elements**:
  - **Tooltip Text**: `color:tooltip`
  - **Tooltip Background**: `color:tooltip-bg`
  - **Stderr Gutter Background**: `color:gutter:stderr:background`
  - **Stderr Gutter Foreground**: `color:gutter:stderr:foreground`

## Reference Implementation

To visualize the intended design and behavior, we provide Python reference scripts that render the UI states in a terminal. These scripts demonstrate the layout, color palette, and animation concepts.

- `tui_mockup.py`: Renders a static mockup of the Agent Activity interface, adapting to the current terminal size.

### Output Inspection Modal

A dedicated modal for examining large or complex command outputs, triggered by clicking the **Output Size Indicator**.

This modal reuses the standard **`tui-textarea`** component **in read-only mode**. Its core behavior (scrolling, selection, search, keybindings) is described in [`TUI-PRD.md`](TUI-PRD.md).

- **Content View**:
  - **Text**: Displayed using `tui-textarea` with syntax highlighting.
    - **Stderr**: Lines containing at least one character of stderr output are indicated by a `color:gutter:stderr:background` colored gutter.
  - **Binary**: Displayed as a two-column hex viewer (Hex bytes | ASCII representation).
    - **Stderr**: Individual bytes/characters from stderr are color-coded in `color:error`.

## Keyboard Shortcuts

The Agent Activity TUI follows the standard keyboard operations defined in [`TUI-Keyboard-Shortcuts.md`](./TUI-Keyboard-Shortcuts.md).

### Global Help

Press `keys:show-help` (Default: `?` or `Ctrl+?`) to open the global help dialog, which lists all available shortcuts. `Ctrl+?` works even when a text area is focused.

### Context-Specific Bindings

The following bindings are specific to the Agent Activity view:

### Global Navigation

Since the "Up" and "Down" keys are reserved for cycling through the prompt history in the Input Field (standard shell behavior), the TUI uses specific modifiers for navigation:

- **Timeline Scrolling**:
  - **keys:scroll-up-one-screen / keys:scroll-down-one-screen**: Scroll the timeline view page by page.
  - **keys:scroll-line-up / keys:scroll-line-down**: Scroll the timeline view line by line.
  - **keys:scroll-to-top / keys:scroll-to-bottom**: Jump to the first/last activity event.

- **Card Selection**:
  - **keys:navigate-up / keys:navigate-down**: Move selection focus between Activity Cards.
    - **Off-screen Behavior**: If the currently selected card is outside the visible viewport, pressing `keys:navigate-down` selects the **topmost visible card**, and pressing `keys:navigate-up` selects the **bottommost visible card**. This ensures the selection "enters" the screen from the appropriate edge.
  - **keys:navigate-right**: When a card is selected, move focus to the Control Box buttons (Copy -> Expand).
  - **keys:navigate-left**: Return focus from Control Box to the Card.
  - **keys:activate-current-item**: Trigger the default action for the selected card or focused button.

- **Forking (Instructions Card)**:
  - **keys:move-to-previous-snapshot / keys:move-to-next-snapshot**: Move the **Instructions Input Card** up or down in the timeline to select a fork point.
    - **Off-screen Behavior**: If the Instructions Card is currently docked or positioned outside the visible viewport, pressing `keys:move-to-next-snapshot` moves it to the **topmost visible insertion point**, and pressing `keys:move-to-previous-snapshot` moves it to the **bottommost visible insertion point**.

### Search and Filtering

- **keys:incremental-search-forward**: Open incremental search mode.
  - **Behavior**: Temporarily replaces the status bar (footer) content with the incremental search input box.
  - **Navigation**: While search is active:
    - Pressing `keys:incremental-search-forward` again navigates to the **next** match.
    - Pressing `keys:incremental-search-backward` navigates to the **previous** match.
    - Pressing `keys:dismiss-overlay` exits incremental search mode and returns to viewport position before starting the search.
    - Pressing `keys:activate-current-item` selects the current match and exits incremental search mode. The viewport stays at the selected match.
  - **Footer Help**: Aligned to the right edge of the screen: "keys:incremental-search-forward Next • keys:incremental-search-backward Prev • keys:activate-current-item Select • keys:dismiss-overlay Cancel"

## 4. Input Minor Modes

These modes map precise `KeyboardOperation`s to the interaction model. The following lists reflect the exact operations defined in the implementation, using the `keys:operation-name` notation.

### 4.1 `ACTIVITY_STREAM_MODE`

Active when the activity timeline is focused (default state).

- **Navigation**:
  - `keys:move-to-previous-line`, `keys:move-to-next-line`: Select previous/next activity card.
  - `keys:scroll-up-one-screen`, `keys:scroll-down-one-screen`: Scroll the timeline view page by page.
  - `keys:move-to-beginning-of-document`, `keys:move-to-end-of-document`: Jump to the first/last activity event.
  - `keys:move-to-previous-snapshot`, `keys:move-to-next-snapshot`: Move the Instructions Card up/down to select a fork point.
- **Selection**:
  - `keys:navigate-right`: Move focus from the card to its Control Box (right).
- **Search**:
  - `keys:incremental-search-forward`: Open the forward direction incremental search overlay.
  - `keys:incremental-search-backward`: Open the backward direction incremental search overlay.
- **System**:
  - `keys:copy`: Copy the content of the selected card.
  - `keys:stop`: Pause/Stop the agent execution.

### 4.2 `DRAFT_TEXT_EDITING_MODE`

Shared with Task Entry. Active when the Instructions Card text area is focused.

> **Reference**: See [`Task-Entry-TUI-PRD.md`](./Task-Entry-TUI-PRD.md#41-draft_text_editing_mode) for the complete list of text editing operations.

**Note**: Operations not handled by this mode (e.g., `keys:move-to-previous-snapshot` for forking) fall through to `ACTIVITY_STREAM_MODE`.

### 4.3 `DRAFT_TEXTAREA_TO_BUTTONS_MODE`

Shared with Task Entry. Handles the transition from Text Area to Action Bar.

> **Reference**: See [`Task-Entry-TUI-PRD.md`](./Task-Entry-TUI-PRD.md#42-draft_textarea_to_buttons_mode).

### 4.4 `DRAFT_BUTTON_NAVIGATION_MODE`

Shared with Task Entry. Active when an Action Bar button is focused.

> **Reference**: See [`Task-Entry-TUI-PRD.md`](./Task-Entry-TUI-PRD.md#43-draft_button_navigation_mode).

### 4.5 `CONTROL_BOX_MODE`

Active when focus is within a card's Control Box (e.g., on the "Copy" or "Expand" buttons).

- `keys:navigate-left`: Return focus from Control Box to the Card (Left).
- `keys:navigate-right`: Cycle focus between Control Box buttons (Right).
- `keys:activate-current-item`: Trigger the focused button (Copy, Expand, etc.).

### 4.6 `SEARCH_MODE`

Active when the incremental search overlay is visible.

- `keys:incremental-search-forward`: Jump to next match.
- `keys:incremental-search-backward`: Jump to previous match.
- `keys:activate-current-item`: Select current match and exit search mode.
- `keys:dismiss-overlay`: Cancel search and return to original position.

### 4.7 `MODAL_NAVIGATION_MODE`

Shared with Task Entry. Active when a selection modal (Agent, Repository, Branch) is open.

> **Reference**: See [`Task-Entry-TUI-PRD.md`](./Task-Entry-TUI-PRD.md#44-modal_navigation_mode).

### 4.8 `MODEL_SELECTION_MODE`

Shared with Task Entry. Active specifically for the Model Selection dialog.

> **Reference**: See [`Task-Entry-TUI-PRD.md`](./Task-Entry-TUI-PRD.md#45-model_selection_mode).

## Footer Shortcuts (Lazygit-style)

Single-line footer without borders showing context-sensitive shortcuts:

- **Input Mode**: "keys:navigate-up/keys:navigate-down Select Card • keys:move-to-previous-snapshot/keys:move-to-next-snapshot Move Fork • keys:scroll-up-one-screen/keys:scroll-down-one-screen Scroll • keys:activate-current-item Send"
- **Card Selection**: "keys:navigate-up/keys:navigate-down Move • keys:navigate-right Focus Buttons • keys:activate-current-item Expand • keys:dismiss-overlay Input"
- **Search Mode**: "keys:incremental-search-forward Next • keys:incremental-search-backward Prev • keys:activate-current-item Select • keys:dismiss-overlay Cancel"

## Configuration

The Agent Activity TUI is configured through the same settings system as the main dashboard:

### Activity Display Settings

```toml
# Number of visible terminal output rows when collapsed
agent-activity-collapsed-terminal-height = 5
agent-activity-collapsed-diffs-height = 5
```

## Integration Points

### ACP Protocol Events

The activity display consumes ACP protocol events and translates them to the unified activity stream:

- `session/update` notifications with `thought`, `tool`, `log`, `file`, `terminal` content
- Tool call lifecycle events (`tool_use`, `tool_result`)
- Status change notifications
- Error and completion events

### SessionViewer Relationship

The Agent Activity TUI is a full alternative to the standard SessionViewer UI (described in [`ah-agent-record.md`](ah-agent-record.md)). It covers all functionality of the SessionViewer while providing a specialized experience for ACP-based agents with structured data (thoughts, tools, files).

Both interfaces integrate with the `ah agent record` command, which selects the appropriate UI to launch:

- **SessionViewer UI**: Used for standard terminal recording sessions or when `--output text` is requested.
- **Agent Activity TUI**: Used for sessions based on ACP agents or when `--output normalized-text` is requested.

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

## Error Handling and Status

- **Connection Status**: Clear indicators for ACP connection state
- **Event Processing Errors**: Graceful handling of malformed events
- **Terminal Resize Handling**: Proper layout adaptation

## Future Extensions

- **Collaborative Sessions**: Multiple users viewing the same activity stream
- **Activity Recording**: Save and replay activity sessions
- **Real-time Collaboration**: Live cursor and annotation sharing

This Agent Activity TUI PRD provides the foundation for a rich, real-time agent monitoring experience that integrates seamlessly with Agent Harbor's existing UI patterns and the ACP protocol requirements.
