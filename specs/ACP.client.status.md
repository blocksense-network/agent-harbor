# ACP Client Implementation — Implementation Status

## Overview

Goal: implement an ACP-compliant client that can connect to external ACP agents, enabling Agent Harbor to act as a universal ACP client that bridges any ACP agent with Harbor's filesystem snapshotting, terminal management, and session recording capabilities. The ACP reference docs in `resources/acp-specs/docs` (notably `protocol/overview.mdx`, `protocol/file-system.mdx`, and `protocol/terminals.mdx`) define the JSON-RPC methods we must implement as a client. The client will be integrated into `ah agent start` as a new agent type `acp` with an `--acp-binary` option, allowing users to launch external ACP agents while benefiting from Harbor's execution environment and tooling.

Target crate: `crates/ah-agents`. We will add an `acp` module that implements the `AgentExecutor` trait and uses the vendored `vendor/acp-rust-sdk` for:

- JSON-RPC framing and method dispatch
- Formal request/response structs for client-side operations
- Transport handling (stdio for external binaries)
- Client capability negotiation and session management

## Execution Strategy

1. Extend `ah-agents` with an ACP client implementation that wraps external ACP agent binaries
2. Add `acp` as a new agent type to `ah agent start` with `--acp-binary` option for specifying the external agent executable
3. Implement client-side ACP methods (file system, terminal, permission requests) using Harbor's existing infrastructure
4. Provide both text-normalized UI output (for interactive use) and json-normalized output (for automation)
5. Integrate automatic filesystem snapshots during agent execution when configured
6. Support stdio transport for local ACP agent binaries

### Integration with `ah agent start`

The ACP client will be invoked through the existing `ah agent start` command with a new agent type:

```bash
ah agent start --agent acp --acp-binary /path/to/agent-binary --prompt "Fix the bug"
```

This allows the ACP client to inherit all of Harbor's execution environment features:

- Sandboxed execution via `--sandbox` options
- Filesystem snapshotting via `--fs-snapshots` and `--working-copy` options
- Session recording via the standard `ah agent record` infrastructure
- Environment isolation and credential management

## Test Strategy

- **Unit tests** for ACP protocol translation, capability negotiation, and client method implementations
- **mock-acp-agent** based on `ah-scenario-format` crate and ACP Rust SDK for deterministic testing - implements scripted ACP agent behavior to test client protocol handling, error scenarios, and edge cases
- **Integration tests** that launch real ACP agent binaries and verify end-to-end communication. The real binaries will be driven by our LLM API Proxy which can be scripted through our scenario files.
- All tests run via `just test-rust` with a dedicated `just test-acp-client` shortcut

## Filesystem Operations Strategy

The ACP client will implement the client-side filesystem methods (`fs/read_text_file`, `fs/write_text_file`) by:

1. **File Reading**: Serving read requests from the current working directory or snapshot mounts
2. **File Writing**: Writing files to the workspace and automatically triggering snapshots when `--fs-snapshots` is enabled
3. **Path Resolution**: Converting between ACP's absolute path requirements and Harbor's workspace-relative paths
4. **Permission Handling**: Implementing `request_permission` to handle potentially destructive operations, using the same interactive approval system described in [`Public/Sandboxing/Agent-Harbor-Sandboxing-Strategies.md`](Public/Sandboxing/Agent-Harbor-Sandboxing-Strategies.md) with dynamic read allow-list and persisted policies, presented through the SessionViewer UI described in [`Public/ah-agent-record.md`](Public/ah-agent-record.md).

## Terminal Operations Strategy

The ACP client will implement terminal methods (`terminal/create`, `terminal/output`, etc.) by:

1. **Command Execution**: Using Harbor's Passthrough recorder (described in [`ACP.server.status.md`](ACP.server.status.md)) with interpose shims to capture output from indirect child processes to implement the SessionViewer UI's standard display of indirect children output
2. **Output Streaming**: Streaming the real-time output of the passthrough recorder via ACP `session/update` notifications
3. **Process Management**: Handling process lifecycle, signals, and exit codes
4. **Resource Limits**: Applying Harbor's sandboxing and resource constraints

## UI Strategy

The ACP client will provide two output modes:

