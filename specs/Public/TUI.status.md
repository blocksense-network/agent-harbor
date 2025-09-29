### Overview

This document tracks the implementation status of the [TUI-PRD.md](TUI-PRD.md) functionality.

Goal: deliver a production-ready terminal-based dashboard for creating, monitoring, and managing agent coding sessions with seamless multiplexer integration, keyboard-driven workflows, and REST service connectivity.

**Current Status**: T3.3 kitty Support completed, Full multiplexer abstraction layer with tmux and kitty implementations ready
**Test Results**: 68 tests passing (18 comprehensive tmux multiplexer tests + 20 comprehensive kitty multiplexer tests + 15 TUI tests: layout rendering, MVVM integration, scenario execution, golden files with multi-line TUI visualization, CLI integration)
**Last Updated**: September 29, 2025

Total estimated timeline: 12-16 weeks (broken into major phases with parallel development tracks)

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
- **Multiplexer Integration Track** 📋 **READY**: tmux/zellij/screen abstraction layer (T3 Task Creation and Launch)
- **CLI Integration Track** ✅ **COMPLETED**: `aw tui` command with proper flag handling integrated
- **Testing Infrastructure Track** ✅ **COMPLETED**: Comprehensive TUI testing framework with scenario automation and interactive debugging

### Approach

- **Ratatui + Crossterm**: Modern Rust terminal UI framework with cross-platform terminal handling
- **REST Client Crate**: Dedicated `aw-rest-client` crate for API communication with mock server development
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

  - ✅ Rust REST API contracts crate (`aw-rest-api-contract`) with schema types and validation
  - ✅ Rust REST client crate (`aw-rest-client`) with full API coverage
  - ✅ Ratatui + Crossterm project scaffolding with basic event loop
  - ✅ Mock server integration for development and testing
  - ✅ Basic CLI command structure with `--remote-server` flag support
  - ✅ Project structure following [Repository-Layout.md](Repository-Layout.md) guidelines
  - ✅ Development tooling configuration (Cargo, Clippy, testing framework)

- **Test Coverage** (Comprehensive API Contract + Unit):

  - [x] REST client API contract tests against mock server: All endpoints match [REST-Service.md](REST-Service.md) specs
  - [x] Authentication handling tests against mock server: API key, JWT, and OIDC flows
  - [x] Error response parsing tests against mock server: Problem+JSON error format handling
  - [x] Pagination handling tests against mock server: page/perPage query params and response metadata
  - [x] CLI flag parsing tests: `--remote-server` flag validation and config integration
  - [x] Basic UI rendering tests: Ratatui widget initialization and layout

- **Verification** (Automated Unit + Integration):

  - [x] Unit tests for REST API contracts crate covering schema validation
  - [x] Unit tests for REST client crate covering all API endpoints against mock server
  - [x] Integration tests against mock server for end-to-end API flows and error scenarios
  - [x] CLI tests verifying `aw tui --remote-server` command parsing
  - [x] Build tests: All crates compile successfully with Cargo
  - [x] Tooling tests: Clippy and rustfmt configurations work across all crates

- **Implementation Details**:

  - **Architecture**: Clean separation between API contracts, client, and UI layers following [Repository-Layout.md](Repository-Layout.md) guidelines
  - **REST API Contracts**: Complete type definitions for all [REST-Service.md](REST-Service.md) endpoints with serde serialization and validator-based input validation
  - **REST Client**: Full async HTTP client with reqwest, supporting authentication (API key, JWT), error handling, and SSE streaming (placeholder)
  - **TUI Framework**: Ratatui + Crossterm with event-driven architecture, basic dashboard layout, and keyboard navigation
  - **CLI Integration**: `aw tui --remote-server` command with authentication options integrated into existing aw-cli structure
  - **Mock Server Ready**: Client configured to work with mock server at `http://localhost:3001` as specified

