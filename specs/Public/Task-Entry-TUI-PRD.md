# Task Entry TUI Specification

**Primary goals:**

- Define the shared Task Entry UI component used across the Dashboard, Agent Activity TUI, and Session Viewer.
- Specify the layout, interaction model, and keyboard shortcuts for creating and editing tasks.
- Detail the behavior of the draft card, including text editing, button navigation, and advanced options.

---

## 1. Overview

The Task Entry TUI is a reusable component for defining and launching agent tasks. It appears in three primary contexts:

1. \*Dashboard\*\*: As "Draft Cards" for creating new tasks.
2. \*Agent Activity TUI\*\*: As the "Instructions Card" for branching or refining active tasks.
3. \*Session Viewer\*\*: As an overlay for injecting instructions into recorded sessions.

Despite these different contexts, the core behavior—text editing, model selection, and launch configuration—remains consistent.

---

## 2. Visual Layout

The component uses a "Floating Box" design where controls straddle the bottom border. It adapts its content based on the context (Dashboard vs. Activity).

### 2.1 Full Layout (Dashboard)

Used when creating new tasks where Repository and Branch must be selected.

```
    │ ╰───────────────────────────────────────────────────────────────────╯ │
    │ ╭───────────────────────────────────────────────────────────────────╮ │
    │ │ Describe your task...                                             │ │
    │ │                                                                   │ │
    │ │                                                                   │ │
    │ │ ╭──────┬────────┬────────╮                   ╭──────┬───────────╮ │ │
    │ ╰─┤ REPO │ BRANCH │ AGENTS ├───────────────────┤ ⏎ GO │ ≡ OPTIONS ├─╯ │
    │   ╰──────┴────────┴────────╯                   ╰──────┴───────────╯   │
```

### 2.2 Compact Layout (Activity / Session)

Used when branching an existing session where context is inherited.

```
    │ ╰───────────────────────────────────────────────────────────────────╯ │
    │ ╭───────────────────────────────────────────────────────────────────╮ │
    │ │ Describe your task...                                             │ │
    │ │                                                                   │ │
    │ │                                                                   │ │
    │ │ ╭────────╮                                   ╭──────┬───────────╮ │ │
    │ ╰─┤ AGENTS ├───────────────────────────────────┤ ⏎ GO │ ≡ OPTIONS ├─╯ │
    │   ╰────────╯                                   ╰──────┴───────────╯   │
```

### 2.3 Text Area

- **Placeholder**: "Describe what you want the agent to do..." (when empty).
- **Height**: Variable/expandable based on content.
- **Styling**:
  - Active/Focused: `color:primary` border.
  - Inactive: `color:border` border.
  - Text: `color:text`.
  - Placeholder: `color:muted`.

### 2.4 Action Bar Elements

- **Layout**: Integrated into the bottom border using the "Floating Box" style.
- **Left Group** (Context Dependent):
  - **Repo Button**: `[ REPO ]` (Full Layout only). Displays current repository.
  - **Branch Button**: `[ BRANCH ]` (Full Layout only). Displays current branch.
  - **Agents Button**: `[ AGENTS ]` (or `[ 🤖 AGENTS ]`).
    - **Display Format**:
      - Single model: `[ 🤖 Claude 3.5 Sonnet ]`
      - Multiple models: `[ 🤖 Claude 3.5 Sonnet, GPT-4o ]`
      - With counts: `[ 🤖 Claude 3.5 Sonnet x2, GPT-4o ]`
      - Falls back to `[ 🤖 AGENTS ]` when no models are selected.
- **Right Group** (Always Present):
  - **Go/Options**: `[ ⏎ GO │ ≡ OPTIONS ]`.
    - **Go Button**: `⏎ GO` (Primary action). Bold text, `color:accent` when focused.
    - **Options Button**: `≡ OPTIONS` (Advanced configuration). `color:primary`. Opens a menu displaying launch options with visible keyboard shortcuts.

### 2.5 Visual Style Details

- **Borders**: Rounded corners (`╭`, `╮`, `╯`, `╰`).
- **Floating Boxes**: Buttons straddle the border line, creating a seamless, integrated look.
- **Background**: Matches the container background (e.g., `color:base`).
- **Auto-Save Indicators**: Displayed near the border or status area.
  - **Unsaved**: Gray dot/text.
  - **Saving...**: Yellow dot/text.
  - **Saved**: Green dot/text.
  - **Error**: Red dot/text.

## 3. Interaction Model

The component has two distinct focus states: **Text Editing** and **Button Navigation**.

### 3.1 Text Editing Mode

Active when the cursor is inside the text description area.

