This is a list of Ratatui APIs that might be helpful for the development of the Agent Harbor TUI.

# Core plumbing

- **Terminal + Backend**
  - `Terminal<CrosstermBackend<Stdout>>`, `Frame<'_>` — main render loop.
  - (Input/raw mode via **crossterm**: `event::read`, `enable_raw_mode`, paste/mouse capture if desired.)

- **Layout**
  - `layout::{Layout, Direction, Constraint, Rect}` — top/bottom split; dynamic resizing of the description editor by adjusting `Constraint::Length/Min/Percentage` on `Ctrl+Up/Down`.

- **Styling**
  - `style::{Style, Color, Modifier}`; `text::{Span, Spans, Line}` — high-contrast theme, focus highlights, inline validation/error messages.

# Dashboard & selectors (Project / Branch / Agent)

- **Lists with fixed height, filtering, and keyboard nav**
  - `widgets::{List, ListItem, Block, Borders}` for the selector widgets.
  - `widgets::ListState` to track the highlighted row and scroll offset.
  - Optional: `widgets::{Scrollbar, ScrollbarState}` to show explicit scroll affordance in each list’s viewport.
  - Use `highlight_style`, `highlight_symbol`, and a focused/unfocused **Block** style to indicate focus.
  - Filtering is your own state; re-generate `Vec<ListItem>` from the filter text each keystroke and keep `ListState` in range.

- **Filter inputs (per list)**
  - Ratatui doesn’t ship a text-input widget; render with:
    - `widgets::Paragraph` (for the visible field) + your own caret drawing and cursor position,
    - or a community widget (commonly used with Ratatui): **`ratatui-textarea`** / **`tui-textarea`** for single-line mode.

  - Show placeholder/help via `Spans` under the list.

# Description editor (multiline, resizable)

- **Multiline text**
  - `widgets::Paragraph` with `Wrap { trim: true }` for display,
  - or **`ratatui-textarea::TextArea`** for robust text editing (scrolling, word-wrap, cursor, undo).

- **Resizing**
  - Change the bottom panel’s `Constraint` in the `Layout` based on `Ctrl+Up/Down`.

- **Scrollable content indicator**
  - Use `Scrollbar` alongside the `Paragraph`/`TextArea`, maintaining a `ScrollbarState` tied to the editor’s scroll offset.

# Footer (dynamic shortcuts)

- Single-line bar with:
  - `widgets::Block` + `Spans` to draw key hints (e.g., “Esc Back • Ctrl+C Abort”).
  - Switch the content based on app mode; use contrasting `Style` for disabled/hidden hints.

# Modals & overlays (Help, prompts, inline validation areas)

- **Overlay/popup**
  - Compute a centered `Rect` and render in order:
    1. `widgets::Clear` (to blank the area),
    2. the modal contents (`Block` + `Paragraph`/`Table`).

- **Help overlay**
  - `widgets::Table` with `Row`/`Cell` or a `Paragraph` listing the keymap.

# Status bar / inline status

- **Status line**
  - `Paragraph` with `Spans` (e.g., backend, multiplexer, last operation).

- **Inline validation**
  - Small `Paragraph` under each selector using `Style::fg(Color::Red)` (or theme color).

# Focus & navigation

- You’ll implement focus as app state (an enum), then:
  - Render focused components with distinct `Style`/borders.
  - Drive `ListState` selection with Arrow/PageUp/PageDown/Home/End.
  - Use `Tabs` (optional) if you later add multiple screens: `widgets::Tabs`.

# Accessibility / theming

- **High-contrast theme**
  - Provide a theme struct of `Style`s and `Color`s; swap at runtime.

- **Predictable focus order**
  - Keep focus traversal (Tab/Shift+Tab) deterministic in your state machine; Ratatui just renders what you decide.

---

## Minimal crate/API checklist

- `ratatui::Terminal`, `backend::CrosstermBackend`
- `layout::{Layout, Direction, Constraint, Rect}`
- `style::{Style, Color, Modifier}`
- `text::{Span, Spans, Line}`
- `widgets::{Block, Borders, Paragraph, Wrap, List, ListItem, ListState, Table, Row, Cell, Tabs, Clear}`
- (Optional but recommended) `widgets::{Scrollbar, ScrollbarState}`
- **Input/eventing**: `crossterm::{event::{read, Event, KeyEvent, KeyCode, KeyModifiers}, terminal::{enable_raw_mode, disable_raw_mode}}`
- (For rich text editing) **`ratatui-textarea`** (community crate) for the description editor and the list filter fields.

> Note on scope: integration with tmux/zellij/screen, REST/SQLite/SSH, and “Ctrl+C twice to quit” semantics are outside Ratatui itself (your app state + crossterm event loop). Ratatui’s role is rendering + widget state (lists, text, overlays, layout, styling); input capture and process control come from crossterm and your own logic.
