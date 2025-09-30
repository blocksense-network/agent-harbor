## Terminal Config — Reusable Terminal Profiles for Test Runners

### Purpose

Provide reusable terminal profiles decoupled from scenario content. Profiles inform runners (TUI, CLI E2E capture) about terminal dimensions and optional rendering knobs without duplicating configuration across scenarios.

### File Format

- JSON; UTF‑8
- Unknown keys ignored for forward‑compatibility

### Schema (high level)

```json
{
  "name": "default-100x30",
  "width": 100,
  "height": 30,
  "theme": "default",
  "rendering": {
    "unicodeBoxChars": true,
    "normalizeWhitespace": true
  }
}
```

### Fields

- **name**: Human‑readable identifier used in snapshot/log paths when available.
- **width,height**: Terminal size in columns/rows.
- **theme**: Optional theme hint (e.g., `default`, `high-contrast`).
- **rendering**:
  - `unicodeBoxChars`: Prefer Unicode box‑drawing chars when rendering.
  - `normalizeWhitespace`: Enable whitespace normalization for stable goldens.

### Usage

- Scenarios reference terminal profiles via `terminalRef` (see [Scenario-Format.md](Scenario-Format.md)).
- When `terminalRef` is missing, runners use built‑in defaults and derive `terminalProfileId` as `<width>x<height>`.

### Paths and Naming

- Recommended location for shared profiles: `specs/Profiles/terminal/*.json` or a similar `configs/terminal/` directory in the test harness repo.
- Snapshot/log path composition uses `name` when present; otherwise `<width>x<height>`.