1. **Text-Normalized**: Human-readable output suitable for terminal display, showing agent thoughts, tool calls, and results
2. **JSON-Normalized**: Structured output for programmatic consumption and automation

Output mode is controlled by the `--output` flag in `ah agent start`, consistent with other agent types.

The Agent Activity TUI (detailed in [`Public/Agent-Activity-TUI-PRD.md`](Public/Agent-Activity-TUI-PRD.md)) is nested within the standard SessionViewer UI (described in [`Public/ah-agent-record.md`](Public/ah-agent-record.md)), replacing only the terminal rendering area that is used for third-party agents. The SessionViewer UI continues to handle snapshot indicators, task entry UI, pipeline explorers, and all other standard functionality.

---

---

### Milestone 0: mock-agent Development (ACP Mode)

**Status**: Partially implemented

#### Deliverables

- [x] Create new `mock-agent` crate focused on ACP protocol testing
- [x] Implement ACP mode using `ah-scenario-format` and ACP Rust SDK for testing (minimal stdio loop with ScenarioAgent playback)
- [x] Create basic ACP SDK example client for mock-agent verification
- [x] Add mock-agent to test utilities with configurable scenario support
- [x] Create integration tests validating mock-agent ACP behavior (in-process stdio + permission/file-read flows)
- [x] Add end-to-end PTY streaming test for terminal tool calls using portable-pty harness (reuse ah-recorder/ah-tui-testing helpers)
- [x] Document mock-agent usage and capabilities
- [x] Update `specs/Public/Repository-Layout.md`
- [x] Add CI-friendly ACP smoke target (`just run-mock-agent-acp-smoke`) covering echo + loadSession/\_meta

#### Implementation Details

- `mock-agent` will be a standalone crate that can be used both as a library and as an executable:
  - **Library**: Core functionality for ACP protocol simulation and scenario playback
  - **Executable**: Thin wrapper providing command-line interface to the library functionality
- **ACP mode**: Uses `ah-scenario-format` and ACP Rust SDK for deterministic ACP protocol testing
- ACP mode will use stdio transport and implement basic ACP agent methods (initialize, new_session, prompt, cancel) with scripted responses
- SDK example client will verify mock-agent protocol compliance
- Support for simulating file system and terminal operations via scenario definitions
- All functionality must be available through the library API

**Current gaps/blockers (as of 2025-11-29):**

- Tool execution validation still partial (tool_execution events not yet compared to expected outputs; assistant/meta propagation could be richer).
- Multiple-scenario heuristics/loadSession: selection now errors on missing sessionId and uses prompt-distance thresholds/ACP tags, but deeper orchestration and validation remain TODO.

#### Key Source Files

- `crates/mock-agent/src/lib.rs`, `crates/mock-agent/src/executor.rs` (core mock-agent playback)
- `crates/mock-agent/src/main.rs` (thin executable wrapper)
- `crates/mock-agent/examples/acp_client.rs` (SDK example client; auto-approves permissions/terminal callbacks, supports image/audio blocks, interactive stdin prompts)
- `tests/tools/mock-agent-acp/run.sh`, `tests/tools/mock-agent-acp/scenarios/*` (utility wrapper + demo scenarios: echo, permission/read, terminal, loadSession+\_meta, multimodal)
- `crates/mock-agent/tests/acp_integration.rs` (integration tests; PTY follower test now enabled and streaming real output)
- `crates/mock-agent/tests/acp_smoke_cli.rs` (invokes `just run-mock-agent-acp-smoke` for quick ACP smoke)
- `crates/ah-scenario-format/` (scenario parsing/validation)
- `vendor/acp-rust-sdk/examples/client.rs` (for SDK example client)

#### Mock-Agent CLI Parameters

The mock-agent executable supports the following CLI parameters for configuring agent capabilities and behavior. These parameters override any equivalent settings specified in scenario files (`acp.capabilities`, `acp.cwd`, `acp.mcpServers`).

**Core Configuration:**

- `--scenario <PATH>`: Path to scenario file, directory containing test scenarios, or multiple space-separated files (supports glob patterns)
- `--protocol-version <VERSION>`: Protocol version to advertise (default: 1)

**Capability Configuration (typically specified in scenario `acp.capabilities`):**