- **Focus**: Text Area.
- **Behavior**: Standard multi-line text editing with autocomplete support.
- **Navigation**:
  - `keys:navigate-up` inside textarea:
    - **If the text area is empty**: Cycles to the **previous** prompt in history (equivalent to pressing`keys:history-prev`).
    - **Otherwise**: Moves the caret up one line. If already at the first line, moves to the start of the line. Does **not** bubble focus to the parent context.
  - `keys:navigate-down` inside textarea:
    - **If the text area is empty**: Cycles to the **next** prompt in history (equivalent to pressing `keys:history-next`).
    - **Otherwise**: Moves the caret down one line. If already at the last line, moves to the end of the line. Does **not** bubble focus to the parent context.
  - `keys:move-to-next-field` (Tab): Moves focus to the Action Bar (specifically the next logical element, e.g., Repo or Agents button).
  - `keys:activate-current-item` (Enter): Launches the task (equivalent to clicking "GO").
  - `keys:open-new-line` (Shift+Enter): Inserts a new line.
  - `keys:show-launch-options`: Opens the Advanced Launch Options modal.
- **Autocomplete**:
  - Triggers: `@` (filenames), `/` (workflows), or typing workspace terms.
  - Visuals: Ghost text for inline suggestions, popup menu for multiple choices.
  - Actions: `keys:indent-or-complete` accepts suggestion.

### 3.2 Button Navigation Mode

- **Focus**: Action Bar Buttons (Repo, Branch, Agents, Go, Options).
- **Behavior**: Horizontal navigation between buttons.
- **Navigation**:
  - `keys:move-to-next-field` (Tab) / `keys:navigate-right`: Focus next button (cycles).
    - Cycling past the last button returns focus to the Text Area (if using Tab).
  - `keys:move-to-previous-field` (Shift+Tab) / `keys:navigate-left`: Focus previous button (cycles).
    - Cycling past the first button returns focus to the Text Area (if using Shift+Tab).
  - `keys:navigate-up` / `keys:navigate-down`: Navigates **away** from the Task Entry card (bubbles to parent context).
  - `keys:activate-current-item`: Triggers the focused button's action (Open Modal or Launch).
  - `keys:dismiss-overlay` (Esc): Returns focus to the Text Area.

### 3.3 Auto-Save

- **Debounce**: 500ms of inactivity.
- **Invalidation**: Typing invalidates pending save requests.
- **Persistence**: Drafts are stored locally and restored across sessions.
- **Defaults**: Default values are populated from last used selections.

## 4. Input Minor Modes

These modes map precise `KeyboardOperation`s to the interaction model. The following lists reflect the exact operations defined in the implementation, using the `keys:operation-name` notation.

### 4.1 `DRAFT_TEXT_EDITING_MODE`

Active when the text area is focused.

- **Cursor Movement**:
  - `keys:move-to-beginning-of-line`, `keys:move-to-end-of-line`: Move cursor to start/end of current line.
  - `keys:move-forward-one-character`, `keys:move-backward-one-character`: Move cursor right/left.
  - `keys:move-forward-one-word`, `keys:move-backward-one-word`: Move cursor forward/backward by word.
  - `keys:move-to-previous-line`, `keys:move-to-next-line`: Move cursor up/down.
  - `keys:move-to-beginning-of-document`, `keys:move-to-end-of-document`: Move cursor to start/end of text.
  - `keys:move-to-beginning-of-paragraph`, `keys:move-to-end-of-paragraph`: Move to start/end of paragraph.
  - `keys:move-to-beginning-of-sentence`, `keys:move-to-end-of-sentence`: Move to start/end of sentence.
  - `keys:recenter-screen-on-cursor`: Scroll to center the cursor.
  - `keys:scroll-down-one-screen`, `keys:scroll-up-one-screen`: Scroll text area page by page.
  - `keys:move-line-up`, `keys:move-line-down`: Move current line up/down.
  - `keys:history-prev`, `keys:history-next`: Cycle prompt history.
- **Editing**:
  - `keys:delete-character-forward`, `keys:delete-character-backward`: Delete character right/left.
  - `keys:delete-word-forward`, `keys:delete-word-backward`: Delete word right/left.
  - `keys:delete-to-end-of-line`, `keys:delete-to-beginning-of-line`: Delete to end/start of line.
  - `keys:open-new-line`: Insert new line (Shift+Enter).
  - `keys:toggle-insert-mode`: Toggle between insert and overwrite modes.
  - `keys:duplicate-line-selection`: Duplicate current line or selection.
  - `keys:join-lines`: Join current line with next line.
  - `keys:toggle-comment`: Toggle comment on current line/selection.
  - `keys:indent-region`, `keys:dedent-region`: Indent/Dedent selected lines.
  - `keys:uppercase-word`, `keys:lowercase-word`, `keys:capitalize-word`: Change case of word.
  - `keys:transpose-characters`, `keys:transpose-words`: Swap characters/words.
  - `keys:bold`, `keys:italic`, `keys:underline`: Apply markdown formatting.
