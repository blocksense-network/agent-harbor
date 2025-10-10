## Can `tui-textarea` be “auto-expandable”?

There’s no built-in auto-grow. In Ratatui, **widgets are rendered into the `Rect` you give them**; size is controlled by your layout, not by the widget. So to “auto-expand”, compute the desired height and supply a taller `Rect` before calling `render_widget`. ([Ratatui][5])

* For each logical line in `textarea.lines()`, compute its **display width** (use `unicode_width::UnicodeWidthStr` and take your tab size into account), then estimate wrapped rows as:

  ```
  rows_for_line = max(1, ceil(display_width / w))
  ```
* Sum those for **all lines** to get `needed_rows`; clamp to `[min_rows, max_rows]`.
* Render with that height.
  This mirrors what the widget will display given width `w`, so the viewport won’t need a scrollbar until you hit `max_rows`.

Either way, **you** change the layout height; the widget won’t resize itself. Ratatui’s layout docs call out that you define the area and widgets fill it. ([Ratatui][5])

### Bonus: recentering vs. growing

If you don’t want to grow further, you can keep height fixed and use `TextArea::scroll(...)` to control the viewport (e.g., page up/down or recenter around the cursor). The crate exposes a `Scrolling` enum for page/half-page/delta scrolling. ([Docs.rs][6])

---

[5]: https://ratatui.rs/concepts/layout/?utm_source=chatgpt.com "Layout | Ratatui"
[6]: https://docs.rs/tui-textarea/latest/tui_textarea/enum.Scrolling.html?utm_source=chatgpt.com "Scrolling in tui_textarea - Rust - Docs.rs"
