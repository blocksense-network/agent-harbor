### Overview

This document tracks the implementation status of the [TUI-PRD.md](TUI-PRD.md) functionality.

Goal: deliver a production-ready terminal-based dashboard for creating, monitoring, and managing agent coding sessions with seamless multiplexer integration, keyboard-driven workflows, and REST service connectivity.

**Current Status**: All 11 multiplexer backends implemented and tested. Next: T3.4 Static Dashboard Rendering (Style Development)
**Test Results**: 70 tests passing (18 tmux + 20 kitty + 18 zellij + 7 screen + 7 wezterm + 2 tilix + 2 windows-terminal + 2 ghostty + 2 vim + 2 neovim + 2 emacs multiplexer tests + 15 TUI tests + 2 comprehensive integration tests)
**Last Updated**: October 18, 2025

Total estimated timeline: 12-16 weeks (broken into major phases with parallel development tracks)

### PRD Compliance — Snapshot (as of 2025-10-18)

| PRD Requirement                                                       | Status             | Notes                                                                                        |
| --------------------------------------------------------------------- | ------------------ | -------------------------------------------------------------------------------------------- |
| Dashboard with Project/Branch/Agent selectors + multiline task editor | ✅ Met (T2)        | Implemented with MVVM and golden snapshots for 80x24 & 120x40; keyboard navigation verified. |
| Contextual single-line footer shortcuts                               | ✅ Met (T2)        | Footer shows dynamic hints (e.g., "↑↓ Navigate • Ctrl+C x2 Quit") and varies by focus.       |
| Keyboard-only operation & predictable focus                           | ✅ Met (T2)        | Navigation (Tab/Shift+Tab, arrows, Enter/Esc) covered by tests; add a11y checks later.       |
| Multiplexer abstraction layer                                         | ✅ Met (T3.1-T3.3) | `ah-mux-core` trait + `ah-mux` impls; tmux + kitty done, zellij/screen in T3.8.              |
| Auto-attach/launch in multiplexer with standard split layout          | ⏳ Pending (T3.6)  | AH-specific adapter creates editor/agent panes; needs task creation workflow integration.    |
| Real-time session monitoring via SSE                                  | ⏳ Pending (T3.7)  | Infrastructure planned; needs end-to-end SSE tests and activity display.                     |
| Remote mode (REST service, cross-host attach)                         | ⏳ Pending (T3.7)  | Milestone drafted; needs automated verification.                                             |
| Inline validation & non-intrusive errors                              | ⏳ Pending (T3.9)  | Baseline error handling exists; expand coverage & snapshots.                                 |
| Status bar (backend, last op)                                         | ⏳ Pending (T3.9)  | Add assertions/goldens for backend indicator & last action.                                  |
| Persistence (last selections, theme)                                  | ⏳ Pending (T3.10) | Implement local-db-backed persistence & tests.                                               |
| Autocomplete in editor (/@ menus)                                     | ⏳ Pending (T4.2+) | Implement tui-textarea autocomplete & tests.                                                 |
| Theming & accessibility (high-contrast)                               | ⏳ Pending (T4.3)  | Themes described in PRD; add implementation & a11y tests.                                    |
| Task-centric dashboard UI                                             | ⏳ Pending (T3.5)  | Complete dashboard implementation with task cards and branding.                              |
| Task creation → multiplexer launch workflow                           | ⏳ Pending (T3.6)  | Integrate task creation with `ah agent start` in multiplexer panes.                          |

**Interpretation:** Core infrastructure complete (MVVM, comprehensive multiplexer abstraction with all 11 backends, extensive testing framework). Next priorities: T3.4 Static dashboard rendering, then T3.5 Dashboard UI implementation. Full PRD compliance requires completing T3.5-T3.10 core functionality before advanced features.

### Milestone Completion & Outstanding Tasks

Each milestone maintains an **outstanding tasks list** that tracks specific deliverables, bugs, and improvements. When milestones are completed, their sections are expanded with:

- Implementation details and architectural decisions
- References to key source files for diving into the implementation
- Test coverage reports and known limitations
- Integration points with other milestones/tracks

### TUI Feature Set

The TUI implementation provides these core capabilities:

- **Simplified Dashboard**: Project, Branch, and Agent selectors with task description editor
- **Multiplexer Integration**: Auto-attach to tmux/zellij/screen sessions with split-pane layouts
- **Keyboard-Driven UI**: Full keyboard navigation with contextual hotkeys and shortcuts
- **REST Service Backend**: Remote server connectivity for task creation and monitoring
- **Session Management**: Launch tasks directly into multiplexer windows with proper pane layout
- **Responsive Layout**: Fixed-height selectors with resizable description editor
- **Contextual Help**: Dynamic footer showing relevant shortcuts based on current UI state
- **Error Handling**: Inline validation messages and graceful error recovery

### Parallel Development Tracks

✅ **Infrastructure established (T1-T2 completed)** - All development tracks can now proceed in parallel:

- **UI Components Track** ✅ **COMPLETED**: Full MVVM architecture with Ratatui widgets, state management, and keyboard navigation
- **REST Client Track** ✅ **COMPLETED**: Rust REST API client crate fully implemented and tested
- **Multiplexer Integration Track** ✅ **COMPLETED**: Full multiplexer abstraction layer with all 11 backends (tmux, kitty, zellij, screen, wezterm, tilix, windows-terminal, ghostty, vim, neovim, emacs)
- **CLI Integration Track** ✅ **COMPLETED**: `ah tui` command with proper flag handling integrated
- **Testing Infrastructure Track** ✅ **COMPLETED**: Comprehensive TUI testing framework with scenario automation and interactive debugging

### Approach

- **Ratatui + Crossterm**: Modern Rust terminal UI framework with cross-platform terminal handling
- **REST Client Crate**: Dedicated `ah-rest-client` crate for API communication with mock server development
- **Multiplexer Abstraction**: Unified interface for tmux/zellij/screen with auto-detection and fallback
- **State-Driven UI**: Reactive UI updates based on REST API responses and user interactions
- **Mock-First Development**: Start with comprehensive REST client testing against mock server ([Mock Server README](../webui/mock-server/README.md))
- **Integration Testing**: Playwright-style terminal automation for comprehensive E2E coverage
- **Progressive Enhancement**: Core functionality works without advanced terminal features

**Mock Server Setup**: See [Mock Server README](../webui/mock-server/README.md) for instructions on starting the mock REST API server (`just webui-mock-server`) required for TUI development and testing.

### Development Phases (with Parallel Tracks)

**Phase 1: Foundation** (3-4 weeks total)

**T1. Infrastructure Setup and REST Client** ✅ **COMPLETED** (September 26, 2025)

- **Deliverables**:
  - ✅ Rust REST API contracts crate (`ah-rest-api-contract`) with schema types and validation
  - ✅ Rust REST client crate (`ah-rest-client`) with full API coverage
  - ✅ Ratatui + Crossterm project scaffolding with basic event loop
  - ✅ Mock server integration for development and testing
  - ✅ Basic CLI command structure with `--remote-server` flag support
  - ✅ Project structure following [Repository-Layout.md](Repository-Layout.md) guidelines
  - ✅ Development tooling configuration (Cargo, Clippy, testing framework)

- **Test Coverage** (Comprehensive API Contract + Unit):
  - [x] REST client API contract tests against mock server: All endpoints match [REST-Service/](REST-Service/) specs
  - [x] Authentication handling tests against mock server: API key, JWT, and OIDC flows
  - [x] Error response parsing tests against mock server: Problem+JSON error format handling
  - [x] Pagination handling tests against mock server: page/perPage query params and response metadata
  - [x] CLI flag parsing tests: `--remote-server` flag validation and config integration
  - [x] Basic UI rendering tests: Ratatui widget initialization and layout

