## Scenario Format — Shared Test Scenarios for CLI/TUI/WebUI/Mock Server

### Purpose

Define a single scenario format used by and compatible with existing scenarios:

- CLI E2E tests with mock agent (Testing-Architecture)
- TUI automated and interactive runners (see [TUI-Testing-Architecture.md](TUI-Testing-Architecture.md))
- Mock API server responses and test orchestration (where applicable)

Goals: determinism, reuse, and clarity across products.

### Component Architecture

The testing infrastructure consists of several components working together across different testing modes:

#### Core Components

- **Mock-Agent**: A deterministic agent implementation that executes
  scenarios and modifies the test workspace
- **Mock API Server**: Implements the [Agent Harbor REST API](REST-Service/API.md)
  for CLI/WebUI/TUI testing
- **Mock LLM API Server**: Simulates external LLM services (Anthropic, OpenAI,
  etc.) that coding agents communicate with
- **Test Executor**: Orchestrates scenario execution and handles user
  interactions
- **Mock ACP Server**: Implements the ACP agent side for ACP-client testing. It translates scenario events into ACP messages sent to the client, including terminal and filesystem client methods, permission requests, and passthrough sandbox commands.

#### Testing Modes

The scenario format supports three distinct testing modes, each with different
execution semantics:

1. **Mock-Agent Integration Tests**: Test the Agent Harbor CLI/TUI/WebUI with a
   deterministic mock agent that simulates LLM responses and executes actual
   tools to produce real side-effects in the test environment.

2. **Real-Agent Integration Tests**: Test actual coding agents (Claude Code,
   Codex, etc.) against the Agent Harbor infrastructure using mocked LLM API
   responses while the agents execute real tools.

3. **Simulation Mode**: Fully simulate complete coding sessions by streaming
   pre-scripted events through the mock Agent Harbor REST API without executing
   any tools, for UI testing and demonstration purposes.

Each mode processes different combinations of timeline events and executes them
through appropriate components, as detailed in the "Event Execution by Component"
section below.

### File Format

- UTF‑8 YAML; comments allowed.
- Top-level keys are stable; unknown keys ignored (forward-compatible).

### Rules and Conditional Configuration

