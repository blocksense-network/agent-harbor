# Ratatui Inline Auto-Completion (with tiny `tui-textarea` fork)

This guide shows exactly how to implement auto-completion-style, inline autocomplete menus inside our editor area using Ratatui, Crossterm, and a minimal fork of `tui-textarea`. It’s designed for drop-in execution by the implementation team.

---

## 0) Overview

- **Goal**: caret-anchored, inline suggestion list rendered _inside the editor rect_; editor keeps focus at all times.
- **Trigger tokens**: `/` (workflows), `@` (files/resources).
- **Matcher**: `nucleo` in a background task; returns top-K with match indices for highlighting.
- **Testing**: golden buffer tests @ 80×24 & 120×40; unit tests for coordinate mapping and insertion.

---

## 1) Dependencies

**Cargo.toml (workspace members that render the TUI):**

```toml
[dependencies]
ratatui = "0.27"
crossterm = "0.27"
# Forked textarea under feature flag:
tui-textarea = { git = "https://github.com/blocksense-network/tui-textarea", branch = "inline-autocomplete", package = "tui-textarea", optional = true }

# Matching
nucleo = "0.5"          # or current
# Fallback (optional):
fuzzy-matcher = { version = "0.3", optional = true }

[features]
default = []
inline-autocomplete = ["tui-textarea", "nucleo"]
```

> The fork should be a minimal delta **only** adding:
>
> ```rust
> impl TextArea {
>     pub fn viewport_origin(&self) -> (u16, u16) { /* top_row, left_col */ }
>     pub fn gutter_width(&self) -> u16 { /* 0 if disabled */ }
> }
> ```
>
> No behavior change; just getters reading existing internal state.

---

## 2) Data Model & Traits

### 2.1 Provider & Matcher

```rust
/// Fired when user types `/` or `@` and continues typing
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Trigger { Slash, At }

/// Item presented in suggestions
#[derive(Debug, Clone)]
pub struct Item {
    pub id: String,      // stable id
    pub label: String,   // rendered text
    pub detail: String,  // optional right-side hint
}

#[async_trait::async_trait]
pub trait Provider: Send + Sync {
    fn trigger(&self) -> Trigger;
    async fn complete(&self, query: &str) -> Vec<Item>; // can cache, remote, etc.
}

/// Returned by matcher with indices for highlighting
pub struct ScoredMatch {
    pub item: Item,
    pub score: i32,
    pub indices: Vec<usize>, // byte offsets or char positions; keep consistent with renderer
}

pub trait Matcher: Send + Sync {
    fn query(&self, items: &[Item], pattern: &str, k: usize) -> Vec<ScoredMatch>;
}
```

**Concrete matcher (`nucleo`)** — implement `Matcher` by compiling a pattern and scoring; keep a small arena for throughput.

### 2.2 ViewModel for Inline Menu

```rust
pub struct InlineMenuVM {
    pub open: bool,
    pub trigger: Option<Trigger>,
    pub query: String,
    pub selected: usize,         // index into `results`
    pub results: Vec<ScoredMatch>,
    pub request_id: u64,         // monotonic; drop stale responses
    pub top_row: u16,            // scroll origin mirror
    pub left_col: u16,
}
```

---

## 3) Event Loop & Focus

Ratatui doesn’t read input—use `crossterm::event`:

```rust
use crossterm::event::{self, Event, KeyEvent, KeyCode, KeyModifiers, KeyEventKind};

fn handle_event(vm: &mut InlineMenuVM, textarea: &mut TextArea, tx_query: &mpsc::Sender<QueryMsg>) -> anyhow::Result<()> {
    if event::poll(Duration::from_millis(16))? {
        if let Event::Key(KeyEvent { code, kind: KeyEventKind::Press, .. }) = event::read()? {
            if vm.open {
                match code {
                    KeyCode::Up => { vm.selected = vm.selected.saturating_sub(1); return Ok(()); }
                    KeyCode::Down => { vm.selected = (vm.selected + 1).min(vm.results.len().saturating_sub(1)); return Ok(()); }
                    KeyCode::PageUp => { /* adjust selection by page */ return Ok(()); }
                    KeyCode::PageDown => { /* adjust selection by page */ return Ok(()); }
                    KeyCode::Tab | KeyCode::Enter => { insert_completion(vm, textarea); vm.open = false; return Ok(()); }
                    KeyCode::Esc => { vm.open = false; return Ok(()); }
                    _ => { /* fallthrough to editor to update query */ }
                }
            }
            // Always pass keys to the editor first (except consumed nav keys above)
            textarea.input(code.into());
            maybe_update_trigger_and_query(vm, textarea);
            if let Some(trigger) = vm.trigger {
                debounce_and_send_query(vm, trigger, &vm.query, tx_query)?;
            }
        }
    }
    Ok(())
}
```