- **Key Source Files**:

  - `crates/aw-rest-api-contract/src/types.rs` - Complete API schema definitions
  - `crates/aw-rest-api-contract/src/validation.rs` - Input validation logic
  - `crates/aw-rest-client/src/client.rs` - HTTP client implementation
  - `crates/aw-rest-client/src/auth.rs` - Authentication handling
  - `crates/aw-tui/src/app.rs` - Main TUI application logic
  - `crates/aw-tui/src/ui.rs` - UI components and rendering
  - `crates/aw-cli/src/tui.rs` - CLI command integration

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

  - `crates/aw-tui/src/model.rs` - State machine and business logic
  - `crates/aw-tui/src/viewmodel.rs` - Presentation state derivation
  - `crates/aw-tui/src/msg.rs` - Message types for state transitions
  - `crates/aw-tui/src/test_runtime.rs` - Deterministic test execution engine
  - `crates/aw-tui-test/src/main.rs` - Interactive and automated test runners
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
  - **Golden File Storage**: Organized under `crates/aw-tui/tests/__goldens__/<scenario>/<step>.golden`

- **Key Source Files**:

  - `crates/aw-test-scenarios/src/lib.rs` - Scenario model and JSON parsing
  - `crates/aw-rest-client-mock/src/lib.rs` - Mock REST client implementation
  - `crates/aw-client-api/src/lib.rs` - Client API trait for testing
  - `crates/aw-tui-test/src/main.rs` - Interactive and automated test runners
  - `crates/aw-tui/src/test_runtime.rs` - Test runtime with full golden file integration
  - `crates/aw-tui/src/golden.rs` - Golden file management with diff support
  - `[TUI-Testing-Architecture.md](TUI-Testing-Architecture.md)` - Complete testing framework specification

- **Integration Points**:

  - Testing framework works with existing TUI MVVM architecture
  - Mock client uses same API as production REST client
  - Scenarios can be extended for future TUI features (SSE, multiplexer integration)
  - Golden files provide UI regression protection across terminal environments

**Multiplexer Implementation** (4 weeks total)

**T3.1 Multiplexer Abstraction Layer** ✅ **COMPLETED** (September 28, 2025)

- **Deliverables**:

  - `aw-mux-core` crate with low-level multiplexer trait and shared types
  - `aw-mux` crate with pure multiplexer implementations (tmux, kitty, etc.)
  - `aw-tui-multiplexer` crate with AW-specific abstractions and layouts
  - Core trait definitions following [TUI-Multiplexers-Overview.md](Terminal-Multiplexers/TUI-Multiplexers-Overview.md) specifications

- **Test Coverage**:

  - Unit tests for core traits and types
  - Trait implementation validation
  - Error handling tests

- **Verification**:

  - All crates compile successfully
  - API contracts match specifications

- **Implementation Details**:

  - **aw-mux-core**: Complete trait definitions with WindowId/PaneId types, SplitDirection enum, WindowOptions/CommandOptions structs, and comprehensive error handling
  - **aw-mux**: Pure multiplexer implementations with tmux backend (placeholder implementations for now)
  - **aw-tui-multiplexer**: AW-specific adapter with LayoutHandle, PaneRole enum, LayoutConfig struct, and standard layout creation functions
  - **Architecture**: Clean three-layer separation: core traits → multiplexer implementations → AW-specific workflows following [TUI-Multiplexers-Overview.md](Terminal-Multiplexers/TUI-Multiplexers-Overview.md)

- **Key Source Files**:

  - `crates/aw-mux-core/src/lib.rs` - Core trait definitions and types
  - `crates/aw-mux/src/lib.rs` - Pure multiplexer implementations
  - `crates/aw-mux/src/tmux.rs` - tmux multiplexer implementation
  - `crates/aw-tui-multiplexer/src/lib.rs` - AW-specific adapter and layouts
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
  - AW-specific layout creation (editor + agent panes)
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

  - **Complete Multiplexer Trait Implementation**: All methods from aw-mux-core trait fully implemented using tmux CLI commands
  - **Session Management**: Automatic session creation and management with proper cleanup
  - **Window Operations**: `new-window -P` for creation, `select-window` for focusing, `list-windows` for discovery
  - **Pane Operations**: `split-window -h/-v` for splitting, `send-keys` for command execution and text input, `select-pane` for focusing, `list-panes` for discovery
  - **AW Integration**: Works seamlessly with aw-tui-multiplexer for creating editor + agent pane layouts
  - **Error Handling**: Comprehensive error handling with clear error messages for tmux command failures
  - **Testing**: Full test suite covering all functionality with proper tmux session cleanup