Scenario files support a `rules` construct for parametric configuration, allowing scenarios to adapt their behavior based on runtime conditions. Rules work similarly to conditional compilation in programming languages (#ifdef/#if directives) and can appear at any level in the YAML structure.

#### Rule Structure

```yaml
rules:
  - when: '$condition_name'
    config:
      field1: value1
      field2: value2

  - when: '$numeric_symbol >= 3'
    config:
      replicas: 5
      timeout: 30000

  - default: true
    config:
      replicas: 1
      timeout: 10000
```

#### Symbol-Based Conditions

Rules support conditional configuration based on symbols provided by the test runner or mock-agent CLI via `--define` options. Conditions use simple expressions:

- **Symbol existence**: `$symbol_name` - true if the symbol is defined via `--define symbol`
- **Numeric comparison**: `$symbol == value`, `$symbol != value`, `$symbol < value`, `$symbol <= value`, `$symbol > value`, `$symbol >= value`
- **String comparison**: `$symbol == "string_value"`, `$symbol != "string_value"`
- **Undefined symbols**: References to undefined symbols cause condition evaluation to fail (rule skipped)

#### Symbol Sources & Precedence

- **CLI overrides env**: Runners that accept `--scenario-define KEY=VAL` (LLM API proxy test server, mock REST server) populate the symbol table from those flags. When the flag is absent, symbols fall back to the `AH_SCENARIO_DEFINES` env var (`KEY=VAL,FLAG=true,N=3`). CLI-provided symbols always win over environment values for the same key.
- **Duplicate keys**: When a symbol key is specified multiple times in a single source, the last occurrence wins (later CLI flags override earlier ones; later comma-separated pairs override earlier ones).
- **Type inference**: Values are parsed as `true`/`false`, integers, or strings (default). Strings used in comparisons must be quoted in rule expressions (`$env == "prod"`).
- **Scope**: Symbols are global for the scenario load; all nested `rules` blocks see the same table.

#### Merging Behavior

- **Multiple matches**: When multiple `when` conditions match, their `config` sections are merged (later rules override earlier ones)
- **Default rule**: Applied only when no `when` conditions match
- **Inlining**: The merged configuration is inlined as if written directly in place of the `rules` field
- **Nested rules**: Rules can appear at any nesting level and are resolved recursively

#### Realistic Examples

**ACP Capabilities Based on Test Mode:**

```yaml
acp:
  rules:
    - when: '$full_test_suite'
      config:
        capabilities:
          loadSession: true
          promptCapabilities:
            image: true
            audio: true
            embeddedContext: true
          mcpCapabilities:
            http: true
            sse: false
    - default: true
      config:
        capabilities:
          loadSession: false
          promptCapabilities:
            image: false
            audio: false
            embeddedContext: false
          mcpCapabilities:
            http: false
            sse: false
```

**Server Configuration Based on Environment:**

```yaml
server:
  rules:
    - when: '$production_env'
      config:
        mode: 'mock'
        llmApiStyle: 'openai'
        coalesceThinkingWithToolUse: true
    - when: '$development_env'
      config:
        mode: 'none'
        llmApiStyle: 'anthropic'
        coalesceThinkingWithToolUse: false
    - default: true
      config:
        mode: 'none'
        llmApiStyle: 'openai'
        coalesceThinkingWithToolUse: true
```

**Timeline Events Based on Protocol Version:**

```yaml
timeline:
  - initialize:
      protocolVersion: 1
      clientCapabilities:
        fs:
          readTextFile: true
          writeTextFile: true
        terminal: true
  rules:
    - when: "$protocol_version >= 2"
      config:
        - userInputs:
            - relativeTime: 0
              input:
                - type: "text"
                  text: "Enhanced protocol test"
                - type: "resource"
                  resource:
                    uri: "file:///workspace/enhanced.py"
                    mimeType: "text/x-python"
                    text: "print('Enhanced test')"
    - default: true
      config:
        - userInputs:
            - relativeTime: 0
              input: "Basic protocol test"
```

### Top-Level Schema (high level)

```yaml
name: task_creation_happy_path
tags: ['cli', 'tui', 'local']
terminalRef: 'configs/terminal/default-100x30.json'
compat:
  allowInlineTerminal: true
  allowTypeSteps: true
repo:
  init: true
  branch: 'feature/test'
  dir: './repo'
  files:
    - path: 'README.md'
      contents: 'hello'
ah:
  cmd: 'task'
  flags: ['--agent=mock', '--working-copy', 'in-place']
  env:
    AH_LOG: 'debug'
server:
  mode: 'none'
acp:
  # ACP-specific configuration for mock-agent testing
  # Working directory for the session (can be overridden by --cwd CLI parameter)
  cwd: '/tmp/workspace'
  # MCP servers to connect to (can be overridden by --mcp-servers CLI parameter)
  mcpServers: []
timeline:
  - think:
      - [500, "Analyzing the user's request"]
      - [800, 'I need to create a hello.py file']
  - agentToolUse:
      toolName: 'runCmd'
      args:
        cmd: 'python hello.py'
        cwd: '.'
      progress:
        - [300, 'Running Python script...']
        - [100, 'Script executed successfully']
      result: 'Hello, World!'
      status: 'ok'
  - agentEdits:
      path: 'hello.py'
      linesAdded: 1
      linesRemoved: 0
  - screenshot: 'after_task_file_written'
  - baseTimeDelta: 50
  - assert:
      fs:
        exists: ['.agents/tasks']
  - screenshot: 'after_commit'
  - applyPatch:
      path: './patches/add_file.patch'
      commit: true
      message: 'Apply scenario patch'
expect:
  exitCode: 0
  artifacts:
    - type: 'taskFile'
      pattern: '.agents/tasks/*'
```

### Sections

- **name**: Scenario identifier (string).
- **tags**: Array of labels to filter/select scenarios in runners.
- **terminalRef**: Optional path to a terminal configuration file describing size and rendering options. See [Terminal-Config.md](Terminal-Config.md). When omitted, runners use their defaults.
- **effectiveInitialPrompt** (computed): Runners and the `ah-scenario-format` library derive the “initial prompt” as the first `userInputs` event after `sessionStart` (if present) or otherwise the first `userInputs` event in the timeline. This value is used for scenario selection/matching and auto-start behavior; it is not a stored YAML field.
- **repo**:
  - `init`: Whether to initialize a temporary git repo.
  - `branch`: Optional branch to start on or create.
  - `dir`: Optional path to a folder co‑located with the scenario that seeds the initial repository contents. When provided, its tree is copied into the temp repo before the run.
  - `files[]`: Optional inline seed files (path, contents as string or base64 object `{ base64: "..." }`). Applied after `dir`.
- **ah**:
  - `cmd`: Primary command (e.g., `task`).
  - `flags[]`: Flat array of CLI tokens (exact order preserved).
  - `env{}`: Extra environment variables for the process under test.
- **server**:
  - `mode`: `none|mock|real` (tests typically use `none` or `mock`).
  - `llmApiStyle`: `openai|anthropic` (determines response coalescing rules,
    defaults to `openai`).
  - `coalesceThinkingWithToolUse`: `true|false` (only used for Anthropic,
    defaults to `true`).
  - Optional seed objects for mock server endpoints.
- **acp**: ACP-specific configuration for mock-agent testing (only used when testing ACP agents):
  - `capabilities`: Agent capabilities to advertise during initialization (interpreted by test runner to configure mock-agent launch):
    - `loadSession`: Whether the agent supports `session/load` method
    - `promptCapabilities`: Object specifying support for rich content types. ACP baseline requires `text` and `resource_link` (see [ACP prompt capabilities](../../resources/acp-specs/docs/protocol/initialization.mdx#prompt-capabilities)); runners MUST assume these are available and MAY validate scenarios accordingly.
      - `image`: Support for image content in prompts
      - `audio`: Support for audio content in prompts
      - `embeddedContext`: Support for embedded resource content
    - `mcpCapabilities`: MCP transport support (must align with transports used in `mcpServers`; see [ACP MCP capabilities](../../resources/acp-specs/docs/protocol/initialization.mdx#mcp-capabilities) and [MCP transports](../../resources/acp-specs/docs/protocol/session-setup.mdx#mcp-servers-and-transports)):
      - `http`: HTTP transport for MCP servers
      - `sse`: SSE transport for MCP servers (deprecated in ACP; runners SHOULD warn when enabled; see [SSE transport](../../resources/acp-specs/docs/protocol/session-setup.mdx#sse-transport))
    - Runners MUST validate that any transport used in `mcpServers` is allowed by the advertised `mcpCapabilities`; mismatches are errors.
  - `cwd`: Working directory for the ACP session (absolute path, can be overridden by `--cwd` CLI parameter)
  - `mcpServers[]`: Array of MCP server configurations (same format as ACP protocol, can be overridden by `--mcp-servers` CLI parameter)
- **timeline[]** (unified event sequence):

#### LLM Response Events (`llmResponse`)

Groups multiple response elements into a single LLM API response. Contains
sub-events that get coalesced based on the target LLM API style (OpenAI vs
Anthropic). When the client requests streaming responses, the server
automatically converts this into a sequence of incremental streaming events
with preserved timing.

**Important**: When an `llmResponse` event is immediately followed by
`agentToolUse` events in the timeline, the mock LLM API server must include
tool use suggestions in the LLM response using the appropriate API format
(OpenAI `tool_calls` array or Anthropic `tool_use` content blocks). This
ensures that real agents receive tool use instructions as part of their LLM
responses.

Scenarios **MUST** place `agentToolUse` events outside `llmResponse` blocks as
separate `agentActions` events. Inline tool use inside `llmResponse` is no
longer supported.

- `think`: Array of `[milliseconds, text]` pairs for agent thinking events. For
  OpenAI API style: thinking is processed internally but **NOT included in API
  responses** (matches OpenAI's behavior where thinking is never exposed). For
  Anthropic API style: thinking is exposed as separate "thinking" blocks in
  the response content array.
- `assistant`: Array of `[milliseconds, content_block]` pairs for assistant responses. Each content block can be:
  - String: Simple text response
  - Object: Rich content block with `type`, `text`, `mimeType`, `data`, etc. (see Content Types below)
- `error`: Error response element for modeling LLM API error conditions (rate
  limiting, invalid requests, etc.). Generates an appropriate HTTP error response
  from the mock LLM API server. Fields:
  - `errorType`: String identifier for the error type (e.g., "rate_limit_exceeded",
    "invalid_request", "tool_not_found")
  - `statusCode`: Optional HTTP status code (defaults to 400)
  - `message`: Human-readable error message
  - `details`: Optional structured error details (JSON value)
  - `retryAfterSeconds`: Optional retry-after header value for rate limiting

#### Meta Fields in Timeline Events

ACP protocol messages can include `_meta` fields for extensibility. Scenarios support specifying `_meta` fields in any timeline event that corresponds to an ACP message:

```yaml
timeline:
  - initialize:
      protocolVersion: 1
      clientCapabilities:
        fs:
          readTextFile: true
          writeTextFile: true
        terminal: true
      # Custom client capabilities via _meta
      _meta:
        client.extensions:
          customFeature: true
    expectedResponse:
      protocolVersion: 1
      agentCapabilities:
        loadSession: false
        promptCapabilities:
          image: true
      # Custom agent capabilities via _meta
      _meta:
        agent.harbor:
          snapshots:
            version: 1
            supportsTimelineSeek: true

  - sessionStart:
      expectedPromptResponse:
        sessionId: "test-session-123"
        stopReason: "completed"        # ACP stop reason (e.g., completed, cancelled, interrupted, max_tokens) — see [stopReason](../../resources/acp-specs/docs/protocol/prompt-turn.mdx#stopreason)
        usage:
          inputTokens: 12
          outputTokens: 34
  - userInputs:
      - relativeTime: 0
        input:
          - type: "text"
            text: "Test message"
        expectedResponse:
          stopReason: "completed"
          usage:
            inputTokens: 12
            outputTokens: 34
            totalTokens: 46
        _meta:
          request.trackingId: "req_12345"
          request.priority: "high"

  - llmResponse:
      _meta:
        response.model: "claude-3-sonnet"
        response.tokens: 150
      - assistant:
          - [100, "Response with metadata"]
```

#### Content Types in Timeline Events

When using rich content blocks in timeline events, the following formats are supported:

**File Organization for Media Content:**

For scenarios that include image or audio content, organize files in a directory structure alongside the scenario file:

```
scenario-directory/
├── scenario.yaml
├── images/
│   ├── diagram.png
│   └── screenshot.jpg
└── audio/
    ├── recording.wav
    └── instructions.mp3
```

All paths in content blocks are resolved relative to the scenario file's directory. The mock-agent will load the referenced files at runtime and encode them appropriately for the ACP protocol.

**Text Content:**

```yaml
assistant:
  - relativeTime: 100
    content: 'Simple text response'
  - relativeTime: 200
    content:
      type: 'text'
      text: 'Annotated text'
      annotations:
        priority: 0.8
```

Annotations follow the MCP/ACP `Annotations` shape and MAY appear on any content block type (see [Content annotations](../../resources/acp-specs/docs/protocol/content.mdx#text-content)).

**Image Content:**

```yaml
assistant:
  - relativeTime: 100
    content:
      type: 'image'
      mimeType: 'image/png'
      path: 'images/example.png' # Relative to the scenario file location; must exist
      # At least one of `path` or `data` is required. When `path` is present the file
      # is resolved relative to the scenario file and validated at load time. When
      # `data` is present it MUST be valid base64 for the declared MIME type.
```

**Audio Content:**

```yaml
assistant:
  - relativeTime: 100
    content:
      type: 'audio'
      mimeType: 'audio/wav'
      path: 'audio/example.wav' # Relative to the scenario file location; must exist
      # At least one of `path` or `data` is required. Base64 payloads are validated.
```

**Embedded Resource:**

```yaml
assistant:
  - relativeTime: 100
    content:
      type: 'resource'
      resource:
        uri: 'file:///workspace/main.py'
        mimeType: 'text/x-python'
        text: "def hello():\n    print('Hello, World!')\n"
```

**Resource Link:**

```yaml
assistant:
  - relativeTime: 120
    content:
      type: 'resource_link'
      uri: 'file:///workspace/README.md'
      name: 'README.md'
      mimeType: 'text/markdown'
      title: 'Project README'
      description: 'Main project documentation'
      size: 1024
      annotations:
        - priority: 0.9
          audience: ['user']
```

**Diff Content:**

```yaml
assistant:
  - relativeTime: 100
    content:
      type: 'diff'
      path: '/home/user/project/src/config.json' # MUST be absolute to avoid ambiguity
      oldText: '{\n  "debug": false\n}'
      newText: '{\n  "debug": true\n}'
```

**Plan Content:**

```yaml
assistant:
  - relativeTime: 100
    content:
      type: 'plan'
      entries:
        - content: 'Analyze the existing codebase structure'
          priority: 'high'
          status: 'pending'
        - content: 'Identify components that need refactoring'
          priority: 'high'
          status: 'pending'
        - content: 'Create unit tests for critical functions'
          priority: 'medium'
          status: 'pending'
```

#### Agent Action Events (`agentActions`)

Tool invocations, planning activities, and file operations performed by the agent. These represent the
actual work the agent does in response to LLM instructions.

- `agentPlan`: Agent creates or updates an execution plan for complex multi-step tasks. Fields:
  - `entries`: Array of plan entry objects, each containing:
    - `content`: Human-readable description of the task
    - `priority`: Priority level (`high`, `medium`, `low`)
    - `status`: Current execution status (`pending`, `in_progress`, `completed`)
  - `planUpdate`: Optional boolean indicating if this replaces the entire plan (default: true)

- `setMode`: User switches the agent to a different operating mode. Fields:
  - `modeId`: The ID of the mode to switch to (e.g., 'ask', 'architect', 'code')
  - Maps to ACP `session/set_mode` method call (see [session modes](../../resources/acp-specs/docs/protocol/session-modes.mdx#setting-the-current-mode))

- `setModel`: User switches the LLM model during the session. Fields:
  - `modelId`: The ID of the model to switch to
  - **UNSTABLE**: Requires `unstable: true` in the ACP configuration. The ACP spec marks `session/set_model` as unstable and subject to removal/change (see [schema.unstable set_model](../../resources/acp-specs/docs/protocol/schema.unstable.mdx#session-set_model)).

- `agentToolUse`: Tool invocation with detailed execution flow. Fields:
  - `toolName`: Name of the tool being invoked
  - `args`: Tool-specific arguments object
  - `progress`: Array of `[milliseconds, message]` pairs for execution progress
    updates
  - `result`: Expected result string
  - `status`: Expected execution status (`ok`, `error`)
  - `toolExecution`: Detailed tool execution specification (see below)
- `agentEdits`: File modification event with path and change metrics. Gets
  mapped to appropriate file editing tools for the target agent. Fields:
  - `path`: File path that was modified
  - `linesAdded`: Number of lines added
  - `linesRemoved`: Number of lines removed
- Tool-specific events (mapped to `agentToolUse` internally):
  - `runCmd`: Execute terminal/shell commands. Handled by the **Mock ACP Server** as ACP client terminal methods (`terminal/create`, `terminal/output`, `terminal/wait_for_exit`, `terminal/kill`). Fields: `cmd` (command string),
    `cwd` (optional working directory), `timeout` (optional milliseconds),
    `description` (optional description), `run_in_background` (optional boolean). In passthrough mode (see `specs/ACP.server.status.md`), the Mock ACP Server instructs the client to run a `show-sandbox-execution` command instead of executing directly, so the client’s sandbox/recorder performs the command while streaming output back via ACP terminal notifications.
  - `grep`: Search for patterns in files. Fields: `pattern`, `path`, `glob`
    (optional glob pattern), `output_mode` (optional:
    content/files_with_matches/count), `-B` (optional before context), `-A`
    (optional after context), `-C` (optional context), `-n` (optional line
    numbers), `-i` (optional case insensitive), `type` (optional file type),
    `head_limit` (optional result limit), `multiline` (optional multiline mode).
  - `readFile`: Read file contents. Fields: `path`, `encoding` (optional,
    defaults to utf-8), `offset` (optional line offset), `limit` (optional line
    limit).
  - `listDir`: List directory contents. Fields: `path`, `recursive` (optional
    boolean), `pattern` (optional glob pattern).
  - `find`: Find files by pattern. Fields: `path`, `name` (filename pattern),
    `type` (optional: file/dir).
  - `sed`: Stream editor operations. Fields: `expression`, `path`, `inplace`
    (optional boolean).
  - `editFile`: Edit file with exact string replacements. Fields: `path`,
    `old_string`, `new_string`, `replace_all` (optional boolean).
  - `writeFile`: Write content to file. Fields: `path`, `content`.
  - `task`: Launch specialized agent for complex tasks. Fields: `description`,
    `prompt`, `subagent_type`.
  - `webFetch`: Fetch content from URL with AI analysis. Fields: `url`,
    `prompt`.
  - `webSearch`: Search the web for information. Fields: `query`,
    `allowed_domains` (optional array), `blocked_domains` (optional array).
  - `todoWrite`: Manage structured task lists. Fields: `todos` (array of todo
    objects).
  - `notebookEdit`: Edit Jupyter notebook cells. Fields: `notebook_path`,
    `cell_id` (optional), `new_source`, `cell_type` (optional), `edit_mode`
    (optional: replace/insert/delete).
  - `exitPlanMode`: Exit plan mode with summary. Fields: `plan`.
  - `bashOutput`: Retrieve output from background bash shell. Fields:
    `bash_id`, `filter` (optional regex).
  - `killShell`: Kill running background bash shell. Fields: `shell_id`.
  - `slashCommand`: Execute slash command. Fields: `command`.

#### User Action Events (`userActions`)

Simulated user interactions that drive the scenario forward.

- `userInputs`: Array of inputs with `relativeTime` using the new object form:
  - `relativeTime`: Absolute time in milliseconds from scenario start (post-`baseTimeDelta` accumulation)
  - `input`: string or array of content blocks (required)
  - `_meta`: optional ACP meta to attach to the prompt request
  - `expectedResponse`: optional per-prompt assertion mirroring ACP `PromptResponse` (`sessionId`, `stopReason`, `usage`, optional `_meta`)
  - `target`: optional `tui|webui|cli` (defaults to all)
    This is the **only** way to send ACP `session/prompt` messages; the runner converts each entry to a `session/prompt` call in relativeTime order. Can run concurrently with agent events.

  Response assertions for the _first_ prompt after a boundary belong on `sessionStart.expectedPromptResponse` (see below); per-entry `expectedResponse` covers subsequent prompts.

- `userEdits`: Simulate user editing files. Fields:
  - `patch`: Path to unified diff or patch file relative to the scenario folder.
- `userCommand`: Simulate user executing a command (not an agent tool call).
  Fields:
  - `cmd`: Command string to execute.
  - `cwd`: Optional working directory relative to the scenario.
- `userCancelSession`: Simulate user cancelling an ongoing agent operation.
  Maps to ACP `session/cancel` notification (see [cancellation](../../resources/acp-specs/docs/protocol/prompt-turn.mdx#cancellation)). No additional fields required.

#### Test and Control Events

Events that control test execution and validation.

- `baseTimeDelta`: Advance logical time by specified milliseconds. Must be >= max
  time from concurrent events.
- `screenshot`: Ask harness to capture vt100 buffer with a label.
- `log`: Emit a harness log/message at the current timeline position (mapped to playback `log` events).
- `assert`: Structured assertions (see below).
- `complete`: Event indicating that the scenario task has completed
  successfully. This marks the session status as completed and triggers any
  completion logic.
- `sessionStart`: Boundary marker for `loadSession` functionality. Events before
  this marker are considered historical and are replayed during `session/load`.
  Events after this marker are streamed live after session loading completes.
  When `acp.capabilities.loadSession` is enabled the timeline MUST include exactly one
  `sessionStart` marker; conversely, scenarios that use `sessionStart` MUST advertise
  `loadSession` in their ACP capabilities. The loader partitions timeline events into
  historical (pre-boundary) and live (post-boundary) segments using this marker.
  Optional fields:
  - `sessionId`: The session ID to use for the ensuing `session/new` (or `session/load`) call; the Mock ACP Server uses this when responding to initialization/session setup.
  - `expectedPromptResponse`: Response assertions for the first `userInputs` after this boundary (e.g., `sessionId`, `stopReason`, `usage`).

#### Client-Side ACP Method Simulation Events

Execution model: the mock-agent plays the **agent** role and will emit these ACP client-facing calls; the **test executor** plays the client and returns the responses defined in the event. Use these to exercise bidirectional flows without a real client UI.

- `agentFileReads`: Simulates file read operations from the agent. Takes a list of files to read and translates to one or more ACP `fs/read_text_file` method calls depending on the tool profile (e.g., when tools allow reading one file at a time, this will result in multiple separate calls).
  - `files[]`: Array of file read specifications:
    - `path`: Absolute file path to read
    - `expectedContent`: Expected file content to return (optional, for validation)
- `agentPermissionRequest`: Simulates an ACP `session/request_permission` flow where the **agent** sends the request and the **test executor** returns the user decision (see [request_permission](../../resources/acp-specs/docs/protocol/tool-calls.mdx#requesting-permission)):
  - `sessionId`: Target session ID (optional; default: current session)
  - `toolCall`: Minimal tool call context to include in the request (e.g., `{ toolCallId, title, kind }`)
  - `options[]`: Permission options to present, each with:
    - `id`: Unique identifier for the option
    - `label`: Human-readable label to display
    - `kind`: Permission kind (`allow_once`, `allow_always`, `reject_once`, `reject_always`)
  - `decision`: User decision simulated by the runner; object with:
    - `outcome`: `selected|cancelled` (matches ACP response outcome)
    - `optionId`: Required when `outcome=selected`; must match one of the provided `options`
  - `granted`: Boolean shorthand for decision (when true, selects the first `allow_once` option; when false, selects the first `reject_once` option). Cannot be used with `decision`.
- `merge`: Event indicating that this scenario session should be merged into
  the session list upon completion. When present, the scenario session will be
  marked as completed but remain visible in session listings. When omitted,
  completed scenarios are not shown in session listings.

#### Tool Execution Details (`toolExecution`)

A new event type that provides detailed, timed tool execution output for
simulation mode. This allows the mock REST API server to stream realistic tool
execution events without actually executing tools.

- `toolExecution`: Detailed tool execution specification within `agentToolUse`.
  Fields:
  - `startTimeMs`: Execution start time relative to the tool use event
  - `events`: Array of execution events with timing and output:
    - `type`: `stdout|stderr|progress|completion`
    - `timeMs`: Time offset from execution start
    - `content`: Output content or progress message
    - `exitCode`: Final exit code (for completion events)

  Legacy timeline shapes (top-level `think`, `agentToolUse`, `agentEdits`, or
  `assistant` events; type-tagged timeline objects) are **not supported**.

- **expect**:
  - `exitCode`: Expected process exit code.
  - `artifacts[]`: File/glob expectations after run.

### Assertions

Assertions verify that the expected outcomes of previous responses have been met. They execute **before the next response is returned** only in the mock LLM server in order to ensure filesystem state and other conditions are validated after the expected actions of the agent in handling a previous llmResponse.

- `assert.fs.exists[]`: Paths that must exist.
- `assert.fs.notExists[]`: Paths that must not exist.
- `assert.text.contains[]`: Strings expected in terminal buffer (normalized).
- `assert.json.file`: `{ path, pointer, equals }` JSON pointer equality for structured files.
- `assert.git.commit`: `{ messageContains: "..." }` simple commit message checks.

Runners MAY extend assertions; unknown keys are ignored with a warning.

### Unified Timeline Model

Scenarios use a unified timeline containing all events (agent actions, user inputs, assertions, screenshots, etc.):

- **Timeline Progression**: Time advances through `baseTimeDelta` events, establishing absolute relativeTimes.
- **Delta Timing**: Event-internal timing values are millisecond deltas from the current timeline position; encoders MUST emit ACP messages in non-decreasing absolute relativeTime order to preserve streaming semantics.
- **Agent Events** execute sequentially and consume time based on their millisecond values.
- **User Events** (userInputs, userEdits, userCommand) can execute concurrently with agent events.
- **Test Events** (assert, screenshot) execute at specific timeline points.
- **Time Advancement** (`baseTimeDelta`) ensures proper synchronization: `baseTimeDelta >= max(time_from_concurrent_events)`; runners MUST reject timelines that would produce decreasing ACP message relativeTimes.

### Event Execution by Component and Testing Mode

The scenario format supports three distinct testing modes, each processing different combinations of timeline events through specific components. Below are detailed specifications for how each component handles events in each mode.

#### ACP Direction of Events (clarification)

Each event maps to an ACP role. Use this table as the source of truth when wiring tests/harnesses:

| Event                                                                     | ACP mapping                                          | Originator     | Notes                                                                                                           |
| ------------------------------------------------------------------------- | ---------------------------------------------------- | -------------- | --------------------------------------------------------------------------------------------------------------- |
| `initialize`                                                              | `initialize` request/response                        | Client → Agent | Client sends capabilities; agent responds with capabilities                                                     |
| `sessionStart`                                                            | Drives `session/new` vs `session/load` boundary      | Client → Agent | Pre-boundary events are historical replay; post-boundary are live                                               |
| `userInputs`                                                              | `session/prompt`                                     | Client → Agent | Carries content blocks/meta; `expectedResponse` asserts first post-boundary prompt                              |
| `userCancelSession`                                                       | `session/cancel`                                     | Client → Agent | Agent must return `stopReason=cancelled` for the affected prompt                                                |
| `llmResponse` (assistant/think)                                           | `session/update` AgentMessageChunk/AgentThoughtChunk | Agent → Client | Agent streams content/meta                                                                                      |
| `agentToolUse`                                                            | Tool call lifecycle                                  | Agent → Client | Agent initiates tool call (often terminal/fs); **client executes tool and streams output/results per scenario** |
| `agentEdits`                                                              | Tool call update (file edit content)                 | Agent → Client | Represents edits produced by the agent                                                                          |
| `agentPlan`                                                               | `session/update` Plan                                | Agent → Client | Plan entries with priority/status                                                                               |
| `agentPermissionRequest`                                                  | `session/request_permission`                         | Agent → Client | Client replies; unexpected extra requests are errors                                                            |
| `agentFileReads`                                                          | `fs/read_text_file`                                  | Agent → Client | Client replies with scripted content                                                                            |
| `setMode`                                                                 | `session/set_mode`                                   | Client → Agent | Mode changes originate from the client                                                                          |
| `setModel` (unstable)                                                     | `session/set_model`                                  | Client → Agent | Unstable/opt-in                                                                                                 |
| `log`, `assert`, `complete`, `status`, `advanceMs`, `merge`, `screenshot` | Harness-only                                         | Harness        | Not ACP messages                                                                                                |

Key clarifications:

- `agentToolUse`: the **agent** creates the tool call and any terminal/fs requests; the **client** (test harness) returns results. In ACP mock-agent mode there are three patterns: (1) fully simulated clients return the scripted execution/result from the scenario, (2) clients may actually run tools and stream real output back, or (3) ACP server mode (see ACP.server.status.md) asks the client to launch `ah show-sandbox-execution "<cmd>" --id <exec_id>` as a follower terminal so Harbor’s recorder streams the real sandboxed run. When testing real agents (non-ACP mock), `agentToolUse` is only used to seed tool-use suggestions inside `llmResponse` from the mock LLM API Proxy.
- `setMode`/`setModel`: these are **client-originated** ACP methods, not agent notifications.
- `userCancelSession`: the client sends `session/cancel`; the agent must return `stopReason=cancelled` for that prompt turn.

#### 1. Mock-Agent Integration Tests

In this mode, the Agent Harbor CLI/TUI/WebUI is tested end-to-end using a
deterministic mock agent that simulates LLM responses while executing real tools
to produce actual side-effects in the test environment. The test executor
simulates user interactions, and the mock agent handles both LLM response
simulation and tool execution.

**Mock-Agent Component:**

- **Configuration**: Reads the `acp` section of the scenario file to determine how to launch and configure itself:
  - Uses `acp.capabilities` to advertise specific ACP capabilities during initialization (loadSession, prompt content types, MCP transport support)
  - Uses `acp.cwd` as the working directory for the ACP session
  - Uses `acp.mcpServers` to configure MCP server connections
  - These configuration values are interpreted only by the test runner, which uses them to decide how to launch the mock-agent process with appropriate CLI parameters
- Processes `llmResponse` events by printing thinking and assistant messages
  directly to the terminal/console to simulate LLM API responses (no actual LLM
  API calls are made)
- Executes all `agentActions` events (`agentToolUse`, `agentEdits`, and
  individual tool events like `runCmd`, `grep`, `readFile`, etc.) by actually
  invoking the corresponding tools and modifying the test workspace
- For `agentToolUse` events, executes the specified tool with real arguments and
  captures actual results, comparing them against expected `result` and `status`
  fields for validation
- For `agentEdits` events, performs actual file modifications in the test
  workspace and verifies the changes match the expected line counts
- When `--checkpoint-cmd` is provided, executes the checkpoint command
  (typically `ah agent fs snapshot`) after each tool execution and file edit
  completes
- Skips `toolExecution` details entirely since it executes real tools and
  captures real output

**Mock API Server (Agent Harbor REST API):**

- Not used in mock-agent integration tests (server mode is `none`)
- All interactions happen directly through the CLI/TUI components

**Mock LLM API Server:**

- Not used in mock-agent integration tests (the mock agent simulates LLM
  responses internally)

**Test Executor:**

- Processes all `userActions` events (`userInputs`, `userEdits`, `userCommand`)
  by simulating actual user interactions with the CLI/TUI interface
- For `userInputs`, sends keystrokes to the running CLI/TUI process
- For `userEdits`, applies the specified patches to files in the test workspace
- For `userCommand`, executes shell commands in the test environment
- Handles test control events (`baseTimeDelta`, `screenshot`, `assert`) by
  controlling timing, capturing screenshots, and validating assertions
- Executes `complete` and `merge` events by modifying an in-memory database
  instance created for each test

**Event Processing Summary:**

- `llmResponse` → Mock-Agent (simulated output only, including error responses)
- `agentActions` → Mock-Agent (real execution)
- `userActions` → Test Executor (simulated user interactions)
- Test/Control events → Test Executor

#### 2. Real-Agent Integration Tests

In this mode, actual coding agents (Claude Code, Codex, etc.) are tested against
the Agent Harbor infrastructure. The mock LLM API server provides deterministic
responses while the real agents execute actual tools. This validates that real
agents correctly interpret the output of the Agent Harbor LLM API proxy and mock
server and that they are properly set up to create FS snapshots after each edit
and tool use.

**Mock-Agent Component:**

- Not used in real-agent integration tests

**Mock API Server (Agent Harbor REST API):**

- May be used if testing WebUI components, but typically not involved in
  CLI-focused real-agent tests
- If used, streams session events based on real agent activity rather than
  scenario events

**Mock LLM API Server:**

- Processes `llmResponse` events and converts them into proper API responses for
  the target LLM API style (OpenAI or Anthropic)
- Groups thinking, assistant text, and tool use events into single API responses
  with correct coalescing
- Converts responses to streaming format when clients request streaming
- Validates that real agents send correct tool definitions matching the expected
  tools profile
- In strict validation mode, fails immediately on unexpected tools to catch
  integration issues
- Skips all `agentActions` and `userActions` events entirely (these are handled
  by the real agent and test executor)

**Test Executor:**

- Processes `userActions` events by sending actual input to the running real
  agent process
- For `userInputs`, simulates user keystrokes to the agent interface
- For `userEdits`, applies patches to the workspace
- For `userCommand`, executes shell commands that affect the agent environment
- Handles test control events for timing, screenshots, and assertions
- The real agent processes its own tool executions and file edits based on mock
  LLM responses

**Real Agent Behavior:**

- Receives `llmResponse` events through the mock LLM API server
- Interprets assistant messages and thinking (if exposed by API style)
- Executes tools based on tool use instructions in LLM responses
- Performs file edits as directed by the scenario
- Produces real side-effects that are validated by assertions

**Event Processing Summary:**

- `llmResponse` → Mock LLM API Server (converted to real API responses for the
  agent, including error responses)
- `agentActions` → Real Agent (executes actual tools and file operations)
- `userActions` → Test Executor (simulates user interactions with real agent)
- Test/Control events → Test Executor

#### 3. Simulation Mode

In this mode, the mock Agent Harbor REST API server fully simulates complete
coding sessions by streaming pre-scripted events without executing any actual
tools. This is used for UI testing, demonstrations, and validating the REST API
event streaming behavior with realistic timing.

**Mock-Agent Component:**

- Not used in simulation mode

**Mock API Server (Agent Harbor REST API):**

- Processes all timeline events and converts them into SSE event streams for the
  REST API
- For `llmResponse` events, creates appropriate SSE events with thinking traces,
  assistant messages, and tool use notifications
- For `agentActions` events, simulates tool execution by streaming realistic
  progress events based on `toolExecution` details within `agentToolUse` events
- Uses the detailed `toolExecution.events` array to stream timed stdout/stderr
  output, progress messages, and completion events
- For `agentEdits` events, streams file modification notifications with change
  metrics
- Processes `userActions` events by streaming simulated user interaction events
- Handles all test control events (`baseTimeDelta`, `screenshot`, `assert`) for
  timing control and validation
- Maintains accurate event timing and sequencing to simulate realistic session
  flow
- Does not execute any actual tools or modify files - everything is event
  streaming only

**Mock LLM API Server:**

- Not used in simulation mode (simulation happens at the REST API level)

**Test Executor:**

- May be used to simulate a streamed events sessions with a remote Agent Harbor REST server
- Doesn't process `assert` events in this mode as real-side effects are not produced.
- Handles `screenshot` events if testing visual components

**Event Processing Summary:**

- `llmResponse` → Mock API Server (streamed as SSE events, including error events)
- `agentActions` → Mock API Server (simulated execution via `toolExecution`
  details)
- `userActions` → Mock API Server (streamed as user interaction events)
- Test/Control events → Mock API Server and Test Executor

**Example Timeline:**

````yaml
timeline:
  # LLM Response Events - API level interactions
  - llmResponse:
      - think:
          - relativeTime: 500
            content: 'I need to examine the current code first'
          - relativeTime: 300
            content: 'Let me check what functions exist...'
      - assistant:
          - relativeTime: 200
            content: 'Let me search for function definitions in the codebase.'

  # Agent Action Events - Tool executions and file operations
  - agentToolUse:
      toolName: 'grep'
      args:
        pattern: 'def'
        path: '.'
        glob: '*.py'
      progress:
        - [200, 'Searching for function definitions...']
      result: 'main.py:1:def main():'
      status: 'ok'
      # Detailed execution for simulation mode
      toolExecution:
        startTimeMs: 0
        events:
          - type: 'stdout'
            timeMs: 50
            content: 'Searching for pattern "def" in Python files...'
          - type: 'stdout'
            timeMs: 150
            content: 'main.py:1:def main():'
          - type: 'completion'
            timeMs: 200
            exitCode: 0

  - agentEdits:
      path: 'main.py'
      linesAdded: 3
      linesRemoved: 0

  # User Action Events - Simulated user interactions
  - userInputs:
      - [200, 'some input'] # 200ms
      - [400, 'more input'] # 400ms, concurrent with agent actions
    target: 'tui'

  # Test and Control Events
  - assert:
      fs:
        exists: ['main.py']
  - screenshot: 'after_edits'
  - baseTimeDelta: 1000 # Must be >= max concurrent event times

**Example with Error Response:**

```yaml
timeline:
  # LLM response that generates an error
  - llmResponse:
      - error:
          errorType: 'rate_limit_exceeded'
          statusCode: 429
          message: 'Rate limit exceeded. Please try again later.'
          retryAfterSeconds: 60
````

**Example with Tool Execution Details (Simulation Mode):**

```yaml
timeline:
  # LLM response instructing tool use
  - llmResponse:
      - think:
          - [300, 'The user wants me to run tests']
      - assistant:
          - relativeTime: 100
            content: 'Running the test suite to check for any issues.'

  # Detailed agent action with realistic execution simulation
  - agentActions:
      - agentToolUse:
          toolName: 'runCmd'
          args:
            cmd: 'npm test'
            cwd: '.'
          result: 'Test suite passed'
          status: 'ok'
          toolExecution:
            startTimeMs: 0
            events:
              - type: 'stdout'
                timeMs: 100
                content: '> npm test\n'
              - type: 'stdout'
                timeMs: 500
                content: '> jest\n'
              - type: 'stdout'
                timeMs: 800
                content: 'PASS src/App.test.js\n'
              - type: 'stdout'
                timeMs: 850
                content: 'PASS src/utils.test.js\n'
              - type: 'stdout'
                timeMs: 900
                content: 'Test Suites: 2 passed, 2 total\n'
              - type: 'stdout'
                timeMs: 905
                content: 'Tests: 15 passed, 15 total\n'
              - type: 'completion'
                timeMs: 950
                exitCode: 0

  # User interaction during execution
  - userActions:
      - userInputs:
          - [700, 'q'] # User presses 'q' during test execution
        target: 'tui'

  - baseTimeDelta: 1000
```

**Timing Rules:**

- **Timeline Progression**: Time advances through `baseTimeDelta` events, which set the
  absolute relativeTime for subsequent events
- **Delta Timing**: Numeric time values within event blocks (thinking pairs,
  progress updates, tool execution events, etc.) are millisecond deltas from
  the absolute relativeTime established by the most recent `baseTimeDelta` event
- Agent events never overlap (sequential execution)
- User and test events can execute concurrently with agent events
- `baseTimeDelta` ensures proper synchronization: `baseTimeDelta >= max(time_from_concurrent_events)`
- Millisecond precision allows deterministic replay

### Screenshot Integration

- `screenshot`: Event in the timeline that asks the harness to capture a screenshot of the terminal buffer.
- The harness stores screenshots under a directory that includes both scenario and terminal profile identifiers (see Screenshot Paths) and includes their paths in the report.

### FS Snapshot Integration

- **FS snapshots** (filesystem snapshots for time travel) are taken by executing the `ah agent fs snapshot` command during agent execution.
- Unlike screenshots (using `screenshot`), FS snapshots preserve filesystem state at specific points in time for later restoration.
- FS snapshots are triggered programmatically by agents or manually via CLI commands, not through scenario step definitions.

### Screenshot Paths (Scenario × Terminal)

- Screenshot directory scheme:
  - `target/tmp/<runner>/<scenarioName>/<terminalProfileId>/screenshots/<label>.golden`
  - `terminalProfileId` is taken from the terminal config `name` if present; otherwise computed as `<width>x<height>`.
- Log directory scheme (example):
  - `target/tmp/<runner>/<scenarioName>/<terminalProfileId>/<relativeTime>-<pid>/`

### Conventions

- All paths are relative to the temporary test workspace root unless prefixed with `/`.
- Shell tokens in `ah.flags[]` are not re‑parsed; runners pass them verbatim.
- Time values are in milliseconds; millisecond precision enables deterministic scenario replay.
- Agent events execute sequentially; user and test events can execute concurrently with agent events.
- `baseTimeDelta` values must account for all concurrent activity to maintain proper synchronization.

### ACP Agent Testing Extensions

The scenario format includes special support for testing ACP (Agent Client Protocol) agents through the `acp` configuration section and enhanced content handling:

#### Example: ACP Capability Negotiation Test

```yaml
name: acp_capability_negotiation_test
tags: ['acp', 'capabilities']

acp:
  capabilities:
    loadSession: false
    promptCapabilities:
      image: true
      audio: false
      embeddedContext: true
    mcpCapabilities:
      http: true
      sse: false
  cwd: '/tmp/test-workspace'
  mcpServers:
    - name: 'filesystem'
      command: '/usr/local/bin/mcp-server-filesystem'
      args: ['/tmp/test-workspace']
      env: []

timeline:
  # Test initialization handshake (capabilities configured via scenario file)
  - initialize:
      protocolVersion: 1
      clientCapabilities:
        fs:
          readTextFile: true
          writeTextFile: true
        terminal: true
      clientInfo:
        name: 'test-client'
        version: '1.0.0'
    expectedResponse:
      protocolVersion: 1
      agentCapabilities:
        loadSession: false
        promptCapabilities:
          image: true
          audio: false
          embeddedContext: true
        mcpCapabilities:
          http: true
          sse: false

  # Test rich content prompt (sent via first userInputs)
  - sessionStart:
      expectedPromptResponse:
        sessionId: 'test-session-123'
  - userInputs:
      - relativeTime: 0
        input:
          - type: 'text'
            text: 'Analyze this image and describe what you see:'
          - type: 'image'
            mimeType: 'image/png'
            path: 'images/test-diagram.png'

  # Test client-side ACP method simulation
  - agentFileReads:
      files:
        - path: '/tmp/test-workspace/main.py'
          expectedContent: "print('Hello, World!')"
  - agentPermissionRequest:
      toolCall:
        toolCallId: 'perm-1'
        title: 'file_write'
        kind: 'fs/write_text_file'
      options:
        - id: 'allow'
          label: 'Allow once'
          kind: 'allow_once'
        - id: 'deny'
          label: 'Deny once'
          kind: 'reject_once'
      granted: true

  - complete: true
```

#### Example: LoadSession with SessionStart Boundary

```yaml
name: 'session_with_history'
tags: ['acp', 'loadsession']

acp:
  capabilities:
    loadSession: true
    promptCapabilities:
      image: false
      audio: false
      embeddedContext: false
    mcpCapabilities:
      http: false
      sse: false

timeline:
  # Historical events (replayed during session/load)
  - llmResponse:
      - assistant:
          - relativeTime: 100
            content: "I'll create a simple Python script for you."
  - agentToolUse:
      toolName: 'runCmd'
      args:
        cmd: 'echo "print(\\"Hello, World!\\")" > hello.py'
      result: 'File created'
      status: 'ok'
  - agentEdits:
      path: 'hello.py'
      linesAdded: 1
      linesRemoved: 0

  # Session boundary - everything before this is historical
  - sessionStart

  # Live events (streamed after session/load completes)
  - llmResponse:
      - assistant:
          - relativeTime: 100
            content: "Now let's modify the script to be more interesting."
  - agentToolUse:
      toolName: 'runCmd'
      args:
        cmd: 'echo "print(\\"Hello from loaded session!\\")" > hello.py'
      result: 'File updated'
      status: 'ok'

  - complete: true
```

When `session/load` is called with this scenario's name, the mock-agent will:

1. Replay all events before `sessionStart` as historical conversation
2. Continue streaming events after `sessionStart` as live activity

### References

- CLI behaviors and flow: [CLI.md](CLI.md)
- TUI testing approach: [TUI-Testing-Architecture.md](TUI-Testing-Architecture.md)
- CLI E2E plan: [Testing-Architecture.md](Testing-Architecture.md)
- Terminal config format: [Terminal-Config.md](Terminal-Config.md)
- Existing example scenario: `tests/tools/mock-agent/scenarios/realistic_development_scenario.yaml` (comprehensive timeline-based scenario with ~30-second execution)
- REST API behavior: [REST-Service/API.md](REST-Service/API.md)
- Shared playback crate: [`crates/ah-scenario-format`](../../crates/ah-scenario-format)

### Reference Implementations

- `crates/ah-scenario-format/` – Rust crate with the Scenario-Format structs, YAML loader, Levenshtein matcher, and playback iterator reused by the TUI mock dashboard, the Rust mock REST server, and the LLM API proxy.
- `crates/ah-rest-server/src/bin/mock_server.rs` – Native mock REST server that accepts `--scenario <file|dir>` (repeatable) and `--scenario-speed <float>` to select fixtures and scale their playback timelines.
- `crates/llm-api-proxy/` – Uses the same crate to drive deterministic Anthropic/OpenAI mock responses, ensuring parity with the REST mock server.

### Scenario Selection & Playback Controls

- **Scenario discovery:** When multiple Scenario-Format files are provided (via `--scenario DIR` or configuration), the runtime uses Levenshtein distance between the incoming task prompt (or any `x-scenario-prompt` header for LLM proxy requests) and each scenario’s **effective initial prompt**, computed as the first `userInputs` after `sessionStart` (if present) or otherwise the first `userInputs` in the timeline. Explicit `x-scenario-name` headers or CLI overrides still take precedence.
- **Speed scaling:** Every timeline delay (`think`, `assistant`, `progress`, `toolExecution`, and `baseTimeDelta` entries) is multiplied by the `scenario-speed` factor (defaults to `1.0`). Values < 1.0 speed up playback; values > 1.0 slow it down. Current implementations clamp the multiplier to a minimum of `0.01` to avoid zero-duration events.
- **SSE catch-up:** The Rust mock REST server persists emitted events and replays the complete history to new SSE subscribers before streaming live updates, so clients that connect mid-scenario still receive a consistent timeline.
  **Mock ACP Server (ACP client test mode):**

- Translates scenario events into ACP messages sent to the client under test.
- For `runCmd` tool events, issues ACP terminal method calls so the client executes the command in its sandbox. In passthrough mode (see `specs/ACP.server.status.md`), it asks the client to invoke a `show-sandbox-execution` command so the client’s sandbox/recorder performs the command and streams output back via ACP terminal notifications.
- Handles `agentPermissionRequest` by sending `session/request_permission` and returning the scripted user decision.
- Handles `agentFileReads` by issuing ACP filesystem client calls (potentially multiple calls depending on tool profile).
- Uses `sessionStart.sessionId` and `expectedPromptResponse` to craft the `session/new` (or `session/load`) response and validate the first prompt turn after the boundary.