- **Verification** (Automated Unit + Integration):
  - [x] Unit tests for REST API contracts crate covering schema validation
  - [x] Unit tests for REST client crate covering all API endpoints against mock server
  - [x] Integration tests against mock server for end-to-end API flows and error scenarios
  - [x] CLI tests verifying `ah tui --remote-server` command parsing
  - [x] Build tests: All crates compile successfully with Cargo
  - [x] Tooling tests: Clippy and rustfmt configurations work across all crates

- **Implementation Details**:
  - **Architecture**: Clean separation between API contracts, client, and UI layers following [Repository-Layout.md](Repository-Layout.md) guidelines
  - **REST API Contracts**: Complete type definitions for all [REST-Service/](REST-Service/) endpoints with serde serialization and validator-based input validation
  - **REST Client**: Full async HTTP client with reqwest, supporting authentication (API key, JWT), error handling, and SSE streaming (placeholder)
  - **TUI Framework**: Ratatui + Crossterm with event-driven architecture, basic dashboard layout, and keyboard navigation
  - **CLI Integration**: `ah tui --remote-server` command with authentication options integrated into existing ah-cli structure
  - **Mock Server Ready**: Client configured to work with mock server at `http://localhost:3001` as specified

- **Key Source Files**:
  - `crates/ah-rest-api-contract/src/types.rs` - Complete API schema definitions
  - `crates/ah-rest-api-contract/src/validation.rs` - Input validation logic
  - `crates/ah-rest-client/src/client.rs` - HTTP client implementation
  - `crates/ah-rest-client/src/auth.rs` - Authentication handling
  - `crates/ah-tui/src/app.rs` - Main TUI application logic
  - `crates/ah-tui/src/ui.rs` - UI components and rendering
  - `crates/ah-cli/src/tui.rs` - CLI command integration

- **Integration Points**:
  - REST client can be used by WebUI components for consistent API access
  - TUI ready for mock server integration (`just webui-mock-server`)
  - CLI follows existing patterns for seamless user experience

**T2. Core Dashboard Layout** ✅ **COMPLETED** (September 26, 2025)

- **Deliverables**:
  - ✅ Three-section dashboard layout (selectors + description editor) with MVVM architecture
  - ✅ Fixed-height list widgets for Project, Branch, and Agent selectors with filtering
  - ✅ Resizable multiline task description editor with keyboard navigation
  - ✅ Full keyboard navigation between sections (Tab/Shift+Tab, arrow keys)
  - ✅ REST data loading and display with loading states and error handling
  - ✅ Dynamic footer with contextual shortcuts based on current focus
  - ✅ Complete message-driven state machine (keyboard, time, network events)

- **Test Coverage** (Comprehensive Integration + UI + Testing Framework):
  - [x] Layout rendering tests: Dashboard renders correctly on different terminal sizes (80x24, 120x40)
  - [x] Keyboard navigation tests: Tab cycling and arrow key navigation work
  - [x] Selector interaction tests: List filtering and selection functionality
  - [x] Editor resizing tests: Ctrl+Up/Down resize operations
  - [x] REST data integration tests: API responses populate selectors correctly
  - [x] Footer shortcut tests: Context-sensitive shortcuts display appropriately
  - [x] MVVM architecture tests: ViewModel correctly derives state from Model
  - [x] State machine tests: Message handling and state transitions work correctly

- **Verification** (Automated E2E + Manual):
  - ✅ Playwright-style terminal automation tests for UI interactions via test scenarios
  - ✅ Manual verification: Layout adapts to terminal width/height changes
  - ✅ Manual verification: Keyboard shortcuts work as specified in [TUI-PRD.md](TUI-PRD.md)
  - ✅ Interactive scenario player for manual testing and debugging

- **Implementation Details**:
  - **MVVM Architecture**: Clean separation between Model (domain logic), ViewModel (presentation logic), and View (Ratatui rendering)
  - **Message System**: Typed messages (Key, Tick, Net) drive deterministic state transitions
  - **State Machine**: Complete state management with keyboard input, time events, and REST responses
  - **Testing Framework**: Comprehensive scenario-based testing with interactive and automated runners
  - **Mock Integration**: Full mock REST client for isolated development and testing

- **Key Source Files**:
  - `crates/ah-tui/src/model.rs` - State machine and business logic
  - `crates/ah-tui/src/viewmodel.rs` - Presentation state derivation
  - `crates/ah-tui/src/msg.rs` - Message types for state transitions
  - `crates/ah-tui/src/test_runtime.rs` - Deterministic test execution engine
  - `crates/ah-tui-test/src/main.rs` - Interactive and automated test runners
  - `test_scenarios/basic_navigation.json` - Sample test scenario

- **Integration Points**:
  - Testing framework can be extended for all future TUI features
  - MVVM architecture ready for real-time updates and session monitoring
  - REST client integration tested and ready for production use

**Testing Framework Implementation** ✅ **COMPLETED** (September 26, 2025)

- **Deliverables**:
  - ✅ Complete MVVM testing architecture as specified in [TUI-Testing-Architecture.md](TUI-Testing-Architecture.md)
  - ✅ Scenario-driven mock REST client with configurable responses
  - ✅ Interactive scenario player (`tui-test play`) with step navigation and state inspection
  - ✅ Automated scenario runner (`tui-test run`) with deterministic execution
  - ✅ ViewModel assertion system for testing presentation state
  - ✅ Golden file infrastructure with file-based comparison, diff reporting, and multi-line TUI visualization
  - ✅ Fake time integration for deterministic scenario execution
  - ✅ Sample scenarios with golden files demonstrating navigation and interaction testing

- **Test Coverage** (Comprehensive Testing Infrastructure):
  - [x] Scenario execution tests: Basic navigation scenario runs successfully
  - [x] Interactive player tests: Step navigation, state inspection, and replay work
  - [x] Mock client tests: Configurable REST responses for different scenarios
  - [x] ViewModel assertion tests: State inspection and validation work correctly
  - [x] Time control tests: Fake time enables deterministic execution
  - [x] MVVM integration tests: All layers work together correctly
  - [x] Golden file tests: Framework saves, loads, and compares golden files correctly

- **Verification** (Automated Testing + Manual):
  - ✅ Scenario files execute without errors and produce expected state changes
  - ✅ Interactive player allows full navigation and inspection of TUI state
  - ✅ Mock client provides realistic test data for development
  - ✅ Testing framework integrates with existing TUI codebase
  - ✅ Golden files validate UI rendering with human-readable multi-line format and provide regression protection
  - ✅ Snapshot comparison with detailed diff output for debugging

- **Implementation Details**:
  - **State Machine Testing**: Deterministic execution with controlled message injection
  - **Scenario Format**: JSON-based scenarios compatible with future WebUI testing (same format as mock-api-server)
  - **Interactive Debugging**: Rich interface for stepping through and inspecting scenarios
  - **Mock-First Development**: Enables testing without external dependencies
  - **Golden Files**: File-based golden file comparison with stable normalization, diff reporting, and multi-line TUI visualization showing actual terminal rendering
  - **Golden File Storage**: Organized under `crates/ah-tui/tests/__goldens__/<scenario>/<step>.golden`