- `--capabilities <JSON>`: JSON string specifying complete agent capabilities object (overrides individual flags and scenario settings)
- Individual flags: `--load-session`, `--image-support`, `--audio-support`, `--embedded-context`, `--mcp-http`, `--mcp-sse` (override scenario `acp.capabilities`)

**Runtime Configuration (typically specified in scenario `acp` settings):**

- `--cwd <PATH>`: Working directory for the ACP session (overrides scenario `acp.cwd`)
- `--mcp-servers <JSON>`: JSON array of MCP server configurations (overrides scenario `acp.mcpServers`)
- `--verbose`: Enable verbose logging for debugging

**Symbol Definition (for scenario rules evaluation):**

- `--define <KEY=VALUE>`: Define a symbol with a string/numeric value for scenario rules evaluation
- `--define <KEY>`: Define a boolean symbol (existence check) for scenario rules evaluation
- Multiple `--define` options can be used to define multiple symbols

#### Multiple Scenario File Support

When multiple scenario files are provided, the mock-agent automatically selects scenarios using Levenshtein distance matching (see [Scenario-Format.md](./Public/Scenario-Format.md#scenario-selection--playback-controls)):

- **For new sessions** (`session/new`): Matches the `initialPrompt` against scenario `initialPrompt` fields
- **For session loading** (`session/load`): Matches the session ID against scenario `name` fields
- **Fallback behavior**: If no close match is found, the first scenario is used as default

#### Example Usage

```bash
# Basic ACP agent with file system and terminal capabilities
mock-agent --scenario test_scenario.yaml

# Agent with rich content support
mock-agent --scenario rich_content_test.yaml --image-support --embedded-context

# Agent with MCP server support
mock-agent --scenario mcp_test.yaml --mcp-http --mcp-servers '[{"name":"fs","command":"mcp-server-filesystem","args":["/tmp/workspace"]}]'

# Custom capabilities for testing
mock-agent --scenario capability_test.yaml --capabilities '{"loadSession":true,"promptCapabilities":{"image":false,"audio":true}}'
```

#### Verification

### Scenario Format ↔ ACP Protocol Mapping Tests

#### Core ACP Message Round-trip Tests

- [x] `test_initialize_request_response_mapping` - Verifies scenario `initialize` events properly map to ACP `initialize` requests and responses ([../resources/acp-specs/docs/protocol/initialization.mdx](../resources/acp-specs/docs/protocol/initialization.mdx))
- [x] `test_session_new_request_response_mapping` - Verifies scenario configuration properly maps to ACP `session/new` method calls and responses ([../resources/acp-specs/docs/protocol/session-setup.mdx#creating-a-session](../resources/acp-specs/docs/protocol/session-setup.mdx#creating-a-session))
- [x] `test_session_load_optional_mapping` - Verifies `sessionStart` boundary markers and historical/live event separation for ACP `session/load` method calls when `loadSession` capability is enabled ([../resources/acp-specs/docs/protocol/session-setup.mdx#loading-sessions](../resources/acp-specs/docs/protocol/session-setup.mdx#loading-sessions))
- [x] `test_session_prompt_content_mapping` - Verifies `userInputs` scenario events map correctly to ACP `session/prompt` method calls ([../resources/acp-specs/docs/protocol/content.mdx](../resources/acp-specs/docs/protocol/content.mdx))
- [x] `test_session_update_all_types_mapping` - Verifies `llmResponse` and `agentToolUse` scenario events properly map to ACP `session/update` notifications (agent responses, tool calls, tool results, plans, etc.) ([../resources/acp-specs/docs/protocol/prompt-turn.mdx#3-agent-reports-output](../resources/acp-specs/docs/protocol/prompt-turn.mdx#3-agent-reports-output))
- [x] `test_session_cancel_mapping` - Verifies `userCancelSession` scenario events map to ACP `session/cancel` notifications and interrupt scenario execution ([../resources/acp-specs/docs/protocol/prompt-turn.mdx#cancellation](../resources/acp-specs/docs/protocol/prompt-turn.mdx#cancellation))

#### ACP Content Handling Tests

- [x] `test_content_block_text_parsing` - Verifies Text content blocks are properly parsed from scenarios and delivered as ACP messages
- [x] `test_content_block_image_delivery` - Verifies Image content blocks with mimeType/data are correctly mapped to ACP protocol
- [x] `test_content_block_audio_delivery` - Verifies Audio content blocks are properly handled in ACP message flow
- [x] `test_content_block_resource_embedding` - Verifies Resource content blocks (file references, embedded code) map to ACP resource blocks
- [x] `test_content_block_diff_representation` - Verifies diff content blocks for file modifications are correctly handled ([../resources/acp-specs/docs/protocol/tool-calls.mdx#diffs](../resources/acp-specs/docs/protocol/tool-calls.mdx#diffs))
- [x] `test_content_block_mixed_prompts` - Verifies prompts containing multiple content block types are correctly sequenced

#### ACP Session Lifecycle Tests

- [x] `test_session_lifecycle_complete_flow` - Verifies full session lifecycle (new → prompt → updates → completion) mapping
- [x] `test_session_concurrent_operations` - Verifies multiple sessions can operate concurrently without interference
- [x] `test_session_error_conditions` - Verifies error responses for invalid session IDs, malformed requests, etc.
- [x] `test_session_mcp_server_integration` - Verifies MCP server configurations are properly passed to session creation
- [x] `test_terminal_follower_pty_streaming` - End-to-end PTY-backed follower test (mock-agent `runCmd` with real PTY output/input via portable-pty harness); follower parsing, PTY spawning, streaming, and exit propagation now wired and test enabled.

#### ACP Protocol Extension Tests

- [x] `test_acp_extension_methods_mapping` - Verifies custom ACP methods (prefixed with `_`) are properly handled via scenario extensions ([../resources/acp-specs/docs/protocol/extensibility.mdx](../resources/acp-specs/docs/protocol/extensibility.mdx))
- [x] `test_acp_meta_fields_preservation` - Verifies `_meta` fields in ACP messages are preserved and accessible in scenarios
- [x] `test_acp_meta_fields_initialization` - Verifies `_meta` fields in initialize requests/responses are correctly handled
- [x] `test_acp_meta_fields_session_messages` - Verifies `_meta` fields in session/prompt and session/update are preserved
- [x] `test_acp_session_mode_switching` - Verifies `setMode` scenario events map to `session/set_mode` ACP method calls ([../resources/acp-specs/docs/protocol/session-modes.mdx#setting-the-current-mode](../resources/acp-specs/docs/protocol/session-modes.mdx#setting-the-current-mode))
- [x] `test_acp_session_model_switching` - Verifies `setModel` scenario events map to `session/set_model` ACP method calls (UNSTABLE feature) ([../resources/acp-specs/docs/protocol/schema.unstable.mdx#session-set_model](../resources/acp-specs/docs/protocol/schema.unstable.mdx#session-set_model))
- [x] `test_acp_custom_capabilities` - Verifies custom capabilities can be advertised and negotiated ([../resources/acp-specs/docs/protocol/extensibility.mdx#advertising-custom-capabilities](../resources/acp-specs/docs/protocol/extensibility.mdx#advertising-custom-capabilities))

### Scenario Format Completeness Tests

- [x] `test_scenario_format_exhaustive_coverage` - Verifies every ACP protocol message type has corresponding scenario format representation
- [x] `test_scenario_rules_conditional_mapping` - Verifies `rules` construct properly maps different ACP behaviors based on conditions
- [x] `test_scenario_initialprompt_rich_content` - Verifies `initialPrompt` supports all ACP content block types for initial session prompts
- [x] `test_scenario_timeline_comprehensive_events` - Verifies timeline supports all ACP message flows and notification types

### ACP Transport and Framing Tests

- [x] `test_stdio_notification_delivery` - Verifies ACP notifications are properly delivered over stdio transport ([../resources/acp-specs/docs/protocol/prompt-turn.mdx#3-agent-reports-output](../resources/acp-specs/docs/protocol/prompt-turn.mdx#3-agent-reports-output))
  - Test `session/update` notifications with agent message chunks
  - Test `session/update` notifications with tool call updates
  - Test `session/update` notifications with plan entries
  - Test `current_mode_update` notifications ([../resources/acp-specs/docs/protocol/session-modes.mdx#from-the-agent](../resources/acp-specs/docs/protocol/session-modes.mdx#from-the-agent))
  - Test extension notifications starting with underscore ([../resources/acp-specs/docs/protocol/extensibility.mdx#custom-notifications](../resources/acp-specs/docs/protocol/extensibility.mdx#custom-notifications))

### Library and Configuration Tests

- [x] `test_library_scenario_driven_execution` - Verifies library API can execute complete scenarios and generate ACP message sequences
- [x] `test_configuration_symbol_injection` - Verifies symbols can be specified for conditional scenario execution

#### Client-Side ACP Method Simulation Tests

- [x] `test_client_fs_read_simulation` - Verifies `readFile` scenario events properly map to client `fs/read_text_file` ACP method calls to the agent
- [x] `test_client_fs_write_simulation` - Verifies `agentEdits` and `editFile`/`writeFile` scenario events properly map to client `fs/write_text_file` ACP method calls to the agent
- [x] `test_client_terminal_operations_simulation` - Verifies `runCmd` scenario events properly map to client terminal ACP method flows (create, output, kill, etc.)
- [x] `test_client_permission_request_simulation` - Verifies permission-required scenario events properly map to client `session/request_permission` ACP method calls to the agent

#### ACP Error and Edge Case Tests

- [x] `test_acp_error_response_simulation` - Verifies error conditions in ACP responses are properly simulated via scenario events
- [x] `test_acp_authentication_flow` - Verifies `authenticate` method flow when agent requires authentication
- [x] `test_acp_session_modes` - Verifies `session/set_mode` method support when agent supports operating modes
- [x] `test_acp_notification_all_types` - Verifies all `session/update` notification variants (status, log, thought, tool_call, tool_result, file_edit, terminal) are simulable

#### ACP Comprehensive Integration Tests

- [x] `test_acp_comprehensive_scenario_execution` - Executes a complex, multi-feature scenario combining session lifecycle, rich content, tool calls, file operations, mode switching, and error conditions to validate end-to-end system integration and catch interaction issues between features

#### LoadSession Functionality Tests

- [x] `test_loadsession_capability_advertisement` - Verifies `loadSession` capability is properly advertised when enabled
- [x] `test_session_load_historical_replay` - Verifies events before `sessionStart` are replayed during `session/load`
- [x] `test_session_load_live_streaming` - Verifies events after `sessionStart` are streamed live after loading
- [x] `test_multiple_scenarios_session_matching` - Verifies correct scenario selection for `session/load` by session ID matching
- [x] `test_multiple_scenarios_new_session_matching` - Verifies Levenshtein distance matching for new sessions across multiple scenarios ([Scenario-Format.md#scenario-selection--playback-controls](Public/Scenario-Format.md#scenario-selection--playback-controls))

---

### Milestone 0.5: Agent Activity TUI Mock Mode & Session Viewer Integration

**Status**: Planned

#### Deliverables

- [ ] Implement TUI mode in `ah-tui` crate that simulates Agent Activity TUI output format
- [ ] Create ViewModel and View components following MVVM architecture
- [ ] Integrate TUI mode with existing Agent Activity TUI infrastructure
- [ ] Manual testing and acceptance of visual styles

#### Implementation Details

- **Location**: Implementation resides in `crates/ah-tui` crate following strong ViewModel/View separation
- **Architecture**: Follows existing MVVM pattern with separate ViewModel and View modules
- **TUI mode**: Simulates the output format expected by the Agent Activity TUI (thoughts, tool calls, file edits, logs, etc.)
- **Integration**: Works with existing Agent Activity TUI components and SessionViewer UI

#### Session Viewer Refactoring for mock-agent integration

**Existing Session Viewer Components:**

- `crates/ah-tui/src/view/session_viewer.rs` - Already implemented Ratatui rendering functions (603+ lines)
- `crates/ah-tui/src/view_model/session_viewer_model.rs` - Already implemented ViewModel with state management (1242+ lines)
- `crates/ah-tui/src/viewer.rs` - Already implemented main viewer event loop (425+ lines)

**Required Refactoring:**

- **Dependency Injection Pattern**: Refactor `viewer.rs` following the `dashboard_loop.rs` pattern:
  - Extract hard-coded dependencies into `SessionViewerDependencies` struct
  - Support both production (real dependencies) and test (mock dependencies) modes through a new executable a
  - Enable standalone session viewer testing similar to `just run-tui-mock-dashboard` with a new target `just run-mock-agent-session`. It will use the mock-agent crate as a library and the refactored session viewer UI to simulate an agent session specified as a scenario file.

- **Test/Simulation Mode**:
  - Create mock ACP client implementation for scenario-driven testing
  - Enable session viewer to run in standalone test mode with scenario playback

#### Key Source Files

- `crates/ah-tui/src/view_model/agent_activity_model.rs` (ViewModel for TUI mode)
- `crates/ah-tui/src/view/agent_activity_view.rs` (View rendering for TUI mode)
- `crates/ah-tui/src/viewer.rs` (Main session viewer loop with dependency injection)
- `crates/ah-tui/src/session_viewer_deps.rs` (Dependency injection structure)
- `crates/ah-tui/src/view_model/session_viewer_model.rs` (Session viewer ViewModel)
- `crates/ah-tui/src/view/session_viewer.rs` (Session viewer rendering)

#### Reference Implementations

- **Dashboard Loop Pattern**: `crates/ah-tui/src/dashboard_loop.rs` - Shows dependency injection pattern with `TuiDependencies`

- **Mock Dashboard Command**: Study `just run-tui-mock-dashboard` implementation for how test modes are structured

#### Verification

- [x] Manual testing demonstrates proper visual styling and layout
- [x] TUI mode integrates seamlessly with existing Agent Activity TUI
- [x] Session viewer supports both production and test modes through dependency injection (refactored from existing viewer.rs)
- [x] Visual styles accepted by design review
- [x] Session viewer can be run in standalone test mode similar to mock dashboard

---

### Milestone 1: ACP Client Architecture & Agent Integration

**Status**: Planned

#### Deliverables

- [x] Create `acp` module in `crates/ah-agents/src/acp.rs` implementing the `AgentExecutor` trait
- [x] Add `acp` to the available agents list and `agent_by_name()` function
- [x] Add `--acp-binary` option to `AgentLaunchConfig` and CLI parsing
- [x] Implement basic ACP client scaffolding with SDK integration
- [x] Add ACP client feature flag and dependency on `vendor/acp-rust-sdk`
- [x] Create unit tests for client initialization and basic method dispatch

#### Implementation Details

- The ACP client will implement `AgentExecutor` and handle the protocol translation between Harbor's agent abstraction and ACP
- Client will support stdio transport (for `--acp-binary`)
- Initial implementation will provide stub responses for all client methods, to be filled in subsequent milestones
- Integration with existing credential and environment setup from `AgentLaunchConfig`

#### Key Source Files

- `crates/ah-agents/src/acp.rs`
- `crates/ah-agents/src/lib.rs` (add ACP to agent lists)
- `crates/ah-agents/src/traits.rs` (extend `AgentLaunchConfig` if needed)
- `crates/ah-cli/src/commands/agent/start.rs` (add `--acp-binary` option)

#### Verification

- [x] `cargo test -p ah-agents acp_client_initialization` verifies client can be constructed with binary path
- [x] `cargo test -p ah-agents acp_agent_by_name` ensures `acp` agent type is discoverable
- [x] CLI parsing test validates `--acp-binary` option is accepted
- [x] `just lint-rust` passes on new ACP client code

---

### Milestone 2: Transport Layer & Connection Management

**Status**: Planned

#### Deliverables

- [x] Implement stdio transport using the ACP SDK's stdio connection
- [x] Add connection lifecycle management (connect, disconnect, error handling)
- [x] Implement basic capability negotiation during `initialize`
- [x] Add connection health monitoring and automatic reconnection
- [x] Create integration tests for stdio transport

#### Implementation Details

- Stdio transport: spawn the `--acp-binary` process and connect via stdin/stdout
- Connection management: handle process lifecycle and connection establishment/teardown
- Capability negotiation: advertise client capabilities (filesystem, terminal) during initialization
- Error handling: translate transport errors into appropriate ACP error responses

#### Key Source Files

- `crates/ah-agents/src/acp/transport.rs`
- `crates/ah-agents/src/acp/connection.rs`
- `crates/ah-agents/tests/acp_transport.rs`

#### Verification

- [x] `cargo test -p ah-agents --test acp_stdio_transport` verifies stdio connection to mock-acp-agent
- [x] `cargo test -p ah-agents acp_capability_negotiation` ensures proper initialization handshake
- [x] Integration test spawns mock-acp-agent binary and verifies basic communication
- [x] `cargo test -p ah-agents acp_prompt_execution` ensures prompts are sent and responses received
- [x] `cargo test -p ah-agents acp_event_streaming` verifies session update notifications are processed
- [x] Integration test with SDK example agent validates end-to-end prompt flow

---

### Milestone 3: Filesystem Method Implementation

**Status**: Planned

#### Deliverables

- [x] Implement `fs/read_text_file` and `fs/write_text_file` client methods
- [x] Add filesystem capability advertisement during initialization
- [x] Implement path resolution between ACP absolute paths and Harbor workspace paths
- [x] Add automatic snapshot creation on file writes when configured
- [x] Handle file access permissions and error cases
- [x] Create filesystem operation tests with mock scenarios

#### Implementation Details

- File reading: serve `fs/read_text_file` requests from current workspace or snapshot mounts
- File writing: write to workspace and trigger snapshots via existing FS snapshot infrastructure
- Path handling: convert between ACP's absolute path requirements and Harbor's relative workspace paths
- Snapshot integration: use `ah-fs-snapshots` provider to create snapshots after file modifications
- Permission checks: implement basic access control for file operations

#### Key Source Files

- `crates/ah-agents/src/acp/filesystem.rs`
- `crates/ah-agents/tests/acp_filesystem.rs`

#### Verification

- [x] `cargo test -p ah-agents acp_file_read` verifies file content serving via ACP
- [x] `cargo test -p ah-agents acp_file_write` ensures file writes trigger snapshots when enabled
- [x] `cargo test -p ah-agents acp_path_resolution` validates path conversion logic
- [x] Integration test verifies snapshots are created after ACP file operations

---

### Milestone 4: Terminal Method Implementation

**Status**: Planned

#### Deliverables

- [x] Implement terminal capability advertisement and all terminal methods (`create`, `output`, `wait_for_exit`, `kill`, `release`)
- [x] Add terminal creation and process management
- [x] Implement output streaming and real-time updates
- [x] Handle process lifecycle, signals, and exit codes
- [x] Add resource limits and sandboxing integration
- [x] Create terminal operation tests with process mocking

#### Implementation Details

- Terminal creation: spawn processes using Harbor's existing command execution infrastructure
- Output handling: stream terminal output via ACP `session/update` notifications
- Process management: handle process lifecycle, signal handling, and cleanup
- Resource control: apply Harbor's sandboxing and resource limits to terminal processes
- Error handling: translate process errors into appropriate ACP responses

#### Key Source Files

- `crates/ah-agents/src/acp/terminal.rs`
- `crates/ah-agents/tests/acp_terminal.rs`

#### Verification

- [x] `cargo test -p ah-agents acp_terminal_lifecycle` verifies complete terminal creation/execution/cleanup flow
- [x] `cargo test -p ah-agents acp_output_streaming` ensures real-time output delivery
- [x] `cargo test -p ah-agents acp_process_signals` validates signal handling and process control
- [x] Integration test spawns actual processes and verifies ACP terminal operations

---

### Milestone 5: Permission Request Handling & UI Integration

**Status**: Planned

#### Deliverables

- [x] Implement `request_permission` client method for handling agent permission requests
- [x] Add permission policy configuration and automatic approval rules
- [x] Implement text-normalized and json-normalized output modes
- [x] Create interactive permission prompts for terminal use
- [x] Add programmatic permission handling for automation
- [x] Create UI integration tests for both output modes

#### Implementation Details

- Permission handling: implement policy-based automatic approval or interactive prompts
- UI modes: text-normalized for human-readable output, json-normalized for programmatic use
- Interactive prompts: handle permission requests in terminal sessions
- Automation support: allow pre-approval of permission types for CI/CD use cases
- Output formatting: translate ACP events into appropriate output format

#### Key Source Files

- `crates/ah-agents/src/acp/permissions.rs`
- `crates/ah-agents/src/acp/ui.rs`
- `crates/ah-agents/tests/acp_ui.rs`

#### Verification

- [x] `cargo test -p ah-agents acp_permission_handling` verifies permission request/response flow
- [x] `cargo test -p ah-agents acp_text_output` ensures proper text-normalized formatting
- [x] `cargo test -p ah-agents acp_json_output` validates json-normalized output structure
- [x] Integration test exercises permission prompts in interactive mode

---

### Milestone 6: Advanced Features & Extensions

**Status**: Planned

#### Deliverables

- [x] Implement ACP extension methods for Harbor-specific features
- [x] Add support for multimodal inputs (images, files) if agent supports them
- [x] Implement session pause/resume functionality
- [x] Add agent plan support and mode switching
- [x] Create extension method tests and integration validation
- [x] Add support for MCP server connections

#### Implementation Details

- Extensions: implement custom `_ah/*` methods for Harbor-specific functionality
- Multimodal: handle file/image attachments in prompts when supported
- Session control: implement pause/resume and mode switching
- MCP integration: support connecting to Model Context Protocol servers
- Advanced features: implement agent plans and slash commands

#### Key Source Files

- `crates/ah-agents/src/acp/extensions.rs`
- `crates/ah-agents/src/acp/multimodal.rs`
- `crates/ah-agents/tests/acp_extensions.rs`

#### Verification

- [x] `cargo test -p ah-agents acp_extensions` verifies custom method handling
- [x] `cargo test -p ah-agents acp_multimodal` tests file/image attachment handling
- [x] `cargo test -p ah-agents acp_session_control` validates pause/resume functionality
- [x] Integration test with extension-supporting agent validates advanced features

---

### Milestone 7: Performance & Resilience

**Status**: Planned

#### Deliverables

- [x] Add connection pooling and request batching optimizations
- [x] Implement retry logic and circuit breaker patterns
- [x] Add comprehensive error handling and recovery
- [x] Optimize memory usage for large file operations
- [x] Add performance monitoring and metrics
- [x] Create stress tests and performance benchmarks

#### Implementation Details

- Connection management: implement connection reuse and request multiplexing
- Error recovery: add automatic retry with exponential backoff
- Resource optimization: stream large files and implement memory-efficient buffering
- Monitoring: add performance metrics and health checks
- Stress testing: validate performance under high load

#### Key Source Files

- `crates/ah-agents/src/acp/performance.rs`
- `crates/ah-agents/tests/acp_stress.rs`

#### Verification

- [x] Performance benchmarks validate throughput and latency targets
- [x] `cargo test -p ah-agents acp_error_recovery` ensures robust error handling
- [x] `cargo test -p ah-agents acp_resource_limits` verifies memory and connection limits
- [x] Stress tests validate performance under load

---

### Milestone 8: Documentation & Packaging

**Status**: Planned

#### Deliverables

- [x] Create comprehensive documentation for ACP client usage
- [x] Add examples and tutorials for common use cases
- [x] Create packaging and distribution configuration
- [x] Add CLI help text and man page generation
- [x] Create migration guides for users of other ACP clients
- [x] Add final integration and end-to-end tests

#### Implementation Details

- Documentation: create user guides, API docs, and examples
- Packaging: add feature flags and build configuration
- CLI integration: ensure proper help text and shell completion
- Migration support: document differences from other ACP clients
- Final testing: comprehensive integration tests and validation

#### Key Source Files

- `docs/acp-client/`
- `crates/ah-agents/README.md` (ACP client section)
- `crates/ah-cli/src/commands/agent/start.rs` (help text updates)

#### Verification

- [x] Documentation builds and links are valid
- [x] `just lint-specs` passes on all documentation
- [x] CLI help text is comprehensive and accurate
- [x] End-to-end integration tests pass with real ACP agents

## Outstanding Tasks After Milestones

- Define interoperability matrix with popular ACP agents (Claude Code, Continue, etc.)
- Add support for ACP over HTTP streaming transport when standardized
- Implement ACP federation for multi-agent coordination
- Add support for ACP session forking and branching
- Create ACP client plugins/extensions system
- Add telemetry and usage analytics
- Implement ACP client marketplace/registry integration

Once all milestones are implemented and verified, update this status document with:

1. Implementation details and source file references per milestone
2. Checklist updates (`[x]`) and remaining outstanding tasks
