# Terminal Multiplexers Status

## Overview

- **Goal:** Deliver robust, cross-platform multiplexer integrations for the Agent Harbor TUI and CLI, enabling all supported backends to participate in task layouts with consistent selection logic, diagnostics, and automated regression coverage. This status plan focuses on the work tracked as **R1. Multiplexer enablement and automated regression coverage** in `specs/Public/First.Release.status.md` and breaks it into granular milestones with testable completion criteria.
- **Scope:** Backend wiring in `ah-mux`, shared multiplexer selection in `ah-core`/`ah-cli`, CLI diagnostics (`ah health`), manual exerciser tooling, and automated tests covering all validated multiplexers as described in `TUI-Multiplexers-Overview.md` and the per-backend guides in this folder.

### Outstanding Multiplexer Milestones

- [ ] MUX1. Backend enablement and feature flags for all supported multiplexers
- [ ] MUX2. Shared multiplexer selection and environment detection
- [ ] MUX3. CLI diagnostics and documentation alignment
- [ ] MUX4. Multiplexer exerciser tooling and manual-test workflow
- [ ] MUX5. TUI dashboard coverage across multiplexers

---

## MUX1. Backend enablement and feature flags for all supported multiplexers

**Status:** Not started — only tmux/iTerm2 backends are wired through the task manager today and the default feature set only enables `tmux` in `ah-mux`.

**Deliverables:**

- [ ] Extend `ah-core::create_local_task_manager_with_multiplexer` so it can construct every backend exported from `ah-mux` (tmux, kitty, wezterm, zellij, screen, tilix, windows-terminal, ghostty, vim, neovim, emacs). When a concrete backend is selected but unavailable on the host (binary missing, unsupported platform, or runtime probe failure), the factory must return an explicit error instead of silently falling back to auto-detection.
- [ ] Ensure the `CliMultiplexerType` surface (in `ah-cli`) covers the same set of backends as `ah-mux` and that each variant maps one-to-one to a concrete `Multiplexer` implementation. Unknown/legacy values must produce clear CLI errors.
- [ ] Promote all validated multiplexers to the default feature set in `ah-mux` using `cfg(target_os)` guards where appropriate so release builds compile every supported backend on their respective platforms.
- [ ] Update the Nix flake, development shells, and CI images so the required binaries for each backend are installed (or stubbed via controlled fallbacks) on Linux and macOS, ensuring both compilation and runtime availability checks succeed during automated tests.
- [ ] Introduce a dedicated Justfile target (e.g., `just test-multiplexers`) that runs the multiplexer-focused test suites (unit + integration) across all enabled backends and is wired into CI workflows for the supported platforms.

**Verification (automated):**

- [ ] Add or extend integration tests in `crates/ah-mux/tests/` so that, for each backend feature, the corresponding `Multiplexer` implementation:
  - Reports `is_available() == true` when the binary is present and minimal smoke commands succeed,
  - Fails predictably with a well-typed error when binaries are missing or the platform is unsupported.
- [ ] Add tests in `ah-core` verifying that `create_local_task_manager_with_multiplexer`:
  - Constructs the requested backend when `CliMultiplexerType::<backend>` is provided and the environment is compatible,
  - Returns a structured error (not a fallback selection) when the backend is unavailable.
- [ ] Configure CI jobs (Linux/macOS) to invoke `just test-multiplexers` and fail if any backend is skipped, does not compile, or fails its availability checks.

---

## MUX2. Shared multiplexer selection and environment detection

**Status:** Not started — environment detection and selection logic is duplicated between `ah-core` and `ah-cli`, only a subset of multiplexers are considered, and nested terminal/multiplexer scenarios are handled inconsistently.

**Deliverables:**

- [ ] Expand the multiplexer environment detection logic so that it:
  - Detects both terminal emulators (wezterm, kitty, ghostty, tilix, Windows Terminal, iTerm2) and multiplexers/editors (tmux, zellij, screen, vim/neovim, emacs),
  - Correctly resolves nested cases (e.g., tmux inside WezTerm) by preserving the innermost supported multiplexer when both a terminal and a multiplexer are present.
- [ ] Refactor `ah-cli::tui::determine_multiplexer_choice` to delegate to a shared implementation in `ah-core` (e.g., `ah_core::multiplexers::determine_multiplexer_choice`), eliminating duplicated logic while keeping CLI-specific flag precedence clearly defined.
- [ ] Ensure the selection logic respects the full set of inputs:
  - Explicit `--multiplexer` flag,
  - Configuration from `.ah/config.toml` or environment variables (where applicable),
  - Auto-detection when no explicit preference is provided.
    In all cases, the chosen multiplexer must be validated against `is_available()` and the concrete backend capability set.

**Verification (automated):**

- [ ] Add comprehensive unit tests for the shared `determine_multiplexer_choice` logic in `ah-core` that cover:
  - All combinations of CLI flag, config, and environment detection,
  - Nested terminal + multiplexer environments, ensuring the innermost supported multiplexer is selected,
  - Error paths when a requested multiplexer is unavailable.
