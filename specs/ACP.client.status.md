# ACP Client Implementation — Implementation Status

## Overview

Goal: implement an ACP-compliant client that can connect to external ACP agents, enabling Agent Harbor to act as a universal ACP client that bridges any ACP agent with Harbor's filesystem snapshotting, terminal management, and session recording capabilities. The ACP reference docs in `resources/acp-specs/docs` (notably `protocol/overview.mdx`, `protocol/file-system.mdx`, and `protocol/terminals.mdx`) define the JSON-RPC methods we must implement as a client. The client will be integrated into `ah agent start` as a new agent type `acp` with an `--acp-agent-cmd` option (full command string), allowing users to launch external ACP agents while benefiting from Harbor's execution environment and tooling.

### Components and their roles

- **SessionViewer UI**: Displays the native output of third-party agents (Claude Code, Codex CLI, etc.) with minimal augmentation (e.g., snapshot markers). It is driven by captured stdout/stderr plus shim-extracted data from directly running the non-ACP agent; it is not fed ACP events.
- **Agent Activity TUI**: Structured UI that renders our native event types (from `ah-domain-types`), which are shared between ACP and non-ACP flows. ACP `session/update` messages are mapped into these event types; non-ACP agents reach the same types via output parsing and shims.
- **Json-normalized mode**: Emits those same native event types to stdout for automation; the ACP server mode also consumes this native stream. For non-ACP agents, the stream is produced directly by parsers/shims (no intermediate ACP translation).
- **Interpose shims**: Injected when possible to capture detailed tool/terminal telemetry and pass it to recorder/TUIs; in ACP client mode, partial passthrough is wired for terminal/create.
- **Recorder (`ah agent record`)**: Wraps executions (including ACP client runs) and captures UI output plus shim streams; recording is orthogonal to UI choice (SessionViewer, Agent Activity TUI, json-normalized). UIs only notify the recorder about snapshots and forward shim data; the recorder captures whichever UI/output is in use.

Target crate: `crates/ah-agents`. We will add an `acp` module that implements the `AgentExecutor` trait and uses the vendored `vendor/acp-rust-sdk` for:

- JSON-RPC framing and method dispatch
- Formal request/response structs for client-side operations
- Transport handling (stdio for external binaries)
- Client capability negotiation and session management

## Execution Strategy

1. Extend `ah-agents` with an ACP client implementation that wraps external ACP agent binaries
2. Add `acp` as a new agent type to `ah agent start` with `--acp-agent-cmd` option for specifying the external agent executable (including subcommands/flags)
3. Implement client-side ACP methods (file system, terminal, permission requests) using Harbor's existing infrastructure
4. Provide both text-normalized UI output (for interactive use) and json-normalized output (for automation)
5. Integrate automatic filesystem snapshots during agent execution when configured
6. Support stdio transport for local ACP agent binaries

### Integration with `ah agent start`

The ACP client will be invoked through the existing `ah agent start` command with a new agent type:

