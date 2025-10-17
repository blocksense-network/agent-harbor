### Overview

This document tracks the implementation status of the [Handling-AW-URL-Scheme](Handling-AW-URL-Scheme.md) functionality.

Goal: Implement a secure, cross‑platform URL scheme handler that opens tasks and (optionally) creates tasks with an explicit confirmation dialog. Support TUI reuse when a task is already followed in an existing terminal window.

### Milestones and tasks (with automated success criteria)

M0. Protocol installation and basic registration (3–4d)

- Implement Windows registry (HKCU) and MSIX URI activation.
- Implement macOS `CFBundleURLTypes` and Electron deep‑link handling.
- Implement Linux `.desktop` with `x-scheme-handler/agent-harbor`.
- Create minimal test handler binary that logs URL reception for testing automation frameworks.
- Success criteria (system integration tests):
  - OS registration succeeds without elevation.
  - Handler launches when URL is clicked in browsers (logs URL reception).
  - Unregistration cleans up registry/desktop entries.
- Deliverable: Cross-platform protocol registration working, minimal handler binary for testing.

Acceptance checklist (M0)

- [ ] S1 Windows registry registration works (HKCU)
- [ ] S2 macOS CFBundleURLTypes registration succeeds
- [ ] S3 Linux desktop file registration functions
- [ ] Handler launches from browser clicks and logs URLs

M1. Test automation framework selection (2–3d)

- Evaluate candidates for reliably handling browser external‑protocol prompts and the handler's native confirmation dialog across Windows/macOS/Linux.
- Test Playwright + OS accessibility automation (macOS AppleScript/JXA; Windows UIAutomation; Linux AT‑SPI/dogtail).
- Test OpenQA with VM orchestration and needle‑based UI assertions for full desktop flows.
- Optional: WebDriver BiDi + OS automation glue.
- Use the registered protocol from M0 to test end-to-end URL clicking flows.
- Criteria: flake rate in CI, cross‑OS coverage, maintenance overhead, artifact quality (video/screenshot/traces), ability to drive native dialogs, and total runtime.
- Success criteria:
  - PoC automation can detect and click "Open <app>" buttons in external protocol dialogs across all target browsers.
  - Test execution completes within 5 minutes per scenario on CI hardware.
- Deliverable: a short report with PoC repos, CI configs, and a recommended primary+fallback stack.

Acceptance checklist (M1)

- [ ] PoC repos demonstrate browser protocol prompt handling
- [ ] Native dialog automation works on Windows/macOS/Linux
- [ ] CI matrix configured for cross-platform testing
- [ ] Performance benchmarks meet 5-minute target

M2. Core handler skeleton (3–5d)

- Build Rust binary `ah-url-handler` (Windows/macOS/Linux) with structured logging.
- Implement URL parsing/validation; reject unknown hosts/components and secrets in query.
- Implement config resolution (webui base, rest base) + health probes.
- Success criteria (unit tests):
  - URL parsing rejects malformed schemes, unknown paths, and query secrets.
  - Config resolution finds WebUI ports from state files and health endpoints.
  - Binary startup time < 100ms; memory usage < 10MB resident.

Acceptance checklist (M2)

- [ ] U1 URL validation rejects invalid schemes and malicious inputs
- [ ] U2 Config resolution finds local WebUI/REST endpoints
- [ ] Binary performance meets targets
- [ ] Structured logging emits parse/config events

M3. WebUI bootstrap and REST probing (3–4d)

- Implement WebUI startup when absent, waiting for `/_ah/healthz`.
- Implement optional REST probing at `/api/v1/readyz`.
- Implement cross-platform browser opening to `${webuiBase}/tasks/<id>`.
- Success criteria (integration tests):
  - Handler starts WebUI process when health check fails.
  - Browser opens correct URL after WebUI becomes responsive.
  - REST probing succeeds when service is available.

Acceptance checklist (M3)

- [ ] I1 WebUI auto-start works when health check fails
- [ ] I2 Browser opens correct task URL across platforms
- [ ] REST optional probing succeeds when available
- [ ] Startup time < 3s p95 on target hardware

M4. TUI integration and reuse (4–6d)

- Implement TUI control index reading `${STATE_DIR}/tui-sessions.json` and socket queries.
- Implement existing session reuse via tmux/WezTerm/Kitty commands with platform focus helpers.
- Implement fallback: start `ah tui --follow <id>` (non-blocking).
- Success criteria (integration tests):
  - Existing TUI sessions are detected and reused instead of spawning new ones.
  - Platform-specific focus commands execute successfully.
  - Fallback TUI startup works when no existing session found.

Acceptance checklist (M4)

- [ ] I3 TUI session index reading works
- [ ] I4 Existing session reuse succeeds across multiplexers
- [ ] Platform focus helpers activate correct windows
- [ ] Fallback TUI startup works when sessions absent

M5. Create flow with confirmation dialog (5–8d)