- **Selection**:
  - `keys:select-all`: Select all text.
  - `keys:select-word-under-cursor`: Select word under cursor.
  - `keys:set-mark`: Start selection (Emacs style).
- **Clipboard**:
  - `keys:cut`, `keys:copy`, `keys:paste`: Standard clipboard operations.
  - `keys:cycle-through-clipboard`: Cycle through clipboard history.
- **Search**:
  - `keys:incremental-search-forward`, `keys:incremental-search-backward`: Search within text area.
  - `keys:find-next`, `keys:find-previous`: Navigate search matches.
- **System/Action**:
  - `keys:undo`, `keys:redo`: Undo/Redo last action.
  - `keys:indent-or-complete`: Trigger autocomplete or indent.
  - `keys:show-launch-options`: Open Advanced Options modal.
  - `keys:launch-and-focus`, `keys:launch-in-split-view`, `keys:launch-in-split-view-and-focus`: Launch task with specific layout.
  - `keys:launch-in-horizontal-split`, `keys:launch-in-vertical-split`: Launch task in specific split.

### 4.2 `DRAFT_TEXTAREA_TO_BUTTONS_MODE`

Handles the transition from Text Area to Action Bar.

- `keys:move-to-next-field`: Focus the first button in the Action Bar (Tab).
- `keys:move-to-previous-field`: Focus the last button in the Action Bar (Shift+Tab).
- `keys:delete-current-task`: Delete/Archive the current draft.

### 4.3 `DRAFT_BUTTON_NAVIGATION_MODE`

Active when an Action Bar button is focused.

- `keys:move-to-next-field`: Cycle focus to the next button (or back to Text Area).
- `keys:move-to-previous-field`: Cycle focus to the previous button (or back to Text Area).
- `keys:move-forward-one-character`: Move focus right (same as next field).
- `keys:move-backward-one-character`: Move focus left (same as previous field).
- `keys:navigate-up`, `keys:navigate-down`: Bubble focus to parent context.
- `keys:activate-current-item`: Trigger the focused button's action.
- `keys:dismiss-overlay`: Return focus to Text Area.

### 4.4 `MODAL_NAVIGATION_MODE`

Active for general modal dialogs (Repository/Branch selection).

- **Navigation**:
  - `keys:move-to-next-line`, `keys:move-to-previous-line`: Navigate through list items.
  - `keys:move-to-next-field`, `keys:move-to-previous-field`: Navigate between modal sections (if any).
  - `keys:activate-current-item`: Confirm selection.
  - `keys:dismiss-overlay`: Close modal without selecting.
- **Text Editing** (for search inputs):
  - `keys:move-to-beginning-of-line`, `keys:move-to-end-of-line`: Move cursor in input.
  - `keys:move-forward-one-character`, `keys:move-backward-one-character`: Move cursor right/left.
  - `keys:move-forward-one-word`, `keys:move-backward-one-word`: Move cursor by word.
  - `keys:delete-character-forward`, `keys:delete-character-backward`: Delete character.
  - `keys:delete-word-forward`, `keys:delete-word-backward`: Delete word.
  - `keys:delete-to-end-of-line`, `keys:delete-to-beginning-of-line`: Delete to end/start.
- **Clipboard**:
  - `keys:cut`, `keys:copy`, `keys:paste`, `keys:cycle-through-clipboard`: Standard clipboard operations.
- **Values**:
  - `keys:increment-value`, `keys:decrement-value`: Adjust numeric values (if applicable).

### 4.5 `MODEL_SELECTION_MODE`

Active specifically for the Model Selection dialog.

- **Navigation**:
  - `keys:move-to-next-line`, `keys:move-to-previous-line`: Navigate through model list.
  - `keys:activate-current-item`: Toggle selection of current model.
  - `keys:dismiss-overlay`: Close modal.
- **Values**:
  - `keys:increment-value`, `keys:decrement-value`: Adjust instance count for selected model.
- **Text Editing** (for filter input):
  - `keys:move-to-beginning-of-line`, `keys:move-to-end-of-line`: Move cursor in filter.
  - `keys:move-forward-one-character`, `keys:move-backward-one-character`: Move cursor right/left.
  - `keys:move-forward-one-word`, `keys:move-backward-one-word`: Move cursor by word.
  - `keys:delete-character-forward`, `keys:delete-character-backward`: Delete character.
  - `keys:delete-word-forward`, `keys:delete-word-backward`: Delete word.
  - `keys:delete-to-end-of-line`, `keys:delete-to-beginning-of-line`: Delete to end/start.

## 5. Footer Shortcuts

