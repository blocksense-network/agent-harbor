# Scenario Format Extensions — Implementation Status

## Overview

Goal: Extend the Scenario Format (`specs/Public/Scenario-Format.md`) to support comprehensive ACP (Agent Client Protocol) agent testing, including capability negotiation, rich content handling, and MCP server configurations. The extensions enable the mock-agent crate to simulate different ACP agent profiles and handle complex content scenarios for thorough protocol compliance testing.

Target: `crates/ah-scenario-format` (existing crate) and `specs/Public/Scenario-Format.md` (documentation). Legacy timeline support has been removed; scenarios must use the structured format (`llmResponse`/`userInputs` objects, `relativeTime`, `baseTimeDelta`) and will be rejected otherwise.

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

- [ ] Implement `rules` construct parsing and evaluation
  - Parse rule conditions (`when` and `default`)
  - Implement condition evaluation (symbol existence, numeric/string comparisons)
  - Support rule merging and inlining at any YAML level

- [ ] Add symbol definition and passing mechanism
  - Support passing symbols during scenario instantiation (e.g. the mock-agent gets those via `--define` CLI options)
  - Implement symbol lookup in condition evaluation (existence and value comparison)
  - Handle undefined symbols gracefully in rule evaluation

- [ ] Implement rule merging logic
  - Handle multiple matching conditions with field merging
  - Support recursive rule evaluation in nested structures
  - Ensure deterministic merging order and precedence

- [ ] Add rule validation and error handling
  - Validate condition syntax and symbol references
  - Provide clear error messages for invalid rules
  - Handle circular dependencies and infinite recursion

### Meta Field Support

- [ ] Implement `_meta` field support in timeline events
  - Parse and preserve `_meta` fields in all ACP message types
  - Support custom capabilities in `initialize` events via `_meta`
  - Handle extension fields in session and update messages
  - Ensure `_meta` fields are passed through in bidirectional mapping

- [ ] Add meta field validation
  - Validate `_meta` field structure against ACP specification
  - Ensure `_meta` fields don't conflict with core ACP protocol fields
  - Support nested `_meta` structures (e.g., `agent.harbor.snapshots`)

- [ ] Implement meta field extension handling
  - Handle Harbor-specific extensions (`_meta.agent.harbor.*`)
  - Support custom client extensions in `_meta` fields
  - Enable testing of protocol extensions without breaking compatibility

### Test Runner Integration

- [ ] Implement test runner interpretation of `acp` configuration section
  - Read `acp.capabilities` from scenario files to determine mock-agent launch parameters
  - Map scenario capabilities to appropriate CLI flags for mock-agent process launch
  - Handle capability override precedence (CLI parameters override scenario settings)
  - Validate that scenario capabilities are compatible with requested test behavior

- [ ] Implement symbol evaluation and rule processing
  - Parse and evaluate rule conditions using provided symbols
  - Apply conditional configuration based on symbol values
  - Handle rule merging and precedence for multiple matching conditions
  - Support nested rules evaluation at any YAML level

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

- [ ] Implement `acp.capabilities` parsing in `ah-scenario-format` crate
  - Parse `loadSession`, `promptCapabilities`, `mcpCapabilities` fields
  - Validate capability structure against ACP specification
  - Provide default values for optional capabilities

- [ ] Implement `acp.cwd` and `acp.mcpServers` parsing
  - Validate working directory paths
  - Parse MCP server configurations matching ACP protocol format
  - Support stdio, HTTP, and SSE transport configurations

- [ ] Add ACP configuration validation
  - Ensure MCP capabilities match transport configurations
  - Validate capability combinations are logically consistent
  - Validate MCP server configurations are well-formed

- [ ] Implement ACP meta capability support
  - Parse custom capabilities in `_meta` fields (e.g., `agent.harbor` extensions)
  - Support nested meta capability structures
  - Validate meta capability advertisement during initialization

### Rich Content Support

- [ ] Extend `userInputs` to support rich content blocks, `_meta`, and `expectedResponse` for prompt assertions

- [ ] Extend timeline `assistant` events for rich content
  - Parse content block objects with timestamp/content structure (new format only, no backwards compatibility)
  - Support all ACP content types in scenario responses including diff and plan content blocks
  - Maintain timing precision for multi-part content

