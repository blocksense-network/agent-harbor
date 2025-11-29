# Scenario Format Extensions — Implementation Status

## Overview

Goal: Extend the Scenario Format (`specs/Public/Scenario-Format.md`) to support comprehensive ACP (Agent Client Protocol) agent testing, including capability negotiation, rich content handling, and MCP server configurations. The extensions enable the mock-agent crate to simulate different ACP agent profiles and handle complex content scenarios for thorough protocol compliance testing.

Target: `crates/ah-scenario-format` (existing crate) and `specs/Public/Scenario-Format.md` (documentation). Legacy timeline support has been removed; scenarios must use the structured format (`llmResponse`/`userInputs` objects, `relativeTime`, `baseTimeDelta`) and will be rejected otherwise.

Progress update (2025-11-28):

- ✅ Rules resolved during load with recursive merging; undefined symbols skip; symbol table API and env/CLI injection (`AH_SCENARIO_DEFINES`, `--scenario-define`) added for runners and proxy/test servers.
- ✅ Agent permission schema/docs aligned; SetModel guard enforced at load; MCP transport capability check tightened for HTTP/SSE-style servers.
- ✅ `_meta` validated on AgentFileReads/AgentPermissionRequest and surfaced through playback/rest-server logs.
- ✅ Loader now rejects legacy shapes (`events`/`assertions`, non-mapping timeline items) before deserialization.
- ✅ Mock-agent simulation timing uses a shared clock respecting relativeTime/baseTimeDelta.
- 🔜 LoadSession support, richer transport validation (per-server typing/SSE surfacing), rich content extensions, and E2E runner wiring remain outstanding.

## Test Strategy

- **Unit tests** for new ACP-specific parsing and validation live in `crates/ah-scenario-format/src/lib.rs` and `crates/ah-scenario-format/tests/`
- **Integration tests** in `crates/mock-agent/tests/` verify that scenarios with ACP extensions load and execute correctly
- **Backward compatibility tests** ensure existing scenarios continue to work unchanged
- All tests run via `just test-rust`

## New Feature Deliverables

### LoadSession Support

- [ ] Implement `sessionStart` event parsing and timeline boundary logic
  - Parse `sessionStart` as timeline boundary marker
  - Separate historical events (before `sessionStart`) from live events (after `sessionStart`)
  - Ensure events before boundary are replayed during `session/load`
  - ✅ Loader enforces capability/boundary alignment and exposes `partition_by_session_start` helper; playback wiring still pending

- [ ] Add multiple scenario file support to mock-agent
  - Support loading multiple scenario files or directories
  - Implement session ID matching for `session/load` operations
  - Integrate Levenshtein distance matching for new session selection

- [ ] Implement session loading state management
  - Track which events belong to historical vs live phases
  - Ensure proper event streaming after `session/load` completion
  - Handle scenario transitions and state consistency

- [ ] Add loadSession capability validation
  - Verify `loadSession` capability advertisement when enabled
  - Validate session ID format and scenario matching logic
  - Test error handling for invalid session load requests

### Rules and Conditional Configuration

- [x] Apply `rules` during scenario load/playback
  - Rules resolved recursively with merging; undefined symbols skip the rule
- [x] Wire symbol definition/passing from CLI/test runners (`--define` / `--scenario-define`) and env into loader
- [x] Document symbol usage and merging precedence

### Meta Field Support

- [x] Preserve and validate `_meta` on AgentFileReads and AgentPermissionRequest; carried through playback
- [x] Ensure downstream consumers (rest-server logs) surface `_meta`; mock-agent remains passthrough

### Test Runner Integration

- [ ] Implement test runner interpretation of `acp` configuration section
  - Read `acp.capabilities` from scenario files to determine mock-agent launch parameters
  - Map scenario capabilities to appropriate CLI flags for mock-agent process launch
  - Handle capability override precedence (CLI parameters override scenario settings)
  - Validate that scenario capabilities are compatible with requested test behavior

- [ ] Implement multiple scenario file loading and selection
  - Support loading multiple scenario files or directories with glob patterns
  - Implement Levenshtein distance matching for new session scenario selection
  - Implement session ID matching for loadSession scenario selection
  - Provide fallback behavior when no matching scenarios are found

- [ ] Add test runner scenario execution coordination
  - Coordinate between scenario playback and mock-agent process lifecycle
  - Handle scenario `sessionStart` boundaries for loadSession functionality
  - Synchronize timeline events with mock-agent ACP message handling
  - Ensure proper cleanup and teardown of mock-agent processes

### ACP Configuration Section

- [x] Enforce ACP validation during load (including setModel unstable guard; MCP server name/command/env validation; transport gating for missing http/sse)
- [ ] Deep transport/capability alignment (per-server transport typing, SSE warnings surfaced to callers)
- [ ] Preserve meta capability extensions end-to-end

### Rich Content Support

- [x] Extend `userInputs` to support rich content blocks, `_meta`, and `expectedResponse` for prompt assertions

