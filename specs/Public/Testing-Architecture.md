## Testing Architecture — ah task Verification and Scenario Framework

### Purpose

This document defines a practical, scenario-driven testing architecture for verifying the `ah task` workflow end-to-end. It translates the high-level behaviors and flow chart in `CLI.md` into concrete, automated, and reproducible tests that exercise real agent execution paths using a mock agent and terminal capture.

This architecture complements and reuses the testing foundations from the TUI work (vt100 + insta golden snapshots, scenario runners) to ensure consistency across CLI, TUI, and service components.

### Involved Components

- **AH CLI (`ah task`)**: Entry point under test. Behaviors and decision points follow the flow and rules in [CLI.md](CLI.md).
- **Mock Agent**: Hidden agent kind launched via `--agent=mock` (test‑only); additional parameters forwarded with `--agent-flags "..."`. Capabilities and usage per [AGENTS.md](../../tests/tools/mock-agent/AGENTS.md).
- **Scenario Files (Shared YAML)**: Drive inputs (flags, env, repo state), mock‑agent steps, snapshot labels, and expected artifacts. Shared across CLI E2E, TUI tests, WebUI tests, and mock API server (see [Scenario-Format.md](Scenario-Format.md)).
- **Terminal Capture Harness**: vt100 parser + insta golden snapshots reused from TUI tests to record human‑readable "terminal screenshots" at labeled moments.
- **Test‑Executor with RPC (test‑only)**: The test entry point - a process that sets up the terminal capture harness, launches ah task with the mock agent, provides an RPC endpoint which the mock-agent uses to notify the text-executor when a certain event in the scenario is reached. Once an event of interest is reached, the executor can verify the state of the file system (or other expected side-effects from the session), it can take a snapshot of the UI and compare it with a golden reference snapshot, etc.
The test executor knows how to handle different scenario regimes such as "mock remote server", "mock local agent", "mock LLM API server with real agent software", etc.
- **Mock API Server (optional, TA‑3)**: Used to exercise remote mode and REST contracts during CLI tests. See `webui/mock-server` documentation ([README](../webui/mock-server/README.md)).
- **Logs and Artifacts**: Unique per‑run log directory, golden snapshots, and a structured summary JSON emitted by the runner.

### Scope and Principles

- Directly map verification to the `ah task` flow (see [CLI.md](CLI.md)) and its decision points: inside‑repo vs outside‑repo, interactive vs non‑interactive, branch creation, task files, metadata‑only commits, push behavior, working copy and snapshot provider selection, and local vs remote execution.
- Prefer black‑box, E2E tests using a real agent process (mock agent) and real terminal capture; minimize mocking.
- Preserve fast feedback: scenario execution completes in seconds, with focused coverage per decision path.
- Each test writes a unique log file and prints its path on failure (per repository testing guidelines).

### Building Blocks

1) Mock Agent Integration

- Hidden agent type for test harnesses: `--agent=mock` (intentionally undocumented in help screens) launches the mock agent instead of real agents.
- Pass‑through flags to agent binaries: `--agent-flags "..."` forwards additional parameters verbatim to the agent process, enabling scenario/behavior selection without polluting the AH flag space.
- Test coverage uses the mock agent’s capabilities documented in `tests/tools/mock-agent/AGENTS.md` (see [AGENTS.md](../../tests/tools/mock-agent/AGENTS.md)).

2) Terminal Capture and Golden Snapshots

- Reuse the vt100 parser + insta golden snapshot setup from TUI testing to capture stable, human‑readable "terminal screenshots" during CLI E2E runs.
- Introduce a lightweight test‑executor RPC that the mock agent can call at key moments to request snapshot capture from the test harness. Requests include a semantic label (e.g., `after_branch_created`, `after_task_file_written`, `error_prompt_shown`).
  - Minimal shape (local test runner): `POST /snapshot { label: string }` → stores a labeled vt100 snapshot and returns `{ path }`.
  - The RPC is only available in test builds and is not part of the shipping product.

3) Scenario Format (Shared)

- Use a shared, file‑based scenario format (YAML) to orchestrate inputs and expected events across (see [Scenario-Format.md](Scenario-Format.md)):
  - Mock Agent (steps, pauses, outputs)
  - Mock API Server (REST mode)
  - TUI tests (existing) and CLI E2E (this document)
- Scenarios define: inputs (flags, environment, repo state), agent behavior script (for mock), snapshot labels to capture, and expected artifacts (files, commits, exit codes).

### Verification Matrix — Mapping to ah task Flow

The following test groups map directly to the decision nodes and behaviors in the `ah task` flow from [CLI.md](CLI.md). All tests run with `--agent mock` (hidden) and may use `--agent-flags` to drive specific mock behaviors. Initial milestones use `--working-copy in-place` and keep snapshots disabled unless the case explicitly requires otherwise.

TA‑1 In‑Place Local Mode (Foundational E2E)

- Inside repo, task files enabled (default):
  - `ah task --prompt "..." --agent mock --working-copy in-place --fs-snapshots disable`
  - Verify: task file path `.agents/tasks/YYYY/MM/DD-HHMM-<branch>` created; first commit includes metadata lines per spec; vt100 snapshot captured after commit.