- [ ] Implement diff content type parsing and validation
  - Parse `type: "diff"` with `path`, `oldText`, `newText` fields
  - Validate absolute file paths and content fields
  - Support diff content in both prompts and responses

- [ ] Add rich content validation
  - Validate content block structure against ACP specification
  - Ensure proper MIME types for image/audio content
  - Validate relative file paths for image/audio resources
  - Verify referenced files exist and are readable at scenario load time
  - Validate plan entry structure (content, priority, status fields)
  - Handle mixed content types in single messages

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

### Backward Compatibility

- [x] Remove legacy timeline parsing/coercion (tuple `think`/`assistant`/`advanceMs`, inline arrays, top-level `events`/`assertions`) from `ah-scenario-format`, proxy, and mock-agent runners
- [x] Migrate repository scenarios to structured format with named fields (`relativeTime`, `baseTimeDelta`)
- [x] Update matching to use computed effective initial prompt (derived from first `userInputs`)
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

## Outstanding Tasks

- [ ] Design ACP timeline event schema for comprehensive protocol testing
- [ ] Implement content block validation against ACP specification
- [ ] Add ResourceLink content block parsing, validation (including annotations), and examples in docs; ensure MCP parity
- [ ] Add MCP server configuration validation
- [x] **Migrate existing scenario files to new timestamp/content format** - No backwards compatibility required - update all existing scenario files in `test_scenarios/` and `tests/` directories to use the new YAML structure with `relativeTime` and `content` keys instead of inline arrays
- [x] Remove legacy scenario parsing/coercion (top-level `think`/`agentToolUse`/`assistant` items and type-tagged timeline objects) from `ah-scenario-format` and all runners; add validation/lints that reject legacy shapes
- [ ] Model `session/prompt` responses with ACP stop reasons and token usage; add scenario assertions and parser fields
- [ ] Enforce ACP capability baseline (text + resource_link) and validate promptCapabilities against initialization
- [ ] Add SSE deprecation warning/validation for MCP transports; ensure mcpCapabilities align with mcpServers transports
- [ ] Gate `session/set_model` behind explicit unstable/opt-in flag and add tests
- [ ] Extend clientPermissionRequest to cover ACP permission option kinds and validate outcomes
- [x] Enforce monotonic ACP message timestamp ordering derived from `baseTimeDelta` + `relativeTime`; reject invalid timelines in loader/playback
- [x] Compute effective initial prompt from timeline (first `userInputs` after `sessionStart`, else first `userInputs`); remove reliance on legacy `initialPrompt` in matching
- [x] Adopt object-based `userInputs` with `relativeTime` field; update parser/encoder, migrate existing scenarios, and deprecate tuple `[ms, value]` form
- [x] Rename timing fields repo-wide to `relativeTime` and `baseTimeDelta`; remove legacy `timestamp`/`advanceMs` references from code, docs, and fixtures
- [ ] Move prompt response assertions to `sessionStart.expectedPromptResponse`; remove per-`userInputs` expectedResponse parsing; migrate scenarios and update runner logic
- [ ] Remove `sessionPrompt` timeline event; map all prompts from `userInputs`; update parser/playback/runners and migrate scenarios
- [ ] Implement Mock ACP Server handling for terminal (`runCmd` → ACP `terminal/*`), filesystem client calls, permission requests, and passthrough `show-sandbox-execution`; add integration tests
- [ ] Use `sessionStart.sessionId` and `expectedPromptResponse` when crafting ACP `session/new`/`session/load` responses and validating the first prompt turn; add tests
- [ ] Add performance benchmarks for large scenario files with rich content
- [ ] Implement scenario compression for bandwidth-efficient storage
- [ ] Add scenario diffing and merging capabilities for collaborative testing

## Verification

When all deliverables are implemented, this status document will be updated with:

1. Implementation details and architectural decisions for each feature
2. References to key source files in `ah-scenario-format` and `mock-agent` crates
3. Test coverage reports and performance benchmarks
4. Integration points with ACP client and server implementations
5. Migration notes for existing scenario files
