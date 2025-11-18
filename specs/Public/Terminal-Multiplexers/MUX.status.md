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
- [ ] **M2**: Kitty (Linux, macOS)
- [ ] **M3**: WezTerm (Linux, macOS, Windows)
- [ ] **M4**: Zellij (Linux, macOS, BSD)
- [ ] **M5**: GNU Screen (Linux, macOS, BSD)
- [ ] **M6**: Tilix (Linux only)
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

- [ ] Review and update `specs/Public/Terminal-Multiplexers/Kitty.md`:
  - Add section on socket setup and `KITTY_LISTEN_ON` environment variable
  - Document limitations compared to tmux (e.g., less mature window management)
  - Add examples of layout creation using `--location=hsplit|vsplit`
  - Add troubleshooting section (socket not found, permission denied)
- [ ] Review and enhance `crates/ah-mux/src/kitty.rs`:
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
- [ ] Document Kitty-specific configuration in developer guide.

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

- [ ] Review and update `specs/Public/Terminal-Multiplexers/WezTerm.md`:
  - Document WezTerm CLI commands for pane/tab management
  - Add examples for creating Agent Harbor task layouts
  - Document Windows-specific considerations
  - Add version compatibility notes (tested with WezTerm 20240203+)
  - Add troubleshooting section
- [ ] Review and enhance `crates/ah-mux/src/wezterm.rs`:
  - Ensure cross-platform compatibility (Linux, macOS, Windows)
  - Implement all `Multiplexer` trait methods
  - Add WezTerm-specific error handling
  - Add support for WezTerm configuration file generation if needed
  - Implement proper cleanup logic
- [ ] Add comprehensive automated tests:
  - Unit tests for WezTerm detection and version parsing
  - Integration tests for window/pane creation
  - Test for command execution in panes
  - Test for focus switching
  - Platform-specific tests (Windows behavior differs from Unix)
- [ ] Add WezTerm to CI matrix for Linux, macOS, and Windows.
- [ ] Document WezTerm setup in Nix flake and CI configuration.

**Verification (automated):**

- [ ] Unit tests for WezTerm availability detection
- [ ] Integration tests for pane/tab creation on all supported platforms
- [ ] Cross-platform test ensuring Windows and Unix implementations behave consistently
- [ ] Cleanup test verifying no stray WezTerm processes remain
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

**Status:** Partially complete — implementation exists but limited pane splitting support, spec is good.

**Context:**

- Zellij uses KDL layout files for defining complex layouts, not direct CLI commands for splitting.
- The current implementation (`zellij.rs`) has limitations (noted in integration tests as "limited pane splitting support").
- The spec file (`Zellij.md`) documents the layout file approach but implementation doesn't fully leverage this.
- Integration tests are currently skipped for Zellij.

**Deliverables:**

- [ ] Review and update `specs/Public/Terminal-Multiplexers/Zellij.md`:
  - Expand documentation on KDL layout file generation
  - Document workarounds for missing direct CLI commands
  - Add examples of Agent Harbor task layouts in KDL format
  - Document known limitations (e.g., no stable send-keys API)
  - Add troubleshooting section
- [ ] Enhance `crates/ah-mux/src/zellij.rs`:
  - Implement dynamic KDL layout generation for task layouts
  - Use `zellij run` for command execution where direct splits aren't available
  - Implement session management (`zellij list-sessions`, `attach`, `kill-session`)
  - Add proper error handling for Zellij-specific failure modes
  - Implement cleanup logic for test sessions
- [ ] Add automated tests:
  - Unit tests for KDL layout generation
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

- KDL layout generation
- Pane splitting workarounds
- Enable integration tests
- Document limitations

---

## M5. GNU Screen (Linux, macOS, BSD)

**Status:** Partially complete — implementation exists, spec is comprehensive, tests needed.

**Context:**

- GNU Screen is a mature multiplexer with wide platform support but less feature-rich than tmux.
- The spec file (`GNU-Screen.md`) documents integration approach.
- The implementation (`screen.rs`) exists but integration tests are limited.
- Screen has some limitations compared to tmux (e.g., less sophisticated pane management).

**Deliverables:**

- [ ] Review and update `specs/Public/Terminal-Multiplexers/GNU-Screen.md`:
  - Document version compatibility (tested with Screen 4.x)
  - Add examples for creating Agent Harbor task layouts
  - Document limitations compared to tmux (e.g., no built-in pane splitting)
  - Add workarounds using split commands or region management
  - Add troubleshooting section
- [ ] Review and enhance `crates/ah-mux/src/screen.rs`:
  - Ensure all feasible `Multiplexer` trait methods are implemented
  - Implement region-based layout creation if pane splitting is needed
  - Add proper error handling for Screen-specific issues
  - Implement cleanup logic for test sessions
  - Document which features are not supported (return appropriate errors)
- [ ] Add automated tests:
  - Unit tests for Screen detection and version parsing
  - Integration tests for window creation
  - Test for command execution in windows
  - Test for session attachment and focusing
  - Test cleanup and session teardown
- [ ] Add Screen to the integration test suite with appropriate skip conditions for unsupported features.

**Verification (automated):**

- [ ] Unit tests for Screen availability detection
- [ ] Integration tests for window creation and command execution
- [ ] Test verifying `screen -ls` output parsing
- [ ] Cleanup test ensuring no stray Screen sessions remain
- [ ] Documented limitations test (verify unsupported operations return appropriate errors)

**Implementation Details:**

_(To be filled after implementation)_

**Key Source Files:**

