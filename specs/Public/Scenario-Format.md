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

- UTF‑8 YAML; comments allowed.
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
  - `llmApiStyle`: `openai|anthropic` (determines response coalescing rules, defaults to `openai`).
  - `coalesceThinkingWithToolUse`: `true|false` (only used for Anthropic, defaults to `true`).
  - Optional seed objects for mock server endpoints.
- **timeline[]** (unified event sequence):
  - `llmResponse`: **NEW** - Groups multiple response elements into a single LLM API response. Contains sub-events that get coalesced based on the target LLM API style (OpenAI vs Anthropic).
    - `think`: Array of `[milliseconds, text]` pairs for agent thinking events. For OpenAI API style: thinking is processed internally but **NOT included in API responses** (matches OpenAI's behavior where thinking is never exposed). For Anthropic API style: thinking is exposed as separate "thinking" blocks in the response content array.
    - `runCmd`: Execute terminal/shell commands. Fields: `cmd` (command string), `cwd` (optional working directory), `timeout` (optional milliseconds), `description` (optional description), `run_in_background` (optional boolean).
    - `grep`: Search for patterns in files. Fields: `pattern`, `path`, `glob` (optional glob pattern), `output_mode` (optional: content/files_with_matches/count), `-B` (optional before context), `-A` (optional after context), `-C` (optional context), `-n` (optional line numbers), `-i` (optional case insensitive), `type` (optional file type), `head_limit` (optional result limit), `multiline` (optional multiline mode).
    - `readFile`: Read file contents. Fields: `path`, `encoding` (optional, defaults to utf-8), `offset` (optional line offset), `limit` (optional line limit).
    - `listDir`: List directory contents. Fields: `path`, `recursive` (optional boolean), `pattern` (optional glob pattern).
    - `find`: Find files by pattern. Fields: `path`, `name` (filename pattern), `type` (optional: file/dir).
    - `sed`: Stream editor operations. Fields: `expression`, `path`, `inplace` (optional boolean).
    - `editFile`: Edit file with exact string replacements. Fields: `path`, `old_string`, `new_string`, `replace_all` (optional boolean).
    - `writeFile`: Write content to file. Fields: `path`, `content`.
    - `task`: Launch specialized agent for complex tasks. Fields: `description`, `prompt`, `subagent_type`.
    - `webFetch`: Fetch content from URL with AI analysis. Fields: `url`, `prompt`.
    - `webSearch`: Search the web for information. Fields: `query`, `allowed_domains` (optional array), `blocked_domains` (optional array).
    - `todoWrite`: Manage structured task lists. Fields: `todos` (array of todo objects).
    - `notebookEdit`: Edit Jupyter notebook cells. Fields: `notebook_path`, `cell_id` (optional), `new_source`, `cell_type` (optional), `edit_mode` (optional: replace/insert/delete).
    - `exitPlanMode`: Exit plan mode with summary. Fields: `plan`.
    - `bashOutput`: Retrieve output from background bash shell. Fields: `bash_id`, `filter` (optional regex).
    - `killShell`: Kill running background bash shell. Fields: `shell_id`.
    - `slashCommand`: Execute slash command. Fields: `command`.
    - `agentEdits`: File modification event with path and change metrics. Gets mapped to appropriate file editing tools for the target agent.
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
  - `complete`: Event indicating that the scenario task has completed successfully. This marks the session status as completed and triggers any completion logic.
  - `merge`: Event indicating that this scenario session should be merged into the session list upon completion. When present, the scenario session will be marked as completed but remain visible in session listings. When omitted, completed scenarios are not shown in session listings.


  **Legacy support**: Individual `think`, `agentToolUse`, `agentEdits`, and `assistant` events at the top level are treated as single-element `llmResponse` groups for backward compatibility.
  
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
- **Started with a specific tools profile** (server startup option) that defines valid tool schemas for the target coding agent (Codex, Claude, Gemini, etc.)
- Groups **`llmResponse`** events into single API responses with appropriate coalescing based on `llmApiStyle`
- **Validates client tool definitions** sent in API requests against the tools profile; in strict tools validation mode (server startup option --strict-tools-validation), aborts immediately on unknown tools to help identify missing mappings during development
- For OpenAI: Thinking is processed internally but **NOT included in API responses** - only text content and tool calls appear in the assistant message (matches OpenAI's actual API behavior)
- For Anthropic: Can include thinking blocks, text, and tool_use blocks in a single response when `coalesceThinkingWithToolUse` is enabled
- Instructs the coding agent via mock LLM API responses to perform tool invocations

**Test Executor:**
- **`userInputs`** are carried out in all testing modes (TUI, WebUI, CLI)
- **`userEdits`** are carried out by the test executor in all modes except WebUI testing with mock API server (where they can be ignored)
- **`userCommand`** events are executed by the test executor as shell commands

**Example Timeline:**
```yaml
timeline:
  # Single LLM response combining thinking + tool use (realistic)
  - llmResponse:
      - think:
          - [500, "I need to examine the current code first"]      # 500ms
          - [300, "Let me check what functions exist..."]          # 300ms, total: 800ms
      - agentToolUse:
          toolName: "run_command"
          args:
            command: "grep -n 'def' main.py"
            cwd: "."
          progress:
            - [200, "Searching for function definitions..."]
          result: "main.py:1:def main():"
          status: "ok"  # 200ms total

  # Test harness events (handled separately from API responses)
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
