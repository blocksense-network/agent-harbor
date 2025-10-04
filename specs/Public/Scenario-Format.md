## Scenario Format — Shared Test Scenarios for CLI/TUI/WebUI/Mock Server

### Purpose

Define a single scenario format used by and compatible with existing scenarios:

- CLI E2E tests with mock agent (Testing-Architecture)
- TUI automated and interactive runners (see [TUI-Testing-Architecture.md](TUI-Testing-Architecture.md))
- Mock API server responses and test orchestration (where applicable)

Goals: determinism, reuse, and clarity across products.

### Component Architecture

The testing infrastructure consists of several components working together:

- **Mock-Agent**: A deterministic agent implementation that executes scenarios and modifies the test workspace
- **Mock API Server**: Implements the [Agent Harbor REST API](../REST-Service/API.md) for CLI/WebUI/TUI testing
- **Mock LLM API Server**: Simulates external LLM services (Anthropic, OpenAI, etc.) that coding agents communicate with
- **Test Executor**: Orchestrates scenario execution and handles user interactions

### File Format

- YAML only; UTF‑8; comments allowed.
- Top-level keys are stable; unknown keys ignored (forward-compatible).

### Top-Level Schema (high level)

```yaml
name: task_creation_happy_path
tags: ["cli", "tui", "local"]
terminalRef: "configs/terminal/default-100x30.json"
compat:
  allowInlineTerminal: true
  allowTypeSteps: true
repo:
  init: true
  branch: "feature/test"
  dir: "./repo"
  files:
    - path: "README.md"
      contents: "hello"
ah:
  cmd: "task"
  flags: ["--agent=mock", "--working-copy", "in-place"]
  env:
    AH_LOG: "debug"
server:
  mode: "none"
timeline:
  - think:
      - [500, "Analyzing the user's request"]
      - [800, "I need to create a hello.py file"]
  - agentToolUse:
      toolName: "run_command"
      args:
        command: "python hello.py"
        cwd: "."
      progress:
        - [300, "Running Python script..."]
        - [100, "Script executed successfully"]
      result: "Hello, World!"
      status: "ok"
  - agentEdits:
      path: "hello.py"
      linesAdded: 1
      linesRemoved: 0
  - screenshot: "after_task_file_written"
  - advanceMs: 50
  - assert:
      fs:
        exists: [".agents/tasks"]
  - screenshot: "after_commit"
  - applyPatch:
      path: "./patches/add_file.patch"
      commit: true
      message: "Apply scenario patch"
expect:
  exitCode: 0
  artifacts:
    - type: "taskFile"
      pattern: ".agents/tasks/*"
```

### Sections

- **name**: Scenario identifier (string).
- **tags**: Array of labels to filter/select scenarios in runners.
- **terminalRef**: Optional path to a terminal configuration file describing size and rendering options. See [Terminal-Config.md](Terminal-Config.md). When omitted, runners use their defaults.
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
  - Optional seed objects for mock server endpoints.
- **timeline[]** (unified event sequence):
  - `think`: Array of `[milliseconds, text]` pairs for agent thinking events
  - `agentToolUse`: External tool/command execution with progress array, tool name, and final result
  - `agentEdits`: File modification event with path and change metrics
  - `assistant`: Array of `[milliseconds, text]` pairs for assistant responses
  - `advanceMs`: Advance logical time by specified milliseconds. Must be >= max time from concurrent events.
  - `screenshot`: Ask harness to capture vt100 buffer with a label.
  - `assert`: Structured assertions (see below).
  - `userInputs`: Array of `[milliseconds, input]` pairs for user input simulation. Can run concurrently with agent events. Fields:
    - `target`: `tui|webui|cli` (optional, defaults to all targets if not specified).
  - `userEdits`: Simulate user editing files. Fields:
    - `patch`: Path to unified diff or patch file relative to the scenario folder.
  - `userCommand`: Simulate user executing a command (not an agent tool call). Fields:
    - `cmd`: Command string to execute.
    - `cwd`: Optional working directory relative to the scenario.

  Compatibility with existing scenarios (type‑based events):
  - Runners MUST also accept timeline of the form `{ "type": "advanceMs", "ms": 50 }`, `{ "type": "screenshot", "name": "..." }`, `{ "type": "userInputs", "inputs": [[100, "text"]] }`, `{ "type": "assertVm", ... }` as used in `test_scenarios/basic_navigation.yaml`.
- **expect**:
  - `exitCode`: Expected process exit code.
  - `artifacts[]`: File/glob expectations after run.

### Assertions

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

### Event Execution by Component

**Mock-Agent Execution:**
- **`agentToolUse`** events with `toolName: "run_command"` are actually executed by the mock-agent
- **`agentEdits`** events are carried out by the mock-agent (file modifications happen in the test workspace)
- When the `--checkpoint-cmd` option is provided, the mock-agent must automatically execute the specified command after each `agentEdits` and `agentToolUse` event completes (after the edits are carried out and the tool execution finishes). The typical command to use is `ah agent fs snapshot` which captures the filesystem state changes for time travel functionality. By default, no checkpoint commands are executed.

**Mock API Server (Agent Harbor REST API):**
- Implements the [Agent Harbor REST API](../REST-Service/API.md) for CLI/WebUI/TUI integration testing
- Receives task creation requests and returns session IDs
- Streams session events and status updates
- Uses **`agentToolUse.progress`** events to simulate command execution without executing them
- Reports/simulates **`agentEdits`** events

**Mock LLM API Server (External LLM Services):**
- Simulates external LLM APIs (Anthropic, OpenAI, GitHub Copilot, etc.) that coding agents communicate with
- Translates **`agentEdits`** events to tool use suggestions through the LLM API
- Instructs the coding agent via mock LLM API responses to perform the `run_command` tool invocations

**Test Executor:**
- **`userInputs`** are carried out in all testing modes (TUI, WebUI, CLI)
- **`userEdits`** are carried out by the test executor in all modes except WebUI testing with mock API server (where they can be ignored)
- **`userCommand`** events are executed by the test executor as shell commands

**Example Timeline:**
```yaml
timeline:
  - think:
      - [500, "Thinking..."]      # 500ms
      - [300, "More thinking..."] # 300ms, total: 800ms
  - agentToolUse:
      toolName: "run_command"
      args:
        command: "grep -n 'def' main.py"
        cwd: "."
      progress:
        - [200, "Searching for function definitions..."]
      result: "main.py:1:def main():"
      status: "ok"  # 200ms total
  - userInputs:
      - [200, "some input"]   # 200ms
      - [400, "more input"]   # 400ms, total: 600ms, concurrent with agent
    target: "tui"
  - assert:
      fs:
        exists: ["main.py"]
  - advanceMs: 1000  # Must be >= max(1000ms agent, 600ms user) = 1000ms
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