- `specs/Public/Terminal-Multiplexers/GNU-Screen.md` - Screen integration specification
- `crates/ah-mux/src/screen.rs` - Screen multiplexer implementation
- `crates/ah-mux/tests/integration_tests.rs` - integration tests (to be added)

**Outstanding Tasks:**

- Spec updates for limitations
- Region-based layout workarounds
- Integration test suite
- Documentation of unsupported features

---

## M6. Tilix (Linux only)

**Status:** Partially complete — implementation exists, spec is good, tests needed.

**Context:**

- Tilix is a tiling terminal emulator for Linux using GTK+.
- The spec file (`Tilix.md`) documents integration approach.
- The implementation (`tilix.rs`) is Linux-only (correctly gated with `cfg(target_os = "linux")`).
- Integration tests need to handle Tilix-specific quirks (D-Bus interface, GTK+ dependencies).

**Deliverables:**

- [ ] Review and update `specs/Public/Terminal-Multiplexers/Tilix.md`:
  - Document D-Bus interface usage
  - Add examples for creating layouts via D-Bus commands
  - Document GTK+ dependency requirements
  - Add version compatibility notes
  - Add troubleshooting section (D-Bus service not found, GTK+ issues)
- [ ] Review and enhance `crates/ah-mux/src/tilix.rs`:
  - Ensure D-Bus communication is robust
  - Implement all feasible `Multiplexer` trait methods
  - Add proper error handling for D-Bus failures
  - Implement cleanup logic for test sessions
  - Add detection for Tilix availability and D-Bus service
- [ ] Add automated tests:
  - Unit tests for D-Bus interface detection
  - Integration tests for terminal creation via D-Bus (Linux CI only)
  - Test for command execution in terminals
  - Test cleanup and session teardown
- [ ] Ensure Tilix is only tested on Linux CI runners.
- [ ] Add Tilix to the integration test suite with Linux-only gating.

**Verification (automated):**

- [ ] Unit tests for Tilix detection (D-Bus service availability)
- [ ] Integration tests run successfully on Linux CI with Tilix installed
- [ ] Test verifying D-Bus command execution
- [ ] Cleanup test ensuring no stray Tilix instances remain
- [ ] Verify tests are correctly skipped on non-Linux platforms

**Implementation Details:**

_(To be filled after implementation)_

**Key Source Files:**

- `specs/Public/Terminal-Multiplexers/Tilix.md` - Tilix integration specification
- `crates/ah-mux/src/tilix.rs` - Tilix multiplexer implementation (Linux-only)
- `crates/ah-mux/tests/integration_tests.rs` - integration tests (Linux-gated)

**Outstanding Tasks:**

- D-Bus interface robustness
- GTK+ dependency documentation
- Linux-only integration tests
- Error handling improvements

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

**Status:** Partially complete — implementation exists, spec is good, tests needed.

**Context:**

- iTerm2 is the most popular terminal for macOS with rich AppleScript/Python API support.
- The spec file (`iTerm2.md`) documents integration approach.
- The implementation (`iterm2.rs`) uses iTerm2's Python API or AppleScript for control.
- Testing requires macOS CI runners and iTerm2 installation.

**Deliverables:**

- [ ] Review and update `specs/Public/Terminal-Multiplexers/iTerm2.md`:
  - Document iTerm2 Python API usage
  - Add examples for creating split layouts via Python API
  - Document AppleScript alternative for basic operations
  - Add version compatibility notes (iTerm2 3.x)
  - Add troubleshooting section
- [ ] Review and enhance `crates/ah-mux/src/iterm2.rs`:
  - Ensure Python API or AppleScript usage is robust
  - Implement all feasible `Multiplexer` trait methods
  - Add proper error handling for API failures
  - Implement cleanup logic for test sessions
  - Add detection for iTerm2 availability and API access
- [ ] Add automated tests:
  - Unit tests for iTerm2 detection (macOS only)
  - Integration tests for window/pane creation (macOS CI only)
  - Test for command execution in panes
  - Test cleanup and session teardown
- [ ] Add iTerm2 to CI matrix with macOS runners.
- [ ] Ensure tests are correctly skipped on non-macOS platforms.

**Verification (automated):**

- [ ] Unit tests for iTerm2 availability detection (macOS only)
- [ ] Integration tests run successfully on macOS CI with iTerm2 installed
- [ ] Test verifying Python API command execution
- [ ] Cleanup test ensuring no stray iTerm2 windows remain
- [ ] Verify tests are correctly skipped on non-macOS platforms

**Implementation Details:**

_(To be filled after implementation)_

**Key Source Files:**

- `specs/Public/Terminal-Multiplexers/iTerm2.md` - iTerm2 integration specification
- `crates/ah-mux/src/iterm2.rs` - iTerm2 multiplexer implementation (macOS-only)
- `crates/ah-mux/tests/integration_tests.rs` - integration tests (macOS-gated)

**Outstanding Tasks:**

- Python API robustness
- AppleScript fallback implementation
- macOS-only integration tests
- Error handling improvements

---

## References

- [First.Release.status.md](../First.Release.status.md) - R1: Multiplexer enablement and automated regression coverage
- [TUI-Multiplexers-Overview.md](TUI-Multiplexers-Overview.md) - High-level overview of terminal multiplexer integration
- [AGENTS.md](../../AGENTS.md) - Methodology and structure for status files
- [MVP.status.md](../MVP.status.md) - Reference implementation status tracking structure
- [Component-Architecture.md](../../../docs/Component-Architecture.md) - Philosophy on manual testing utilities