**Key rule:** editor retains focus; inline menu only consumes navigation/commit keys when open.

---

## 4) Coordinate Mapping (the forked getters)

With the fork we can precisely place the popup at **caret+1 line** and the caret **column**, even under horizontal scroll and with wide graphemes.

```rust
pub fn caret_anchor(textarea: &TextArea, editor_area: ratatui::layout::Rect) -> (u16, u16) {
    let (row, col) = textarea.cursor();              // text coords
    let (top, left) = textarea.viewport_origin();    // from fork
    let gutter = textarea.gutter_width();            // from fork

    let vis_row = row.saturating_sub(top);
    let vis_col = col.saturating_sub(left);

    let anchor_y = editor_area.y + vis_row + 1;      // one line below caret
    let anchor_x = editor_area.x + gutter + 1 + vis_col; // +1 editor padding if applicable
    (anchor_x, anchor_y)
}
```

**Clipping**: compute desired width (`w_menu`) and height (`h_menu`), then clamp to the editor `Rect`:

```rust
fn clip_popup(editor: Rect, x: u16, y: u16, w: u16, h: u16) -> Rect {
    let x = x.min(editor.x + editor.width.saturating_sub(1));
    let y = y.min(editor.y + editor.height.saturating_sub(1));
    let w = w.min(editor.x + editor.width - x);
    let h = h.min(editor.y + editor.height - y);
    Rect { x, y, width: w, height: h }
}
```

---

## 5) Drawing the Inline Menu

Render **editor first**, then inline menu **inside the editor rect**.

```rust
use ratatui::{
  widgets::{Block, Borders, Clear, List, ListItem, ListState},
  style::{Style, Modifier},
  text::{Span, Spans},
  Frame
};

pub fn draw_editor_and_menu(f: &mut Frame, area: Rect, textarea: &TextArea, vm: &InlineMenuVM) {
    // 1) Editor
    f.render_widget(&*textarea, area);

    // 2) Inline menu
    if vm.open {
        let (ax, ay) = caret_anchor(textarea, area);
        let menu_w = area.width.min(48);
        let menu_h = vm.results.len().min(8) as u16;
        let popup = clip_popup(area, ax, ay, menu_w, menu_h);

        let items: Vec<ListItem> = vm.results.iter().map(|m| {
            // highlight matched indices in m.item.label
            let spans = highlight_with_indices(&m.item.label, &m.indices);
            ListItem::new(spans)
        }).collect();

        // Clear only the popup area to “float” within the editor
        f.render_widget(Clear, popup);

        let mut state = ListState::default();
        state.select(Some(vm.selected));

        let list = List::new(items)
            .block(Block::default().borders(Borders::ALL).title("Suggestions"))
            .highlight_symbol("▶")
            .highlight_style(Style::default().add_modifier(Modifier::BOLD));

        f.render_stateful_widget(list, popup, &mut state);
    }
}
```

---

## 6) Background Matching

### 6.1 Query pipeline

- Debounce keystrokes (60–100ms).
- Attach a **request_id** to each query; matcher echoes it back.
- On response: if `id < vm.request_id`, drop it (stale).

```rust
enum QueryMsg { New(u64 /*id*/, Trigger, String) }
enum ResultMsg { Ready(u64 /*id*/, Vec<ScoredMatch>) }

async fn matcher_task(mut rx: mpsc::Receiver<QueryMsg>, tx: mpsc::Sender<ResultMsg>, providers: Arc<Vec<Box<dyn Provider>>>, matcher: Arc<dyn Matcher>) {
    while let Some(QueryMsg::New(id, trig, q)) = rx.recv().await {
        let provider = providers.iter().find(|p| p.trigger() == trig).unwrap();
        let items = provider.complete(&q).await;
        let topk = matcher.query(&items, &q, 50);
        let _ = tx.send(ResultMsg::Ready(id, topk)).await;
    }
}
```