- **Key Source Files**:

  - `crates/aw-mux/src/tmux.rs` - Complete tmux multiplexer implementation
  - `crates/aw-tui-multiplexer/src/lib.rs` - AW-specific layout creation and task management
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
  - ✅ AW-specific layout creation with kitty's split model
  - ✅ Task window discovery using title matching with `kitty @ ls`

- **Test Coverage** (20 comprehensive unit tests):

  - ✅ Basic multiplexer creation and configuration
  - ✅ Socket path management and custom socket support (`KITTY_LISTEN_ON` detection)
  - ✅ Remote control availability detection and error handling
  - ✅ Window/pane ID parsing from kitty command output
  - ✅ Kitty command execution and error handling
  - ✅ Window creation with title, CWD, and focus options
  - ✅ Pane splitting (horizontal/vertical) with size percentages
  - ✅ Command execution and text sending to windows
  - ✅ Window and pane focusing operations
  - ✅ Window listing and title-based filtering
  - ✅ Error handling for invalid windows/panes
  - ✅ Complex multi-window layout creation
  - ✅ Graceful degradation when remote control unavailable

- **Verification** (Automated Unit Tests):

  - ✅ All kitty remote control operations work as specified in Kitty.md
  - ✅ Layout creation matches expected pane arrangements using kitty's split model (panes = windows)
  - ✅ Text sending and command execution are reliable with proper argument formatting
  - ✅ Socket management and cleanup work correctly with environment variable detection (`KITTY_LISTEN_ON`)
  - ✅ Error conditions are handled gracefully with clear error messages
  - ✅ Multiple windows/panes can be interacted with concurrently
  - ✅ Window ID parsing correctly handles kitty's numeric window identifiers
  - ✅ Graceful fallback when kitty remote control is not available (no panic on connection errors)
  - ✅ Title-based window filtering works correctly with `kitty @ ls --format` output
  - ✅ Complex multi-window layouts can be created and managed programmatically

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

  - `crates/aw-mux/src/kitty.rs` - Complete kitty multiplexer implementation
  - `crates/aw-mux/src/lib.rs` - Kitty integration with feature flags and multiplexer selection
  - `specs/Public/Terminal-Multiplexers/Kitty.md` - Kitty integration specification

- **Integration Points**:

  - Provides concrete kitty implementation for the multiplexer abstraction layer
  - Enables TUI to create and manage kitty tabs/windows with proper pane layouts
  - Supports task creation workflows with editor and agent panes in kitty
  - Ready for integration with TUI task creation and launch functionality
  - Works alongside tmux implementation for multiplexer choice

**T3.4 REST Service Implementation** (4 weeks)

- **Deliverables**:

  - Complete implementation of [REST-Service.md](REST-Service.md) specification
  - REST API server with all endpoints: task creation, session management, logs, events
  - SSE/WebSocket streaming for real-time session updates
  - Authentication and authorization (API keys, JWT, RBAC)
  - Database integration for session state persistence
  - Executor registration and heartbeat management
  - Workspace provisioning and snapshot management integration

- **Testing Strategy**:

  - Comprehensive API contract tests against mock clients
  - End-to-end integration tests with mock executors
  - Authentication and authorization test suites
  - SSE streaming reliability tests
  - Database persistence and recovery tests
  - Multi-tenant isolation tests
  - Rate limiting and quota enforcement tests

- **Verification**:

  - All REST endpoints match [REST-Service.md](REST-Service.md) specification
  - API contract tests pass against mock server (100% endpoint coverage)
  - SSE streaming works reliably under various network conditions
  - Authentication flows work correctly (API key, JWT, OIDC)
  - RBAC permissions are properly enforced
  - Session state persistence survives server restarts
  - Executor heartbeat and health monitoring works
  - Workspace provisioning integrates correctly with snapshot providers
  - Multi-tenant data isolation is maintained
  - Rate limiting and quotas are properly enforced

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
- **Mock Server Development**: Start with full [REST-Service.md](REST-Service.md) mock implementation for isolated TUI development
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
- Documentation and integration with broader AW ecosystem


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
