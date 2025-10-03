## Scenario Format — Shared Test Scenarios for CLI/TUI/WebUI/Mock Server

### Purpose

Define a single scenario format used by and compatible with existing scenarios:

- CLI E2E tests with mock agent (Testing-Architecture)
- TUI automated and interactive runners (see [TUI-Testing-Architecture.md](TUI-Testing-Architecture.md))
- Mock API server seeds and scripted responses (where applicable)

Goals: determinism, reuse, and clarity across products.

### File Format

- JSON (YAML optional in runners); UTF‑8; comments disallowed in JSON.
- Top-level keys are stable; unknown keys ignored (forward-compatible).

### Top-Level Schema (high level)

```json
{
  "name": "task_creation_happy_path",
  "tags": ["cli", "tui", "local"],
  "terminalRef": "configs/terminal/default-100x30.json",
  "compat": {
    "allowInlineTerminal": true,
    "allowTypeSteps": true
  },
  "repo": {
    "init": true,
    "branch": "feature/test",
    "dir": "./repo", 
    "files": [
      { "path": "README.md", "contents": "hello" }
    ]
  },
  "ah": {
    "cmd": "task",
    "flags": ["--agent=mock", "--working-copy", "in-place"],
    "env": { "AH_LOG": "debug" }
  },
  "mockAgent": {
    "flags": ["--scenario", "basic"],
    "steps": [
      { "emit": "thinking", "text": "I'll create the file" },
      { "rpcSnapshot": "after_task_file_written" }
    ]
  },
  "server": {
    "mode": "none"
  },
  "steps": [
    { "advanceMs": 50 },
    { "assert": { "fs": { "exists": [".agents/tasks"] } } },
    { "snapshot": "after_commit" },
    { "applyPatch": { "path": "./patches/add_file.patch", "commit": true, "message": "Apply scenario patch" } }
  ],
  "expect": {
    "exitCode": 0,
    "artifacts": [
      { "type": "taskFile", "pattern": ".agents/tasks/*" }
    ]
  }
}
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
- **mockAgent**:
  - `flags[]`: Additional tokens forwarded via `--agent-flags`.
  - `steps[]`: Agent script with semantic events (e.g., `emit`, `rpcSnapshot`, `pauseMs`).
- **server**:
  - `mode`: `none|mock|real` (tests typically use `none` or `mock`).
  - Optional seed objects for mock server endpoints.
- **steps[]** (runner-driven):
  - `advanceMs`: Advance logical time.
  - `snapshot`: Ask harness to capture vt100 buffer with a label.
  - `assert`: Structured assertions (see below).
  - `keys`: For TUI, send key presses; for CLI, may be ignored.
  - `userEdits`: Simulate user editing files. Fields:
    - `patch`: Path to unified diff or patch file relative to the scenario folder.
  - `userCommand`: Simulate user executing a command (not an agent tool call). Fields:
    - `cmd`: Command string to execute.
    - `cwd`: Optional working directory relative to the scenario.

  Compatibility with existing scenarios (type‑based steps):
  - Runners MUST also accept steps of the form `{ "type": "advanceMs", "ms": 50 }`, `{ "type": "snapshot", "name": "..." }`, `{ "type": "key", "key": "Up" }`, `{ "type": "assertVm", ... }` as used in `test_scenarios/basic_navigation.json`.
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

### RPC Snapshot Integration

- `mockAgent.steps[].rpcSnapshot`: Label emitted by the agent to ask the harness to capture a snapshot via the test‑executor RPC.
- The harness stores snapshots under a directory that includes both scenario and terminal profile identifiers (see Snapshot Paths) and includes their paths in the report.

### Snapshot Paths (Scenario × Terminal)

- Snapshot directory scheme:
  - `target/tmp/<runner>/<scenarioName>/<terminalProfileId>/snapshots/<label>.golden`
  - `terminalProfileId` is taken from the terminal config `name` if present; otherwise computed as `<width>x<height>`.
- Log directory scheme (example):
  - `target/tmp/<runner>/<scenarioName>/<terminalProfileId>/<timestamp>-<pid>/`

### Conventions

- All paths are relative to the temporary test workspace root unless prefixed with `/`.
- Shell tokens in `ah.flags[]` and `mockAgent.flags[]` are not re‑parsed; runners pass them verbatim.
- Keep scenarios small and focused; prefer composing many short scenarios.

### References

- CLI behaviors and flow: [CLI.md](CLI.md)
- TUI testing approach: [TUI-Testing-Architecture.md](TUI-Testing-Architecture.md)
- CLI E2E plan: [Testing-Architecture.md](Testing-Architecture.md)
 - Terminal config format: [Terminal-Config.md](Terminal-Config.md)
 - Existing example scenario: `test_scenarios/basic_navigation.json` (type‑based steps and inline terminal)


