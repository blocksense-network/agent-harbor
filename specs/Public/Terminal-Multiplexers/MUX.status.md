# Terminal Multiplexers Implementation Status

## Overview

Goal: Implement comprehensive support for all terminal multiplexers and terminals in the `ah-mux` and `ah-mux-core` crates, enabling Agent Harbor to seamlessly integrate with any terminal environment a developer uses. This status file tracks the implementation progress of each multiplexer/terminal backend, ensuring spec completeness, robust implementations, and thorough automated testing.

This work directly supports **R1 (Multiplexer enablement and automated regression coverage)** from `First.Release.status.md`, which requires all backends to be wired through the task manager, properly detected, and validated with automated tests before the first public release.

## Methodology

Each multiplexer/terminal follows a three-phase implementation approach:

1. **Specification**: Complete and accurate documentation in `specs/Public/Terminal-Multiplexers/<Tool>.md`
2. **Implementation**: Robust Rust implementation in `crates/ah-mux/src/<tool>.rs`
3. **Testing**: Comprehensive automated test coverage in `crates/ah-mux/tests/`

## Supported Multiplexers and Terminals

The `ah-mux` crate supports the following backends (12 total):

- **Terminal Multiplexers**: tmux, GNU Screen, Zellij, WezTerm
- **Terminal Emulators with Multiplexing**: Kitty, iTerm2, Ghostty, Tilix, Windows Terminal
- **Editor-based Multiplexers**: Vim, Neovim, Emacs

## Outstanding Tasks

### Release Integration

- [ ] **R1**: Multiplexer enablement and automated regression coverage (from First.Release.status.md)

### Per-Multiplexer Implementation Milestones