Context-sensitive shortcuts displayed in the footer when the Task Entry component is active.

- **Draft Text Area Focused**:
  "keys:show-launch-options Advanced Options • keys:activate-current-item Launch Agent(s) • keys:open-new-line New Line • keys:indent-or-complete Complete/Next Field"
  - "Agent(s)" is plural if multiple agents are selected.

- **Draft Button Focused**:
  "keys:navigate-up Text Area • keys:move-to-next-field Next Button • keys:activate-current-item Select • keys:dismiss-overlay Cancel"

## 6. Advanced Features

### 6.1 Autocomplete

The autocomplete system provides intelligent, context-aware suggestions as the user types.

#### 6.1.1 Triggers and Sources

- **Files (`@`)**: Typing `@` triggers a fuzzy search of files in the repository.
- **Workflows (`/`)**: Typing `/` triggers a fuzzy search of available workflows (from `.agents/workflows/`).
- **Workspace Terms (Typing)**: Typing any regular token (two or more characters) triggers a search against the `WorkspaceTermsEnumerator`, which indexes repository tokens.
  - **Preference**: Can be disabled via `workspace_terms_menu = false` in settings.

#### 6.1.2 Visual Presentation

- **Popup Menu**: A vertical list of ranked suggestions.
  - **Immediate Opening**: Opens immediately upon typing a trigger, showing cached results while a background refresh occurs.
  - **Updates**: Refreshes automatically when background indexing completes, preserving selection if possible.
- **Ghost Text**: Inline suggestions displayed ahead of the cursor.
  - **Two-Segment Dimming**:
    1. \*Shared Continuation\*\*: The portion of text guaranteed across every match (lighter dim + muted color).
    2. \*Shortest Completion\*\*: The remainder needed to reach the shortest matching term (brighter dim + normal text color).
  - **Mirroring**: When the popup menu is focused, ghost text mirrors the currently selected item.

#### 6.1.3 Interaction Model (`Autocomplete Active Mode`)

When suggestions are visible, the input system enters `Autocomplete Active` mode, prioritizing completion actions over standard navigation.

- **`keys:indent-or-complete` (Tab)**:
  1. \*First Press\**: Inserts the*Shared Continuation\*\* (if it exists).
  2. \*Second Press\**: Inserts the*Shortest Completion\*\* remainder.
  3. \*Single Match\*\*: If only one match exists, inserts the full completion immediately.
  4. \*Fallback\*\*: If no inline completion is available, acts as `keys:move-to-next-field`.
- **`keys:move-forward-one-character` (Right Arrow)**: Accepts the currently active suggestion (quick select).
- **Navigation**:
  - `keys:move-to-next-field` or `keys:navigate-up`/`keys:navigate-down`: Cycles through popup menu items.
  - **Mouse Wheel**: Scrolls the popup menu.
  - **Mouse Click**: Selects an item.

#### 6.1.4 Performance

- **Caching**: File and workflow lists are cached for instant display.
- **Fuzzy Matching**: All filtering uses fuzzy matching logic.
- **Non-Blocking**: Ghost text rendering consumes pre-calculated suffixes from `WorkspaceTermsEnumerator` to avoid filesystem scanning on every keystroke.

### 6.2 Auto-Save

- **Mechanism**: Drafts are saved to local storage (SQLite/JSON).
- **Debounce**: 500ms of inactivity required before saving.
- **Invalidation**: Any keystroke during the debounce period invalidates the pending save.
- **Status Indicators**:
  - **Unsaved**: Gray dot/text (content changed, timer running).
  - **Saving...**: Yellow dot/text (save request in flight).
  - **Saved**: Green dot/text (save confirmed).
  - **Error**: Red dot/text (save failed).

### 6.3 Selection Dialogs

When buttons are activated (`keys:move-to-next-field`/`keys:activate-current-item`), overlay dialog windows are displayed.

- **Common Characteristics**:
  - Full-screen overlay.
  - Dedicated input box.
  - `keys:dismiss-overlay` to cancel.
  - `keys:activate-current-item` to confirm.

#### 6.3.1 Repository Selector

- **Behavior**: Fuzzy search through available repositories.

#### 6.3.2 Branch Selector

- **Behavior**: Fuzzy search through repository branches.

#### 6.3.3 Agent Multi-Selector

- **Interface**: Multi-select interface with instance counts and +/- controls.
- **Display**: Shows selected models as comma-separated list with instance counts (e.g., "🤖 model1, model2 x2").
- **Persistence**: Selections are saved per-draft and restored.

---

## 6. Integration Points

- **Dashboard**: Used as the primary interface for creating new tasks.
- **Agent Activity**: Used as the "Instructions Card" for providing feedback or branching.
- **Session Viewer**: Used for injecting instructions into past sessions.
