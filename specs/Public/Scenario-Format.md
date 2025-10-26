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

### Top-Level Schema (high level)

```yaml
name: task_creation_happy_path
tags: ['cli', 'tui', 'local']
terminalRef: 'configs/terminal/default-100x30.json'
compat:
  allowInlineTerminal: true
  allowTypeSteps: true
initialPrompt: "Create a hello.py file that prints 'Hello, World!'"
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
  - advanceMs: 50
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
- **initialPrompt**: The initial prompt text that will be given to the agent when the scenario starts.
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
- **timeline[]** (unified event sequence):

#### LLM Response Events (`llmResponse`)

Groups multiple response elements into a single LLM API response. Contains
sub-events that get coalesced based on the target LLM API style (OpenAI vs
Anthropic). When the client requests streaming responses, the server
automatically converts this into a sequence of incremental streaming events
with preserved timing.

- `think`: Array of `[milliseconds, text]` pairs for agent thinking events. For
  OpenAI API style: thinking is processed internally but **NOT included in API
  responses** (matches OpenAI's behavior where thinking is never exposed). For
  Anthropic API style: thinking is exposed as separate "thinking" blocks in
  the response content array.
- `assistant`: Array of `[milliseconds, text]` pairs for assistant responses

#### Agent Action Events (`agentActions`)

Tool invocations and file operations performed by the agent. These represent the
actual work the agent does in response to LLM instructions.

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
  - `runCmd`: Execute terminal/shell commands. Fields: `cmd` (command string),
    `cwd` (optional working directory), `timeout` (optional milliseconds),
    `description` (optional description), `run_in_background` (optional boolean).
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

- `userInputs`: Array of `[milliseconds, input]` pairs for user input
  simulation. Can run concurrently with agent events. Fields:
  - `target`: `tui|webui|cli` (optional, defaults to all targets if not
    specified).
- `userEdits`: Simulate user editing files. Fields:
  - `patch`: Path to unified diff or patch file relative to the scenario folder.
- `userCommand`: Simulate user executing a command (not an agent tool call).
  Fields:
  - `cmd`: Command string to execute.
  - `cwd`: Optional working directory relative to the scenario.

#### Test and Control Events

Events that control test execution and validation.

- `advanceMs`: Advance logical time by specified milliseconds. Must be >= max
  time from concurrent events.
- `screenshot`: Ask harness to capture vt100 buffer with a label.
- `assert`: Structured assertions (see below).
- `complete`: Event indicating that the scenario task has completed
  successfully. This marks the session status as completed and triggers any
  completion logic.
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

  **Legacy support**: Individual `think`, `agentToolUse`, `agentEdits`, and `assistant` events at the top level are treated as single-element `llmResponse` groups for backward compatibility.

  Compatibility with existing scenarios (type‑based events):
  - Runners MUST also accept timeline of the form `{ "type": "advanceMs", "ms": 50 }`, `{ "type": "screenshot", "name": "..." }`, `{ "type": "userInputs", "inputs": [[100, "text"]] }`, `{ "type": "assertVm", ... }` as used in `test_scenarios/basic_navigation.yaml`.

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

- **Agent Events** execute sequentially and consume time based on their millisecond values
- **User Events** (userInputs, userEdits, userCommand) can execute concurrently with agent events
- **Test Events** (assert, screenshot) execute at specific timeline points
- **Time Advancement** (`advanceMs`) ensures proper synchronization: `advanceMs >= max(time_from_concurrent_events)`

### Event Execution by Component and Testing Mode

The scenario format supports three distinct testing modes, each processing different combinations of timeline events through specific components. Below are detailed specifications for how each component handles events in each mode.

#### 1. Mock-Agent Integration Tests

In this mode, the Agent Harbor CLI/TUI/WebUI is tested end-to-end using a
deterministic mock agent that simulates LLM responses while executing real tools
to produce actual side-effects in the test environment. The test executor
simulates user interactions, and the mock agent handles both LLM response
simulation and tool execution.

**Mock-Agent Component:**

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
- Handles test control events (`advanceMs`, `screenshot`, `assert`) by
  controlling timing, capturing screenshots, and validating assertions
- Executes `complete` and `merge` events by modifying an in-memory database
  instance created for each test

**Event Processing Summary:**

- `llmResponse` → Mock-Agent (simulated output only)
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
  agent)
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
- Handles all test control events (`advanceMs`, `screenshot`, `assert`) for
  timing control and validation
- Maintains accurate event timing and sequencing to simulate realistic session
  flow
- Does not execute any actual tools or modify files - everything is event
  streaming only

**Mock LLM API Server:**

- Not used in simulation mode (simulation happens at the REST API level)

**Test Executor:**

- May be used to simulate a stremed events sessions with a remote Agent Harbor REST server
- Doesn't process `assert` events in this mode as real-side effects are not produced.
- Handles `screenshot` events if testing visual components

**Event Processing Summary:**

- `llmResponse` → Mock API Server (streamed as SSE events)
- `agentActions` → Mock API Server (simulated execution via `toolExecution`
  details)
- `userActions` → Mock API Server (streamed as user interaction events)
- Test/Control events → Mock API Server and Test Executor

**Example Timeline:**

```yaml
timeline:
  # LLM Response Events - API level interactions
  - llmResponse:
      - think:
          - [500, 'I need to examine the current code first'] # 500ms
          - [300, 'Let me check what functions exist...'] # 300ms, total: 800ms
      - assistant:
          - [200, 'Let me search for function definitions in the codebase.'] # 200ms

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
  - advanceMs: 1000 # Must be >= max concurrent event times
```

**Example with Tool Execution Details (Simulation Mode):**

```yaml
timeline:
  # LLM response instructing tool use
  - llmResponse:
      - think:
          - [300, 'The user wants me to run tests']
      - assistant:
          - [100, 'Running the test suite to check for any issues.']

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
          - [700, 'q']  # User presses 'q' during test execution
        target: 'tui'

  - advanceMs: 1000
```

**Timing Rules:**

- Agent events never overlap (sequential execution)
- User and test events can execute concurrently with agent events
- `advanceMs` ensures proper synchronization: `advanceMs >= max(time_from_concurrent_events)`
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
  - `target/tmp/<runner>/<scenarioName>/<terminalProfileId>/<timestamp>-<pid>/`

### Conventions

- All paths are relative to the temporary test workspace root unless prefixed with `/`.
- Shell tokens in `ah.flags[]` are not re‑parsed; runners pass them verbatim.
- Time values are in milliseconds; millisecond precision enables deterministic scenario replay.
- Agent events execute sequentially; user and test events can execute concurrently with agent events.
- `advanceMs` values must account for all concurrent activity to maintain proper synchronization.

### References

- CLI behaviors and flow: [CLI.md](CLI.md)
- TUI testing approach: [TUI-Testing-Architecture.md](TUI-Testing-Architecture.md)
- CLI E2E plan: [Testing-Architecture.md](Testing-Architecture.md)
- Terminal config format: [Terminal-Config.md](Terminal-Config.md)
- Existing example scenario: `tests/tools/mock-agent/scenarios/realistic_development_scenario.yaml` (comprehensive timeline-based scenario with ~30-second execution)
