### Overview

This document tracks the implementation status of the CLI Cloud Automation functionality and serves as the single source of truth for the execution plan, milestones, automated success criteria, and integration points.

Goal: Deliver production-ready cloud agent execution support with browser automation workers, real-time streaming, provider-specific adapters, and seamless integration with agent browser profiles for authenticated cloud platform access.

Approach: Build reusable Rust components for browser automation and provider adapters with custom streaming. Integrate with the existing agent browser profiles system for seamless authentication. Execute in phases with parallelizable tracks for different cloud providers.

### Component architecture (parallel tracks)

- `crates/agent-browser-profiles`: Core library for discovering, validating, and managing browser profiles per [Agent-Browser-Profiles.md](Agent Browsers/Agent-Browser-Profiles.md).
- `crates/cloud-automation-core`: Core browser automation engine and session handling.
- `crates/cloud-providers`: Provider-specific adapters with custom streaming for monitoring and control (OpenAI Codex, GitHub Copilot, Cursor Claude, Google Jules).
- `bins/cloud-worker`: Rust binary for browser automation workers with isolated execution.
- `apps/agent-harbor-desktop` (Electron): Desktop app that bundles the Node.js runtime and Chromium engine used as the browser automation host. Cloud workers delegate automation to this Electron host, which creates per-task hidden/visible windows bound to Agent Browser Profiles.

All crates target stable Rust with cross-platform browser automation support. Browser automation is driven by Rust (`cloud-worker`) but executed inside an Electron host that reuses the shipped Node.js and Chromium runtimes.
Compatibility note: ensure the bundled Electron/Node version stays within Playwright’s supported range (currently Node 20/22/24) and lock it in CI checks.

### Milestones and tasks (with automated success criteria)

M0. Agent browser profiles crate and project bootstrap (3–4d)

- Create `crates/agent-browser-profiles` crate implementing the [Agent-Browser-Profiles.md](Agent Browsers/Agent-Browser-Profiles.md) specification.
- Implement profile discovery, validation, and management APIs.
- Add cross-platform base directory resolution (Linux/macOS/Windows).
- Initialize Cargo workspace for remaining cloud automation crates.
- Set up CI: build + test on Linux/macOS/Windows.
- Success criteria (unit + integration tests):
  - Profile directory resolution works correctly across platforms.
  - Profile metadata parsing and validation (schema v1).
  - Profile discovery finds existing profiles by login expectations.
  - Profile name validation and default profile handling.
  - Environment variable overrides (`AGENT_BROWSER_PROFILES_DIR`, `AGENT_BROWSER_PROFILE`).
- Deliverable: Reusable `agent-browser-profiles` crate with comprehensive test coverage.

Acceptance checklist (M0)

- [ ] `agent-browser-profiles` crate created and builds
- [ ] Cross-platform directory resolution implemented
- [ ] Profile metadata parsing and validation working
- [ ] Profile discovery by login expectations functional
- [ ] Environment variable overrides tested
- [ ] Cargo workspace initialized for cloud automation

M0.E. Electron browser host & RPC integration (3–5d)

- Add an automation-host entrypoint to the Electron app (e.g., `apps/agent-harbor-desktop/src/automationHost.ts`) that runs without showing the main UI and listens for a small RPC protocol over stdio or a local TCP/Unix socket.
- RPC methods (initial): `startSession(provider, profilePath, options)`, `sendInput(sessionId, event)`, `closeSession(sessionId)`.
- Wire `cloud-worker` to spawn the Electron automation host instead of a standalone browser, passing the resolved `<profile>/browsers/chromium` path.
- In automation mode, set `app.setPath("userData", "<profile>/browsers/chromium")` before creating windows, then create hidden `BrowserWindow` instances per session with automation-friendly defaults.
- Success criteria (integration tests):
  - Launching a session from `cloud-worker` starts Electron in automation mode and navigates a hidden window to `https://chatgpt.com`.
  - Authentication persists via the profile directory across multiple sessions.
  - RPC round-trips are reliable with acceptable latency/backpressure.
  - Electron/Node version used for automation is validated against the Playwright support matrix.

Acceptance checklist (M0.E)

- [ ] Electron automation host entrypoint exists and runs headless UI
- [ ] RPC start/close/input round-trips validated
- [ ] userData is set to the selected profile’s `browsers/chromium`
- [ ] Hidden automation window reaches target URL under automation
- [ ] `cloud-worker` launches and controls Electron host end-to-end

M1. Core browser automation engine (4–6d)

- Implement browser automation core targeting the Electron automation host using Playwright for DOM-level automation.
- Add session management, authentication handling, and progress monitoring.
- Implement cross-platform launch of the Electron host with profile isolation.
- Success criteria (integration tests):
  - Can launch the Electron automation host with a specific profile and navigate to target cloud platforms.
  - Authentication state persists across sessions via Electron `userData` bound to `<profile>/browsers/chromium`.
  - Progress monitoring captures console output and completion signals.