- [ ] **M1**: tmux (Linux, macOS, BSD)
- [x] **M2**: Kitty (Linux, macOS)
- [x] **M3**: WezTerm (Linux, macOS, Windows)
- [ ] **M4**: Zellij (Linux, macOS, BSD)
- [x] **M5**: GNU Screen (Linux, macOS, BSD)
- [x] **M6**: Tilix (Linux only
- [ ] **M7**: Windows Terminal (Windows only)
- [ ] **M8**: Ghostty (Linux, macOS)
- [ ] **M9**: Neovim (cross-platform)
- [ ] **M10**: Vim (cross-platform)
- [ ] **M11**: Emacs (cross-platform)
- [ ] **M12**: iTerm2 (macOS only)

---

## R1. Multiplexer Enablement and Automated Regression Coverage

**Status:** In progress — some deliverables completed, packaging and testing infrastructure remaining (assignees: Emil, Mila, Danny)

**Context:**

- The CLI exposes many backends via `CliMultiplexerType`, but any selection other than `Auto`, `Tmux`, or `ITerm2` falls back to auto-detection with a warning (`crates/ah-cli/src/tui.rs`).
- The task manager factory only instantiates `TmuxMultiplexer` (or `ITerm2Multiplexer` on macOS) regardless of the detected environment (`crates/ah-core/src/task_manager_init.rs`).
- `ah-mux` already ships per-backend implementations, yet the default feature set only enables `tmux` (`crates/ah-mux/Cargo.toml`), so the remaining integrations never compile in release builds.
- This work is critical for the first public release to ensure users can work with their preferred terminal environment.

**Deliverables:**

- [x] Extend `ah-core::create_local_task_manager_with_multiplexer` to construct every backend exported from `ah_mux` (tmux, kitty, wezterm, zellij, screen, tilix, windows-terminal, ghostty, vim, neovim, emacs) and reject selections that are unavailable instead of silently falling back.
- [x] Expand `determine_multiplexer_choice` so that environment detection maps nested terminals/multiplexers to the corresponding `CliMultiplexerType`, preserving the innermost supported multiplexer when both a terminal and multiplexer are detected.
- [x] Refactor `ah-cli::tui::determine_multiplexer_choice` to use the shared implementation from `ah-core`, eliminating code duplication.
- [ ] Promote the additional backends to the default feature set (with `cfg(target_os)` guards where required) and update the Nix flake + CI images to install the matching binaries so the code compiles and runs on all supported platforms.
- [ ] Teach the CLI to surface availability diagnostics (`ah health`) covering binary discovery, version checks, and user guidance when a backend is missing or misconfigured.
- [ ] Update developer documentation (`specs/Public/Terminal-Multiplexers/*.md`) to reference the new automated checks and any OS-specific prerequisites introduced by the releases.
- [ ] Implement a lightweight "multiplexer exerciser" CLI/TUI (living under `tests/tools/`) with keyboard shortcuts that trigger tab/pane creation in a similar way to what the local task manager does during launch_task, enabling manual validation of pane/window behavior without invoking the full Agent Harbor dashboard. Add a just target `manual-test-multiplexers` that launches the exerciser TUI.
- [ ] Ensure all supported/validated multiplexers are packaged in the Nix flake (and other platform manifests) so both automated and manual tests can install and run them consistently.

**Verification (automated):**

- [ ] Add unit tests that exercise the `determine_multiplexer_choice` logic (exists in both `ah-core` and `ah-cli`) with various combinations of detected terminal environments to ensure correct multiplexer choice selection based on priority rules.
- [ ] Add CLI integration tests that launch `ah tui --multiplexer <type>` in a headless harness and verify correct automatic session attachment behavior: launching inside an existing multiplexer session should attach to it, while launching outside should create a new session with the standard TUI layout (editor pane + agent pane). Tests should verify success by examining multiplexer sessions, windows/panes, and log files for evidence of the expected taken actions.
- [ ] Enable and stabilize the real-multiplexer integration tests in `crates/ah-mux/tests/integration_tests.rs` for tmux, kitty, wezterm, screen, zellij, tilix, windows-terminal, ghostty, vim, neovim, and emacs; gate each by the corresponding feature and add them to a `just test-multiplexers` target wired into CI for Linux/macOS/Windows.
- [ ] Extend the TUI scenario suite so that the dashboard launches, creates a task layout, and tears down successfully while bound to each multiplexer backend (record golden layouts per backend for the terminal-based multiplexers).

**Implementation Details:**

_(To be filled as implementation progresses)_

**Key Source Files:**

- `crates/ah-core/src/task_manager_init.rs` - Task manager factory and multiplexer instantiation
- `crates/ah-cli/src/tui.rs` - CLI multiplexer selection and TUI launch logic
- `crates/ah-mux/src/detection.rs` - Environment detection and multiplexer selection logic
- `crates/ah-mux/src/lib.rs` - Default multiplexer selection and priority ordering
- `crates/ah-mux/Cargo.toml` - Feature flags for multiplexer backends
- `crates/ah-mux/tests/integration_tests.rs` - Integration test suite for all multiplexers
- `flake.nix` - Nix flake with multiplexer binary dependencies
- `.github/workflows/ci.yml` - CI pipeline configuration for multiplexer tests
- `Justfile` - `test-multiplexers` and related targets

**Outstanding Tasks:**

- Complete Nix flake updates for all multiplexer binaries (Linux, macOS, Windows)
- Implement `ah health` diagnostics command
- Create multiplexer exerciser manual testing tool
- Enable and stabilize integration tests for all backends
- Add CLI integration tests for multiplexer selection
- Extend TUI scenario suite with golden layout tests
- Update all multiplexer spec files with automated check references

---

## M1. tmux (Linux, macOS, BSD)

**Status:** Mostly complete — implementation exists, spec is comprehensive, but automated tests are limited.

**Context:**

- tmux is the most mature and widely-supported multiplexer, serving as the reference implementation.
- The spec file (`tmux.md`) is detailed with examples for creating layouts, sending keys, and focusing panes.
- The implementation (`tmux.rs`) supports window/pane creation, splitting, command execution, and text sending.
- Snapshot tests exist but integration tests are marked `#[ignore]` and need enabling.

**Deliverables:**

- [ ] Review and update `specs/Public/Terminal-Multiplexers/tmux.md`:
  - Add notes on version compatibility (tested with tmux 2.x, 3.x)
  - Document known limitations (e.g., pane title setting requires recent versions)
  - Add troubleshooting section for common issues (socket permissions, server not found)
- [ ] Review `crates/ah-mux/src/tmux.rs` implementation:
  - Ensure all `Multiplexer` trait methods are fully implemented
  - Add error handling for edge cases (e.g., session already exists, pane not found)
  - Implement cleanup logic for test sessions
  - Add tracing/logging for debugging
- [ ] Enable and stabilize integration tests in `crates/ah-mux/tests/integration_tests.rs`:
  - Remove `#[ignore]` from tmux tests
  - Add test for complex layouts (editor + TUI + logs pane arrangement)
  - Add test for session attachment behavior
  - Add test for window/pane focusing
  - Add test for command execution and text sending
- [ ] Add golden snapshot tests for TUI scenarios using tmux backend.
- [ ] Document any tmux-specific configuration requirements (e.g., `allow-rename` setting).

**Verification (automated):**

- [x] Snapshot tests pass (`ah_mux__tmux__snapshot_testing__*`)
- [ ] Integration tests run successfully in CI without `#[ignore]`
- [ ] Unit tests cover all public methods in `TmuxMultiplexer`
- [ ] End-to-end test launching `ah tui --multiplexer tmux` and verifying layout creation
- [ ] Performance test ensuring tmux operations complete within reasonable timeouts (<2s for typical layouts)
- [ ] Cleanup test verifying no stray tmux sessions remain after test runs

**Implementation Details:**

_(To be filled after implementation complete)_

**Key Source Files:**

- `specs/Public/Terminal-Multiplexers/tmux.md` - tmux integration specification
- `crates/ah-mux/src/tmux.rs` - tmux multiplexer implementation (2400+ lines)
- `crates/ah-mux/tests/integration_tests.rs` - integration test suite
- `crates/ah-mux/src/snapshots/ah_mux__tmux__*.snap` - snapshot test fixtures

**Outstanding Tasks:**

- Enable ignored integration tests
- Add comprehensive error handling
- Document version compatibility
- Add troubleshooting guide

---

## M2. Kitty (Linux, macOS)

**Status:** Partially complete — implementation exists, spec is good, tests need expansion.

**Context:**

- Kitty uses a remote-control interface (`kitty @`) over a Unix socket, requiring `KITTY_LISTEN_ON` or `--listen-on`.
- The spec file (`Kitty.md`) documents the remote control API and provides examples.
- The implementation (`kitty.rs`) exists but may lack full feature parity with tmux.
- Integration tests need to ensure the socket is available and handle Kitty-specific quirks.

**Deliverables:**

- [x] Review and update `specs/Public/Terminal-Multiplexers/Kitty.md`:
  - Add section on socket setup and `KITTY_LISTEN_ON` environment variable
  - Document limitations compared to tmux (e.g., less mature window management)
  - Add examples of layout creation using `--location=hsplit|vsplit`
  - Add troubleshooting section (socket not found, permission denied)
- [x] Review and enhance `crates/ah-mux/src/kitty.rs`:
  - Ensure all `Multiplexer` trait methods are implemented
  - Add socket detection and automatic socket creation if needed
  - Implement proper cleanup for test windows/tabs
  - Add detailed error messages for socket-related failures
  - Add support for detecting Kitty version and features
- [ ] Add comprehensive automated tests:
  - Unit tests for socket detection and connection
  - Integration test creating tabs/windows with splits
  - Test for command execution and focus switching
  - Test for text sending via `kitty @ send-text`
  - Test cleanup and session teardown
- [ ] Add Kitty to the TUI scenario test suite (golden layout tests).

**Verification (automated):**

- [ ] Unit tests for socket detection and Kitty availability check
- [ ] Integration tests run successfully with Kitty installed
- [ ] Test verifying `kitty @ ls` output parsing
- [ ] Test verifying layout creation matches spec examples
- [ ] Cleanup test ensuring no stray Kitty instances remain
- [ ] Cross-platform test on Linux and macOS

**Implementation Details:**

_(To be filled after implementation)_

**Key Source Files:**

- `specs/Public/Terminal-Multiplexers/Kitty.md` - Kitty integration specification
- `crates/ah-mux/src/kitty.rs` - Kitty multiplexer implementation
- `crates/ah-mux/tests/integration_tests.rs` - integration tests (to be added)

**Outstanding Tasks:**

- Socket detection and setup
- Error handling improvements
- Integration test suite
- Documentation updates

---

## M3. WezTerm (Linux, macOS, Windows)

**Status:** Partially complete — implementation exists, spec needs updates, tests needed.

**Context:**

- WezTerm supports programmatic control via CLI and Lua configuration.
- WezTerm is one of the few multiplexers with native Windows support.
- The spec file (`WezTerm.md`) exists but may need updates for the latest WezTerm features.
- The implementation (`wezterm.rs`) exists but integration tests are missing.

**Deliverables:**

- [x] Review and update `specs/Public/Terminal-Multiplexers/WezTerm.md`:
  - Document WezTerm CLI commands for pane/tab management
  - Add examples for creating Agent Harbor task layouts
  - Document Windows-specific considerations
  - Add version compatibility notes (tested with WezTerm 20240203+)
  - Add troubleshooting section
- [x] Review and enhance `crates/ah-mux/src/wezterm.rs`:
  - Ensure cross-platform compatibility (Linux, macOS, Windows)
  - Implement all `Multiplexer` trait methods
  - Add WezTerm-specific error handling
  - Add support for WezTerm configuration file generation if needed
  - Implement proper cleanup logic
- [x] Add comprehensive automated tests:
  - Unit tests for WezTerm detection and version parsing
  - Integration tests for window/pane creation
  - Test for command execution in panes
  - Test for focus switching
  - Platform-specific tests (Windows behavior differs from Unix)
- [x] Add WezTerm to CI matrix for Linux, macOS.

**Verification (automated):**

- [x] Unit tests for WezTerm availability detection
- [x] Integration tests for pane/tab creation on all supported platforms
- [x] Cross-platform test ensuring macOS and Linux implementations behave consistently
- [x] Cleanup test verifying no stray WezTerm processes remain
- [ ] Performance test for layout creation

**Implementation Details:**

_(To be filled after implementation)_

**Key Source Files:**

- `specs/Public/Terminal-Multiplexers/WezTerm.md` - WezTerm integration specification
- `crates/ah-mux/src/wezterm.rs` - WezTerm multiplexer implementation
- `crates/ah-mux/tests/integration_tests.rs` - integration tests (to be added)

**Outstanding Tasks:**

- Spec updates for latest WezTerm features
- Cross-platform implementation review
- Windows-specific testing
- Integration test suite

---

## M4. Zellij (Linux, macOS, BSD)

**Status:** Mostly complete — spec and implementation updated for KDL layouts, `zellij run`‑based pane creation, and session discovery; tests and cleanup logic still missing.

**Context:**

- Zellij uses KDL layout files for defining complex layouts and a CLI oriented around sessions and actions (`zellij run`, `zellij action write-chars`, `list-sessions`, `attach`) rather than low-level split commands.
- The current implementation (`zellij.rs`) now leverages KDL layouts when a `cwd` is provided, uses `zellij run` with `--direction` / `--cwd` for splits and per-pane commands, and maps `ZELLIJ_SESSION_NAME` / `ZELLIJ_PANE_ID` for session and pane discovery.
- Zellij's CLI still lacks stable pane identifiers and direct pane focusing/listing; these operations intentionally return `MuxError::NotAvailable("zellij")` in the implementation.
- Integration tests are currently skipped for Zellij; only lightweight unit tests cover helpers and basic behavior.

**Deliverables:**

- [x] Review and update `specs/Public/Terminal-Multiplexers/Zellij.md`:
  - Expanded documentation on KDL layout file generation and task layouts.
  - Documented workarounds for missing direct split commands using `zellij run`, `--direction`, and layout-based splits.
  - Added examples of Agent Harbor task layouts in KDL format plus an end-to-end `ah tui --follow <TASK_ID>` flow.
  - Documented known limitations (no stable pane IDs, limited programmatic focus) and added troubleshooting / compatibility notes.
- [x] Enhance `crates/ah-mux/src/zellij.rs` to align with the spec:
  - Implement minimal KDL layout generation for new sessions when `cwd` is provided, falling back to the user's default layout otherwise.
  - Use `zellij run` (with `--direction` and `--cwd`) for pane creation and per-pane command execution, including support for full shell command lines via `sh -lc`.
  - Implement session discovery/attachment via `zellij list-sessions` and `zellij attach`, with environment-based discovery for the current pane/window via `ZELLIJ_PANE_ID` / `ZELLIJ_SESSION_NAME`.
  - Improve error handling by mapping missing binaries to `MuxError::NotAvailable("zellij")` and process failures to `MuxError::Io` / `MuxError::CommandFailed`.
- [ ] Add explicit session lifecycle helpers in `crates/ah-mux/src/zellij.rs` for cleanup (`kill-session` / `delete-session`) suitable for automated tests and task teardown.
- [ ] Add automated tests:
  - Unit tests for KDL layout generation (beyond string escaping) and session / pane helpers.
  - Integration test creating sessions with generated layouts
  - Test for `zellij run` command execution
  - Test for session attachment and focusing
  - Test cleanup and session teardown
- [ ] Enable Zellij tests in the integration test suite (remove skip conditions).
- [ ] Add Zellij to the TUI scenario test suite with appropriate expectations.

**Verification (automated):**

- [ ] Unit tests for KDL layout file generation (verify syntax and semantics)
- [ ] Integration tests for session creation with layouts
- [ ] Test verifying `zellij list-sessions` output parsing
- [ ] Test for command execution via `zellij run`
- [ ] Cleanup test ensuring no stray Zellij sessions remain
- [ ] Explicitly document which `Multiplexer` trait methods have limited support

**Implementation Details:**

_(To be filled after implementation)_

**Key Source Files:**

- `specs/Public/Terminal-Multiplexers/Zellij.md` - Zellij integration specification
- `crates/ah-mux/src/zellij.rs` - Zellij multiplexer implementation (280+ lines)
- `crates/ah-mux/tests/integration_tests.rs` - integration tests (currently skipped)

**Outstanding Tasks:**

- Implement explicit Zellij session cleanup helpers and wire them into automated tests.
- Enable and stabilize Zellij integration tests (including layout-based sessions and `zellij run` flows).
- Extend unit tests to cover KDL layout generation and session parsing.
- Add Zellij to the TUI scenario test suite with appropriate expectations.

---

## M5. GNU Screen (Linux, macOS, BSD)

**Status:** Complete — implementation, spec, and comprehensive test suite all complete with 40/40 tests passing.

**Context:**

- GNU Screen is a mature multiplexer with wide platform support, using the `-X` command interface for automation.
- The spec file (`GNU-Screen.md`) has been updated to reflect the actual implementation patterns (layout management, focus behavior, command execution).
- The implementation (`screen.rs`) is complete with ~1800 lines total (including comprehensive test suite), with full tracing/logging and error handling.
- Screen uses layouts and regions rather than direct pane management, with specific patterns for Agent Harbor task layouts.
- Several features have known limitations: `focus_pane()` and `list_panes()` return `NotAvailable` due to CLI restrictions, which is properly documented and tested.

**Deliverables:**

- [x] Review and update `specs/Public/Terminal-Multiplexers/GNU-Screen.md`:
  - Documented layout-based approach (agent-harbor base layout, per-task layouts)
  - Added examples for split pane creation (horizontal and vertical)
  - Documented command execution patterns with proper escaping (`bash -lc` wrapper)
  - Documented environment variables (`STY`, `WINDOW`) and their usage
  - Added notes on implementation patterns and limitations
- [x] Review and enhance `crates/ah-mux/src/screen.rs`:
  - All feasible `Multiplexer` trait methods implemented
  - Layout-based window management using `layout new <name>`
  - Region-based pane splitting with proper focus handling (right for vertical, down for horizontal)
  - Command execution via `stuff` with comprehensive escaping logic
  - Comprehensive structured logging with tracing instrumentation
  - Proper error handling with detailed error messages
  - Environment-based session/window detection using `STY` and `WINDOW`
- [x] Add comprehensive test suite (40 tests total, all passing):
  - **Unit Tests (26 tests)**:
    - Window output parsing (empty, single, multiple, filtered, malformed, special flags, many windows, numbers in titles)
    - Environment variable resolution (current window/pane with various states)
    - Command escaping (single quotes, dollar signs, backslashes, mixed special chars)
    - Error handling for unsupported methods
  - **Integration Tests (14 tests)**:
    - Window creation (with/without title, special chars, idempotent, multiple windows, reuse existing)
    - Pane splitting (horizontal/vertical, with cwd, with initial commands, special chars in cwd)
    - Text sending and command execution
    - Invalid session error handling
    - Complete lifecycle testing
- [x] Test infrastructure:
  - Helper functions: `start_screen_session()`, `kill_screen_session()`, `init_tracing()`
  - Proper session cleanup after each test
  - Serial test execution to prevent environment variable conflicts

**Verification (automated):**

- [x] Unit tests for Screen availability detection (`screen --version`)
- [x] Unit tests for `screen -Q windows` output parsing (regex-based window title extraction)
- [x] Unit tests for environment variable resolution (`STY`, `WINDOW`)
- [x] Unit tests verifying unsupported operations return `NotAvailable` errors
- [x] Unit tests for command escaping (quotes, dollar signs, backslashes, mixed)
- [x] Integration tests for window creation (with/without title, special chars, idempotent, multiple windows)
- [x] Integration tests for pane splitting (horizontal/vertical, with cwd and initial commands)
- [x] Integration tests for command execution and text sending
- [x] Integration tests for error handling (invalid sessions, missing environment variables)
- [x] Complete lifecycle test with session creation, window/pane management, and cleanup
- [x] All 40 tests passing (verified with `cargo test -p ah-mux screen::tests`)

**Implementation Details:**

The GNU Screen implementation is **complete and production-ready**, with comprehensive test coverage validating all functionality against real Screen sessions. All 40 automated tests pass consistently, covering both unit-level functionality and full integration scenarios.

Core functionality implemented:

- ✅ `new()` - Availability check via `screen --version`
- ✅ `is_available()` - Version check with proper error handling
- ✅ `open_window()` - Layout-based window creation with one-time agent-harbor layout initialization
- ✅ `split_pane()` - Region splitting with direction-aware focus (right/down) and command execution via `stuff`
- ✅ `run_command()` - Command execution via `screen -X stuff` with newline termination
- ✅ `send_text()` - Text injection via `stuff` command
- ✅ `list_windows()` - Session listing via `screen -ls` with regex parsing
- ✅ `current_window()` - Window detection via `WINDOW` environment variable
- ✅ `current_pane()` - Pane detection via `STY` environment variable with formatted ID
- ✅ `focus_window()` - Session reattachment via `screen -r`

Known limitations (properly handled):

- ❌ `focus_pane()` - Returns `NotAvailable` (no CLI-based region focusing)
- ❌ `list_panes()` - Returns `NotAvailable` (no programmatic region enumeration)

Implementation highlights:

- **Layout management**: Uses `layout new agent-harbor` for base layout, `layout new <task-name>` for task windows
- **Command escaping**: Comprehensive escaping for `stuff` command (backslashes → `\\\\`, dollar signs → `\\$`, single quotes → `'\\''`)
- **Focus handling**: Correct direction-based focus after splits (vertical → right, horizontal → down)
- **Environment detection**: Uses `STY` for session name, `WINDOW` for window number
- **Structured logging**: Full tracing instrumentation on all methods with operation/component fields
- **Error handling**: Detailed error messages with context (session name, pane ID, command details)

Testing coverage:

- ✅ 40 comprehensive tests (26 unit + 14 integration) all passing
- ✅ Unit tests cover parsing, resolution, escaping, and error handling
- ✅ Integration tests verify real Screen session interaction
- ✅ Tests use `#[serial]` attribute to prevent concurrent environment variable conflicts
- ✅ Test infrastructure with proper session setup/teardown (`start_screen_session`, `kill_screen_session`)
- ✅ Verified passing with `cargo test -p ah-mux screen::tests` (40 passed, 0 failed)
- ✅ No linter errors in implementation
- ✅ Pre-commit hooks pass (rustfmt, clippy, SPDX headers)

**Key Source Files:**

- `specs/Public/Terminal-Multiplexers/GNU-Screen.md` - Screen integration specification (comprehensive)
- `crates/ah-mux/src/screen.rs` - Screen multiplexer implementation (~1800 lines including 40 tests)
- `crates/ah-mux-core/src/lib.rs` - Multiplexer trait definition
- Test helpers: `start_screen_session()`, `kill_screen_session()`, `init_tracing()`

**Outstanding Tasks:**

- [ ] Test Screen with all the different options provided by the tui

---

## M6. Tilix (Linux only)

**Status:** Complete — implementation exists with comprehensive functionality, spec is thorough, limitations documented, and testing completed.

**Context:**

- Tilix is a tiling terminal emulator for Linux using GTK+.
- The spec file (`Tilix.md`) is comprehensive and documents CLI-based integration approach.
- The implementation (`tilix.rs`) is complete with ~380 lines, Linux-only (correctly gated with `cfg(target_os = "linux")`).
- Integrated into task manager and available in default feature set.
- Several advanced features return `NotAvailable` errors due to Tilix CLI limitations (documented).
- Comprehensive testing implemented: 18 unit tests + 3 integration tests with proper Linux-only gating.

**Deliverables:**

- [x] Review and update `specs/Public/Terminal-Multiplexers/Tilix.md`:
  - CLI action-based integration documented (no D-Bus requirement)
  - Examples for creating layouts via `--action session-add-right/down`
  - Version compatibility notes included
  - Limitations clearly documented
- [x] Review and enhance `crates/ah-mux/src/tilix.rs`:
  - Core functionality implemented (`open_window`, `split_pane`)
  - All `Multiplexer` trait methods implemented (unsupported ones return `NotAvailable`)
  - Proper error handling with detailed explanations
  - Availability detection via `tilix --version`
  - PATH environment propagation for command execution
- [x] Integration into task manager (`ah-core/src/task_manager_init.rs`)
- [x] Add automated tests:
  - Unit tests for Tilix detection and availability
  - Integration tests for window/pane creation (Linux CI only)
  - Test for command execution via `--command` parameter
  - Test cleanup and session management
- [x] Ensure Tilix is only tested on Linux CI runners.
- [x] Add Tilix to the integration test suite with Linux-only gating.

**Verification (automated):**

- [x] Unit tests for Tilix availability detection (`tilix --version`)
- [x] Integration tests run successfully on Linux CI with Tilix installed
- [x] Test verifying window creation with custom titles and commands
- [x] Test verifying pane splitting with different directions
- [x] Cleanup test ensuring no stray Tilix instances remain
- [x] Verify tests are correctly skipped on non-Linux platforms
- [x] Document which `Multiplexer` methods have limited support (return `NotAvailable`)

**Implementation Details:**

Core functionality implemented:

- ✅ `open_window()` - Creates new Tilix session with `--action app-new-session`
- ✅ `split_pane()` - Splits panes using `session-add-right`/`session-add-down` actions
- ✅ Platform detection and availability checking
- ✅ Linux-only compilation with proper feature gating
- ✅ Integration with task manager and CLI selection

Known limitations (properly handled):

- ❌ `run_command()` - Returns `NotAvailable` (commands must be specified at creation)
- ❌ `send_text()` - Returns `NotAvailable` (no native send-keys capability)
- ❌ `focus_window()`/`focus_pane()` - Returns `NotAvailable` (no CLI-based focusing)
- ❌ `list_windows()`/`list_panes()` - Returns `NotAvailable` (no programmatic enumeration)

Testing coverage:

- ✅ 18 comprehensive unit tests covering all functionality areas
- ✅ 3 integration tests with real Tilix binary interaction
- ✅ Platform-specific testing (Linux-only with proper gating)
- ✅ Error handling tests for all unsupported operations
- ✅ Command processing and options validation tests

**Key Source Files:**

- `specs/Public/Terminal-Multiplexers/Tilix.md` - Tilix integration specification (comprehensive)
- `crates/ah-mux/src/tilix.rs` - Tilix multiplexer implementation (~770 lines including tests, Linux-only)
- `crates/ah-core/src/task_manager_init.rs` - Task manager integration (complete)
- `crates/ah-mux/tests/integration_tests.rs` - Integration tests with Tilix-specific tests

**Implementation Complete:**

All deliverables and verification requirements have been completed. Tilix implementation provides the maximum functionality possible given the CLI limitations of the terminal emulator, with proper error handling and comprehensive test coverage.

- Add border characteristics to integration test framework
- Document testing approach for multiplexers with limited CLI capabilities

---

## M7. Windows Terminal (Windows only)

**Status:** Partially complete — implementation exists, spec is good, tests needed.

**Context:**

- Windows Terminal is the modern terminal for Windows 10/11 with tab and pane support.
- The spec file (`Windows-Terminal.md`) documents integration approach.
- The implementation (`windows_terminal.rs`) is Windows-only.
- Testing requires Windows CI runners or WSL environment.

**Deliverables:**

- [ ] Review and update `specs/Public/Terminal-Multiplexers/Windows-Terminal.md`:
  - Document Windows Terminal command-line arguments
  - Add examples for creating pane layouts using `wt.exe`
  - Document WSL integration considerations
  - Add version compatibility notes (Windows Terminal 1.x)
  - Add troubleshooting section
- [ ] Review and enhance `crates/ah-mux/src/windows_terminal.rs`:
  - Ensure Windows-specific APIs are used correctly
  - Implement all feasible `Multiplexer` trait methods
  - Add proper error handling for Windows-specific failures
  - Implement cleanup logic for test sessions
  - Add detection for Windows Terminal availability
- [ ] Add automated tests:
  - Unit tests for Windows Terminal detection
  - Integration tests for pane/tab creation (Windows CI only)
  - Test for command execution in panes
  - Test cleanup and session teardown
- [ ] Add Windows Terminal to CI matrix with Windows runners.
- [ ] Ensure tests are correctly skipped on non-Windows platforms.

**Verification (automated):**

- [ ] Unit tests for Windows Terminal detection
- [ ] Integration tests run successfully on Windows CI
- [ ] Test verifying `wt.exe` command-line argument handling
- [ ] Cleanup test ensuring no stray Windows Terminal instances remain
- [ ] Verify tests are correctly skipped on non-Windows platforms

**Implementation Details:**

_(To be filled after implementation)_

**Key Source Files:**

- `specs/Public/Terminal-Multiplexers/Windows-Terminal.md` - Windows Terminal integration specification
- `crates/ah-mux/src/windows_terminal.rs` - Windows Terminal multiplexer implementation
- `crates/ah-mux/tests/integration_tests.rs` - integration tests (Windows-gated)

**Outstanding Tasks:**

- Windows-specific API usage review
- WSL integration testing
- Windows CI pipeline setup
- Error handling improvements

---

## M8. Ghostty (Linux, macOS)

**Status:** Partially complete — implementation exists, spec is good, tests needed.

**Context:**

- Ghostty is a relatively new, fast terminal emulator with multiplexing support.
- The spec file (`Ghostty.md`) documents integration approach.
- The implementation (`ghostty.rs`) exists but is less mature than tmux/kitty.
- Ghostty features and APIs may evolve rapidly, requiring periodic updates.

**Deliverables:**

- [ ] Review and update `specs/Public/Terminal-Multiplexers/Ghostty.md`:
  - Document Ghostty CLI commands for window/pane management
  - Add examples for creating Agent Harbor task layouts
  - Document version compatibility (Ghostty is in active development)
  - Add troubleshooting section
  - Note any experimental or unstable features
- [ ] Review and enhance `crates/ah-mux/src/ghostty.rs`:
  - Ensure compatibility with current Ghostty releases
  - Implement all feasible `Multiplexer` trait methods
  - Add proper error handling
  - Implement cleanup logic for test sessions
  - Add version detection and feature checking
- [ ] Add automated tests:
  - Unit tests for Ghostty detection and version parsing
  - Integration tests for window/pane creation
  - Test for command execution
  - Test cleanup and session teardown
- [ ] Add Ghostty to CI with version pinning (due to active development).
- [ ] Document Ghostty installation in Nix flake.

**Verification (automated):**

- [ ] Unit tests for Ghostty availability detection
- [ ] Integration tests for window/pane creation on Linux and macOS
- [ ] Version compatibility test ensuring minimum required version
- [ ] Cleanup test ensuring no stray Ghostty instances remain
- [ ] Test suite handles missing Ghostty gracefully (informative skip messages)

**Implementation Details:**

_(To be filled after implementation)_

**Key Source Files:**

- `specs/Public/Terminal-Multiplexers/Ghostty.md` - Ghostty integration specification
- `crates/ah-mux/src/ghostty.rs` - Ghostty multiplexer implementation
- `crates/ah-mux/tests/integration_tests.rs` - integration tests (to be added)

**Outstanding Tasks:**

- Version compatibility tracking
- Feature detection for evolving APIs
- Integration test suite
- Nix packaging

---

## M9. Neovim (cross-platform)

**Status:** Partially complete — implementation exists, spec is comprehensive, tests needed.

**Context:**

- Neovim supports terminal buffers and can act as a multiplexer within the editor.
- The spec file (`Vim.md`) covers both Vim and Neovim (may need split into separate files).
- The implementation (`neovim.rs`) uses Neovim's RPC interface for control.
- Testing requires Neovim to be running and accessible via RPC.

**Deliverables:**

- [ ] Create dedicated `specs/Public/Terminal-Multiplexers/Neovim.md` (split from `Vim.md`):
  - Document Neovim terminal buffer management
  - Add examples for creating split layouts with `:terminal` command
  - Document RPC interface usage for programmatic control
  - Add version compatibility notes (Neovim 0.5+)
  - Add troubleshooting section
- [ ] Review and enhance `crates/ah-mux/src/neovim.rs`:
  - Ensure RPC interface is robust
  - Implement all feasible `Multiplexer` trait methods
  - Add proper error handling for RPC failures
  - Implement cleanup logic for test buffers
  - Add detection for Neovim availability and RPC socket
- [ ] Add automated tests:
  - Unit tests for Neovim detection and RPC connection
  - Integration tests for terminal buffer creation
  - Test for command execution in terminal buffers
  - Test for buffer focusing
  - Test cleanup and session teardown
- [ ] Add Neovim to the integration test suite.
- [ ] Document Neovim setup requirements in developer guide.

**Verification (automated):**

- [ ] Unit tests for Neovim availability detection and RPC socket discovery
- [ ] Integration tests for terminal buffer creation and management
- [ ] Test verifying RPC command execution
- [ ] Cleanup test ensuring no stray Neovim instances remain
- [ ] Cross-platform test on Linux, macOS, and Windows

**Implementation Details:**

_(To be filled after implementation)_

**Key Source Files:**

- `specs/Public/Terminal-Multiplexers/Neovim.md` - Neovim integration specification (to be created)
- `crates/ah-mux/src/neovim.rs` - Neovim multiplexer implementation
- `crates/ah-mux/tests/integration_tests.rs` - integration tests (to be added)

**Outstanding Tasks:**

- Create separate Neovim spec file
- RPC interface robustness
- Integration test suite
- Documentation updates

---

## M10. Vim (cross-platform)

**Status:** Partially complete — implementation exists, spec needs update, tests needed.

**Context:**

- Vim supports terminal buffers (`:terminal` in Vim 8.1+) but with fewer features than Neovim.
- The spec file (`Vim.md`) currently covers both Vim and Neovim (needs split).
- The implementation (`vim.rs`) may have limited capabilities compared to Neovim.
- Testing requires careful version detection (terminal support only in Vim 8.1+).

**Deliverables:**

- [ ] Update `specs/Public/Terminal-Multiplexers/Vim.md` (focus on Vim-specific features):
  - Document Vim terminal buffer management (`:terminal` command)
  - Add examples for creating split layouts
  - Document limitations compared to Neovim
  - Add version compatibility notes (Vim 8.1+ required for terminal support)
  - Add troubleshooting section
- [ ] Review and enhance `crates/ah-mux/src/vim.rs`:
  - Ensure compatibility with Vim 8.1+
  - Implement all feasible `Multiplexer` trait methods
  - Add proper error handling
  - Implement cleanup logic for test buffers
  - Add version detection (require Vim 8.1+)
- [ ] Add automated tests:
  - Unit tests for Vim detection and version checking
  - Integration tests for terminal buffer creation (Vim 8.1+ only)
  - Test for command execution in terminal buffers
  - Test cleanup and session teardown
- [ ] Add Vim to the integration test suite with version gating.
- [ ] Document Vim version requirements in developer guide.

**Verification (automated):**

- [ ] Unit tests for Vim availability detection and version parsing
- [ ] Integration tests for terminal buffer creation (skipped if Vim < 8.1)
- [ ] Test verifying terminal command execution
- [ ] Cleanup test ensuring no stray Vim instances remain
- [ ] Cross-platform test on Linux, macOS, and Windows

**Implementation Details:**

_(To be filled after implementation)_

**Key Source Files:**

- `specs/Public/Terminal-Multiplexers/Vim.md` - Vim integration specification
- `crates/ah-mux/src/vim.rs` - Vim multiplexer implementation
- `crates/ah-mux/tests/integration_tests.rs` - integration tests (to be added)

**Outstanding Tasks:**

- Spec split from Neovim
- Version detection and gating
- Integration test suite
- Documentation of limitations

---

## M11. Emacs (cross-platform)

**Status:** Partially complete — implementation exists, spec is comprehensive, tests needed.

**Context:**

- Emacs supports terminal emulation through `term`, `ansi-term`, and `vterm` modes.
- The spec file (`Emacs.md`) documents integration approach.
- The implementation (`emacs.rs`) uses Emacs server/client architecture for control.
- Testing requires Emacs server to be running and accessible.

**Deliverables:**

- [ ] Review and update `specs/Public/Terminal-Multiplexers/Emacs.md`:
  - Document Emacs terminal modes (term, ansi-term, vterm)
  - Add examples for creating split layouts with terminal buffers
  - Document Emacs server/client setup
  - Add version compatibility notes
  - Add troubleshooting section (server not running, connection refused)
- [ ] Review and enhance `crates/ah-mux/src/emacs.rs`:
  - Ensure Emacs server communication is robust
  - Implement all feasible `Multiplexer` trait methods
  - Add proper error handling for server communication failures
  - Implement cleanup logic for test buffers
  - Add detection for Emacs availability and server status
- [ ] Add automated tests:
  - Unit tests for Emacs detection and server connection
  - Integration tests for terminal buffer creation
  - Test for command execution in terminal buffers
  - Test for window/buffer focusing
  - Test cleanup and session teardown
- [ ] Add Emacs to the integration test suite.
- [ ] Document Emacs server setup requirements in developer guide.

**Verification (automated):**

- [ ] Unit tests for Emacs availability detection and server status check
- [ ] Integration tests for terminal buffer creation and management
- [ ] Test verifying Emacs Lisp command execution via server
- [ ] Cleanup test ensuring no stray Emacs instances remain
- [ ] Cross-platform test on Linux, macOS, and Windows

**Implementation Details:**

_(To be filled after implementation)_

**Key Source Files:**

- `specs/Public/Terminal-Multiplexers/Emacs.md` - Emacs integration specification
- `crates/ah-mux/src/emacs.rs` - Emacs multiplexer implementation
- `crates/ah-mux/tests/integration_tests.rs` - integration tests (to be added)

**Outstanding Tasks:**

- Server communication robustness
- vterm mode support verification
- Integration test suite
- Documentation updates

---

## M12. iTerm2 (macOS only)

**Status:** Complete — implementation exists with comprehensive functionality, spec is thorough, and comprehensive testing completed with proper assertions.

**Context:**

- iTerm2 is the most popular terminal for macOS with rich AppleScript/Python API support.
- The spec file (`iTerm2.md`) documents the integration approach via AppleScript.
- The implementation (`iterm2.rs`) is complete with ~1100 lines, macOS-only, using AppleScript for control.
- Integrated into task manager and available in default feature set.
- Comprehensive testing implemented: 18 tests (3 unit + 15 integration) with proper macOS-only gating.

**Deliverables:**

- [x] Review and update `specs/Public/Terminal-Multiplexers/iTerm2.md`:
  - AppleScript-based integration documented
  - Examples for creating layouts via AppleScript commands
  - Version compatibility notes included (tested with iTerm2 3.x)
  - Limitations clearly documented
- [x] Review and enhance `crates/ah-mux/src/iterm2.rs`:
  - Core functionality implemented via AppleScript (`open_window`, `split_pane`, `run_command`, `send_text`)
  - All `Multiplexer` trait methods implemented
  - Proper error handling with detailed AppleScript failure messages
  - Availability detection via osascript and iTerm2 version check
  - Structured logging with tracing instrumentation
- [x] Add automated tests:
  - Unit tests for ID generation and availability detection
  - Integration tests for window creation with title and cwd
  - Integration tests for window focusing
  - Integration tests for pane splitting (horizontal/vertical with initial commands)
  - Integration tests for command execution and text sending
  - Integration tests for window/pane focusing
  - Integration tests for window filtering and listing
  - Integration tests for current window detection
  - Integration tests for error handling (invalid panes)
  - Integration tests for complex layout creation (3-pane layouts)
  - Integration tests for pane listing
  - Integration tests for AppleScript execution
  - Integration tests for window counting
- [x] Refactored tests to follow kitty.rs patterns with proper assertions:
  - Replaced defensive nested match statements with `.unwrap()` for fail-fast behavior
  - Added descriptive assertions (e.g., `assert_ne!(pane_id, window_id, "message")`)
  - Tests now fail immediately with clear error messages
- [x] Tests are correctly gated for macOS-only platforms.

**Verification (automated):**

- [x] Unit tests for iTerm2 availability detection (`test_id`, `test_next_window_id`, `test_next_pane_id`)
- [x] Integration tests for multiplexer creation (`test_iterm2_multiplexer_creation`, `test_iterm2_availability`)
- [x] Integration tests for window operations (`test_open_window_with_title_and_cwd`, `test_open_window_focus`)
- [x] Integration tests for pane operations (`test_split_pane`, `test_split_pane_with_initial_command`)
- [x] Integration tests for command execution (`test_run_command_and_send_text`)
- [x] Integration tests for focus operations (`test_focus_window_and_pane`)
- [x] Integration tests for listing operations (`test_list_windows_filtering`, `test_list_panes`, `test_current_window`)
- [x] Integration tests for error handling (`test_error_handling_invalid_pane`)
- [x] Integration tests for complex layouts (`test_complex_layout_creation`)
- [x] Integration tests for AppleScript basics (`test_run_applescript_basic`, `test_get_window_count`)
- [x] All 18 tests passing (verified with `cargo test --package ah-mux --lib -- iterm2::tests`)
- [x] Tests skip correctly on non-macOS platforms with proper logging
- [x] Clippy passes with `-D warnings` (no warnings in implementation)
- [x] Proper test isolation using `#[serial_test::file_serial]` attribute

**Implementation Details:**

The iTerm2 implementation is **complete and production-ready**, with comprehensive test coverage validating all functionality against real iTerm2 sessions via AppleScript. All 18 automated tests pass consistently, covering both unit-level functionality and full integration scenarios.

Core functionality implemented:

- ✅ `new()` - Availability check via osascript and iTerm2 version
- ✅ `is_available()` - Checks for osascript and iTerm2 installation
- ✅ `open_window()` - Window creation with title, cwd, and focus support via AppleScript
- ✅ `split_pane()` - Pane splitting (horizontal/vertical) with initial commands and cwd support
- ✅ `run_command()` - Command execution via AppleScript `write text`
- ✅ `send_text()` - Text injection via AppleScript `write text`
- ✅ `focus_window()` - Window focusing via AppleScript `activate`
- ✅ `focus_pane()` - Session focusing via AppleScript `select`
- ✅ `current_window()` - Current window detection via AppleScript
- ✅ `list_windows()` - Window enumeration with title filtering
- ✅ `list_panes()` - Pane listing (simplified implementation)

Implementation highlights:

- **AppleScript integration**: All operations via osascript command execution
- **ID generation**: Atomic counters for unique window/pane IDs
- **Error handling**: Detailed error messages with stderr capture from AppleScript failures
- **Structured logging**: Full tracing instrumentation with debug/info/error levels
- **Test assertions**: Proper fail-fast behavior with descriptive error messages
- **Platform gating**: macOS-only with `cfg(target_os = "macos")` checks

Testing coverage:

- ✅ 18 comprehensive tests (3 unit + 15 integration) all passing
- ✅ Unit tests cover ID generation and basic availability
- ✅ Integration tests verify real iTerm2 interaction via AppleScript
- ✅ Tests use `#[serial_test::file_serial]` attribute for proper isolation
- ✅ Test infrastructure with `start_test_iterm2()` helper for session management
- ✅ Verified passing with `cargo test --package ah-mux --lib -- iterm2::tests`
- ✅ No clippy warnings with `-D warnings` flag
- ✅ Tests properly skip on non-macOS platforms

**Key Source Files:**

- `specs/Public/Terminal-Multiplexers/iTerm2.md` - iTerm2 integration specification (comprehensive)
- `crates/ah-mux/src/iterm2.rs` - iTerm2 multiplexer implementation (~1100 lines including 18 tests)
- `crates/ah-mux-core/src/lib.rs` - Multiplexer trait definition
- Test helpers: `start_test_iterm2()`, `stop_test_iterm2()`

**Outstanding Tasks:**

- [ ] Add iTerm2 to CI matrix with macOS runners (tests currently skip in CI)

---

## References

- [First.Release.status.md](../First.Release.status.md) - R1: Multiplexer enablement and automated regression coverage
- [TUI-Multiplexers-Overview.md](TUI-Multiplexers-Overview.md) - High-level overview of terminal multiplexer integration
- [AGENTS.md](../../AGENTS.md) - Methodology and structure for status files
- [MVP.status.md](../MVP.status.md) - Reference implementation status tracking structure
- [Component-Architecture.md](../../../docs/Component-Architecture.md) - Philosophy on manual testing utilities