- [ ] Add CLI-level tests in `ah-cli` that launch `ah tui --multiplexer <type>` in a headless harness and assert:
  - The correct variant is passed through to `create_local_task_manager_with_multiplexer`,
  - Launching inside an existing multiplexer session attaches to it, while launching outside creates a new session with the standard TUI layout (editor pane + agent pane).
- [ ] Ensure the existing `determine_multiplexer_choice` logic in `ah-cli` is fully covered by tests before and after the refactor, and add regression tests to guard against reintroducing duplicated or divergent selection rules.

---

## MUX3. CLI diagnostics and documentation alignment

**Status:** Not started — the CLI exposes multiplexer selection flags but provides limited visibility into which backends are available, how they are detected, and why a particular backend was rejected or selected.

**Deliverables:**

- [ ] Extend the `ah` CLI with health diagnostics for multiplexers (e.g., via `ah health` or a dedicated subcommand) that report, for each supported backend:
  - Discovery status (found binary path, version string),
  - Platform compatibility notes (e.g., “iTerm2: macOS only”),
  - Any known configuration issues (e.g., missing environment variables, permission problems).
- [ ] Ensure diagnostics are structured (e.g., JSON or machine-readable text) so automated tooling and tests can parse the output reliably while still remaining readable to humans.
- [ ] Update the multiplexer documentation in this folder (`TUI-Multiplexers-Overview.md` and the per-backend guides such as `tmux.md`, `Kitty.md`, `WezTerm.md`, etc.) to:
  - Reference the new health checks and typical outputs,
  - Document OS-specific prerequisites introduced by the enablement work,
  - Clarify which multiplexers are considered “tier-1 validated” for the first public release.

**Verification (automated):**

- [ ] Add CLI integration tests that invoke the health diagnostics command in a controlled environment (with and without specific binaries installed) and assert that:
  - Each backend’s reported status matches the runtime environment,
  - Error and warning messages are emitted for missing or misconfigured backends,
  - The output schema remains backward compatible (validated via snapshot tests).
- [ ] Add markdown linting and link checking for the updated multiplexer docs as part of `just lint-specs`, and ensure CI fails when references to CLI options or commands drift from the current implementation.

---

## MUX4. Multiplexer exerciser tooling and manual-test workflow

**Status:** Not started — there is no lightweight, reusable tool to exercise multiplexer behavior independently of the full TUI dashboard, making manual debugging and interactive validation harder than necessary.

**Deliverables:**

- [ ] Implement a small “multiplexer exerciser” TUI/CLI under `tests/tools/` (or an equivalent utilities module) that:
  - Connects to the selected multiplexer backend via `ah-mux`,
  - Provides keyboard shortcuts to create standard Agent Harbor task layouts (editor + agent + logs) and additional panes,
  - Logs each action (window creation, pane split, command launch) to a per-run log file.
- [ ] Add a Justfile recipe `just manual-test-multiplexers` that launches the exerciser with the current repository as workspace, wired to use the same multiplexer selection logic as `ah tui`.
- [ ] Ensure the exerciser can run in both “attach to existing session” and “create new session” modes, mirroring the intended behavior of the local task manager.

**Verification (automated):**

- [ ] Add smoke tests that run the exerciser in headless mode against a subset of multiplexers in CI, asserting that:
  - It exits successfully after creating and tearing down a basic layout,
  - The expected log files are created and contain evidence of the requested actions.
- [ ] Add integration tests that verify the exerciser honors the same `CliMultiplexerType` selection semantics as `ah tui` (flags, config, environment), ensuring there is no divergence between manual-test tooling and production code paths.

---

## MUX5. TUI dashboard coverage across multiplexers

**Status:** Not started — TUI layouts and session flows are primarily validated against tmux/iTerm2; other multiplexers lack end-to-end coverage and golden layouts.

**Deliverables:**

- [ ] Extend the TUI scenario suite so that the main dashboard:
  - Launches using each supported multiplexer backend,
  - Creates the standard task layout (with editor and agent panes),
  - Tears down cleanly without leaving stray windows/panes.
- [ ] Record golden layouts (per backend) for terminal-based multiplexers where layout metadata is stable enough to be asserted (e.g., relative pane arrangement and roles).
- [ ] Ensure TUI logs and scenario metadata capture the selected multiplexer backend so layout regressions can be triaged in the context of the correct implementation.

**Verification (automated):**

- [ ] Add scenario-based golden tests under `crates/ah-tui/tests/` (or the equivalent scenario harness) that:
  - Run the dashboard bound to each multiplexer backend,
  - Compare captured layouts and key events against per-backend golden files,
  - Fail when pane arrangements or launch behavior deviate unexpectedly.
- [ ] Integrate these multiplexer-specific TUI tests into `just test-rust` (or a dedicated `just test-tui-multiplexers` target invoked from CI) so regressions are caught automatically for every supported backend.