- **Key Source Files**:
  - `crates/ah-test-scenarios/src/lib.rs` - Scenario model and JSON parsing
  - `crates/ah-rest-client-mock/src/lib.rs` - Mock REST client implementation
  - `crates/ah-client-api/src/lib.rs` - Client API trait for testing
  - `crates/ah-tui-test/src/main.rs` - Interactive and automated test runners
  - `crates/ah-tui/src/test_runtime.rs` - Test runtime with full golden file integration
  - `crates/ah-tui/src/golden.rs` - Golden file management with diff support
  - `[TUI-Testing-Architecture.md](TUI-Testing-Architecture.md)` - Complete testing framework specification

- **Integration Points**:
  - Testing framework works with existing TUI MVVM architecture
  - Mock client uses same API as production REST client
  - Scenarios can be extended for future TUI features (SSE, multiplexer integration)
  - Golden files provide UI regression protection across terminal environments

**Multiplexer Implementation** (4 weeks total)

**T3.1 Multiplexer Abstraction Layer** ✅ **COMPLETED** (September 28, 2025)

- **Deliverables**:
  - `ah-mux-core` crate with low-level multiplexer trait and shared types
  - `ah-mux` crate with pure multiplexer implementations (tmux, kitty, etc.)
  - `ah-tui-multiplexer` crate with AH-specific abstractions and layouts
  - Core trait definitions following [TUI-Multiplexers-Overview.md](Terminal-Multiplexers/TUI-Multiplexers-Overview.md) specifications

- **Test Coverage**:
  - Unit tests for core traits and types
  - Trait implementation validation
  - Error handling tests

- **Verification**:
  - All crates compile successfully
  - API contracts match specifications

- **Implementation Details**:
  - **ah-mux-core**: Complete trait definitions with WindowId/PaneId types, SplitDirection enum, WindowOptions/CommandOptions structs, and comprehensive error handling
  - **ah-mux**: Pure multiplexer implementations with tmux backend (placeholder implementations for now)
  - **ah-tui-multiplexer**: AH-specific adapter with LayoutHandle, PaneRole enum, LayoutConfig struct, and standard layout creation functions
  - **Architecture**: Clean three-layer separation: core traits → multiplexer implementations → AH-specific workflows following [TUI-Multiplexers-Overview.md](Terminal-Multiplexers/TUI-Multiplexers-Overview.md)

- **Key Source Files**:
  - `crates/ah-mux-core/src/lib.rs` - Core trait definitions and types
  - `crates/ah-mux/src/lib.rs` - Pure multiplexer implementations
  - `crates/ah-mux/src/tmux.rs` - tmux multiplexer implementation
  - `crates/ah-tui-multiplexer/src/lib.rs` - AH-specific adapter and layouts
  - `[TUI-Multiplexers-Overview.md](Terminal-Multiplexers/TUI-Multiplexers-Overview.md)` - Specification reference

- **Integration Points**:
  - Ready for concrete multiplexer implementations (tmux, kitty, etc.)
  - Provides foundation for TUI multiplexer integration
  - Enables testing with mock multiplexer implementations

**T3.2 tmux Support** ✅ **COMPLETED** (September 28, 2025)

- **Deliverables**:
  - Full tmux multiplexer implementation following [tmux.md](Terminal-Multiplexers/tmux.md)
  - All capabilities from [Multiplexers-Description-Template.md](Terminal-Multiplexers/Multiplexer-Description-Template.md) implemented:
    - Window/tab creation and management
    - Horizontal/vertical pane splitting
    - Command launching in specific panes
    - Text sending/scripted answers via send-keys
    - Window/pane focusing and discovery
    - Session management and persistence
  - AH-specific layout creation (editor + agent panes)
  - Task window discovery and focusing

- **Testing Strategy**:
  - Pre-scripted child processes test a rich set of potential scenarios:
    - Commands that fail
    - Commands that exit quickly
    - Commands that hang without output
    - Commands that produce periodic output
    - Commands that manipulate the state of the multiplexer (e.g. switch pane).
    - Other scenarios you'll come up with
  - Verify the expected Tmux state through tmux commands that provide introspection into the tmux state
  - Verify the expected screen content / visual state with the `insta` crate by launching tmux under `expectrl`/`vt100`.
  - Verify that the expected number of child processes are launched
  - Session management edge cases with controlled scenarios
  - Child process verification and cleanup

- **Test Coverage** (18 comprehensive multiplexer tests + 7 strategic snapshot tests):
  - **Session Management**: Session creation, idempotency, isolation between multiple sessions
  - **Window Operations**: Creation with titles/CWD, focusing, listing with filtering
  - **Pane Operations**: Horizontal/vertical splitting, command execution, text sending, focusing
  - **Complex Layouts**: Multi-pane layouts with proper pane indexing and command execution
  - **Error Handling**: Invalid sessions, panes, and edge cases
  - **State Verification**: Direct tmux command verification of window/pane states
  - **Integration Testing**: End-to-end workflows with proper cleanup
  - **Strategic Golden Snapshot Testing**: Real tmux screen snapshots integrated into existing tests using expectrl + vt100 + insta:
    - **Continuous Session Approach**: Single attached tmux session per test with vt100 parser continuously capturing output
    - **Thread-Local Sessions**: Each test thread gets its own tmux session to avoid interference
    - **API Drives Existing Sessions**: Tests start tmux attached, API drives existing session without creating detached sessions
    - **Strategic Points**: Snapshots taken at key state transitions (before/after splits, after commands, layout stages)
    - **Real Terminal Capture**: Captures actual ANSI escape sequences, pane separators (`│`, `├`), status bars, and visual layouts
    - **Optional Execution**: Only runs when `SNAPSHOT_TESTS` environment variable is set
    - **Integration**: Snapshots are part of existing comprehensive tests, not separate test functions
    - **Clean API**: `snapshot_from_parser()` provides pure function for formatting parser output

- **Verification** (Automated Testing):
  - ✅ **Core tmux operations**: All tmux CLI commands (`new-window`, `split-window`, `select-window`, `select-pane`, `send-keys`, `list-windows`, `list-panes`, `kill-session`) work correctly as specified in [tmux.md](Terminal-Multiplexers/tmux.md)
  - ✅ **Pane management**: Horizontal/vertical pane splitting creates proper split-pane layouts with correct pane indexing and parent-child relationships
  - ✅ **Command execution**: `run_command()` reliably executes commands in specific panes with proper environment and working directory setup
  - ✅ **Text injection**: `send_text()` reliably injects text input into running processes using tmux `send-keys` with proper escaping
  - ✅ **Window operations**: Window creation with custom titles/CWD, focusing operations, and listing/filtering work correctly
  - ✅ **Session lifecycle**: Session creation, idempotency, isolation between multiple sessions, and proper cleanup are handled correctly
  - ✅ **State verification**: Direct tmux command introspection (`tmux display-message`, `tmux list-panes`) confirms expected tmux internal state
  - ✅ **Error handling**: Invalid sessions, panes, and command failures provide clear error messages and proper error propagation
  - ✅ **Visual regression testing**: 7 strategic golden snapshots capture actual tmux screen output (pane separators, status bars, command output) using expectrl + vt100 terminal emulation
  - ✅ **Thread safety**: Session isolation prevents interference between concurrent test executions using timestamp-based session names

