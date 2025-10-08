### Status

- Status: Draft
- Last Updated: September 26, 2025

### Overview

This document specifies the testing architecture for the agent-harbor TUI. The goal is to provide a deterministic, comprehensive, and scalable test system that validates user flows end-to-end while keeping the majority of logic tests fast and reliable.

The architecture combines:

- A pure state-machine driven GUI with fake time
- MVVM rendering separation for testability
- Scenario-driven mock REST client using schema types from `ah-rest-api-contract`
- Automated and interactive test runners
- Assertions on both ViewModel state and golden files rendered via Ratatui `TestBackend`

### Design Principles

- Determinism first: tests must be reproducible with fake time and controlled inputs
- Separation of concerns: domain Model, ViewModel (presentation), and View (Ratatui widgets)
- Black-box friendly: end-to-end tests through scenario playback without network or real terminals
- Extensibility: scenario format grows with product features; same scenarios used across TUI/WebUI when feasible

### Architecture

1. State Machine Core

- The TUI runtime funnels all external stimuli into typed messages/events consumed by a state-machine (the Model’s `update(msg)`):
  - Keyboard input (mapped into `Msg::Key`)
  - Time (mapped into `Msg::Tick`), driven by Tokio fake time during tests
  - REST results and SSE streams (mapped into `Msg::Net(…)` variants reflecting `ah-rest-api-contract` types)

2. MVVM Layering

- Model: domain state and rules (no I/O, no Ratatui). Fully unit-testable by supplying the state machine with new events without involving any rendering
- ViewModel: derived presentation state shaped for rendering (strings, selection flags). Fully unit-testable by testing state transitions and property derivations without UI rendering
- View: pure Ratatui rendering from `&ViewModel`, used by both the app and tests via `TestBackend`

3. Scenario-Driven Mocking

- A new Rust crate `ah-test-scenarios` reads a structured scenario format (compatible with the mock-server inputs)
- A `ah-rest-client-mock` crate implements the same public trait(s)/interface as the real REST client but is fed by `ah-test-scenarios` to return prespecified responses
- Scenarios express sequences of:
  - User actions (key presses, filters, selections)
  - Agent actions and state transitions
  - SSE events (status/log/moment/delivery, matching `ah-rest-api-contract` event types)
  - Timing directives (logical time or steps to advance fake time)

4. Runners

- Automated Runner: consumes a scenario head-to-tail, driving the TUI in a fixed-size `TestBackend` terminal; emits assertions and golden files
- Interactive Runner: starts at a specified step (CLI option) or prompts; supports step-forward, jump-to-step, and replay

### Data & Types

- All REST entities and SSE events in scenarios use the canonical types from `ah-rest-api-contract` to avoid drift between product and tests
- Scenario file schema (high-level):

```json
{
  "name": "task_creation_happy_path",
  "terminal": { "width": 100, "height": 30 },
  "steps": [
    { "advanceMs": 50 },
    { "sse": { "type": "Status", "status": "Running" } },
    { "key": "Tab" },
    { "key": "Down" },
    { "assertVM": { "focus": "repository", "selected": 1 } },
    { "snapshot": "after_repo_select" }
  ]
}
```

Note: Concrete schema lives with `ah-test-scenarios` and is validated in CI.

### Execution Flow

1. Load scenario (JSON/YAML) → validate
2. Initialize runtime with `TestBackend` (width/height) and fake time
3. For each scenario step:
   - Inject keys/events or advance time
   - Call single deterministic `step()` that handles exactly one message and draws once
   - Run assertions (ViewModel and/or golden file)
4. Emit report (per-step logs, failures, golden file diffs)

### Assertions

- ViewModel Assertions: inspect derived state (focus, selections, error banners, footer hints)
- Golden Files: serialize Ratatui buffer from `TestBackend` and compare
  - Prefer stable golden files (strip volatile metadata, normalize whitespace)
  - Store under `crates/ah-tui/tests/__goldens__/<scenario>/<step>.golden`

### Tooling & Libraries

- Ratatui `TestBackend` for deterministic rendering
- Tokio test with `#[tokio::test(start_paused = true)]` and `time::advance()`
- Optional `insta` for golden file assertions
- Optional PTY/E2E layer (future): `expectrl`/`portable-pty` for black-box terminal tests

### CLI

```
tui-test run [OPTIONS] <SCENARIO_PATH>

DESCRIPTION: Run the TUI against a scenario file and validate assertions

OPTIONS:
  --start-step <n>            Start from step index n (0-based)
  --until-step <n>            Stop after step index n (inclusive)
  --update-goldens            Update golden files on disk instead of asserting
  --terminal-width <cols>     Override terminal width (falls back to scenario)
  --terminal-height <rows>    Override terminal height (falls back to scenario)
  --seed <value>              Seed for any randomized components (if any)
  --report <path>             Write a JSON report of results and timing

ARGUMENTS:
  SCENARIO_PATH               Path to JSON/YAML scenario
```

```
tui-test play [OPTIONS] <SCENARIO_PATH>

DESCRIPTION: Interactive player for stepping through a scenario

OPTIONS:
  --start-step <n>            Start from step index n (0-based)
  --jump <n>                  Jump directly to step index n after load
  --headless                  Run without opening a real TTY (uses TestBackend)
  --trace                     Record per-step VM state and buffer for debugging

ARGUMENTS:
  SCENARIO_PATH               Path to JSON/YAML scenario
```

### Package & Code Layout

- `crates/ah-tui/` TUI app (Model/ViewModel/View + runtime)
- `crates/ah-rest-api-contract/` canonical API/SSE types
- `crates/ah-rest-client/` real client
- `crates/ah-test-scenarios/` scenario loader + validator
- `crates/ah-rest-client-mock/` client trait impl backed by scenarios
- `crates/ah-tui/tests/` scenario-backed tests, VM assertions, golden files

### Verification Strategy

- Unit Tests (Model, ViewModel): comprehensive, no terminal or async
- Rendering Tests: minimal golden files for critical screens/states
- Scenario Tests: automated runner across curated scenarios in CI
- Interactive Debugging: use `ah tui-test play` to diagnose failures locally

### Risks & Mitigations

- Golden file brittleness → normalize buffer output, keep golden files minimal and focused
- Scenario drift vs API → reuse `ah-rest-api-contract` types and validate scenarios in CI
- Flakiness → use fake time; single-drah-per-step; avoid parallel UI draws

### Future Work

- Optional full PTY E2E for critical flows with `expectrl`
- Cross-terminal visual checks (themes, contrast) via golden file variants
- Load testing for high-frequency SSE streams with time advancing