### 6.2 Highlight rendering

```rust
fn highlight_with_indices(label: &str, indices: &[usize]) -> Spans<'_> {
    // Assumes indices are byte offsets in UTF-8; convert to spans conservatively
    // Option: precompute char indices to avoid splitting graphemes mid-way
    let mut spans = Vec::new();
    let mut last = 0;
    let mut idxs = indices.to_vec(); idxs.sort_unstable();
    for &i in &idxs {
        if i > last { spans.push(Span::raw(&label[last..i])); }
        // assume single-byte for simplicity; for Unicode use grapheme cluster math
        spans.push(Span::styled(&label[i..i+1], Style::default().add_modifier(Modifier::BOLD)));
        last = i+1;
    }
    if last < label.len() { spans.push(Span::raw(&label[last..])); }
    Spans::from(spans)
}
```

> For full Unicode correctness, build indices at **grapheme** boundaries (via `unicode-segmentation`) and have the matcher return positions in grapheme units.

---

## 7) Insertion Semantics

When committing a suggestion:

1. Determine the current token span (from trigger to caret).
2. Replace that span with the canonical token (provider-specific).
3. Keep editor focus and set caret after inserted token.

```rust
fn insert_completion(vm: &InlineMenuVM, ta: &mut TextArea) {
    if let Some(sel) = vm.results.get(vm.selected) {
        let (cursor_row, cursor_col) = ta.cursor();
        let (start_col, end_col) = token_bounds_from_trigger(ta, vm.trigger.unwrap(), cursor_row, cursor_col);
        ta.delete_range(cursor_row, start_col, cursor_row, end_col);
        ta.insert_str_at(cursor_row, start_col, &sel.item.label);
        ta.set_cursor(cursor_row, start_col + sel.item.label.chars().count() as u16);
    }
}
```

---

## 8) Edge Cases & Clipping

- **Near bottom/right**: `clip_popup` ensures the menu shrinks and remains within editor bounds.
- **Horizontal scroll**: fork getters make column mapping exact.
- **Tabs**: ensure editor and coordinate mapper use the same tab width (e.g., 4).
- **Wide graphemes**: prefer grapheme-awareness in both editor cursor math and highlight spans.

---

## 9) Testing (must-have)

1. **Golden Snapshots** `crates/ah-tui/tests/golden_inline_menu.rs`
   - `golden_inline_{80x24,120x40}_caret_anchor_ok`
   - `golden_inline_high_contrast_visible`
   - `golden_inline_right_bottom_clipping_ok`

2. **Unit** `crates/ah-tui/tests/coord_map.rs`
   - `coord_map_with_scroll_and_gutter_ok`
   - `coord_map_tabs_and_wide_graphemes_ok`

3. **Behavior** `crates/ah-tui/tests/behavior_autocomplete.rs`
   - `keystroke_passthrough_and_nav_ok`
   - `insert_and_restore_focus_ok`

4. **Perf (sanity)** `crates/ah-tui/tests/perf_matcher.rs`
   - `topk_on_10k_items_sub_budget`

Make snapshots deterministic (fixed terminal size, theme, corpus, and fake time). Use `insta` or equivalent for golden buffers.

---

## 10) CI & Feature Flag

- Gate the entire feature behind `inline-autocomplete`.
- CI matrix:
  - `--features inline-autocomplete` (with fork)
  - base build (without the fork)

- Optional job to compare fork’s public API surface vs upstream (alert-only).

---

## 11) Upstreaming

- Open a PR to upstream `tui-textarea` with the two getters + docstrings.
- Keep our branch rebased; avoid behavior changes in the fork.

---

## 12) Implementation Checklist

- [ ] Add fork dependency + `inline-autocomplete` feature.
- [ ] Implement `Provider` for `/` workflows and `@` files/resources.
- [ ] Implement `Matcher` using `nucleo`.
- [ ] Build `InlineMenuVM`, debounce, and request-ID wiring.
- [ ] Add coordinate mapper using fork getters; add clipping.
- [ ] Render inside editor rect: editor → `Clear(popup)` → `List`.
- [ ] Insert semantics; caret restore.
- [ ] Golden/unit/perf tests; CI matrix.
- [ ] Prepare upstream PR with getters.

---