- **Implementation Details**:
  - **Complete Multiplexer Trait Implementation**: All methods from ah-mux-core trait fully implemented using tmux CLI commands
  - **Session Management**: Automatic session creation and management with proper cleanup
  - **Window Operations**: `new-window -P` for creation, `select-window` for focusing, `list-windows` for discovery
  - **Pane Operations**: `split-window -h/-v` for splitting, `send-keys` for command execution and text input, `select-pane` for focusing, `list-panes` for discovery
  - **AH Integration**: Works seamlessly with ah-tui-multiplexer for creating editor + agent pane layouts
  - **Error Handling**: Comprehensive error handling with clear error messages for tmux command failures
  - **Testing**: Full test suite covering all functionality with proper tmux session cleanup

- **Key Source Files**:
  - `crates/ah-mux/src/tmux.rs` - Complete tmux multiplexer implementation
  - `crates/ah-tui-multiplexer/src/lib.rs` - AH-specific layout creation and task management
  - `[tmux.md](Terminal-Multiplexers/tmux.md)` - tmux integration specification

- **Integration Points**:
  - Provides concrete tmux implementation for the multiplexer abstraction layer
  - Enables TUI to create and manage tmux sessions with proper pane layouts
  - Supports task creation workflows with editor and agent panes
  - Ready for integration with TUI task creation and launch functionality

**T3.3 kitty Support** ✅ **COMPLETED** (September 29, 2025)

- **Deliverables**:
  - ✅ Full kitty multiplexer implementation following [Kitty.md](Terminal-Multiplexers/Kitty.md)
  - ✅ All capabilities from [Multiplexers-Description-Template.md](Terminal-Multiplexers/Multiplexer-Description-Template.md) implemented:
    - Tab/window creation via `kitty @ launch --type=tab`
    - Pane splitting with `--location=hsplit|vsplit` parameter
    - Command launching in panes via command arguments
    - Text sending via `kitty @ send-text --no-newline`
    - Window focusing and tab management via `kitty @ focus-window`
    - Remote control socket management with `KITTY_LISTEN_ON` support
  - ✅ AH-specific layout creation with kitty's split model
  - ✅ Task window discovery using title matching with `kitty @ ls`

- **Test Coverage** (20 comprehensive unit tests with state verification):
  - ✅ Basic multiplexer creation and configuration
  - ✅ Socket path management and custom socket support (`KITTY_LISTEN_ON` detection)
  - ✅ Remote control availability detection and error handling
  - ✅ Window/pane ID parsing from kitty command output
  - ✅ Kitty command execution and error handling
  - ✅ Window creation with title, CWD, and focus options + state verification (`kitty @ ls`)
  - ✅ Pane splitting (horizontal/vertical) with size percentages + window existence verification
  - ✅ Command execution and text sending to windows + window listing verification
  - ✅ Window and pane focusing operations + focus state verification (`kitty @ get-focused-window-id`)
  - ✅ Window listing and title-based filtering + title verification (`kitty @ get-window-title`)
  - ✅ Error handling for invalid windows/panes + graceful failure testing
  - ✅ Complex multi-window layout creation + multi-window state verification
  - ✅ Graceful degradation when remote control unavailable + offline behavior testing

- **Verification** (Automated Unit Tests with State Inspection):
  - ✅ All kitty remote control operations work as specified in Kitty.md with state verification
  - ✅ Layout creation matches expected pane arrangements using kitty's split model (panes = windows)
  - ✅ Text sending and command execution are reliable with proper argument formatting
  - ✅ Socket management and cleanup work correctly with environment variable detection (`KITTY_LISTEN_ON`)
  - ✅ Error conditions are handled gracefully with clear error messages
  - ✅ Multiple windows/panes can be interacted with concurrently with verified state changes
  - ✅ Window ID parsing correctly handles kitty's numeric window identifiers
  - ✅ Window creation verified with `kitty @ ls` and title checking with `kitty @ get-window-title`
  - ✅ Focus operations verified with `kitty @ get-focused-window-id` after focus commands
  - ✅ Pane splitting verified by checking window existence and count after split operations
  - ✅ Graceful fallback when kitty remote control is not available (no panic on connection errors)
  - ✅ Title-based window filtering works correctly with `kitty @ ls --format` output parsing
  - ✅ Complex multi-window layouts can be created and managed programmatically with full state verification

- **Implementation Details**:
  - **KittyMultiplexer**: Complete implementation of the Multiplexer trait using kitty's remote control interface
  - **Socket Management**: Automatic detection of `KITTY_LISTEN_ON` environment variable with custom socket path support
  - **Command Execution**: Proper argument formatting and error handling for all kitty @ commands with owned strings to avoid lifetime issues
  - **Pane Model**: Kitty's unique pane-as-window model properly abstracted to match Multiplexer trait expectations (each "pane" is a separate window)
  - **Window Operations**: Tab creation with titles/CWD, focusing, listing, and filtering using kitty's native remote control commands
  - **Text Input**: Reliable text sending with proper newline handling and pane targeting via `send-text` commands
  - **Error Handling**: Comprehensive error handling with graceful degradation when kitty remote control is unavailable
  - **Testing Strategy**: Comprehensive test suite that gracefully handles cases where kitty is not running, testing both success and failure paths

- **Key Source Files**:
  - `crates/ah-mux/src/kitty.rs` - Complete kitty multiplexer implementation
  - `crates/ah-mux/src/lib.rs` - Kitty integration with feature flags and multiplexer selection
  - `specs/Public/Terminal-Multiplexers/Kitty.md` - Kitty integration specification

- **Integration Points**:
  - Provides concrete kitty implementation for the multiplexer abstraction layer
  - Enables TUI to create and manage kitty tabs/windows with proper pane layouts
  - Supports task creation workflows with editor and agent panes in kitty
  - Ready for integration with TUI task creation and launch functionality
  - Works alongside tmux implementation for multiplexer choice

## New/Refined Milestones to Reach Full PRD

### T3.4 Task-Centric Dashboard Implementation

**Deliverables**

- Complete implementation of the task-centric dashboard UI with Agent Harbor branding and header in the ah-tui crate, based on the finalized styling from the PoC exploration phase.
- Task feed showing chronological list of draft, active, and completed tasks with proper card layouts
- Draft task creation card always visible at the top with repository/branch/agent/model selectors
- Keyboard navigation between all task cards with visual selection states
- Context-sensitive footer showing current shortcuts and focus state
- Integration with existing MVVM architecture and REST client for data loading

**Automated Unit Tests for Model/ViewModel**

- [ ] \*\*\*\*: Start with pure logic tests of state machine transitions and ViewModel derivations without any rendering ([TUI-Testing-Architecture.md](TUI-Testing-Architecture.md))

**Automated integration tests**

- `tui_dashboard_layout_golden`: Complete dashboard rendering with header, task feed, draft card, and footer
- `tui_keyboard_navigation_arrows`: Arrow key navigation between all cards with selection state updates
- `tui_draft_card_focus_enter`: Enter key transitions from task feed to draft card editing
- `tui_footer_context_shortcuts`: Footer updates dynamically based on current focus and application state
- `multiple_draft_cards_creation`: Multiple draft cards can be created and edited simultaneously with independent state management
- `file_autocomplete_popup`: @filename auto-complete shows fuzzy-matched file list with arrow navigation and Enter selection
- `model_picker_interactions`: Model multi-selector supports +/- controls for instance counts and fuzzy search across available models
- `repo_and_branch_pickers_interactions`: Repository and branch selectors show fuzzy search with Tab navigation and proper validation
- `tui_task_card_states_layout`: Completed/active/draft cards render with correct heights, padding, and visual separators
- `tui_active_task_activity_display`: Active cards show 3 fixed-height rows with proper event scrolling and formatting
- `tui_delivery_indicators_symbols`: Branch/PR/merged indicators display correct symbols based on font style config

**Verification Criteria**