- Implement native confirmation dialog with required fields (title, source, summary, execution venue, access scope, buttons, trust checkbox).
- Implement trust store with per‑source ephemeral rules and global policy support.
- Implement safe rendering with user content escaping; cancellation prevents creation.
- Success criteria (integration tests):
  - Dialog displays all required fields with escaped content.
  - Cancellation prevents task creation; confirmation proceeds with proper handoff.
  - Trust policy suppresses dialog for whitelisted sources within time window.

Acceptance checklist (M5)

- [ ] I5 Confirmation dialog shows all required fields
- [ ] I6 Cancellation prevents creation; confirmation proceeds
- [ ] Trust policy suppresses dialog appropriately
- [ ] Content escaping prevents injection attacks

M6. Telemetry, docs, and hardening (2–3d)

- Implement rotating logs and error surfacing with minimal diagnostics.
- Update docs and examples in spec and CLI.md.
- Add input validation hardening and timeout handling.
- Success criteria (system tests):
  - Logs rotate properly and contain actionable error information.
  - Documentation examples work end-to-end.
  - Handler gracefully handles network timeouts and malformed inputs.

Acceptance checklist (M6)

- [ ] Logs rotate and contain diagnostic information
- [ ] Documentation examples are accurate and testable
- [ ] Error handling covers network failures and timeouts
- [ ] Security hardening prevents common attacks

### Overall success criteria

- Cold start to task page < 3s p95 on macOS/Windows/Linux default dev boxes.
- TUI reuse succeeds when index/socket indicates an existing session.
- Create flow always presents the handler confirmation UI (unless test override explicitly enabled) and behaves per spec.
- All acceptance checklists pass on CI matrix.

### Test strategy & tooling

- Unit tests (cargo test) for URL parsing, config resolution, and core logic.
- Integration tests for WebUI startup, browser opening, TUI session management, and dialog interactions.
- System tests for OS registration, browser protocol handling, and end-to-end URL processing.
- E2E automation using selected framework (from M1) for browser + native dialog flows.
- Cross-platform CI matrix: GitHub Actions with `windows-latest`, `macos-latest`, `ubuntu-latest`.
- Test harness components:
  - Browser automation: Playwright.
  - Desktop automation for external-protocol prompts and handler confirmation:
    - Windows: PowerShell + UIAutomation (or Python `uiautomation`) to locate and click buttons by name; fallback AutoHotkey.
    - macOS: AppleScript/JXA using Accessibility API to detect browser confirmation sheets; standard `osascript` runner in CI.
    - Linux: AT‑SPI via `dogtail` (preferred) to find "External Protocol Request" dialogs; X11 fallback via `xdotool`.
  - Handler observability: temporary test log sink `${TMPDIR}/ah-url-handler-test.log` with structured events.
- Fixtures: Stub WebUI server, optional stub REST, TUI control stub with sessions.json and socket.
- Test matrix: OS (Windows Win10/11, macOS 13+, Linux GNOME/KDE), Browsers (Chrome/Chromium, Edge, Firefox, Safari).
- Golden tests for structured logs, protocol compliance, and dialog interactions.

### Deliverables

- Rust binary `ah-url-handler` with cross-platform builds.
- OS-specific packaging and registration scripts.
- Comprehensive automated test suite with CI matrix.
- Updated documentation and CLI examples.
- Security audit report for confirmation dialog and URL handling.

### Risks & mitigations

- Browser/OS protocol handling variance: extensive testing across browser/OS combinations; fallback automation strategies.
- Native dialog accessibility APIs unstable: feature-gate automation approaches; manual testing as backup.
- Security risks from URL handling: strict input validation; no secrets in URLs; safe dialog rendering.
- CI performance for UI automation: optimize test flows; parallel execution where possible; acceptable timeouts.

### Parallelization notes

- M0 (protocol registration) can proceed independently as the foundation.
- M1 (test framework evaluation) can start immediately after M0 completes, using the registered protocol.
- M2 (core handler) can start after M0 and proceed in parallel with M1.
- M3/M4 can proceed in parallel after M2 stabilizes.
- M5 requires M2–M4 for integration testing (needs handler skeleton and TUI/WebUI integration).
- M6 finalizes after all milestones complete.

### Status tracking

- M0: pending
- M1: pending
- M2: pending
- M3: pending
- M4: pending
- M5: pending
- M6: pending

### GUI integration notes (when AH GUI is installed)

- Delegation: The protocol handler delegates to the GUI main process via IPC when the GUI is running.
- Window reuse: A new browser window is not spawned if the GUI window exists; the GUI focuses its window and navigates to the route.
- TUI preference: With `tui=1`, the GUI checks the TUI control index/socket and reuses an existing terminal window if present; otherwise it spawns a follower according to user preferences.
- Create flow: For `create?...`, the GUI shows the same native confirmation dialog (owned by handler/GUI) before creation proceeds.