- [ ] Extend timeline `assistant` events for rich content
  - Parse content block objects with timestamp/content structure (new format only, no backwards compatibility)
  - Support all ACP content types in scenario responses including diff and plan content blocks
  - Maintain timing precision for multi-part content

- [ ] Implement diff content type parsing and validation
  - Parse `type: "diff"` with `path`, `oldText`, `newText` fields
  - Validate absolute file paths and content fields
  - Support diff content in both prompts and responses
  - ✅ Loader enforces absolute paths for diff content; runtime wiring pending

- [ ] Add rich content validation
  - Validate content block structure against ACP specification
  - Ensure proper MIME types for image/audio content
  - Validate relative file paths for image/audio resources
  - Verify referenced files exist and are readable at scenario load time
  - Validate plan entry structure (content, priority, status fields)
  - Handle mixed content types in single messages
  - ✅ Loader validates image/audio path or data presence, resolves relative paths against the scenario file, and enforces plan entry enums
  - ✅ Gate assistant (response) content types against advertised promptCapabilities to mirror ACP negotiation

- [ ] Implement scenario file organization guidelines
  - Support `images/` and `audio/` subdirectories alongside scenario files
  - Resolve relative paths from scenario file's directory
  - Provide clear documentation for expected directory structure

### ACP Timeline Events

- [ ] Implement `initialize` timeline event
  - Parse client capabilities and info from scenario
  - Support custom capabilities via `_meta` fields
  - Support expected response validation with meta capabilities
  - Enable capability negotiation testing including extensions

- [ ] Map `userInputs` to ACP `session/prompt`
  - Support rich content prompts with images/audio/resources
  - Support `_meta` fields in prompt requests
  - Validate `expectedResponse` structure (stopReason, usage, sessionId)

- [ ] Add meta field support to timeline events
  - Parse `_meta` fields in all ACP timeline events
  - Preserve meta fields during scenario playback
  - Support extension testing in timeline events

- [ ] Add ACP extension/custom notification coverage
  - Represent `_`-prefixed ACP notifications/methods in timeline schema
  - Preserve `_meta` on extension messages end-to-end
  - Provide examples/tests for custom extension notifications

### Backward Compatibility

- [x] Reject legacy timeline shapes explicitly in loader; migration of any remaining fixtures still pending
- [ ] Graceful handling of unknown ACP fields (forward compatibility)

### Documentation & Examples

- [ ] Update `specs/Public/Scenario-Format.md` with ACP extensions
  - Document new `acp` configuration section
  - Document rich content formats in timeline events
  - Provide comprehensive ACP testing example

- [ ] Create example ACP scenario files
  - Capability negotiation test scenario
  - Rich content handling test scenario
  - MCP server configuration test scenario

### Integration with Mock-Agent

- [ ] Update mock-agent to consume ACP scenario configuration
  - Use `acp.capabilities` for agent capability advertisement including meta extensions
  - Use `acp.cwd` and `acp.mcpServers` for session setup
  - Support rich content in prompts and responses including plan content blocks
  - Handle `_meta` fields in all ACP message types

- [ ] Implement scenario-driven ACP session flow
  - Map existing scenario events to ACP protocol:
    - `userInputs` events → Client `session/prompt` calls
    - `userCancelSession` events → Client `session/cancel` notifications
    - `llmResponse` events → Agent `session/update` notifications (responses, tool calls, plans)
    - `agentToolUse` events → Client tool call requests
    - `agentEdits` events → Client `fs/write_text_file` calls
    - `runCmd` events → Client terminal method calls
    - `readFile` events → Client `fs/read_text_file` calls
  - Handle rich content in `session/prompt` and `session/update` events including plan updates
  - Preserve and pass through `_meta` fields in bidirectional mapping
  - Validate responses against scenario expectations including meta content
  - ACP stdio transport and a minimal CLI entrypoint exist; scenario selection by name/sessionId/prompt and capability overrides are wired, but richer selection heuristics and loadSession orchestration remain TODO
  - Permission/file-read flows are emitted with best-effort validation; follower terminal creation (`ah show-sandbox-execution "<cmd>" --id <exec_id>`) is issued for runCmd, tool execution events stream into ToolCallUpdate content, and agentEdits trigger write_text_file; terminal output RPCs are simulated (not sourced from a real PTY)
  - Tool call lifecycle still needs stable IDs derived from scenarios, richer content/progress updates, integration with client responses, and an end-to-end PTY streaming test harness (leveraging portable-pty/expectrl helpers)

- [ ] Add agentPlan event support
  - Parse `agentPlan` events with plan entries and update flags
  - Map `agentPlan` events to ACP `session/update` notifications with plan content
  - Simulate plan creation and updates during agent execution

- [ ] Add setMode event support
  - Parse `setMode` events with mode ID
  - Map to ACP `session/set_mode` method calls
  - Support mode switching during scenario execution

- [ ] Add setModel event support (UNSTABLE)
  - Parse `setModel` events with model ID
  - Map to ACP `session/set_model` method calls (unstable feature)
  - Support model switching during scenario execution
  - Document unstable nature of this ACP feature