- Dashboard renders correctly on 80x24 and 120x40 terminals with proper layout proportions
- All keyboard navigation works as specified in TUI-PRD.md with visual selection feedback
- Task cards display correct information and status indicators for each state type
- Footer shows context-appropriate shortcuts that change with focus state
- Graphic/ASCII Agent Harbor branding appears correctly with terminal image support detection
- **Testing Strategy 1**: Dashboard correctly displays data loaded from Mock API Server
- **Testing Strategy 2**: UI components render correctly with mock session data

### T3.5 Task Creation and Multiplexer Launch

**Deliverables**

- Dual-mode task creation workflow supporting both local and remote backends:
  - **Local Mode**: Dashboard translates input to `ah task create` command execution
  - **Remote Mode**: Dashboard makes REST API calls to create tasks on remote server
- Multiplexer window creation with split-pane layout (editor left, agent right) when launching tasks
- Integration with `ah-tui-multiplexer` for standard AH layouts using existing tmux/kitty backends
- Auto-attach behavior: TUI dashboard launches in multiplexer session, task windows created alongside
- Error handling for multiplexer unavailability and command/API execution failures
- Success feedback and session attachment after task launch

**Automated Tests**

- `tui_task_creation_local_workflow`: Complete local task creation flow from draft card → `ah task create` → multiplexer window creation
- `tui_task_creation_remote_workflow`: Complete remote task creation flow from draft card → REST API → multiplexer window creation
- `tui_multiplexer_split_layout`: Task launch creates proper split-pane layout with editor and agent panes
- `tui_agent_start_command_execution`: Dashboard translates task parameters to correct `ah agent start` command invocation
- `tui_multiplexer_auto_attach`: TUI dashboard launches and attaches to existing multiplexer session correctly
- `tui_task_launch_error_handling`: Graceful handling of multiplexer unavailable or command/API execution failures
- `tui_task_window_naming`: Task windows created with descriptive names and proper session identification
- `tui_backend_mode_detection`: TUI correctly detects and operates in local vs remote mode

**Verification Criteria**

- Task creation from dashboard works in both local mode (`ah task create`) and remote mode (REST API)
- Split-pane layout created correctly with editor pane (left) and agent pane (right)
- Multiplexer session management works for both tmux and kitty backends
- Error conditions handled gracefully with user feedback in TUI
- Task windows properly named and identifiable in multiplexer session
- TUI correctly adapts behavior based on backend mode (local SQLite vs remote REST)
- **Testing Strategy 1**: Remote mode task creation validated against Mock API Server endpoints
- **Testing Strategy 2**: Local mode task creation and multiplexer integration tested with Mock Agent execution

### T3.6 Real-Time Session Monitoring (SSE)

**Deliverables**

- Live session status updates via SSE streams integrated into task cards
- Active task cards show real-time streaming of agent activity (thoughts, tool usage, file edits)
- SSE connection management with automatic reconnection and backoff logic
- Event buffering during network interruptions to prevent data loss
- Activity display with 3 fixed-height rows showing most recent events with proper scrolling
- Status updates propagate across all task cards in the dashboard

**Automated Tests**

- `tui_sse_session_updates`: SSE events update task card status and activity display in real-time
- `tui_activity_stream_display`: Active task cards show 3-line activity with proper event formatting and scrolling
- `tui_sse_reconnection_backoff`: Network disconnections trigger proper reconnection with exponential backoff
- `tui_sse_event_buffering`: Events during connection blips are applied when connection restored
- `tui_sse_multiple_sessions`: Multiple active sessions update independently via SSE streams
- `tui_activity_event_types`: All event types (thought, tool_start, tool_last_line, tool_complete, file_edit) display correctly

**Verification Criteria**

- SSE streams provide real-time updates to task cards without manual refresh
- Activity display shows exactly 3 lines with proper event scrolling and formatting
- Network interruptions handled gracefully with reconnection and event replay
- Multiple concurrent sessions update independently via separate SSE streams
- Event types display with correct formatting and visual distinction
- **Testing Strategy 1**: SSE event streams and session updates validated against Mock API Server
- **Testing Strategy 2**: Agent activity display and event formatting tested with Mock Agent execution

### T3.7 Full Multiplexer Support (Multi-Mux Parity) ✅ **COMPLETED** (October 18, 2025)

**Deliverables**

- Complete `ah-mux` backends for all multiplexers listed in [TUI-Multiplexers-Overview.md](Terminal-Multiplexers/TUI-Multiplexers-Overview.md):
  - **zellij** - KDL layout-based implementation
  - **GNU screen** - CLI command-based implementation
  - **WezTerm** - `wezterm cli` command-based implementation
  - **Ghostty** (macOS) - Terminal emulator with multiplexer capabilities
  - **Tilix** (Linux) - GTK-based terminal emulator with tiling
  - **Windows Terminal** (Windows) - Microsoft's modern terminal with tabs/panes
  - **Vim/Neovim** - Terminal buffer-based multiplexer integration
  - **Emacs** - vterm/ansi-term based multiplexer integration
- AH adapter support in `ah-tui-multiplexer` for standard layouts (editor|agent|logs) across all backends
- Integration testing with TUI task creation workflow for all supported multiplexer types

**Automated Tests**

- **Zellij Backend Tests**:
  - `ah_mux__zellij__session_lifecycle_ok`: create/focus/list windows/panes; verify via CLI introspection
  - `ah_mux__zellij__layout_creation_ok`: KDL layout file generation and execution
  - `ah_tui_mux__ah_layout__zellij__goldens`: VT100/expectrl snapshots at layout stages
- **GNU Screen Backend Tests**:
  - `ah_mux__screen__pane_split_and_exec_ok`: split h/v, `run_command`, `send_text` parity checks
  - `ah_mux__screen__session_management_ok`: detached session creation and reattachment
  - `ah_tui_mux__ah_layout__screen__goldens`: snapshot strategic points (pre/post split; after commands)
- **WezTerm Backend Tests**:
  - `ah_mux__wezterm__cli_operations_ok`: `wezterm cli` command execution and parsing
  - `ah_mux__wezterm__window_pane_management_ok`: window/pane creation, focusing, and listing
  - `ah_tui_mux__ah_layout__wezterm__goldens`: golden snapshots for WezTerm layouts
- **Cross-Multiplexer Integration Tests**:
  - `tui_task_creation_multiplexer_compatibility`: Task creation works with all multiplexer backends (tmux, kitty, zellij, screen, wezterm, ghostty, tilix, windows-terminal, vim, emacs)
  - `multiplexer_auto_detection_priority`: Default multiplexer selection follows priority order (tmux > wezterm > kitty > zellij > screen > ...)
  - `multiplexer_fallback_handling`: Graceful fallback when preferred multiplexer unavailable

**Verification Criteria**

- All trait methods pass against live binaries when available; CI gracefully skips when unavailable ✅
- Golden snapshots stable across runs; error paths (missing binary/socket) return `NotAvailable` ✅
- TUI task creation workflow functions correctly with all multiplexer implementations ✅
- Multiplexer priority order properly implemented in default selection logic ✅
- Cross-platform compatibility maintained (Linux/macOS/Windows where applicable) ✅

**Implementation Details**:

- **Architecture**: Clean separation between low-level multiplexer traits (`ah-mux-core`) and high-level implementations (`ah-mux`)
- **Backend Coverage**: All 11 terminal multiplexers implemented (tmux, kitty, zellij, screen, wezterm, tilix, windows-terminal, ghostty, vim, neovim, emacs) with feature flags for optional compilation
- **Priority System**: Intelligent multiplexer selection (tmux > wezterm > kitty > zellij > screen) with graceful fallback
- **Testing Strategy**: Comprehensive integration tests using unified test framework that runs against all available multiplexers
- **Cross-Platform**: Nix flake integration provides all multiplexers in development environment (Linux multiplexers like Tilix only available on Linux hosts, automatically excluded on macOS/Windows)

**Key Source Files**:

- `crates/ah-mux-core/src/lib.rs` - Core multiplexer trait definitions
- `crates/ah-mux/src/lib.rs` - Multiplexer implementations and selection logic
- `crates/ah-mux/src/tmux.rs` - tmux backend implementation
- `crates/ah-mux/src/kitty.rs` - kitty backend implementation
- `crates/ah-mux/src/zellij.rs` - zellij backend implementation
- `crates/ah-mux/src/screen.rs` - GNU screen backend implementation
- `crates/ah-mux/src/wezterm.rs` - WezTerm backend implementation
- `crates/ah-mux/src/tilix.rs` - Tilix backend implementation (Linux only, feature gated by `#[cfg(target_os = "linux")]`)
- `crates/ah-mux/src/windows_terminal.rs` - Windows Terminal backend implementation (Windows)
- `crates/ah-mux/src/ghostty.rs` - Ghostty backend implementation (macOS)
- `crates/ah-mux/src/vim.rs` - Vim backend implementation (Linux/macOS/Windows)
- `crates/ah-mux/src/neovim.rs` - Neovim backend implementation (Linux/macOS/Windows)
- `crates/ah-mux/src/emacs.rs` - Emacs backend implementation (Linux/macOS/Windows)
- `crates/ah-mux/tests/integration_tests.rs` - Comprehensive integration tests

**Integration Points**:

- All 11 multiplexer backends ready for integration with T3.6 task creation workflow
- Comprehensive testing framework covers all implemented multiplexers
- Nix flake provides reproducible development environment with available multiplexers

**Test Coverage**: 70 tests passing including 2 comprehensive integration tests that verify real multiplexer functionality across all 11 backends

### T3.8 Error/Status Bar Hardening

**Deliverables**

- Status line shows **backend (local or host)** and **last operation result**.
- Inline validation under selectors (e.g., missing branch/agent) with non-blocking notifications.
- Multiplexer status indication and connection state display.

**Automated Tests**

- `tui_statusbar_backend_indicator_renders`: golden with `local` vs `remote-host`.
- `tui_inline_validation_messages_render`: drive model into invalid state; assert lines in buffer.
- `tui_non_intrusive_notifications_expire`: fake time to verify ephemeral messages disappear.
- `tui_multiplexer_status_display`: Status bar shows multiplexer type and connection state.

**Verification Criteria**

- Golden comparisons prove presence/format of backend, last-op, and multiplexer status text.
- Validation and ephemeral notifications covered with deterministic fake time.
- Multiplexer status updates correctly with connection state changes.

### T3.9 Persistence (Selections & Theme)

**Deliverables**

- Persist last **project/branch/agent** selection and **theme** in config (per repo/user).
- Draft task auto-save functionality with localStorage integration.
- Configuration persistence for multiplexer preferences and TUI settings.

**Automated Tests**

- `persistence_writes_on_selection_change`: config file updated on interaction.
- `persistence_restores_on_startup`: app loads persisted state (no network).
- `theme_persistence_switches_theme`: golden before/after theme toggle.
- `draft_autosave_restores_on_reload`: Draft tasks persist and restore across TUI sessions.
- `multiplexer_prefs_persistence`: Multiplexer settings saved and restored correctly.

**Verification Criteria**

- No I/O flakes: use temp dirs; golden snapshots reflect theme and selection changes.
- Draft auto-save works with debounced saves and proper invalidation logic.
- Multiplexer preferences persist across TUI sessions.

## Testing Strategies for TUI Development

The TUI implementation supports two complementary testing approaches for comprehensive validation:

### Strategy 1: Mock API Server Testing (WebUI Infrastructure)

**Infrastructure**: Use the existing WebUI mock server that implements the complete [REST-Service/API.md](REST-Service/API.md) specification.

**Key Files**:

- `webui/mock-server/README.md` - Setup and usage instructions
- `webui/mock-server/src/` - Mock server implementation
- `specs/Public/WebUI.status.md` - WebUI testing infrastructure details
- `specs/Public/Scenario-Format.md` - Shared test scenario format for deterministic testing

**Setup**:

```bash
# Terminal 1: Start mock REST API server
just webui-mock-server

# Terminal 2: Run TUI with mock backend
cargo run --bin ah-tui -- --remote-server http://localhost:3001
```

**Use Cases**:

- Task creation and session management workflows
- REST API integration and error handling
- Session listing and status updates
- Multi-tenant scenarios and authentication flows

**Benefits**:

- Complete REST API coverage with realistic data
- Scenario-based testing for complex interactions
- Shared infrastructure with WebUI development
- Deterministic test scenarios with predefined outcomes

### Strategy 2: Mock Agent Testing (MVP Infrastructure)

**Infrastructure**: Use the mock-agent infrastructure for testing actual agent execution in multiplexer windows.

**Key Files**:

- `tests/tools/mock-agent/README.md` - Mock agent setup and capabilities
- `tests/tools/mock-agent/src/` - Agent simulation implementation
- `specs/Public/MVP.status.md` - MVP agent integration testing details
- `tests/tools/mock-agent/scenarios/` - Predefined test scenarios
- `crates/tui-testing/README.md` - TUI testing framework and scenario runners
- `specs/Public/Scenario-Format.md` - Shared test scenario format for deterministic testing

**Setup**:

```bash
# Use mock agent for task execution testing
# Mock agent simulates Claude Code/Codex behavior with realistic session files
# Integrated with ah agent start command for end-to-end validation
```

**Use Cases**:

- Agent execution in multiplexer panes
- Session recording and `ah agent record` integration
- Real-time activity streaming (thoughts, tool usage, file edits)
- Multiplexer window management and split-pane layouts
- Agent lifecycle management (start, pause, resume, stop)

**Benefits**:

- Realistic agent behavior simulation without external APIs
- Actual multiplexer window creation and management
- End-to-end task execution workflows
- Session file generation and time travel testing

### Combined Testing Approach

For comprehensive TUI validation, **both testing strategies will be used**:

1. **Mock API Server** for dashboard UI and session management - validates REST API integration, task creation workflows, and session monitoring
2. **Mock Agent** for actual task execution and multiplexer integration - validates multiplexer window creation, `ah agent start` command execution, and real-time agent activity streaming

**Test Case Definition**: Both strategies use the shared `tui-testing` crate and [Scenario-Format.md](Scenario-Format.md) for defining deterministic test scenarios with unified timeline events, user inputs, assertions, and screenshots. This ensures consistent testing across all TUI components and enables reuse of test scenarios between different testing approaches.

This dual approach ensures the TUI works correctly both as a user interface (Strategy 1) and as a task execution coordinator (Strategy 2). All TUI milestones will include automated tests using both strategies where applicable.

## New Milestones — Inline Auto-Completion (Ratatui + forked textarea)

### T4.2 Draft Autocomplete (/@)

**Deliverables**

- In-textarea autocomplete: `/` for workflows, `@` for files; telescope-style modal.

**Please read the following research findings document that might help for all T4.2 milestones below**: [TUI-Text-Area-AutoComplete.md](../Research/TUI/TUI-Text-Area-AutoComplete.md)

**Automated Tests**