### Overview

Goal: Implement a comprehensive terminal multiplexer abstraction layer that provides unified access to various terminal multiplexers and terminal emulators across multiple platforms. The implementation follows a two-layer architecture: a low-level AH-agnostic trait (`ah-mux-core`) defining generic window/pane primitives, and a high-level AH adapter (`ah-mux`) translating Agent Harbor concepts into multiplexer operations.

The multiplexer infrastructure enables Agent Harbor to create, manage, and orchestrate terminal-based agent workflows across tmux, kitty, WezTerm, Zellij, GNU Screen, iTerm2, Windows Terminal, Ghostty, Tilix, Vim, Neovim, and Emacs.

### Architecture

**Two-Layer Design**:

- **Low-level AH-agnostic layer** (`ah-mux-core`): Defines the `Multiplexer` trait with generic primitives (windows, panes, splits, command execution) that any terminal multiplexer can implement, without any Agent Harbor-specific logic.
- **High-level AH adapter** (`ah-mux`): Provides concrete implementations for each multiplexer backend, auto-detection logic, and AH-specific layout orchestration.

This separation keeps the low-level trait reusable outside Agent Harbor while allowing iteration on AH-specific layouts without changing backend implementations.

### System Integration Status

The multiplexer infrastructure is integrated at multiple levels within the Agent Harbor system:

**✅ Fully Integrated Components:**

1. **ah-cli health command** (`crates/ah-cli/src/health.rs`)
   - Uses detection module to identify terminal environments
   - Checks tmux availability for health diagnostics
   - Status: Only tmux is checked, other multiplexers not integrated

2. **ah-core local task manager** (`crates/ah-core/src/local_task_manager.rs`)
   - Uses `AwMultiplexer` wrapper for AH-specific layout creation
   - Calls `create_task_layout()` for standard agent workspace layouts
   - Status: Fully integrated with generic multiplexer support

3. **ah-core task manager initialization** (`crates/ah-core/src/task_manager_init.rs`)
   - Auto-detects terminal environment using detection module
   - Creates `GenericLocalTaskManager` with appropriate multiplexer
   - Status: Only tmux and iTerm2 are instantiated, others return errors

4. **ah-tui record/replay** (`crates/ah-tui/src/record.rs`, `replay.rs`)
   - Hardcoded to use `TmuxMultiplexer` only
   - Status: No dynamic multiplexer selection, tmux-only

5. **ah-tui-multiplexer** (`crates/ah-tui-multiplexer/src/lib.rs`)
   - Provides `AwMultiplexer` wrapper for AH-specific abstractions
   - Implements `LayoutHandle` for role-based pane management
   - Implements `create_task_layout()` for standard AH layouts
   - Status: Generic implementation works with any multiplexer

**⚠️ Partially Integrated Components:**

6. **ah-cli tui command** (`crates/ah-cli/src/tui.rs`)
   - Has CLI enum for all multiplexer types
   - Detection logic exists but only tmux and iTerm2 instantiated
   - 10 multiplexers have TODO comments (kitty, wezterm, zellij, screen, tilix, windows-terminal, ghostty, neovim, vim, emacs)
   - Status: CLI structure ready, implementation incomplete

**❌ Not Integrated Components:**

7. **REST API** - No multiplexer integration (stateless HTTP API)
8. **GUI/Electron** - No multiplexer integration identified
9. **Sandbox components** - No multiplexer integration
10. **Agent implementations** - No direct multiplexer usage

### Integration Gaps and TODOs

**High Priority Integration Work:**

1. **Complete CLI multiplexer instantiation** (`ah-cli/src/tui.rs` lines 293-330)
   - 10 TODO comments for unimplemented multiplexer instantiation
   - Need to add: kitty, wezterm, zellij, screen, tilix, windows-terminal, ghostty, neovim, vim, emacs
   - Each requires: `Ok(Box::new(ah_mux::<MuxName>Multiplexer::new()?))`

2. **Update health check to test all multiplexers** (`ah-cli/src/health.rs`)
   - Currently only checks tmux availability
   - Should iterate through all available multiplexers
   - Use `ah_mux::available_multiplexers()` function

3. **Remove hardcoded tmux from record/replay** (`ah-tui/src/record.rs`, `replay.rs`)
   - Replace `TmuxMultiplexer` with dynamic multiplexer selection
   - Use detection module or CLI argument to choose multiplexer

4. **Add multiplexer_kind tracking** (`ah-cli/src/task.rs` line 420)
   - TODO comment indicates multiplexer tracking not implemented
   - Need to record which multiplexer was used for each task

**Medium Priority Integration Work:**

5. **Multiplexer preference configuration**
   - Add config file support for default multiplexer selection
   - Allow per-project multiplexer preferences
   - Document multiplexer selection precedence

6. **Error handling improvements**
   - Better error messages when requested multiplexer unavailable
   - Graceful fallback to alternative multiplexers
   - User-friendly troubleshooting guidance