- [ ] Add end-to-end ACP mapping tests driven by scenarios
  - Validate initialize → session/new/load → prompt → update → cancel flows against ACP wire formats
  - Cover terminal/file/permission updates and mixed content in both prompts and responses
  - Ensure session/load historical replay and live streaming use `sessionStart` partitioning

## Outstanding Tasks

- [x] Design ACP timeline event schema for comprehensive protocol testing

  **ACP timeline event schema partially implemented:**
  - ✅ Basic TimelineEvent enum structures added
  - ✅ LlmResponse, UserInputs, AgentToolUse, AgentFileReads, AgentPermissionRequest events defined
  - ✅ Session management events (Initialize, SessionStart, SetMode, SetModel) added
  - ✅ Plan events (AgentPlan) added
  - 🔜 Full ACP protocol coverage and comprehensive testing not yet implemented

- [x] Implement content block validation against ACP specification

  **Content block validation partially implemented:**
  - ✅ Basic validation structures for Text, Image, Audio, Resource, ResourceLink, Diff, Plan
  - ✅ ResourceLink validation includes URI/name requirements and MIME types
  - 🔜 Comprehensive validation logic for all content types not fully implemented

- [x] Add ResourceLink content block parsing, validation (including annotations), and examples in docs; ensure MCP parity

  **ResourceLink implementation partially completed:**
  - ✅ ResourceLink struct with ACP schema fields (uri, name, mimeType, title, description, size, annotations)
  - ✅ Basic validation for URI/name requirements and MIME types
  - 🔜 Comprehensive annotation validation and documentation examples not fully implemented

- [x] Add MCP server configuration validation

  **MCP server validation partially implemented:**
  - ✅ Basic server name and command validation
  - ✅ Environment variable name validation
  - 🔜 Transport capability validation and comprehensive validation logic incomplete

- [x] Model `session/prompt` responses with ACP stop reasons and token usage; add scenario assertions and parser fields

  **Parser fields and structures completed:**
  - ✅ `SessionStartData.expected_prompt_response` field exists and parses correctly
  - ✅ `ExpectedPromptResponse` includes `session_id`, `stop_reason`, and `usage` fields
  - ✅ `TokenUsage` includes `input_tokens`, `output_tokens`, and `total_tokens`
  - ✅ Added parser test validating `sessionStart` with `expectedPromptResponse` parsing
  - 🔜 Actual validation logic to be implemented in mock-agent crate

- [ ] Migrate existing scenario files to new timestamp/content format
- [ ] Remove legacy scenario parsing/coercion from ah-scenario-format and all runners
- [ ] Enforce ACP capability baseline and validate promptCapabilities against initialization
- [ ] Add SSE deprecation warning/validation for MCP transports
- [ ] Gate `session/set_model` behind explicit unstable/opt-in flag and add tests
- [ ] Extend agentPermissionRequest to cover ACP permission option kinds and validate outcomes
- [ ] Enforce monotonic ACP message timestamp ordering
- [ ] Compute effective initial prompt from timeline
- [ ] Adopt object-based `userInputs` with `relativeTime` field
- [ ] Rename timing fields repo-wide to `relativeTime` and `baseTimeDelta`
- [ ] Move prompt response assertions to `sessionStart.expectedPromptResponse`
- [ ] Remove `sessionPrompt` timeline event
- [ ] Use `sessionStart.sessionId` and `expectedPromptResponse` when crafting ACP responses
- [ ] Add performance benchmarks for large scenario files with rich content
- [ ] Implement scenario compression for bandwidth-efficient storage
- [ ] Add scenario diffing and merging capabilities for collaborative testing

## ✅ Additional Unit Tests Added

Added comprehensive unit tests covering:

- **ACP Capability Baseline Validation**: Tests that agents must support text and resource_link, and that extended capabilities (image, audio, embedded) require explicit enablement
- **MCP Server Transport Alignment**: Tests SSE deprecation warnings and validation that servers have appropriate transport capabilities enabled
- **Content Block Edge Cases**: Tests validation of empty content, invalid MIME types, malformed base64, and invalid annotations
- **Scenario ACP Integration**: End-to-end validation that scenarios properly enforce ACP capability constraints
- **Permission Request Validation**: Tests for all permission option kinds, decision validation, and shorthand resolution

## Verification

Current verification status:

- ✅ Unit tests pass for implemented features (rules structures, \_meta fields)
- ✅ Pattern matching and serialization work correctly for new timeline events
- ✅ Build passes across all crates with new dependencies
- 🔜 Integration tests and end-to-end verification pending for unimplemented features

When all deliverables are implemented, this status document will be updated with:

1. Implementation details and architectural decisions for each feature
2. References to key source files in `ah-scenario-format` and `mock-agent` crates
3. Test coverage reports and performance benchmarks
4. Integration points with ACP client and server implementations
5. Migration notes for existing scenario files