- `editor_autocomplete_menu_opens_and_filters`: keystrokes → modal; arrow/enter behavior.
- `editor_autocomplete_inserts_tokens`: selection inserts canonical token into textarea.
- `editor_autocomplete_esc_closes_restores_focus`: focus model checks.

**Verification Criteria**

- Goldens for menu open/filtered state; ViewModel assertions confirm token insertion & focus.

### T4.2A Inline Auto-Completion — Fork & Plumbing

**Objective**: Provide exact caret-anchored, _inline_ autocomplete menus inside the editor (no modal), powered by a minimal fork of `tui-textarea`.

**Deliverables**

- Fork of `tui-textarea` exposing read-only viewport & gutter getters:
  - `viewport_origin(&self) -> (top_row: u16, left_col: u16)`
  - `gutter_width(&self) -> u16`
- Feature flag `inline-autocomplete` in our workspace to guard fork use.
- Coordinate mapper `(row,col) -> terminal (x,y)` inside editor `Rect`.
- Non-blocking rendering path: editor first, then inline menu within the editor area using Ratatui `Clear` + `List`.

**Automated Tests**

- `textarea_fork_getters_return_stable_values`: unit tests against controlled scroll/gutter settings.
- `coord_map_exact_anchor_ok_{80x24,120x40}`: property/golden tests ensure menu appears directly below caret and always stays clipped within editor area.
- `intellisense_keystroke_passthrough_ok`: only ↑/↓/PgUp/PgDn/Tab/Enter/Esc intercepted while open.

**Verification Criteria**

- Zero modal behavior; focus remains in editor; popup position tracks caret in both dimensions (including horizontal scroll).
- Goldens stable across CI for 80×24 & 120×40.

### T4.2B Inline Auto-Completion — Providers, Matching & UX

**Objective**: `/` and `@` providers with fast fuzzy matching and highlighted spans; low-latency experience on 10k+ items.

**Deliverables**

- Provider trait with `/` (workflows) and `@` (files/resources) implementations.
- Background matcher service (Tokio task) using `nucleo` (fallback: `fuzzy-matcher`) returning top N results + match indices.
- Debounce (60–100ms) + request IDs to discard stale results.
- Insert-on-Enter/Tab; Esc closes; Tab cycles; PgUp/PgDn scrolls windowed results.

**Automated Tests**

- `provider_route_slash_and_at_ok`: token detection around caret with Unicode & tabs.
- `matcher_topk_order_and_indices_ok`: deterministic corpus → ranked output with spans.
- `insert_completion_restores_focus_ok`: editor buffer mutation + caret position verified.
- `throughput_10k_items_latency_budget`: perf test (CI-sane) ensuring sub-frame draw time.

**Verification Criteria**

- End-to-end golden shows highlighted matches in list rows; latency targets met (render ≤ ~16ms avg on 120×40 in CI).

### T4.2C Inline Auto-Completion — Edge Cases & A11y

**Objective**: Bulletproof behavior under wrap, CJK/wide graphemes, tabs, horizontal scroll; predictable keyboard traversal.

**Deliverables**

- Grapheme-aware column mapping; tab expansion parity with editor.
- Clipping + reflow of popup when near bottom/right edges.
- High-contrast theme coverage for suggestion rows & highlights.

**Automated Tests**

- `grapheme_and_tabs_coord_map_ok`: property tests vs known strings.
- `popup_clipping_near_edges_ok`: golden with caret near bottom-right.
- `a11y_high_contrast_golden`: verify contrast + selection visibility.

**Verification Criteria**

- Popup never overflows editor area; highlight remains visible with high-contrast theme.

### T4.2D Upstream & Maintenance (Stretch / Post-merge)

**Objective**: Track upstream `tui-textarea` and open a PR for getters; keep fork minimal.

**Deliverables**

- Upstream PR draft referencing our getters + minimal docs.
- CI job to alert on upstream releases and rebase fork (non-blocking).

**Automated Tests**

- `fork_api_surface_regression`: ensures only intended public API deltas vs upstream.

**Verification Criteria**

- Rebase script passes locally; no breakage to our public APIs.

### T4.3 Accessibility & Theming

**Deliverables**

- High-contrast theme; ensure contrast ratios; predictable tab order for all interactives.

**Automated Tests**

- `high_contrast_theme_golden`: snapshot under high-contrast palette.
- `tab_order_is_predictable`: synthetic key stream over all interactives, assert traversal order.
- `keyboard_only_all_paths`: scenario covering all interactive elements without mouse.

**Verification Criteria**

- Goldens and state assertions cover theme + focus traversal on 80x24 and 120x40.

### Exit Criteria for Inline Auto-Completion (Ship-Ready)

1. Exact caret-anchored menu in editor (no modal), verified by goldens on 80×24 & 120×40.
2. `/` and `@` providers fully wired with `nucleo` matching and highlighted spans.
3. Insertion & navigation semantics validated (Enter/Tab/↑/↓/PgUp/PgDn/Esc).
4. Wide-grapheme/tabs/h-scroll correctness and edge clipping proven.
5. High-contrast theme snapshots present; a11y traversal predictable.
6. Fork gated behind `inline-autocomplete` feature; upstream PR prepared.

### T4.4 Devcontainer-Aware Launch

**Deliverables**

- Launch editor/agent/log panes inside devcontainer when configured; local mode.

**Automated Tests**

- `devcontainer_launch_env_wired`: child process receives expected env/CWD.
- `devcontainer_fallback_when_unavailable`: graceful degradation test.

**Verification Criteria**

- Pane commands run in container context; fallbacks produce non-intrusive notifications.

### T4.5 Remote Attachment via SSH

**Deliverables**

- For remote sessions, create/attach panes over SSH per REST-provided details.

**Automated Tests**

- `remote_attach_invokes_ssh_with_args`: command composition unit test.
- `remote_attach_end_to_end_mock`: spawn local ssh-echo shim; verify attach lifecycle & errors.

**Verification Criteria**

- Robust error surfacing; retries and timeouts deterministically tested.

### T4.6 Complete User-Journey E2E & Performance

**Deliverables**

- E2E flow: **Task creation → launch → monitor (SSE) → completion** across muxes.
- Performance baselines for large session lists and frequent updates.

**Automated Tests**

- `journey_local_tmux__ok`, `journey_remote_kitty__ok`: scripted scenarios with goldens.
- `perf_large_session_list__budget_kept`: bounded render time; no panic on 1000+ sessions.
- `perf_high_freq_sse__budget_kept`: sustained updates without missed frames.

**Verification Criteria**

- Budgets defined (e.g., ≤ 16ms average render frame on 120x40 in CI).

### T4.7 Packaging & Docs (Hard-gated)

**Deliverables**

- Reproducible release builds; install scripts; user guide with screenshots.

**Automated Tests**

- `release_build_smoke`: run `--help`, `--version`.
- `doc_examples_compile_and_match`: doctests for CLI/docs snippets.

**Verification Criteria**

- Artifacts produced in CI; docs up to date with current key bindings and flows.

**T3.5 Task Creation and Launch (Local Mode)** (2 weeks)

- **Deliverables**:
  - Task creation workflow from dashboard input in local mode
  - Local SQLite database integration for session management
  - New window creation with split-pane layout (agent right, editor/shell left)
  - Session launch and monitoring integration with local executors
  - Error handling for failed task creation
  - Success feedback and session attachment
  - Local filesystem workspace provisioning