Acceptance checklist (M1)

- [ ] Browser automation launches successfully across platforms
- [ ] Profile-based authentication works for target cloud platforms
- [ ] Session management and monitoring operational

M2. Provider-specific adapters with streaming (6–8d)

- Implement adapters for each cloud provider: OpenAI Codex (ChatGPT), GitHub Copilot, Cursor Claude, Google Jules.
- Add provider-specific navigation, task submission, result extraction, and custom streaming for real-time monitoring.
- Implement provider detection and automatic adapter selection.
- Integrate with CLI monitoring commands (`ah agent follow-cloud-task`).
- Success criteria (integration tests):
  - Each provider adapter can submit tasks, extract results, and stream progress.
  - Provider auto-detection works reliably.
  - Real-time streaming connections establish between local CLI and cloud workers.
  - Error handling for authentication failures, rate limits, and streaming issues.

Acceptance checklist (M2)

- [ ] OpenAI Codex adapter functional on ChatGPT platform
- [ ] GitHub Copilot adapter working
- [ ] Cursor Claude adapter operational
- [ ] Google Jules adapter implemented
- [ ] Real-time streaming working for all providers

M3. CLI integration and monitoring (4–6d)

- Integrate cloud automation into main CLI (`ah agent run --cloud-*`).
- Implement `ah agent follow-cloud-task` for browser stream monitoring.
- Add TUI integration for cloud agent progress alongside local activities.
- Success criteria (CLI tests):
  - Cloud agent commands work end-to-end from task submission to completion.
  - Monitoring commands display real-time progress.
  - Error states and completion properly communicated to user.

Acceptance checklist (M3)

- [ ] Cloud agent CLI commands fully integrated
- [ ] Browser stream monitoring working
- [ ] TUI integration functional

M4. Advanced features and hardening (3–5d)

- Add advanced browser automation features: retry logic, anti-detection measures (including Electron-specific fingerprints such as UA/UA-CH headers, window sizing, and feature flags).
- Implement comprehensive error handling and recovery.
- Add performance optimizations and resource management.
- Success criteria (system tests):
  - Robust error recovery for network issues and authentication failures.
  - Performance meets latency targets for cloud agent execution.
  - Resource usage bounded and configurable.

Acceptance checklist (M4)

- [ ] Error recovery and retry logic robust
- [ ] Performance optimizations implemented
- [ ] Resource management and limits working

### Overall success criteria

- All supported cloud agent types (cloud-codex, cloud-copilot, cloud-cursor, cloud-jules) work end-to-end.
- Browser automation reliably handles authentication and task execution.
- Real-time streaming provides monitoring and control of cloud agent execution.
- Integration with agent browser profiles seamless and transparent to users.
- CLI commands provide consistent experience between local and cloud agents.

### Test strategy & tooling

- Unit tests for `agent-browser-profiles` crate: directory resolution, metadata validation, profile discovery.
- Unit tests for individual provider adapters and core components.
- Integration tests for full browser automation workflows with mock cloud platforms.
- System tests for end-to-end CLI workflows with real cloud platforms (where feasible).
- Browser automation testing using Playwright with isolated test profiles.
- Cross-platform CI matrix: GitHub Actions with Windows/macOS/Ubuntu runners.
- Agent browser profile integration tests with mock authentication states.

### Deliverables

- Cloud automation Rust crates: agent-browser-profiles, cloud-automation-core, cloud-providers.
- cloud-worker binary for browser automation execution.
- Updated AH CLI with cloud agent support and monitoring commands.
- Comprehensive integration with agent browser profiles system.
- Documentation and examples for cloud agent usage.

### Risks & mitigations

- Cloud platform API changes: Provider-specific adapters isolated for easy updates; comprehensive monitoring for breakage detection.
- Browser automation detection: Anti-detection measures, fallback to manual authentication flows.
- Authentication complexity: Agent browser profiles provide consistent authentication state management.
- Runtime compatibility: Track Electron/Node versions to remain within Playwright’s supported matrix; gate upgrades with CI checks.
- Network reliability: Robust retry logic and streaming recovery mechanisms.

### Parallelization notes

- M0 (`agent-browser-profiles` crate) can proceed independently as foundation.
- M0.E (Electron host + RPC) can start after M0 and before core automation work.
- M1 (core automation) can start after M0 for basic profile integration.
- M2 (provider adapters with streaming) can proceed in parallel with M1 once core is stable.
- M3 (CLI integration) requires M1–M2 to be stable.
- M4 (hardening) proceeds after all core functionality is working.

### Status tracking

- M0: pending
- M0.E: pending
- M1: pending
- M2: pending
- M3: pending
- M4: pending

### References

- See [CLI.md](CLI.md) for cloud agent command specifications.
- See [Agent-Browser-Profiles.md](Agent Browsers/Agent-Browser-Profiles.md) for profile integration requirements.
- Reference implementations in `reference_projects/` for browser automation patterns.
