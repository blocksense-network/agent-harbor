# TUI Multiplexers — Overview and Abstractions

This folder tracks first‑class integrations with terminal multiplexers and terminal emulators that can host multiple tabs/panes and run interactive agent sessions, as referenced by the TUI PRD and the URL handler spec.

## Rust Traits: Low‑Level Multiplexer API (AH‑agnostic) and AH Adapter

We split responsibilities into two layers:

- Low‑level AH‑agnostic trait: a toolbox of generic window/pane primitives that any terminal multiplexer/editor can implement.
- High‑level AH adapter: translates Agent Harbor concepts (task IDs, standard layouts, pane roles) into calls to the low‑level API. This lives in an AH crate and is not part of the low‑level trait.

### Low‑Level Trait (AH‑agnostic)

```rust
/// Opaque identifiers; implementations may encode native IDs/titles internally.
pub type WindowId = String;
pub type PaneId = String;

#[derive(Clone, Copy, Debug)]
pub enum SplitDirection { Horizontal, Vertical }

#[derive(Clone, Debug, Default)]
pub struct WindowOptions<'a> {
    pub title: Option<&'a str>,
    pub cwd: Option<&'a std::path::Path>,
    pub profile: Option<&'a str>,  // implementation-defined (e.g., iTerm2 profile)
    pub focus: bool,
}

#[derive(Clone, Debug, Default)]
pub struct CommandOptions<'a> {
    pub cwd: Option<&'a std::path::Path>,
    pub env: Option<&'a [(&'a str, &'a str)]>,
}

#[derive(thiserror::Error, Debug)]
pub enum MuxError {
    #[error("multiplexer not available: {0}")] NotAvailable(&'static str),
    #[error("not found")] NotFound,
    #[error("command failed: {0}")] CommandFailed(String),
    #[error("io error: {0}")] Io(#[from] std::io::Error),
    #[error("other: {0}")] Other(String),
}

pub trait Multiplexer {
    /// Implementation identifier (e.g., "tmux", "wezterm", "kitty", "iterm2", "vim", "emacs").
    fn id(&self) -> &'static str;

    /// Check whether the implementation is available and usable on this system.
    fn is_available(&self) -> bool;

    /// Open a new top‑level window/tab. Returns its WindowId.
    fn open_window(&self, opts: &WindowOptions) -> Result<WindowId, MuxError>;

    /// Split the given target pane (or initial window) and return the new PaneId.
    fn split_pane(
        &self,
        window: &WindowId,
        target: Option<&PaneId>,
        dir: SplitDirection,
        percent: Option<u8>,
        opts: &CommandOptions,
        initial_cmd: Option<&str>,
    ) -> Result<PaneId, MuxError>;

    /// Run a command in an existing pane.
    fn run_command(&self, pane: &PaneId, cmd: &str, opts: &CommandOptions) -> Result<(), MuxError>;

    /// Send literal text to a pane (including newlines). Optional to implement; may return NotAvailable.
    fn send_text(&self, pane: &PaneId, text: &str) -> Result<(), MuxError> { let _ = (pane, text); Err(MuxError::NotAvailable(self.id())) }

    /// Focus a window or pane.
    fn focus_window(&self, window: &WindowId) -> Result<(), MuxError>;
    fn focus_pane(&self, pane: &PaneId) -> Result<(), MuxError>;

    /// Discover windows (best effort). Implementations may filter by title substring.
    fn list_windows(&self, title_substr: Option<&str>) -> Result<Vec<WindowId>, MuxError>;

    /// Discover panes within a window (best effort).
    fn list_panes(&self, window: &WindowId) -> Result<Vec<PaneId>, MuxError>;
}
```

Design choices and rationale

- AH‑agnostic surface: No task IDs, pane roles, or AH layouts leak into the trait. Callers work with generic windows/panes, titles, and commands.
- Common denominators only: The methods reflect capabilities present across tmux/wezterm/kitty/iTerm2/Tilix/Vim/Emacs, enabling portable automation.
- Optional `send_text`: Some backends provide reliable text injection (tmux/wezterm/kitty). Others may only support initial commands; making this optional preserves portability.
- Discovery is best‑effort: Not all backends have robust enumeration APIs. We expose list methods with minimal filters and rely on higher layers to track handles/titles.
- Sync Result API: Keeps CLI/TUI code straightforward. Implementations internally enforce timeouts and non‑blocking behavior.

### High‑Level AH Adapter (in `ah-mux` monolith)

An AH‑specific adapter provides:

- Standard AH layouts (per TUI PRD) expressed as composition of `open_window` + `split_pane` + `run_command`.
- Role mapping (editor/tui/logs) to concrete PaneIds maintained in an adapter‑level `LayoutHandle`.
- Discovery/reuse using title hints and the low‑level `list_*` methods.

This separation keeps the low‑level trait reusable outside Agent Harbor and allows us to iterate on AH layouts without changing backend implementations. In the repository, the AH adapter and all backend implementations live in the `ah-mux` monolith crate under feature‑gated modules; optional facade crates re‑export individual backends when separate packages are desirable.

Scope includes classic multiplexers and terminal apps with comparable capabilities (tabs, splits, focus control, programmatic command launch), and editors with built‑in terminals.

- See the template: [Multiplexer-Description-Template](Multiplexer-Description-Template.md)
- Candidate list (initial):
  - tmux
  - Zellij
  - GNU screen (TBD)
  - iTerm2 (macOS)
  - Kitty
  - WezTerm
  - Ghostty (macOS)
  - Tilix (Linux)
  - Windows Terminal
  - Vim (terminal buffers) / Neovim
  - Emacs (vterm/ansi-term/shell)

Each individual doc should answer all template questions and provide copy‑pasteable commands for:

- Creating a new tab with a split layout suited for an agent coding session (as per TUI PRD).
- Launching specific commands in each pane.
- Scripting automated answers/keystrokes when necessary (e.g., “send keys”).
- Focusing an existing task’s pane/window by task id or title hint.