- Inside repo, task files disabled + metadata commit enabled:
  - `--create-task-files no --create-metadata-commits yes`
  - Verify: no task file created; metadata‑only commit present; vt100 snapshots around commit step.
- Editor path (interactive):
  - No `--prompt/--prompt-file`; verify editor template, abort on empty content; exit is non‑error; vt100 snapshot of template/help lines.
- Non‑interactive guardrails:
  - `--non-interactive` without sufficient inputs (e.g., missing `--branch` outside a repo choice) returns exit code 10 with required message.
- Branch name validation:
  - Invalid names rejected with clear error; valid names allowed with correct branching rules (no tasks on primary branches).
- Push decision:
  - With no `--push-to-remote` and no `--yes`, verify prompt shown; with `--yes` or `--push-to-remote true`, verify no prompt path taken. (Network I/O can be stubbed; focus on prompt behavior and branching logic.)

TA‑2 Provider and Working Copy Resolution (Smoke)

- Resolution reporting only (no actual mounts):
  - `--working-copy in-place|worktree|cow-overlay` with `--fs-snapshots auto|git|disable` → verify resolved values and emitted selection records match rules in [FS-Snapshots-Overview](FS-Snapshots/FS-Snapshots-Overview.md). (Local DB entries asserted when applicable.)
- Git fallback snapshot (summary path only, not exercising external tools):
  - `--fs-snapshots git --working-copy worktree` with mock agent run; verify selection and scenario artifacts, but avoid external git calls beyond repository initialization used by test harness.

TA‑3 Remote Mode Skeleton (Contract Surface)

- With mock API server only (no executors), verify CLI selects remote mode flags and emits correct requests/states:
  - Authentication flag plumbed; minimal endpoint smoke with mock server; ensure CLI correctly refuses interactive‑only flows in non‑interactive remote mode as per [CLI.md](CLI.md).

### Test Orchestration and Artifacts

- Each scenario produces:
  - A unique per‑run log directory: `target/tmp/ah-cli-e2e/<scenario>/<terminalProfileId>/<timestamp>-<pid>/`
  - Terminal golden snapshots: `.../<scenario>/<terminalProfileId>/snapshots/<label>.golden`
  - Structured summary JSON: `{ exitCode, snapshots:[...], artifacts:[...], assertions:[...] }`
- On failure, the runner prints the log directory path and size; golden diffs use insta output for quick iteration.

### Flag Specification (Testing Hooks)

- `--agent=mock` (hidden): reserved for tests; launches the mock agent instead of a real agent. Not listed in help text to avoid user confusion. Behavior is identical to other agents from the CLI’s perspective.
- `--agent-flags <ARGS...>` (documented): additional flags passed verbatim to the agent binary (quoted as a single argument string by the shell; AH does no parsing). Example: `--agent-flags "--scenario basic --fast"`.

### Milestones and Verification Criteria

Milestone TA‑1 — In‑Place E2E with Mock Agent

- Deliverables:
  - Scenario runner for CLI E2E using vt100 + insta
  - Test‑executor RPC for labeled snapshots
  - Core scenarios covering inside‑repo flow, task files on/off, non‑interactive guards, branch validation, push prompt
- Verification:
  - [x] Golden snapshots captured at labeled points
  - [x] Task artifacts match spec (paths, commit metadata)
  - [x] Exit codes and messages match spec for guardrails

Milestone TA‑2 — Resolution Smoke (Working Copy + Provider)

- Deliverables:
  - Scenarios enumerating combinations of `--working-copy` and `--fs-snapshots`
  - Assertions on resolved values and emitted records (no mounts)
- Verification:
  - [x] Resolved values match [CLI.md](CLI.md) and [FS-Snapshots-Overview](FS-Snapshots/FS-Snapshots-Overview.md)

Milestone TA‑3 — Remote Mode Surface with Mock API

- Deliverables:
  - Minimal mock server scenarios for authentication and task creation
  - CLI contract assertions for non‑interactive behavior and SSE wiring points (no live SSE needed yet)
- Verification:
  - [x] Requests/flags conform to [REST-Service/API.md](REST-Service/API.md) surfaces used by CLI

### Execution and CI

- Add Just targets for local runs, e.g., `ah-cli-e2e <scenario>`; CI runs the foundational TA‑1 matrix and a subset of TA‑2.
- Tests run in a temporary git repository created by the harness to ensure deterministic inside‑repo behavior.
- No network is required for TA‑1; TA‑3 uses local mock server.

### References

- CLI flow and behaviors: [CLI.md](CLI.md)
- TUI testing foundations and status: [TUI.status.md](TUI.status.md)
- Mock agent test guide: [AGENTS.md](../../tests/tools/mock-agent/AGENTS.md)
- Snapshot providers overview: [FS-Snapshots-Overview](FS-Snapshots/FS-Snapshots-Overview.md)
- REST service contract (for TA‑3 scope): [REST-Service/API.md](REST-Service/API.md)