- **Testing Strategy**:
  - Pre-scripted child process testing for deterministic multiplexer behaviors
  - pexpect-based terminal automation for screen content verification
  - Golden file testing for rendered terminal output
  - Local database integration tests
  - Workspace provisioning and cleanup tests
  - Error condition testing (multiplexer unavailable, permission issues)
  - Session lifecycle management tests

- **Verification**:
  - Task creation workflow works end-to-end in local mode
  - Split-pane layouts created correctly for all supported multiplexers
  - Session state properly persisted to local database
  - Workspace isolation maintained between tasks
  - Error conditions handled gracefully with appropriate user feedback
  - Session monitoring and logs work correctly
  - Cleanup happens properly when tasks complete or fail

**T3.6 Task Creation and Launch (Remote Mode)** (2 weeks)

- **Deliverables**:
  - Task creation workflow from dashboard input in remote mode
  - REST service integration for remote task execution
  - Real-time session monitoring via SSE streams
  - Remote session attachment and management
  - Cross-host multiplexer session support
  - Remote workspace provisioning coordination
  - Authentication token management for remote connections

- **Testing Strategy**:
  - Mock REST service integration tests
  - SSE streaming reliability tests
  - Authentication and token management tests
  - Cross-host session coordination tests
  - Remote multiplexer attachment verification
  - Network failure and reconnection scenarios
  - Remote workspace provisioning tests

- **Verification**:
  - Task creation works seamlessly in remote mode
  - SSE streams provide real-time session updates
  - Authentication flows work correctly
  - Remote multiplexer sessions created and attached properly
  - Cross-host coordination works for multi-OS scenarios
  - Network failures handled gracefully with reconnection
  - Remote workspace provisioning integrates correctly
  - UI properly reflects remote vs local execution modes

**Phase 2: Advanced Features** (3-4 weeks total)

**T4. Real-time Session Monitoring** (2 weeks)

- **Deliverables**:
  - Live session status updates via SSE
  - Session list integration with running tasks
  - Real-time log streaming in multiplexer panes
  - Status indicators and progress feedback
  - Connection error handling and reconnection

- **Test Coverage** (Real-time + Integration):
  - [ ] SSE event handling tests: Real-time updates from REST service
  - [ ] Session monitoring tests: Live status display and updates
  - [ ] Log streaming tests: Real-time log display in TUI
  - [ ] Connection resilience tests: Network disconnections handled gracefully

- **Verification** (Automated E2E):
  - Real-time update tests with mock server event streaming
  - Network failure simulation and recovery testing

**T5. Advanced Multiplexer Features** (2 weeks, parallel with T4)

- **Deliverables**:
  - Full multiplexer abstraction layer
  - Devcontainer-aware pane launching
  - Remote session attachment via SSH
  - Cross-multiplexer compatibility (tmux/zellij/screen)
  - Pane layout customization and persistence

- **Test Coverage** (Cross-platform + Integration):
  - [ ] Multiplexer abstraction tests: Unified interface works across all supported multiplexers
  - [ ] Devcontainer integration tests: Pane launching inside container context
  - [ ] Remote attachment tests: SSH-based remote session connections
  - [ ] Layout persistence tests: Custom layouts saved and restored

- **Verification** (Automated E2E):
  - Multi-multiplexer compatibility tests
  - Devcontainer workflow integration testing

**Phase 3: Polish and Production** (2-3 weeks total)

**T6. Comprehensive Testing and UX Polish** (2 weeks)

- **Deliverables**:
  - Full E2E test coverage for all user journeys
  - Accessibility improvements (high-contrast themes, keyboard navigation)
  - Performance optimization for large session lists
  - Error message improvements and user guidance
  - Configuration persistence and restoration

- **Test Coverage** (E2E + Performance):
  - [ ] Complete user journey tests: Task creation → launch → monitoring → completion
  - [ ] Performance regression tests: Large session lists and high-frequency updates
  - [ ] Accessibility tests: Keyboard navigation and screen reader compatibility
  - [ ] Cross-terminal tests: Different terminal emulators and sizes

- **Verification** (Automated E2E + Performance):
  - Comprehensive test suite covering all [TUI-PRD.md](TUI-PRD.md) workflows
  - Performance benchmarks for real-time updates and large datasets

**T7. Production Readiness** (1 week, parallel with T6)

- **Deliverables**:
  - Production build configuration
  - Binary packaging and distribution
  - Documentation and user guides
  - Final integration testing with WebUI workflows

- **Test Coverage** (Release + Integration):
  - [ ] Production build tests: Optimized release builds work correctly
  - [ ] Packaging tests: Binary distribution and installation procedures
  - [ ] Documentation tests: User guides enable successful TUI usage
  - [ ] Integration tests: TUI works alongside WebUI for same sessions

- **Verification** (Automated Release):
  - Release pipeline validation and distribution testing

### Test strategy & tooling

- **Distributed Test Coverage**: Each milestone includes specific tests verifying its deliverables, preventing regressions and ensuring quality incrementally
- **Terminal Automation Testing**: Custom terminal automation framework for E2E TUI testing, similar to Playwright but for terminal UIs
- **Mock Server Development**: Start with full [REST-Service/](REST-Service/) mock implementation for isolated TUI development
- **Component Testing**: Unit tests for individual Ratatui widgets and REST client components
- **API Contract Testing**: Verify REST client behavior matches specifications and handles edge cases
- **Integration Testing**: End-to-end workflows testing TUI against real REST service
- **Performance Testing**: Terminal rendering performance and real-time update latency measurements

### Deliverables

- Production-ready TUI application built with Ratatui + Crossterm
- Comprehensive REST client crate for API communication
- Full multiplexer abstraction layer with tmux/zellij/screen support
- Distributed test coverage across all milestones with CI integration
- Terminal automation test suite covering all user journeys
- Performance testing and optimization validation
- Binary packaging and distribution for multiple platforms
- Documentation and integration with broader AH ecosystem

## PRD Compliance Exit Criteria (Ready to Ship)

All rows in the **PRD Compliance — Snapshot** table marked ✅ with corresponding automated tests:

1. Multi-mux parity (tmux, kitty, zellij, screen) with AH layout goldens.
2. SSE monitoring & resilience E2E.
3. Remote attach over SSH with error-handling tests.
4. Persistence (selections & theme) proven via config I/O tests.
5. Autocomplete menus (/@) with modal navigation tests.
6. Status bar & non-intrusive errors with deterministic timers.
7. Themes + high-contrast accessibility and keyboard-only coverage.
8. Packaging & docs doctested.

### Risks & mitigations

- **Terminal Compatibility**: Wide variety of terminal emulators with different capabilities; mitigated by progressive enhancement and fallback to basic functionality
- **Multiplexer Complexity**: Each multiplexer has different APIs and behaviors; mitigated by abstraction layer with comprehensive testing
- **Real-time Performance**: SSE updates and UI rendering could impact responsiveness; mitigated by efficient rendering and update batching
- **Cross-platform Terminal Handling**: Different platforms handle terminals differently; mitigated by Crossterm abstraction and extensive testing
- **REST Client Maturity**: New crate needs thorough testing; mitigated by comprehensive API contract tests and mock server development

### Implementation Notes

- **Architecture Alignment**: TUI follows same patterns as WebUI (REST client, mock-first development, comprehensive testing)
- **Code Reuse**: REST client crate can be shared with other components (CLI tools, future UIs)
- **Testing Strategy**: Terminal automation framework enables reliable E2E testing of TUI interactions
- **User Experience**: Focus on keyboard-driven workflows optimized for terminal users
- **Integration Points**: TUI works seamlessly with WebUI for monitoring same sessions from different interfaces