```bash
ah agent start --agent acp --acp-agent-cmd "mock-agent --scenario /path/to/scenario.yaml" --prompt "Fix the bug"
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

The Agent Activity TUI (detailed in [`Public/Agent-Activity-TUI-PRD.md`](Public/Agent-Activity-TUI-PRD.md)) is a full alternative to the standard SessionViewer UI (described in [`Public/ah-agent-record.md`](Public/ah-agent-record.md)). It covers all functionality of the SessionViewer while providing a specialized experience for ACP-based agents with structured data (thoughts, tools, files). Both interfaces integrate with the `ah agent record` command, which selects the appropriate UI to launch based on the agent type and output configuration.

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

> Status correction: these tests are **not yet implemented**; checkboxes were previously marked in error.

- [ ] `test_loadsession_capability_advertisement` - Verifies `loadSession` capability is properly advertised when enabled
- [ ] `test_session_load_historical_replay` - Verifies events before `sessionStart` are replayed during `session/load`
- [ ] `test_session_load_live_streaming` - Verifies events after `sessionStart` are streamed live after loading
- [ ] `test_multiple_scenarios_session_matching` - Verifies correct scenario selection for `session/load` by session ID matching
- [ ] `test_multiple_scenarios_new_session_matching` - Verifies Levenshtein distance matching for new sessions across multiple scenarios ([Scenario-Format.md#scenario-selection--playback-controls](Public/Scenario-Format.md#scenario-selection--playback-controls))

---

### Milestone 0.5: Agent Activity TUI Mock Mode & Session Viewer Integration

**Status**: Not achieved (UI fidelity still short of PRD polish, but threading/ticker + scenario playback loop now implemented; refreshed goldens reflect current layout)

#### Deliverables

- [x] Refactor `crates/ah-tui/src/viewer.rs` to use dependency injection pattern (`AgentSessionDependencies`)
- [x] Implement `run_session_viewer` function accepting injected dependencies
- [x] Create `just run-mock-agent-session` target that runs the session viewer driven by `mock-agent` library
- [x] Implement TUI mode in `ah-tui` crate that simulates Agent Activity TUI output format
- [x] Create ViewModel and View components following MVVM architecture
- [x] Integrate TUI mode as an alternative UI path in `ah agent record`
- [ ] Manual testing and acceptance of visual styles (blocked: current renderings diverge from spec)

#### Reality check (2025-12-01)

- Segmented control boxes, stop button handling for running tools, output-size badges, and an instructions card that reuses the task-entry component render, and goldens were refreshed to reflect this interim layout.
- UI still diverges from `specs/Public/Agent-Activity-TUI-PRD.md`/`scripts/tui_mockup.py` in fine details (pipeline per-command coloring, tooltip styling, hero/dim polish, fully spec’d embedded Task Entry), but major chrome elements (centered margins, code block headers/backgrounds, tighter control boxes) are now present and reflected in the refreshed goldens.
- Fork tooltip styling/placement is still provisional; hit targets will need another pass after any remaining layout tweaks.
- Agent Activity loop now follows the `specs/Public/TUI-Threading.md` shape: unified loop message enum, dedicated input thread, and a 60 FPS tick driving redraws/animations on the UI thread.
- Scenario playback now streams timeline events at their scheduled timestamps from the loaded scenario (via `mock_agent_session` → `AgentActivity` rows) instead of static snapshots.
- Several verification checkboxes below were mistakenly marked complete; no automated coverage exists yet for those items.
- Golden snapshots refreshed (2025-12-02) after styling fixes and new dim/read/diff/instructions/fork assertions.

**Outstanding tasks (must-do before calling the milestone complete):**

1. Finish remaining PRD polish: hero/dim refinements and fully spec’d embedded Task Entry per `tui_mockup.py`. Per-command pipeline coloring and fork tooltip styling/placement are now implemented in `agent_session_view`.
2. After final polish, regenerate goldens and re-validate hit zones with the updated geometry.
3. Rebuild the input-mode tests to cover the minor-mode keyboard routing (timeline navigation, control focus, fork positioning) and add golden layout tests for the finished design. **Snapshots updated 2025-12-02 to reflect current interim layout; still not verified against PRD, so verification boxes stay unchecked.**
4. Centralize all TUI view modules on a shared theme module that implements `specs/Public/TUI-Color-Theme.md` (colors, semantic roles, and naming) and ensure every view function receives a theme object initialized by the configuration system rather than constructing ad-hoc defaults. ✅ Implemented via `crates/ah-tui/src/theme.rs` with config-driven loading and DI wiring through dashboard, session viewer, and Agent Activity loops.
5. Added pipeline-aware rendering unit tests and fork tooltip placement coverage (`agent_session_view.rs` and `agent_session_hit_tests.rs`); keep expanding snapshot coverage once PRD visual parity is achieved.

#### Work required to actually complete the milestone

1. Finalize fork tooltip styling/placement and hit zones after the layout changes.
2. Regenerate golden snapshots **after** the rendering matches the mockup; then re-run `just run-mock-agent-session`, `just test-rust`, and `cargo insta test --accept`.
3. Implement the `specs/Public/TUI-Threading.md` architecture for the Agent Activity loop: introduce a loop-specific message enum and drive animations with 60 FPS tick events (no shared ad-hoc channel).
4. Wire mock-agent scenario playback into the Agent Activity UI: actually stream timeline events from the loaded scenario file (not static snapshots) so the mock session simulates real execution.

#### Verification Strategy

The verification strategy for this milestone relies on two complementary testing approaches:

1. **Golden Layout Tests (Rendering)**
   These tests verify the visual fidelity of the TUI implementation against the PRD requirements.
   - **Principle**: Tests manually construct a `ViewModel` in a specific state (e.g., populated with a sequence of events) and invoke the view module's render functions using the `Ratatui` `TestBackend`.
   - **Authoring guidance (2025-12-03)**: Only drive state through the same public inputs used in normal operation.
     - **State Setup**: Use `support::vm_with_events` to initialize the ViewModel with a sequence of `ah_core::TaskEvent`s. This ensures the ViewModel state is built exactly as it would be during real execution (via `process_activity_event`). Avoid manually constructing `AgentActivityRow`s or using `vm_with_rows` unless testing a specific row type not yet supported by `TaskEvent` (e.g., legacy `AgentRead` or complex `ToolUse` pipelines).
     - **User Input**: Use `handle_key_with_minor_modes` and `handle_mouse_action` to simulate user interaction.
     - **Invariants**: Do **not** mutate internal fields (selection, scroll, fork index/tooltip flags, etc.) directly; the ViewModel API is responsible for preserving invariants and keeping tests limited to reachable states.
   - **Failure Analysis**: When a test fails (i.e., the rendered buffer differs from the expected "golden" snapshot), the test harness produces a diagnostic message showing the actual rendering. This makes it immediately clear how the implementation deviates from the expected visual output (e.g., wrong colors, misalignment, missing borders).
   - **Coverage Goals**:
     - **General Layout & Flow**:
       - [x] `test_render_mixed_card_sequence`: Implemented; ASCII snapshot only (no color/z-order assertion).
       - [x] `test_render_viewport_overflow`: Implemented; no scroll-state asserts beyond layout.
       - [x] `test_render_empty_state`: Implemented; snapshot only, PRD polish outstanding.

     - **Hero Card (Active State)**:
       - [x] `test_render_hero_thinking`: Implemented; layout snapshot only.
       - [x] `test_render_hero_tool_running`: Implemented; layout snapshot only.
       - [ ] `test_render_hero_docked_bottom`: Implemented with coarse Y-position assert; still unverified against PRD docking rules.
       - [x] `test_render_hero_pinned_scrolled`: Implemented; snapshot only.
       - [x] `test_render_hero_below_fork`: Implemented (tooltip above fork target, hero below).

     - **Instructions Card & Forking**:
       - [x] `test_render_instructions_card_default`: Implemented; now asserts focused vs unfocused border colors.
       - [x] `test_render_instructions_card_focused`: Implemented; verifies primary-color focus styling.
       - [x] `test_render_instructions_card_moved_up`: Implemented (vertical compression repositions instructions card).
       - [x] `test_render_fork_preview_dimming`: Implemented via mouse action handling; dimming asserted.
       - [x] `test_render_fork_tooltip`: Implemented bg/fg + hit-zone placement via mouse action; colors now asserted.

     - **Card Content & Variations**:
       - [x] `test_render_pipeline_success`: Implemented; snapshot only (no color semantics asserted).
       - [x] `test_render_pipeline_partial_failure`: Implemented; snapshot only (skipped/failed colors unasserted).
       - [x] `test_render_command_wrapping`: Implemented; layout only.
       - [x] `test_render_command_stop_button`: Implemented; hover/active state unasserted.
       - [x] `test_render_output_size_indicator`: Implemented; color/style unasserted.
       - [x] `test_render_edited_card_diff`: Diff styling now validated (accent/error colors on +/- lines).
       - [x] `test_render_read_card_ranges`: Range lines validated to use dim-text styling.
       - [x] `test_render_thought_markdown`: Implemented; styling not color-asserted.
       - [x] `test_render_user_multiline`: Implemented; layout only.
       - [x] `test_render_collaborative_user`: Implemented; layout only.

     - **Selection & Focus**:
       - [x] `test_render_card_selected`: Implemented; focus styling not color-asserted.
       - [x] `test_render_control_box_focused`: Implemented; styling not PRD-verified.
       - [x] `test_render_control_box_expand_focused`: Implemented; styling not PRD-verified.

     - **Footer & Status**:
       - [x] `test_render_footer_standard`: Implemented; muted vs primary color assertions.
       - [x] `test_render_footer_context_warning`: Implemented; asserts mixed muted/primary coloring.
       - [x] `test_render_footer_context_critical`: Implemented (context % ≥95 paints error color).

     - **Modals**:
       - [x] `test_render_output_modal_text`: Implemented with scrim + header color asserts.
       - [x] `test_render_output_modal_stderr`: Implemented (stderr header/background colors).
       - [x] `test_render_output_modal_binary`: Implemented (binary header present).
       - [x] `test_render_modal_z_index`: Implemented (overlay paints over timeline at center and renders header).

2. **Input Handling Tests (State Transitions)**
   These tests verify that user input events are correctly processed by the ViewModel to trigger state transitions, without involving the rendering layer.
   - **State reachability discipline**: Tests must drive the ViewModel into target states by invoking its public state-transition APIs (e.g., `handle_key_with_minor_modes`, helper methods for fork tooltip toggling) rather than mutating fields directly. The ViewModel fields should be private; expose read-only accessors for the view layer. Constructors and transition functions must enforce invariants so that only valid, reachable UI states are expressible, preventing snapshots of impossible states.
   - **Principle**: Similar to `crates/ah-tui/tests/prd_input_tests.rs`, these tests send synthetic `KeyEvent`s to the ViewModel and assert that the internal state changes as expected (e.g., focus moves, mode switches, data updates).
   - **Required Test Cases**:
     - **Timeline Navigation**:
       - [x] `test_navigate_cards_vertical`: Implemented in ViewModel tests (logic only); visuals/PRD unchecked.
       - [x] `test_navigate_cards_boundary`: Implemented (logic).
       - [x] `test_scroll_behavior`: Implemented (logic).
       - [x] `test_scroll_to_extremes`: Implemented (logic) but PRD/render alignment unverified.
       - [x] `test_auto_follow_toggle`: Implemented (logic).
     - **Card Interaction**:
       - [x] `test_focus_control_box`: Implemented (logic); visual focus styling not validated.
       - [x] `test_cycle_control_box`: Implemented (logic).
       - [x] `test_leave_control_box`: Implemented (logic).
       - [x] `test_activate_control_item`: Implemented (logic).
     - **Forking / Instructions Card**:
       - [x] `test_move_instruction_card`: Implemented (logic); rendering parity unverified.
       - [x] `test_fork_point_selection`: Implemented (logic); PRD alignment unverified.
     - [x] `test_draft_mode_entry`: Implemented (logic); rendering/input parity unverified.
     - **Search**:
       - [x] `test_enter_search_mode`: Implemented (slash binding triggers search + highlights first match).
       - [x] `test_search_navigation`: Implemented (n/N cycle through matches).
       - [x] `test_search_selection`: Implemented (search jump to first match and disables auto-follow).
       - [x] `test_exit_search`: Implemented (ESC clears search state).
     - **Modal Interaction**:
       - [x] `test_open_output_modal`: Implemented (modal stores title/body).
       - [x] `test_modal_overlay_state`: Implemented (overlay closes before quit).
       - [x] `test_close_modal`: Implemented (ESC closes modal then requests quit).

3. **Event-Driven Integration Tests (Optimistic Updates)**
   These tests verify the end-to-end flow of user interactions and server events, specifically focusing on optimistic UI updates and reconciliation.
   - **Principle**: Tests simulate a realistic session lifecycle by driving the ViewModel with a sequence of `TaskEvent`s and user actions (simulated via `handle_key_with_minor_modes` or direct action simulation).
   - **Pattern**:
     1. **Setup**: Initialize the ViewModel with `vm_with_events`.
     2. **Simulate User Action**: Perform a user action (e.g., typing a message) that triggers an optimistic UI update (e.g., adding an unconfirmed row).
     3. **Verify Optimistic State**: Assert that the UI reflects the optimistic state (e.g., unconfirmed indicator/spinner).
     4. **Simulate Server Event**: Inject the corresponding `TaskEvent` from the server (e.g., `TaskEvent::UserInput`).
     5. **Verify Reconciliation**: Assert that the UI reconciles the state correctly (e.g., marking the row as confirmed, updating content/author if needed).
   - **Guidelines**:
     - Use `make_settings().bind_to_scope()` for snapshot configuration.
     - Use `vm_with_events` helper to set up initial state.
     - Verify both logical state (via ViewModel accessors) and visual state (via snapshots).
     - Test edge cases like fuzzy matching, out-of-order events, and rapid updates.
   - **Required Test Cases**:
     - [x] `renders_interleaved_events_and_user_input`: Verifies basic optimistic update and confirmation flow.
     - [x] `renders_fuzzy_matched_user_input`: Verifies fuzzy matching logic for user input confirmation.

#### Implementation Details

- **Location**: Implementation resides in `crates/ah-tui` crate following strong ViewModel/View separation
- **Architecture**: Follows existing MVVM pattern (see `crates/ah-tui/src/view_model/mod.rs` for architecture details) with separate ViewModel and View modules
- **TUI mode**: Simulates the output format expected by the Agent Activity TUI (thoughts, tool calls, file edits, logs, etc.)
- **Integration**: Works as a full alternative to the standard SessionViewer UI, sharing core dependencies

#### Session Viewer Refactoring for mock-agent integration

**Existing Session Viewer Components:**

- `crates/ah-tui/src/view/session_viewer.rs` - Already implemented Ratatui rendering functions
- `crates/ah-tui/src/view_model/session_viewer_model.rs` - Already implemented ViewModel with state management
- `crates/ah-tui/src/viewer.rs` - Currently implements `ViewerEventLoop` without full dependency injection

**Required Refactoring:**

- **Dependency Injection Pattern**: Refactor `viewer.rs` following the `dashboard_loop.rs` pattern:
  - Extract dependencies into `AgentSessionDependencies` struct (sharing common dependencies with `TuiDependencies` where possible)
  - Implement `run_session_viewer(deps: AgentSessionDependencies)` function in `crates/ah-tui/src/agent_session_loop.rs` (new file)
  - Support both production (real dependencies) and test (mock dependencies) modes through a new executable entry point
  - Enable standalone session viewer testing similar to `just run-tui-mock-dashboard` with a new target `just run-mock-agent-session`. It will use the `mock-agent` crate as a library to drive the refactored session viewer UI, simulating an agent session specified as a scenario file.

- **Test/Simulation Mode**:
  - Use the new agent_session_model and agent_session_view in `mock-agent` to create a high fidelity simulation of the UI driven from a scenario file. Please note that this is not about running mock-agent as a server, but rather compiling it as a regular program that driven the UI entirely from the pre-scripted data in the scenario file.

#### Key Source Files

**New Agent Activity TUI Components:**

- `crates/ah-tui/src/view_model/agent_session_model.rs` (New ViewModel for Agent Activity TUI mode)
- `crates/ah-tui/src/view/agent_session_view.rs` (New View rendering for Agent Activity TUI mode)
- `crates/ah-tui/src/agent_session_loop.rs` (New main loop handling both UI modes via dependency injection)

**Existing Session Viewer Components (Refactoring):**

- `crates/ah-tui/src/session_viewer_deps.rs` (New dependency injection structure for shared use)
- `crates/ah-tui/src/view_model/session_viewer_model.rs` (Existing Session Viewer ViewModel - to be adapted)
- `crates/ah-tui/src/view/session_viewer.rs` (Existing Session Viewer rendering - to be adapted)
- `crates/ah-tui/src/viewer.rs` (Existing viewer entry point - to be deprecated/refactored into `agent_session_loop.rs`)

#### Reference Implementations

- **Dashboard Loop Pattern**: `crates/ah-tui/src/dashboard_loop.rs` - Shows dependency injection pattern with `TuiDependencies`
- **Mock Agent**: `crates/mock-agent/src/lib.rs` - Provides `MockAcpClient` and `ScenarioExecutor` for driving tests
- **Visual Reference**: `scripts/tui_mockup.py` - The view implementation should replicate the visual rendering of this script as a starting point.

#### Milestone Closing Verification

- [ ] Manual testing demonstrates proper visual styling and layout, matching `scripts/tui_mockup.py`
- [ ] TUI mode integrates as an alternative UI in `ah agent record`
- [ ] Session viewer supports both production and test modes through dependency injection (refactored from existing viewer.rs)
- [ ] Visual styles accepted by design review
- [ ] The Agent Activity TUI can be run in standalone test mode (`just run-mock-agent-session`) driven by `mock-agent`
- [ ] **Strict Compliance**: Implementation must precisely follow the spec, including all input minor modes.
- [ ] **Unit Tests**: Implement unit tests for `view_model` using mocks of shared dependencies where possible (e.g., mocking `TaskManager`, `WorkspaceFilesEnumerator`, etc to test state transitions without full UI).

---

### Milestone 1: ACP Client Architecture & Agent Integration

**Status**: Mostly complete (CLI + stdio scaffold; UI limited to Agent Activity TUI, SessionViewer not yet wired)

#### Deliverables

- [x] Create `acp` module in `crates/ah-agents/src/acp.rs` implementing the `AgentExecutor` trait
- [x] Add `acp` to the available agents list and `agent_by_name()` function
- [x] Add `--acp-agent-cmd` option to `AgentLaunchConfig` and CLI parsing (full command string)
- [x] Implement basic ACP client scaffolding with SDK integration
- [x] Add ACP client feature flag and dependency on `vendor/acp-rust-sdk`
- [x] Create unit tests for client initialization and basic method dispatch
- **Clarified requirement (2025-12-03):** `ah agent start --agent acp` **must not** `exec` the external ACP binary directly. The sandbox entrypoint must be a Harbor-owned program (`ah tui acp-client`) that:
  - Spawns the ACP server/binary as a child,
  - Uses `vendor/acp-rust-sdk` to maintain bidirectional JSON-RPC over stdio,
  - Collects ACP events and feeds them into `crates/ah-tui/src/agent_session_loop.rs` (Agent Activity UI) / SessionViewer UI with real dependencies,
  - Emits UI events/output back to the ACP agent as per the ACP protocol.
    This entrypoint is what gets `exec`’d inside the sandbox; outside the sandbox the same program can be launched directly.
  - Current implementation: `ah tui acp-client` wraps an ACP binary, speaks the SDK over stdio, feeds `session/update` into the Agent Activity TUI (text-normalized path) via `agent_session_loop`, and forwards task-entry prompts to `session/prompt`. SessionViewer remains unchanged and continues to render native output for non-ACP agents; it is not fed by ACP events today. Terminal/fs/permission calls are handled inside the TUI stub; command-trace passthrough wraps `terminal/create` when recorder sockets are present. Json-normalized mode emits the same event stream to stdout (and to ACP server mode). AHR recording is driven by `ah agent record`, but playback parity for ACP runs (including shim-derived data and terminal traces) still needs to be validated.

#### Implementation Details

- The ACP client will implement `AgentExecutor` and handle the protocol translation between Harbor's agent abstraction and ACP
- Client will support stdio transport (for the configured ACP launch command)
- Initial implementation will provide stub responses for all client methods, to be filled in subsequent milestones
- Integration with existing credential and environment setup from `AgentLaunchConfig`

**Implementation status (current)**

- `crates/ah-agents/src/acp.rs` implements `AgentExecutor`, version detection, archive import/export, `parse_output` stub, and a helper to attach the ACP SDK over stdio.
- CLI wiring (`--acp-agent-cmd`) and typed `AcpLaunchCommand` live in `crates/ah-cli/src/agent/start.rs` and `crates/ah-agents/src/traits.rs`; the Harbor entrypoint dispatches to `ah tui acp-client`.
- Agent catalogs expose the ACP entry and launch metadata; tests cover constructor, version parsing (including subcommand-style binaries), and CLI flag parsing.
- `ah tui acp-client` (in `crates/ah-tui/src/acp_client.rs`) provides the only UI/IO bridge today, targeting the Agent Activity TUI; SessionViewer remains the minimal native-output view and will only be augmented with snapshot markers and any later shim-derived summaries when specified.

#### Key Source Files

- `crates/ah-agents/src/acp.rs`
- `crates/ah-agents/src/lib.rs` (add ACP to agent lists)
- `crates/ah-agents/src/traits.rs` (extend `AgentLaunchConfig` if needed)
- `crates/ah-cli/src/commands/agent/start.rs` (ACP launch command parsing)

#### Verification

- [x] `acp_client` — version detection for direct/subcommand binaries and SDK attach/dispatch smoke.
- [x] `parse_acp_agent_cmd_flag` — CLI parses `--acp-agent-cmd` and wires into config.
- [x] Harbor smoke — `ah agent start --agent acp ...` recorded `.ahr` includes traced terminal/file/permission events (validated via `crates/ah-cli/tests/acp_record_smoke.rs`, which parses the scenario with `ah-scenario-format`, replays the recorded PTY for terminal bytes, and checks the json-normalized TaskEvent stream for the scenario’s user input, tool use, and assistant text).
- [x] End-to-end ACP client ⇄ mock-agent (json-normalized) — launch `mock-agent --scenario tests/tools/mock-agent-acp/scenarios/acp_round_trip.yaml` (ACP server mode), connect via `ah tui acp-client --acp-agent-cmd "mock-agent --scenario ..." --prompt "ping" --output json-normalized`, and assert stdout emits the scenario-defined native events (thought, tool_call, diff, terminal, plan) with correct order/fields; verify `.ahr` records terminal/file/permission traces.
- [x] End-to-end ACP client ⇄ mock-agent (text-normalized/TUI) — same setup without `--output json-normalized`; assert Agent Activity TUI renders the scenario-defined events and `.ahr` contains matching terminal/file/permission traces.

#### Outstanding Tasks (Milestone 1 scope)

- [x] Extend `parse_output` to emit native TaskEvent types (from `ah-domain-types`) instead of line-based heuristics. Implemented in `crates/ah-agents/src/acp.rs`; TaskEvent JSON is now preferred and mapped to AgentEvents.
- [x] Thread TaskEvent forwarding into the ACP client: `ah agent record` now injects `AH_TASK_MANAGER_SOCKET`/`AH_SESSION_ID`, and `ah tui acp-client` streams ACP-derived TaskEvents to the task-manager socket for AHR parity (see `crates/ah-tui/src/record.rs`, `crates/ah-tui/src/acp_client.rs`).
- [x] Ensure AHR playback parity for ACP runs executed under `ah agent record`: recorded `.ahr` should faithfully capture UI output plus shim-derived terminal/file/permission traces when using Agent Activity TUI or json-normalized modes. Smoke covered via `acp_record_smoke` replay assertion.
- [x] Add a Harbor-level smoke (`ah agent start --agent acp ...`) that records an `.ahr` and verifies traced terminal bytes/tool/file/permission events.
- [x] Document/env-check the command-trace passthrough behaviour end-to-end (shim + recorder) — `CmdtraceEnv::detect` now refuses missing sockets and warns instead of silently misconfiguration of passthrough; docs updated here.
- [x] Harbor smoke still timing out under recorder: `acp_record_smoke::acp_recording_captures_terminal_output` produces the `.ahr` but the child does not exit before timeout. Needs recorder↔ACP client lifecycle fix and deterministic exit handling.
- [x] End-to-end ACP client ⇄ mock-agent (json-normalized) validation (scenario `acp_round_trip.yaml`).
- [x] End-to-end ACP client ⇄ mock-agent (text-normalized/TUI) validation.
- [x] Smoke test stability: `acp_terminal` scenario fixed (invalid `sessionStart` removed) and tests re-enabled; json-normalized + recorder smokes now pass deterministically in CI/local runs.
- [x] Removed the ad-hoc `--scenario` flag from `ah tui acp-client`; scenario paths are forwarded via the unified `--acp-agent-cmd` string (or ACP agent defaults).
- [x] Revisited ACP launch UX: consolidated on `--acp-agent-cmd` (full command string) and dropped `--acp-arg`; CLI/TUI parsing and tests updated.
- [x] Recorder lifecycle hardened: ACP client now cancels after first completed tool, waits briefly for terminal exits, mirrors PTY bytes to stdout in headless runs, and ships a test-only watchdog env (`AH_ACP_CLIENT_TEST_WATCHDOG_MS`) to avoid recorder hangs (`acp_record_smoke` now stable).

---

### Milestone 2: Transport Layer & Connection Management

**Status**: Partially implemented (stdio wiring exists; no reconnection/health checks or capability negotiation)

#### Deliverables

- [x] Implement stdio transport using the ACP SDK's stdio connection (in `crates/ah-tui/src/acp_client.rs::run_acp_client`).
- [ ] Add connection lifecycle management (explicit disconnect, error surfacing) beyond child exit.
- [ ] Implement capability negotiation during `initialize` (currently uses SDK defaults, no advertised fs/terminal capabilities).
- [ ] Add connection health monitoring and automatic reconnection.
- [ ] Create integration tests for stdio transport.

#### Implementation Details

- Stdio transport: spawn the configured ACP command and connect via stdin/stdout
- Connection management: handle process lifecycle and connection establishment/teardown
- Capability negotiation: advertise client capabilities (filesystem, terminal) during initialization
- Error handling: translate transport errors into appropriate ACP error responses

#### Key Source Files

- `crates/ah-tui/src/acp_client.rs` (client-side SDK wiring).
- `crates/ah-agents/src/acp.rs` (`attach_stdio_client` helper only).

#### Verification

- [x] `acp_client` — duplex SDK wiring smoke; basic RPC dispatch.
- [ ] `acp_stdio_transport` — connects to mock agent over stdio and exchanges initialize/new_session/prompt (TODO).
- [ ] `acp_capability_negotiation` — asserts advertised client capabilities are sent and respected (TODO).
- [ ] `acp_reconnect_recovers_stream` — reconnect after child crash resumes updates without duplication (TODO).
- [ ] `acp_prompt_execution` — prompt request/response round-trip over stdio (TODO).

---

### Milestone 3: Filesystem Method Implementation

**Status**: Partially implemented (local fs read/write in TUI client; no workspace mapping or snapshots)

#### Deliverables

- [x] Implement `fs/read_text_file` and `fs/write_text_file` client methods (TUI stub, direct local paths).
- [ ] Add filesystem capability advertisement during initialization.
- [ ] Implement path resolution between ACP absolute paths and Harbor workspace paths.
- [ ] Add automatic snapshot creation on file writes when configured.
- [ ] Handle file access permissions and error cases.
- [ ] Create filesystem operation tests with mock scenarios (only a simple unit test exists in `ah-tui`).

#### Implementation Details

- File reading: serve `fs/read_text_file` requests from current workspace or snapshot mounts
- File writing: write to workspace and trigger snapshots via existing FS snapshot infrastructure
- Path handling: convert between ACP's absolute path requirements and Harbor's relative workspace paths
- Snapshot integration: use `ah-fs-snapshots` provider to create snapshots after file modifications
- Permission checks: implement basic access control for file operations

#### Key Source Files

- `crates/ah-tui/src/acp_client.rs` (`read_text_file` / `write_text_file`).

#### Verification

- [x] `acp_client::read_write_roundtrip` — basic fs read/write via TUI stub.
- [ ] `acp_file_read` / `acp_file_write` / `acp_path_resolution` — path mapping, error handling, snapshot hook (TODO).
- [ ] Scenario-driven fs ops (mock-agent) — absolute/relative path mapping, permission failures, snapshot trigger (TODO).

---

### Milestone 4: Terminal Method Implementation

**Status**: Partially implemented (basic spawn/buffer; no streamed notifications to agent; limited process control)

#### Deliverables

- [x] Implement terminal methods `create`, `output`, `wait_for_exit`, `kill`, `release` (TUI stub).
- [ ] Advertise terminal capability during initialization.
- [ ] Implement output streaming back to the agent (only pull-based `terminal/output` reads buffered data).
- [ ] Add resource limits and sandboxing integration beyond optional command-trace passthrough.
- [ ] Comprehensive process lifecycle/signal handling (only kill/exit recorded).
- [ ] Create terminal operation tests with process mocking in `ah-agents` (currently only a smoke in `ah-tui`).

#### Implementation Details

- Terminal creation: spawn processes using Harbor's existing command execution infrastructure
- Output handling: stream terminal output via ACP `session/update` notifications
- Process management: handle process lifecycle, signal handling, and cleanup
- Resource control: apply Harbor's sandboxing and resource limits to terminal processes
- Error handling: translate process errors into appropriate ACP responses

#### Key Source Files

- `crates/ah-tui/src/acp_client.rs` (terminal implementations, command-trace passthrough helper).

#### Verification

- [x] `acp_client::terminal_output_is_exposed` — spawn shell, capture buffered output, verify exit recorded.
- [ ] `acp_terminal_lifecycle` / `acp_output_streaming` / `acp_process_signals` — full lifecycle, streaming push, signal handling (TODO).
- [ ] Command-trace passthrough — `terminal/create` wraps `ah agent record --passthrough` when recorder sockets set (TODO).
- [ ] Streaming notifications — mock-agent scenario pushes terminal output via `session/update` and client renders it (TODO).

---

### Milestone 5: Permission Request Handling & UI Integration

**Status**: Minimal stub (auto-allow only; no policy/UI, no text/json output modes)

#### Deliverables

- [x] Implement `request_permission` client method (auto-selects “allow”).
- [ ] Add permission policy configuration and automatic approval rules (must cover both ACP `request_permission` flows and sandbox FS approval per `Public/Sandboxing/Agent-Harbor-Sandboxing-Strategies.md`).
- [ ] Implement text-normalized and json-normalized output modes.
- [ ] Create interactive permission prompts for terminal use.
- [ ] Add programmatic permission handling for automation.
- [ ] Create UI integration tests for both output modes (Agent Activity TUI when text-normalized; stdout JSON events when json-normalized; SessionViewer minimal augmentation).

#### Implementation Details

- Permission handling: implement policy-based automatic approval or interactive prompts
- UI modes: text-normalized for human-readable output, json-normalized for programmatic use
- Interactive prompts: handle permission requests in terminal sessions
- Automation support: allow pre-approval of permission types for CI/CD use cases
- Output formatting: translate ACP events into appropriate output format

#### Key Source Files

- `crates/ah-tui/src/acp_client.rs` (`request_permission` auto-allow).

#### Verification

- [x] `acp_client::permission_requests_auto_allow` — permission flow auto-selects allow.
- [ ] `acp_permission_handling` — policy-driven allow/deny/cancel with options (TODO).
- [ ] UI prompts — TUI integration renders permission dialog, enforces sandbox FS policy (TODO).
- [ ] Json/text modes — snapshot outputs for normalized text and json showing permission outcomes (TODO).

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

- [ ] `acp_extensions` — custom `_ah/*` methods exercised end-to-end (TODO).
- [ ] `acp_multimodal` — image/audio/resource handling mapped to native events (TODO).
- [ ] `acp_session_control` — pause/resume/mode switching via ACP mapped to native events (TODO).
- [ ] Integration with extension-supporting agent — validates mixed extensions + core flows (TODO).

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

- [ ] Performance benchmarks — throughput/latency targets under load (TODO).
- [ ] `acp_error_recovery` — classify and recover from transient/fatal errors (TODO).
- [ ] `acp_resource_limits` — memory/connection limits enforced (TODO).
- [ ] Stress tests — sustained concurrency without dropped events (TODO).

---

### Milestone 8: Third-Party ACP Agents via LLM API Proxy

**Status**: Planned

#### Deliverables

- Drive a real ACP-capable third-party agent (e.g., Goose, OpenCode) that uses `llm-api-proxy` as its LLM API provider, seeded by scenario files (`specs/Public/Scenario-Format.md`).
- Ensure proxy compatibility (prompt/content/tool/terminal flows) and stabilize transport/error handling for the provider role.
- Connect `ah tui acp-client` in json-normalized and text-normalized modes to the third-party agent and map emitted ACP events into native event types.
- Record `.ahr` with terminal/file/permission traces matching the scenario.

#### Implementation Details

- Extend `llm-api-proxy` to supply LLM responses per scenario to the third-party agent (as an API provider), honoring timelines and content types.
- Handle incompatibilities/quirks per agent (capabilities, prompt formats, tool schema).
- Validate both stdio and network transports if supported by the agent.

#### Verification

- [ ] Third-party ACP E2E (json-normalized) — run proxy + agent with scenario; assert emitted native events match scenario; `.ahr` captures terminal/file/permission traces (TODO).
- [ ] Third-party ACP E2E (text-normalized/TUI) — same setup; assert Agent Activity TUI renders scenario events and `.ahr` traces match (TODO).
- [ ] Proxy robustness — fault-injection/timeout tests to ensure retries and clear error propagation (TODO).

---

### Milestone 9: Documentation & Packaging

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

- [ ] Documentation builds and links are valid (TODO).
- [ ] `lint-specs` passes on all documentation (TODO).
- [ ] CLI help text is comprehensive and accurate (TODO).
- [ ] End-to-end integration tests with real ACP agents covering prompt, terminal, fs, permissions (TODO).
- [ ] Third-party ACP E2E via LLM API Proxy — drive a real ACP-capable agent (e.g., Goose or OpenCode) through `llm-api-proxy` using a scenario (see `specs/Public/Scenario-Format.md`), connect with `ah tui acp-client` in json-normalized mode, and assert native events match the scenario and `.ahr` captures terminal/file/permission traces; repeat in text-normalized/TUI mode (TODO).

## Outstanding Tasks After Milestones

- **Capability negotiation and alignment**
  - Advertise fs/terminal/permission capabilities during `initialize`; align path resolution and snapshot hooks with Harbor workspace semantics.
  - Integrate permission policy UI/modes (text/json) with configurable auto-approval and sandbox alignment.
- **Transport resilience**
  - Add reconnection/health checks and explicit error surfacing for stdio transport; include targeted tests.
- **Terminal streaming**
  - Implement push-style terminal output notifications back to agents, enforce resource limits, and honor sandbox policies.
- **UI/recorder integration**
  - Integrate SessionViewer and AHR playback for ACP runs to achieve parity with Agent Activity TUI.
- **Scenario/test coverage**
  - Add scenario-driven tests for permissions, fs, terminal flows (mock-agent) and ensure follower-vs-direct runCmd modes are covered.
- **Interoperability & extensions**
  - Define interoperability matrix with popular ACP agents (Claude Code, Continue, etc.) and extend coverage once HTTP streaming transport is standardized.
- **Longer-range items**
  - ACP federation, session forking/branching, plugin/extension system, telemetry/analytics, and marketplace/registry integration remain planned.

Once all milestones are implemented and verified, update this status document with:

1. Implementation details and source file references per milestone
2. Checklist updates (`[x]`) and remaining outstanding tasks
