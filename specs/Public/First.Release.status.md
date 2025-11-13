# First Public Release Status

## Overview

- Goal: ship the first public Agent Harbor release with all core developer workflows usable end-to-end on Linux and macOS. The scope pulls from the component responsibilities described in `docs/Component-Architecture.md` and the outstanding work captured across the existing status plans (`specs/Public/*.status.md`). In addition to automated verification, each milestone intentionally introduces targeted manual testing utilities (modeled after the manual-testing philosophy in `docs/Component-Architecture.md`) to make it easy for engineers to reproduce and validate behaviors interactively.
- Methodology: mirror the structure used in `specs/Public/MVP.status.md`—each release task lists concrete deliverables and fully automated verification that must pass before the item can be marked complete.

### Outstanding Release Tasks

- [ ] R1. Multiplexer enablement and automated regression coverage
- [ ] R2. Multi-model selectors for draft task cards
- [ ] R3. Remaining TUI input handling from the PRD
- [ ] R4. AgentFS integration in the task execution path
- [ ] R5. Remote-server manual test mode
- [ ] R6. Linux FUSE build validation for AgentFS
- [ ] R7. Documentation README completion
- [ ] R8. Text-normalized output mode
- [ ] R9. Agent command output capture for debugging
- [ ] R10. Claude credentials regression investigation
- [ ] R11. Recorder UI stability and test hardening
- [ ] R12. Recorder pass-through performance improvements
- [ ] R13. Git worktree snapshot support validation
- [ ] R14. Public documentation site
- [ ] R15. Implement `ah task` command to spec (minus browser automation)
- [ ] R16. Production package validation and FS snapshots daemon install checks
- [ ] R17. Release packaging and publishing workflow
- [ ] R18. Dynamic sandbox approvals in recorder UI
- [ ] R19. Skipped test investigation and remediation (`just test-rust`)
- [ ] R20. Slow test investigation and optimization (`just test-rust`)
- [ ] R21. Restore real-agent scenario integration tests via LLM API proxy
- [ ] R22. Restore hosted setup scripts and workspaces for hosted agent platforms
- [ ] R23. REST API alignment with `LocalTaskManager`
- [ ] R24. SQLite schema alignment and migration reset
- [ ] R25. Consolidate shared enums into `ah-domain-types`
- [ ] R26. MCP tool enablement for `ah agent start`
- [ ] R27. Logging guidelines specification and standardization

## R1. Multiplexer enablement and automated regression coverage (assignee: Emil, Mila, Danny)

**Status:** Not started — only tmux/iTerm2 backends are wired through the task manager today.

**Context:**

- The CLI exposes many backends via `CliMultiplexerType`, but any selection other than `Auto`, `Tmux`, or `ITerm2` falls back to auto-detection with a warning (`crates/ah-cli/src/tui.rs`).
- The task manager factory only instantiates `TmuxMultiplexer` (or `ITerm2Multiplexer` on macOS) regardless of the detected environment (`crates/ah-core/src/task_manager_init.rs`).
- `ah-mux` already ships per-backend implementations, yet the default feature set only enables `tmux` (`crates/ah-mux/Cargo.toml`), so the remaining integrations never compile in release builds.

**Deliverables:**

- [ x ] Extend `ah-core::create_local_task_manager_with_multiplexer` to construct every backend exported from `ah_mux` (tmux, kitty, wezterm, zellij, screen, tilix, windows-terminal, ghostty, vim, neovim, emacs) and reject selections that are unavailable instead of silently falling back.
- [ x ] Expand `determine_multiplexer_choice` so that environment detection maps nested terminals/multiplexers to the corresponding `CliMultiplexerType`, preserving the innermost supported multiplexer when both a terminal and multiplexer are detected.
- [ x ] Refactor `ah-cli::tui::determine_multiplexer_choice` to use the shared implementation from `ah-core`, eliminating code duplication.
- [ ] Promote the additional backends to the default feature set (with `cfg(target_os)` guards where required) and update the Nix flake + CI images to install the matching binaries so the code compiles and runs on all supported platforms.
- [ ] Teach the CLI to surface availability diagnostics (`ah health`) covering binary discovery, version checks, and user guidance when a backend is missing or misconfigured.
- [ ] Update developer documentation (`specs/Public/Terminal-Multiplexers/*.md`) to reference the new automated checks and any OS-specific prerequisites introduced by the releases.
- [ ] Implement a lightweight “multiplexer exerciser” CLI/TUI (living under `tests/tools/`) with keyboard shortcuts that trigger tab/pane creation in a similar way to what the local task manager does during launch_task, enabling manual validation of pane/window behavior without invoking the full Agent Harbor dashboard. Add a just target `manual-test-multiplexers` that launches the exerciser TUI.
- [ ] Ensure all supported/validated multiplexers are packaged in the Nix flake (and other platform manifests) so both automated and manual tests can install and run them consistently.

**Verification (automated):**

- [ ] Add unit tests that exercise the `determine_multiplexer_choice` logic (exists in both `ah-core` and `ah-cli`) with various combinations of detected terminal environments to ensure correct multiplexer choice selection based on priority rules.
- [ ] Add CLI integration tests that launch `ah tui --multiplexer <type>` in a headless harness and verify correct automatic session attachment behavior: launching inside an existing multiplexer session should attach to it, while launching outside should create a new session with the standard TUI layout (editor pane + agent pane). Tests should verify success by examining multiplexer sessions, windows/panes, and log files for evidence of the expected taken actions.
- [ ] Enable and stabilize the real-multiplexer integration tests in `crates/ah-mux/tests/integration_tests.rs` for tmux, kitty, wezterm, screen, zellij, tilix, windows-terminal, ghostty, vim, neovim, and emacs; gate each by the corresponding feature and add them to a `just test-multiplexers` target wired into CI for Linux/macOS/Windows.
- [ ] Extend the TUI scenario suite so that the dashboard launches, creates a task layout, and tears down successfully while bound to each multiplexer backend (record golden layouts per backend for the terminal-based multiplexers).

## R2. Multi-model selectors for draft task cards (assignee: Zahary)

**Status:** Not started — the dashboard ships a placeholder “🤖 Models” button but only the first model is captured and no count editing UI exists.

**Context:**

- `TaskEntryViewModel::models` keeps a `Vec<SelectedModel>` (which already exposes `count`), yet the Model modal (`ModalState::ModelSearch` in `crates/ah-tui/src/view_model/dashboard_model.rs`) only returns a single string and the view never exposes +/- controls mandated by [TUI-PRD.md](TUI-PRD.md#model-multi-selector-modal).
- Available models are hardcoded in `ViewModel::new_internal()` and never refreshed from configuration or REST (`ah-rest-client` exposes `list_agents`, but the dashboard ignores it; local mode should use the same catalog to stay in sync with the CLI/`AgentExecutor` configuration).
- The launchers collapse the selection down to a single model: `GenericRestTaskManager::launch_task` picks `params.models().first()` when building `CreateTaskRequest.agent`, and local launch simply iterates `params.models()` without honoring `SelectedModel.count`.
- TUI tests (e.g. `model_viewmodel_tests.rs`) only assert focus changes; no automated coverage ensures pluralization (“Agent(s)”), “Already Selected” section, or count adjustments described in the PRD.

**Deliverables:**

- [ ] Introduce a `ModelCatalog` abstraction in `ah-core` that normalizes model metadata for both local and remote modes (sources: local config/`AgentExecutor`, REST `GET /api/v1/agents` + `status=models`). Expose it through a `ModelsEnumerator` trait mirroring the repository/branch enumerators so the dashboard, CLI, and REST layer share consistent logic.
- [ ] Mark some agent/model pairs as experimental in the model catalog with appropriate UI indicators and feature gating.
- [ ] Add a new `--experimental-features <set>` configuration variable controlling which experimental features are enabled, including experimental agent/model pairs in the catalog.
- [ ] Replace the hardcoded model list in `ViewModel::new_internal` with asynchronous loading via the enumerator; surface loading/error states through `loading_models` (already present but unused) and refresh hooks.
- [ ] Implement the multi-select modal per PRD:
  - Counts editable with `+/-` (or Left/Right) keys, `Enter` auto-promotes zero→1, separated "Already Selected" section, fuzzy filtering, and footer hint updates.
  - Mouse support (wheel scroll, click to toggle) and keyboard navigation parity with other selectors.
  - Persist selections per draft card and show `"🤖 X model(s)"` with pluralization matching counts.
- [ ] Update task launch paths to respect multi-model inputs:
  - Local manager: launch `count` instances per `SelectedModel`, incorporate the model identifier into session IDs (`task-<model>-<ordinal>`), and propagate counts to recorder layout metadata.
  - REST manager: extend `CreateTaskRequest` in `ah-rest-api-contract` to accept `agents: [{type, version, count}]`, update server/client wiring, and ensure `session_ids` returns one entry per requested instance (maintaining compatibility notes in `REST-Service/API.md`).
  - Draft persistence: store `Vec<SelectedModel>` with counts in `ah-local-db` (currently JSON already supports it) and restore them into the modal controls.
- [ ] Surface configuration for default selections (e.g., `.ah/config.toml` or CLI flags) so operators can pre-seed recommended model bundles.
- [ ] Update end-user documentation: dashboard guide in `specs/Public/TUI-PRD.md` (Implementation Notes) and CLI manual to describe how multi-model launches map to REST/local execution.
- [ ] Document the `--experimental-features` configuration variable in `specs/Public/CLI.md`.
- [ ] Extend `just manual-test-tui` (or a new manual recipe) with scripted prompts that walk through multi-model selection, verifying counts and filters for human reviewers.

**Verification (automated):**

- [ ] Unit tests for the new `ModelCatalog`/`ModelsEnumerator` implementations (local + REST mock) covering filtering, feature gating (e.g., hidden/unavailable models), and count normalization.
- [ ] TUI ViewModel tests exercising the modal: selecting multiple models, adjusting counts via +/- and arrow keys, verifying “Already Selected” ordering, and ensuring Esc/Enter behaviors match the PRD.
- [ ] Scenario-based golden tests (`ah-tui/tests/`) that record the modal UI while:
  - Adding/removing models,
  - Editing counts,
  - Launching a draft and confirming `TaskLaunchParams.models()` contains expected `SelectedModel { name, count }` values.
- [ ] Integration test in `ah-core` confirming `TaskLaunchParams::build` rejects zero-model selections, allows multiple entries with counts, and that both the local and REST task managers emit the correct number of sessions.
- [ ] REST contract tests verifying `CreateTaskRequest.agents[]` serialization/deserialization and that the REST mock client returns `session_ids.len() == sum(count)` for multi-model submissions.
- [ ] Regression test ensuring footer text switches between “Launch Agent” and “Launch Agent(s)” and that the draft card badge shows the correct total count.

## R3. Remaining TUI input handling from the PRD

**Status:** Not started — arrow-key escape rules, autocomplete UX, auto-save indicators, and ghost-text suggestions from [TUI-PRD.md](TUI-PRD.md#handling-arrow-keys-within-text-areas) are partially stubbed or missing.

**Context:**

- Arrow navigation inside draft text areas is only partly implemented (`TaskEntryViewModel::handle_keyboard_operation` has TODO redraw hooks and ignored tests such as `textarea_down_moves_caret_then_leaves_task` in `crates/ah-tui/tests/prd_input_tests.rs`), so focus often escapes the textarea prematurely.
- The PRD-required autocomplete triggers (`@filename`, `/workflow`) should open immediately with cached suggestions, show ghost text, and refresh asynchronously. Today, `InlineAutocomplete` exists but does not expose ghost text, asynchronous refresh, or Tab/Right-arrow acceptance semantics.
- Auto-save state indicators (“Unsaved”, “Saving…”, “Saved”, “Error”) are defined in the PRD but not rendered; drafts silently persist through `TaskEntryViewModel::save_state` without status feedback or debounce guarantees.
- Footer shortcuts should reflect context (“Enter Launch Agent(s) • Shift+Enter New Line • Tab Next Field”), yet footer logic currently hardcodes values without pluralization, and loses synchronization when modals are opened.
- Mouse support (wheel to scroll textarea, click to place caret, selection drag) and keyboard shortcuts such as Shift+Arrow selection or Ctrl+Backspace word deletion have partial coverage with no UI regression tests.

**Deliverables:**

- [ ] Complete the textarea navigation contract:
  - Wrap Up/Down behavior so the caret traverses within the textarea until the top/bottom is reached, only then bubbling focus per PRD hierarchy.
  - Implement smooth scrolling for long drafts, including mouse wheel and Page Up/Down support, and ensure selection (Shift+Arrow) is preserved.
  - Wire the redraw TODOs to trigger minimal repaints in the View layer to keep rendering deterministic for golden tests.
- [ ] Ship the full autocomplete experience:
  - Extend `InlineAutocomplete` to maintain cached file/workflow lists, present ghost text (dimmed overlay), and merge asynchronous refresh results without flicker.
  - Implement immediate popup on trigger characters, fuzzy ranking, Tab/Enter acceptance, Right-arrow quick pick, and Esc dismissal semantics.
  - Support background refreshing by integrating with `WorkspaceFilesEnumerator` and `WorkspaceWorkflowsEnumerator` using Tokio tasks and fake time controls for deterministic tests.
- [ ] Add draft auto-save state machine and indicators:
  - Debounce saves at 500 ms of inactivity, invalidate in-flight requests when the user types, and surface save statuses in the top-right of the textarea (unsaved/saving/saved/error).
  - Emit events into the footer/status bar when saves fail and provide retry shortcuts.
  - Ensure local DB (`ah-local-db`) and REST draft endpoints respect deduplicated updates (idempotent by draft ID).
- [ ] Polish footer and input hints:
  - Centralize shortcut computation so modal state, pluralization (“Agent(s)”), and focus context update the footer every render.
  - Add mouse hint segments when pointer activity is detected (e.g., “Scroll to extend draft • Click to place cursor”).
- [ ] Integrate mouse interactions: click-to-focus positioning accounting for textarea padding, drag selection, wheel scrolling for both textareas and modal lists, with accessibility fallbacks when mouse support is disabled.
- [ ] Update docs (`specs/Public/TUI-PRD.md` Implementation Notes) to reflect the tested keyboard/mouse matrix and link to troubleshooting guidance for terminal key remapping.
- [ ] Provide a curated manual walkthrough (leveraging the interactive `tui-test play` flow) so QA can validate keyboard/mouse behavior step-by-step, as encouraged by `docs/Component-Architecture.md`.

**Verification (automated):**

- [ ] Expand `crates/ah-tui/tests/prd_input_tests.rs` to un-ignore the existing caret tests and add coverage for:
  - Up/Down wrap rules,
  - Shift+Arrow selections,
  - Tab/Shift+Tab cycling through Repository → Branch → Model → Go controls.
- [ ] Add golden scenario tests capturing autocomplete behavior (trigger, ghost text, asynchronous refresh update) at multiple terminal sizes using the deterministic fake-time runner.
- [ ] Unit tests for the debounce/auto-save state machine verifying transition diagrams (`Unsaved → Saving → Saved`, error propagation when mocked enumerators fail).
- [ ] Mouse interaction tests using the scenario player to assert click-to-caret positioning and wheel scrolling (leveraging the `MouseAction` events already in the ViewModel).
- [ ] Footer regression tests ensuring shortcut strings update when modal state or model counts change.
- [ ] Integration tests in `ah-local-db`/REST draft flows confirming duplicate saves are coalesced and `SaveDraftResult::Failure` transitions the UI to the “Error” indicator.

## R4. AgentFS integration in the task execution path

**Status:** Not started — AgentFS crates are built and tested in isolation (`specs/Public/AgentFS/AgentFS.status.md` milestones M1–M10), but `ah agent start`, the task managers, and the TUI still default to legacy CoW providers (ZFS/Btrfs/git worktree).

**Context:**

- The CLI exposes `--fs-snapshots agentfs`, yet the provider selection logic in `ah-fs-snapshots` never bootstraps the AgentFS daemon or macOS interpose shim. MacOS/Windows users continue to fall back to `worktree` or in-place execution.
- AgentFS control plane commands (`ah agent fs ...`) are not wired into the task orchestration loop, so per-process branch binding and snapshot notifications never reach the recorder or TUI.
- Sandbox profiles (`specs/Public/Sandboxing/Agent-Harbor-Sandboxing-Strategies.md`) assume AgentFS provides path-stable views, but the local task manager still mounts repositories directly via host paths.
- The TUI/REST server provide no visibility into AgentFS health (mount state, branch bindings, storage utilization), leaving operators blind when integrating cross-platform fleets.

**Deliverables:**

- [ ] Extend the filesystem provider selection (`ah_fs_snapshots::provider_for`) to detect AgentFS availability (daemon socket presence, platform support) and prefer it on macOS/Windows when `WorkingCopyMode::Auto` or `FsSnapshotsType::Agentfs` is chosen.
- [ ] Implement lifecycle management for the AgentFS daemon:
  - Launch the daemon automatically (with per-user socket paths) when the CLI/TUI needs a workspace, and tear it down when idle,
  - On macOS, inject the interpose shim using `DYLD_INSERT_LIBRARIES`/launch services; on Windows, ensure WinFsp driver availability.
  - Persist branch metadata and snapshot IDs so that `ah agent record` can emit `REC_SNAPSHOT` entries referencing AgentFS branch labels.
- [ ] Update `GenericLocalTaskManager` to materialize workspaces under AgentFS branches:
  - Use `ah agent fs init-session`, `branch create`, and `branch bind` before launching the agent command,
  - Route task manager socket/file paths through the AgentFS namespace so the recorder sees the same view the agent edits.
- [ ] Wire AgentFS into REST workflows:
  - Add feature detection endpoints (`GET /api/v1/system/agentfs`) and include AgentFS branch metadata in session payloads,
  - Ensure remote executors start the AgentFS daemon (linux FUSE, macOS FSKit, Windows WinFsp) and expose health metrics.
- [ ] Expose diagnostics in the TUI/WebUI:
  - Dashboard status bar should indicate when AgentFS is active, show mount path, and surface errors (e.g., daemon crash, mount failure).
  - Provide a detail panel listing current branches, snapshot counts, and storage usage per session.
- [ ] Documentation updates: integrate AgentFS usage guidance into `docs/Component-Architecture.md`, expand `AgentFS.status.md` with cross-component integration notes, and document troubleshooting steps (permission issues, kernel extensions, WinFsp requirements).
- [ ] Restructure the existing filesystem snapshot regression harness (today implemented under `crates/ah-fs-snapshots/tests/`, e.g. `integration.rs` and `provider_core_behavior.rs`) to start the AgentFS daemon. The logic of the tests should be extracted in an external test binary and launched as a separate process when the interpose shim can be loaded. The goal is to verify that AgentFS and the interpose shim have feature parity with the existing snapshot providers before deeper integration work begins. Please note that the AgentFS tests should be enabled only on macOS.
- [ ] Expand the `ah agent sandbox` CLI to accept explicit filesystem snapshot strategy flags (provider preference, working-copy mode, AgentFS enable/disable) and an optional AgentFS daemon socket path. When a socket is supplied the command must reuse the running daemon; otherwise it launches a new daemon as described above.
- [ ] Add a manual validation target (e.g., `just manual-test-agentfs`) that mounts AgentFS (internally using the enhanced `ah agent sandbox` command) and launches a shell where the developer can experiment with the file system.

**Verification (automated):**

- [ ] Add integration tests in `crates/agentfs-daemon` + `ah-fs-snapshots` that launch the daemon, create sessions, branch, bind, and validate file isolation using temporary repos (run on Linux/macOS/Windows CI).
- [ ] Extend `ah agent start` CLI tests to assert that `--fs-snapshots agentfs` results in AgentFS commands being invoked, the daemon socket created, and workspace paths resolved under the AgentFS mount.
- [ ] Add CLI tests for the enhanced `ah agent sandbox` flags verifying provider selection, AgentFS enablement, and working-copy overrides on macOS, covering both scenarios: launching a fresh AgentFS daemon and reusing an existing daemon via the socket flag.
- [ ] Local task manager end-to-end test that spins up AgentFS, launches a mock agent, writes files, and verifies the host filesystem remains untouched while AgentFS branch content persists.
- [ ] Snapshot regression suite updated to run through the restructured harness (the tests in `crates/ah-fs-snapshots/tests/`): launch the AgentFS daemon + interpose shim on macOS, execute the legacy snapshot scenarios, and compare outputs against existing providers.
- [ ] REST server contract tests ensuring AgentFS metadata is returned in session responses and that executor health checks fail if the daemon cannot be reached.
- [ ] TUI golden scenarios showing the AgentFS status indicator and branch list, including failure states (daemon unavailable) with error banners.
- [ ] Performance regression test comparing task launch times before/after AgentFS integration to ensure acceptable overhead (<10% vs. current git worktree path).

## R5. Remote-server manual test mode

**Status:** Not started — existing `just manual-test-*` scripts target local SQLite mode; there is no turnkey workflow for developers to exercise the TUI/CLI against a remote REST server with mock data.

**Context:**

- The PRD and component docs call for a remote mode mirroring production (`ah rest-server` + SSE), but manual testing currently requires hand-wiring environment variables and coordinating multiple processes.
- We already ship a mock REST server (`webui/mock-server`) and REST client crates; however, there is no script to launch the stack (REST server, mock repositories, multiplexer) and connect the dashboard in remote mode.
- CI lacks coverage ensuring remote manual mode stays healthy (ports, credentials, default API keys) and the documentation (`docs/Component-Architecture.md`, `specs/Public/TUI.status.md`) does not list a reproducible recipe.

**Deliverables:**

- [ ] Create a `just manual-test-tui-remote` and `just manual-test-tui-remote-mock` targets that starts the REST server (or the mock server) in the background and the TUI in the foreground (similarly to the existing `just manual-test-agent-start` target) with seeded repositories with example files and workflows (or a mock server configuration with active tasks, executing specific scenarios)
- [ ] Add a companion script (e.g., `scripts/manual-test-remote.py`) that orchestrates the lifecycle: spawns server, waits for health endpoint, launches `ah tui --remote-server <url>` or `ah webui --remote`, and tears everything down cleanly on Ctrl+C.
- [ ] Provide sample credentials/API tokens with clear rotation instructions, storing secrets in `.env.example` and using nix/yarn to provision dependencies.
- [ ] Document remote manual testing in `docs/Component-Architecture.md` and add a dedicated troubleshooting section (ports in use, missing dependencies, multiplexer focus).
- [ ] Ensure the manual mode supports scenario selection (choose repo/task script) and logs server output to per-run files under `manual-tests/logs/`.
- [ ] Bundle a human-operated walkthrough (scripted prompts, expected outcomes) so developers can validate remote mode interactively in line with existing manual test tooling.

**Verification (automated):**

- [ ] Add a smoke test (`just test-manual-remote-smoke`) that runs the orchestration script in headless mode, waits for a mock task to complete, and confirms teardown (process table clean, log files written). This smoke test should be part of the standard Rust workspace test suite (invoked via `just test-rust`) so CI exercises it automatically.
- [ ] Integration test verifying the TUI connects to the remote server by asserting REST calls are issued via `ah_rest_client` (use mock server assertions).

## R6. Linux FUSE build validation for AgentFS

**Status:** Not started — FUSE adapter code exists (`AgentFS.status` milestone M10.5), but we lack continuous validation on real Linux kernels and do not publish tooling to mount/test the filesystem outside unit tests.

**Context:**

- The FUSE host crate (`crates/agentfs-fuse-host`) compiles, yet its integration tests remain unchecked because CI does not install libfuse or run with privileged mount permissions.
- Manual instructions live in `specs/Public/AgentFS/AgentFS.status.md` and `Research/Compiling-and-Testing-FUSE-File-Systems.md`, but there is no automated recipe to build the userspace binary, mount it under `/tmp`, exercise operations, and unmount safely.
- Without a validated FUSE build, Linux developers cannot rely on AgentFS for snapshot-backed workspaces, blocking parity with macOS/Windows plans.
- A detailed milestone-by-milestone execution plan already exists in `specs/Public/AgentFS/FUSE.status.md`; this release plan should track progress against that document and reference it for lower-level task breakdowns.

**Deliverables:**

- [ ] Ship a reproducible FUSE test harness:
  - Nix flake and shell hooks install libfuse (v2 + v3), set `user_allow_other`, and configure mount permissions for CI/dev shells.
  - Provide `just test-agentfs-fuse` target that builds `agentfs-fuse-host`, mounts it on a loopback directory, runs file operation suites, and unmounts reliably (even on failure via trap).
- [ ] Implement automated acceptance tests covering:
  - Basic POSIX operations (create, read, write, rename, symlink, chmod),
  - Branch creation/binding via control file (`.agentfs/control`),
  - Snapshot creation and read-only mounts,
  - pjdfstest or equivalent compliance subset for permission semantics.
- [ ] Add performance probes (optional but recommended) to compare pass-through read/write throughput vs. ext4 baseline, logging results.
- [ ] Document kernel/module prerequisites (fuse kernel module, `/etc/fuse.conf` settings) and troubleshooting in `AgentFS.status.md` and `docs/Component-Architecture.md`.
- [ ] Publish a guided manual checklist (`just manual-test-agentfs-fuse`) that walks developers through mounting, exercising basic operations, and collecting logs for triage.
- [ ] Keep the deliverable list synchronized with the milestone plan in `specs/Public/AgentFS/FUSE.status.md` (F2–F11). When additional goals are added or completed there—mount/unmount cycles, filesystem/overlay semantics, error-code validation, control plane tests, pjdfstests, performance, stress, compatibility, security hardening, packaging—mirror the updates here so both documents reflect the same scope and progress expectations.

**Verification (automated):**

- [ ] GitHub Actions (or Buildkite) job running on a Linux runner with FUSE support that executes `just test-agentfs-fuse`; job must fail if mounts linger or tests fail.
- [ ] Acceptance tests verifying mount/unmount cycles, control plane commands, and pjdfstest subset success, with logs archived as artifacts.
- [ ] Regression test ensuring the harness detects kernel versions lacking required features and emits user-friendly guidance (instead of silent fallbacks).

## R7. Finish the README

**Status:** Not started — the repository README mixes legacy workflow content with new product messaging, contains empty badge links, and omits references to current specs and installation/testing flows.

**Context:**

- Later sections repeat text from the original workflow proposal (“Pushing to git becomes the primary interface…”) that no longer matches today’s TUI/REST-focused experience.
- Quick-start steps reference commands without listing prerequisites (Nix dev shell, Justfile targets, remote manual mode) and fail to mention AgentFS installation or multiplexer requirements.
- Contribution instructions still point at a placeholder `CONTRIBUTING.md` and do not reflect our spec-driven process or linting/test expectations.
- Badge placeholders are empty links, and no automated tooling ensures the README stays synchronized with CLI help output or installation scripts.

**Deliverables:**

- [ ] Rewrite `README.md` with a clear structure: value proposition, installation (packages + source), quick start (TUI, CLI, remote manual test), key features linking to specs, contribution/testing guidelines, support/license information.
- [ ] Remove outdated/duplicated narrative sections and replace them with links to authoritative specs (`specs/Public`) and the upcoming docs site (R14).
- [ ] Add a “Status & Roadmap” snippet referencing this release plan and other major status files.
- [ ] Replace badge placeholders with real badges (CI, crates.io, docs) or drop them entirely if unavailable.

**Verification (automated):**

- [ ] CI job runs `just lint-readme` and fails on markdown style, broken links, or missing TOC.
- [ ] Snapshot test comparing README quick-start snippets against `ah --help` output to ensure commands remain accurate.
- [ ] Spell-check pipeline (cspell) covers the README with a maintained allowlist for product terminology.

## R8. Text-normalized mode

**Status:** Not started — `--output text-normalized` falls back to plain text; no normalization pipeline exists despite the promises in `specs/Public/CLI.md`.

**Context:**

- `OutputFormat::TextNormalized` just disables JSON output in `AgentLaunchConfig`; both CLI and recorder stream raw agent text.
- `ah-agents` defines `AgentEvent`, but the TUI/CLI never consume it. Agents that emit JSON have no adapter to downgrade into normalized text.
- No integration path exists for recorder replays or REST log export to deliver normalized transcripts.

**Deliverables:**

- [ ] Replace the existing mock agent implementation (the one started with `just manual-test-agent-start --agent mock`) with a text-normalized agent that consumes event streams generated from scenario files (see [Scenario-Format.md](Scenario-Format.md)) and renders them as a chat-like stream where user and agent messages appear as styled boxes and tool execution outputs are live streamed.
- [ ] Teach agent executors to emit structured events:
  - For agents with JSON APIs (Claude, Codex, Copilot), parse their JSON output and map to `AgentEvent`.
  - For plain-text agents, add heuristic parsers with safe fallbacks to "Raw Transcript" sections.
- [ ] Create `ah-tui::normalized_text_output.rs` module that ingests `AgentEvent` streams (or raw text fallback) and renders the chat-like interface.
- [ ] Extend the `.ahr` recording format (see `ah_recorder::format`) to store tool execution output blocks alongside PTY data so that recorded sessions can be losslessly converted into the normalized text stream.
- [ ] The standardized format used in text-normalized mode matches the format of our SSE events (see `ah_domain_types::task::AgentEvent` and `ah_rest_api_contract::types::AgentEvent`).
- [ ] By default, each tool execution output is limited in height to N lines (configurable), but the user can interact with the UI to show a modal box where the entire tool output can be inspected and searched with incremental search.
- [ ] Most of the UI uses our standard textarea implementation, so things like mouse selection work as expected.
- [ ] The style of all boxes and modal dialogs closely resembles the ones that we currently use in the `tui dashboard`. The color theme is shared (see `ah_tui::view::Theme`).
- [ ] Integrate the normalizer into:
  - `ah agent start --output text-normalized`,
  - `ah agent record` (live display + replay),
  - REST session logs (`format=text-normalized`) and TUI activity log view.
- [ ] Update CLI & recorder docs to include examples of normalized output and describe limitations per agent.
- [ ] Provide a manual comparison workflow (`just manual-test-text-normalized`) that prints raw vs. normalized output for a representative task.

**Verification (automated):**

- [ ] Snapshot tests comparing normalized transcripts across agents using deterministic mock fixtures (including multi-model runs).
- [ ] Integration test confirming that `.ahr` files captured from third-party agents driven via the LLM API server contain the expected tool execution records and can be replayed into the normalized text output without loss.
- [ ] Unit tests for each parser ensuring malformed data degrades gracefully.
- [ ] Integration test verifying `.ahr` replay produces the same normalized transcript as the live run (round-trip).
- [ ] REST API contract test ensuring the `format=text-normalized` query parameter returns normalized logs and rejects unsupported agents with clear errors.

## R9. Capture agent command output for TUI debugging

**Status:** Not started — the recorder stores raw PTY bytes, but the TUI only shows the last three log lines per task; no structured command history is available for debugging tool executions.

**Context:**

- `ah agent record` can detect tool invocations (snapshot events, branch points), yet it does not emit structured “command started/finished” events that the TUI can render or filter.
- Agents like Claude/Codex stream command output interleaved with UI prompts, making it hard to reconstruct what shell commands ran and what they produced.
- REST/TUI status feeds lack command metadata; session debugging requires downloading `.ahr` logs and replaying manually.

**Deliverables:**

- [ ] Extend the recorder pipeline to detect command executions:
  - Parse structured events from agents that emit JSON (e.g., tool invocation metadata),
  - Offer heuristics for plain-text transcripts (detect `$`, `>_` prompts, or agent-specific markers),
  - Emit new `TaskEvent::Command { command, output, exit_code, duration }` events through task manager channels.
- [ ] Persist command events in the session database and propagate over REST SSE so TUI/WebUI can display history.
- [ ] Enhance the TUI to show a command log drawer per task (with filtering, copy-to-clipboard, search).
- [ ] Provide CLI tools (`ah session commands <id>`) to dump commands and outputs for offline analysis.
- [ ] Update documentation (TUI PRD, recorder spec) describing the new event types and UI affordances.

**Verification (automated):**

- [ ] Recorder unit/integration tests injecting synthetic agent transcripts to ensure commands are detected and serialized correctly.
- [ ] TUI scenario tests verifying the command log renders, supports search/filter, and updates as new events arrive.
- [ ] REST contract tests for the new SSE / REST endpoints (`GET /api/v1/sessions/{id}/commands`).
- [ ] CLI snapshot test for `ah session commands` output.

## R10. Investigate Claude credential regression

**Status:** Not started — Claude sessions intermittently fail because the agent cannot find credentials. Multiple pathways (Keychain, `.claude/.credentials.json`, `ANTHROPIC_API_KEY`) exist, but the current implementation does not reliably discover or copy them into the sandboxed HOME.

**Context:**

- Reports indicate that after the staging refactor, `copy_credentials(true)` no longer migrates Claude’s OAuth files into the sandboxed home; the agent binary drops into the onboarding flow and exits.
- On macOS, the Keychain lookup via `security find-generic-password` appears fragile; on Linux, credentials stored under `~/.config/claude` are ignored because we only look at `~/.claude/.credentials.json`.
- The CLI silently proceeds when credentials are missing, causing confusing failures inside the agent process rather than surfacing actionable diagnostics.

**Deliverables:**

- [ ] Audit the Claude credential discovery flow:
  - Enumerate all supported storage locations (Keychain, `~/.config/claude`, `.claude/.credentials.json`, environment variables) and document precedence.
  - Implement platform-specific accessors with detailed error reporting and telemetry (debug logs, user guidance).
- [ ] Fix credential copying:
  - Ensure sandbox HOME contains all required files (config, cache, OAuth tokens) with correct permissions.
  - Add opt-in redaction logging to confirm which credential schemes were used (without leaking secrets).
- [ ] Surface clear diagnostics when credentials are missing or malformed (CLI exit code, TUI banner, REST task failure message).
- [ ] Update documentation (`specs/Public/3rd-Party-Agents/Claude.md` or equivalent) to describe how credentials are detected and how users can remediate failures.
- [ ] Publish a manual troubleshooting checklist (`just manual-test-claude-credentials`) covering platform-specific verification steps.

**Verification (automated):**

- [ ] Unit tests for each credential discovery path (Keychain mocked, file paths, environment variables).
- [ ] Integration test running the mock Claude agent with a sandboxed HOME verifying credentials are copied and the agent launches successfully.
- [ ] CLI regression test asserting `ah agent start --agent claude` fails fast with a clear error when credentials are absent.

## R11. Recorder UI stability and tests

**Status:** Not started — the Ratatui-based recorder UI (live viewer + replay) still experiences race conditions (flicker, panic on resize), and automated coverage is limited to low-level format tests.

**Context:**

- The recorder spec (`ah-agent-record.md`) calls for live viewer parity with `.ahr` replay, but current implementation lacks deterministic tests; issues arise when high-volume PTY data arrives or window resizes happen mid-frame.
- Snapshot gutter indicators occasionally drift because terminal state updates race with UI rendering; there are TODOs in the recorder crate referencing these edge cases.
- CI only runs unit tests; there is no integration pipeline that spins up the recorder, feeds scripted PTY input, and asserts on Ratatui frame output.

**Deliverables:**

- [ ] Refactor the recorder event loop to decouple PTY ingestion from UI rendering (bounded channel, backpressure, graceful shutdown).
- [ ] Harden resize handling and snapshot gutter alignment (ensure vt100 state and UI dimensions stay in sync).
- [ ] Build an integration harness (`ah-recorder/tests/ui_golden.rs`) that feeds scripted PTY scenarios, captures Ratatui frames via `TestBackend`, and compares to golden files.
- [ ] Add stress/regression tests for:
  - High-frequency output,
  - Concurrent snapshot + resize events,
  - Recording termination during tool execution.
- [ ] Update documentation with known limitations and debugging tips (how to reproduce viewer issues, enabling verbose logs).
- [ ] Supply a manual walk-through (`just manual-test-recorder-ui`) showing how to reproduce common viewer scenarios for human QA.

**Verification (automated):**

- [ ] New integration test suite run via `just test-recorder-ui` (headless) in CI.
- [ ] Golden snapshot tests comparing Ratatui frames for key scenarios (default run, frequent resize, snapshot bursts).
- [ ] Property tests ensuring the recorder event loop drains without lossy frames under bounded buffers.

## R12. Recorder pass-through performance

**Status:** Not started — when recording with `ah agent record`, pass-through latency can exceed 500 ms under heavy output, and throughput drops compared to direct terminal usage.

**Context:**

- The recorder writes Brotli-compressed blocks while mirroring output to the user; synchronous writes and single-threaded compression cause pauses.
- Lack of profiling/benchmarking means regressions go unnoticed; there's no automated benchmark to compare against baseline `cat` throughput.
- The pass-through path does not adapt block size/time thresholds dynamically based on workload.

**Deliverables:**

- [ ] Profile the recorder pipeline (Tokio tasks, compression) to identify bottlenecks; document findings.
- [ ] Introduce configurable buffering (double-buffering or asynchronous writer thread) so mirror output remains responsive while recording.
- [ ] Implement adaptive flush policy (smaller blocks during interactive typing, larger blocks during bulk output).
- [ ] Provide metrics/logging (latency, throughput, dropped bytes) accessible via `RUST_LOG` and expose summary after each session.
- [ ] Update documentation with recommended settings for different workloads and instructions on enabling performance tracing.
- [ ] Deliver an interactive benchmark harness (`just manual-test-recorder-throughput`) to let developers reproduce performance investigations locally, mirroring the style of manual tooling referenced in `docs/Component-Architecture.md`.

**Verification (automated):**

- [ ] Benchmark suite comparing pass-through latency and throughput against baseline; fail CI if latency >200 ms or throughput drops beyond defined threshold.
- [ ] Stress test generating large bursts of PTY output ensuring zero dropped frames and consistent recorder performance.
- [ ] Replay validation confirming optimized pipeline preserves byte-perfect fidelity.

## R13. Test worktrees/git snapshots support

**Status:** Not started — git-based snapshots and worktree isolation exist, but we lack end-to-end validation across concurrent tasks and large repositories.

**Context:**

- The `Git-Based-Snapshots.md` spec describes shadow repos, per-session indices, and worktree management, yet real workflows (multiple concurrent branches, cleanup failures) remain untested.
- `ah agent start` may fall back to worktree mode on platforms without ZFS/Btrfs/AgentFS; without robust tests we risk corrupting user repos.
- Git version differences across CI/client environments can change worktree behavior, so we need matrix coverage (Linux/macOS/Windows).

**Deliverables:**

- [ ] Create integration tests that:
  - Initialize sample repos, take snapshots, create branches, run tasks, and assert isolation (no cross-session file leakage),
  - Exercise concurrent task launches to ensure shadow repo indices remain consistent,
  - Verify cleanup removes worktrees and refs even on failure.
- [ ] Add scenario-driven end-to-end tests that invoke `ah agent start` (with git snapshots) against the mock agent and a curated scenario; these tests must inspect the resulting git worktrees/shadow repos to ensure expected snapshots are recorded with precise file contents.
- [ ] Add performance/regression measurements (snapshot creation time, branch checkout time) and publish thresholds.
- [ ] Document limitations (submodules, LFS, large repos) and configure fallbacks accordingly.
- [ ] Offer a manual validation path (`just manual-test-git-provider`) so developers can inspect shadow repos/worktrees and confirm cleanup.

**Verification (automated):**

- [ ] CI job (`just test-git-provider`) running on Linux/macOS/Windows that executes the new integration suite, including the scenario-driven `ah agent start` workflow.
- [ ] Post-test verification script ensuring no stray worktrees/shadow repos remain.
- [ ] Regression test asserting provider auto-detection selects git when CoW providers unavailable and tasks succeed end-to-end.

## R14. Create docs site

**Status:** Not started — documentation lives in markdown specs scattered across the repo; the README references a “coming soon” docs site.

**Context:**

- Engineers and contributors need a navigable docs hub with versioned content, but there’s no build pipeline or hosting story.
- The specs folder provides detailed plans, yet lacks site navigation, search, or build automation.
- We need alignment with release cadence (publish docs along with binaries).

**Deliverables:**

- [ ] Choose a static site generator (e.g., Docusaurus, Mintlify, MkDocs, MdBook) and set up a docs bundle under `docs/site/` sourced from the spec/markdown files.
- [ ] Implement automated conversion/sync (lint, link validation, diagrams) within CI.
- [ ] Design navigation: intro, quick start, TUI, AgentFS, Recorder, API, development guides.
- [ ] Provide deployment pipeline (GitHub Pages, Cloudflare Pages, or S3) triggered on main/release tags.
- [ ] Update README and `docs/Component-Architecture.md` with site URL and contribution guidelines.
- [ ] Produce a manual docs smoke-test checklist (e.g., `just manual-test-docs-site`) that authors can run before publishing.

**Verification (automated):**

- [ ] CI job building the docs site, running markdownlint/link check, and failing on warnings.
- [ ] PR preview (e.g., deploy preview) to validate changes before merge.
- [ ] Smoke tests ensuring critical pages (Quick Start, TUI, AgentFS) render without broken links/assets.

## R15. Implement `ah task` command to spec (minus browser automation)

**Status:** Not started — `crates/ah-cli/src/task.rs` only implements a subset of the behavior described in [CLI.md](CLI.md#2-tasks); the CLI should now follow the specification directly without relying on the legacy Ruby helpers.

**Context:**

- The CLI spec details prompt collection, branch detection, delivery flows, fleet orchestration, and FS snapshot integration that must be handled natively. Browser-automation features remain future work, but everything else in the spec is required.
- Configuration precedence (flags/env/config files) and multi-agent launches rely on shared helpers in `crates/ah-cli` and `ah-core` that need consolidation.

**Deliverables:**

- [ ] Implement the complete `ah task` workflow directly from the CLI spec, covering prompt collection, branch creation/protection, follow-up tasks, delivery options, multi-agent/multi-instance launches, dev-shell handling, notifications, and FS snapshot selection—excluding only the browser-automation requirements.
- [ ] Wire the CLI to `ah-core` services (`TaskManager`, `AgentTasks`, `local_task_manager`) so `.agents/tasks` files, metadata commits, and repo sanity checks are produced exactly as specified.
- [ ] Ensure configuration precedence and fleet orchestration match the spec by reusing helpers in `crates/ah-cli/src/config.rs` and `crates/ah-core/src/local_task_manager.rs`.
- [ ] Update CLI help text, `specs/Public/CLI.md`, and related docs to reflect the implemented behavior, explicitly noting browser automation remains out of scope for this release.
- [ ] Provide a manual workflow (`just manual-test-task`) that walks through interactive and non-interactive task creation, including follow-up tasks and delivery modes.

**Verification (automated):**

- [ ] Scenario-driven integration tests (see [Scenario-Format.md](Scenario-Format.md)) under `tests/tools/mock-agent/scenarios/` that run `ah task` end-to-end for new tasks, follow-ups, multi-agent launches, delivery flows, and error cases; executed via `just test-rust`.
- [ ] Unit tests covering configuration merging, branch validation, metadata commit generation, and prompt processing.
- [ ] Integration tests asserting `ah task --follow` launches the correct monitoring UI (TUI/WebUI) and that session metadata propagates.
- [ ] Regression test ensuring cloud-agent paths remain guarded (browser automation intentionally disabled) with clear messaging.

## R16. Production package validation and FS snapshots daemon install checks

**Status:** Not started — installer scripts exist but there is no automated validation that the snapshots daemon is installed, permissions are configured, or that users receive actionable guidance.

**Context:**

- Packaging logic spans `scripts/install-*`, `install/`, and documentation (`docs/Component-Architecture.md`). We must verify the resulting packages configure the AgentFS/FS snapshots daemon (`crates/ah-fs-snapshots-daemon`) and required groups/ACLs correctly.
- Release candidates need consistent smoke tests across Linux/macOS/Windows to ensure services start and access restrictions behave as expected.

**Deliverables:**

- [ ] Add automated smoke tests that install the production package, confirm the snapshots daemon/service is present and running, and validate that only authorized group members can create snapshots.
- [ ] Produce validation scripts reporting daemon status, group membership, and installation logs; fail the build when configuration is incomplete.
- [ ] Document the expected installation flow (including troubleshooting) in `docs/Component-Architecture.md` and the relevant AgentFS status files.
- [ ] Publish a manual checklist (`just manual-test-production-package`) for engineers performing spot checks on clean machines.

**Verification (automated):**

- [ ] CI pipeline that provisions clean Linux/macOS environments, installs artifacts, exercises snapshot creation/teardown, and cleans up.
- [ ] Regression checks ensuring installer logs contain no errors/warnings and that daemons auto-start on reboot.
- [ ] Artifacts capturing post-install diagnostics (service status, group membership, permissions) attached to CI runs.

## R17. Release packaging and publishing workflow

**Status:** Not started — releases are assembled manually without a reproducible GitHub workflow or artifact signing.

**Context:**

- Current CI only runs tests; we need an automated pipeline that builds, signs/notarizes, and publishes release artifacts across platforms with consistent versioning.
- Release documentation/changelog updates require coordination; a formal workflow will reduce regressions.

**Deliverables:**

- [ ] Create a GitHub Actions workflow triggered on tags (and optional dry runs) that builds platform packages, performs signing/notarization, and uploads artifacts to GitHub Releases (and future registries).
- [ ] Generate checksums/SBOMs and verify artifacts before publish.
- [ ] Document the release workflow (prep checklist, verification steps, announcement/rollback) in `docs/Release-Workflow.md`.
- [ ] Provide a `just release-dry-run` target to exercise packaging locally/CI without publishing.

**Verification (automated):**

- [ ] CI workflow demonstrating successful builds for Linux/macOS/Windows, attaching artifacts to GitHub Releases with checksums.
- [ ] Automated install smoke tests using freshly built artifacts within the workflow.
- [ ] Release checklist gate ensuring docs, changelog, and version numbers are updated before publishing.

## R18. Dynamic sandbox approvals in recorder UI

**Status:** Not started — interactive sandbox approvals described in `specs/Public/Sandboxing/Agent-Harbor-Sandboxing-Strategies.md` are not exposed in the recorder/TUI.

**Context:**

- The sandbox core can enforce path whitelists but lacks UI prompts for on-demand approvals. Recorder replays also do not reflect approval decisions.
- Both CLI and TUI need a consistent approval workflow for sandboxed sessions.

**Deliverables:**

- [ ] Extend `ah-recorder` and sandbox IPC to emit approval request events when processes access paths outside whitelists.
- [ ] Implement approval dialogs in the TUI recorder UI (`crates/ah-tui/src/view/...`) using shared theme components.
- [ ] Persist approval decisions in local DB, so replays surface the same prompts/outcomes.
- [ ] Update sandbox/recorder documentation to describe configuration, approval UX, and logging.

**Verification (automated):**

- [ ] Scenario tests triggering sandbox approval flows and asserting prompts/decisions are visible and enforce access.
- [ ] Replay tests verifying recorded approvals render identically.
- [ ] Manual walkthrough (`just manual-test-sandbox-approvals`) guiding engineers through approving/denying access and inspecting resulting logs.

## R19. Skipped test investigation and remediation (`just test-rust`)

**Status:** Not started — multiple tests are currently skipped/ignored, reducing coverage for critical components.

**Context:**

- `just test-rust` (cargo nextest) skips tests marked with `#[ignore]` or environment guards; reasons are not tracked.
- Without oversight, new skips could hide regressions.

**Deliverables:**

- [ ] Audit the workspace for skipped tests, document causes, and either fix issues or create explicit tracking tickets.
- [ ] Update CI/Justfile to fail when new skips are introduced without approval (e.g., `cargo nextest --no-skip`).
- [ ] Produce an automatically generated skip report stored in `docs/testing/skipped-tests.md`.

**Verification (automated):**

- [ ] CI job running the suite with `--run-ignored all` (or equivalent) ensuring zero unexpected skips.
- [ ] Tests updated so previously skipped cases now pass (or have documented, justified ignores with linked issues).
- [ ] Automation refreshing the skip report and failing when divergences occur.

## R20. Slow test investigation and optimization (`just test-rust`)

**Status:** Not started — certain integration tests significantly extend CI runtime without clear benefit.

**Context:**

- Recorder, sandbox, and git provider tests can take minutes due to repeated setup/teardown, serialized execution, or non-deterministic waits.
- Performance budgets are undocumented.

**Deliverables:**

- [ ] Instrument test runs (e.g., `cargo nextest --profile time-summary`) to capture per-test timing and identify hotspots.
- [ ] Optimize or parallelize slow tests (fixture reuse, snapshot caching, configurable timeouts) while preserving coverage.
- [ ] Establish target runtimes and document them, adding alerts when thresholds are exceeded.

**Verification (automated):**

- [ ] CI stage that records timing metrics and fails when thresholds are breached.
- [ ] Regression tests ensuring optimized cases still validate intended behaviors.
- [ ] Historical timing reports stored as artifacts to monitor drift.

## R21. Restore real-agent scenario integration tests via LLM API proxy

**Status:** Not started — scenario-driven real-agent tests are disabled because the LLM API proxy cannot reconcile unexpected agent requests with scripted timelines.

**Context:**

- Real agents send auxiliary API calls (status checks, tool schema queries) not captured in scenarios, causing proxy desynchronization.
- The current proxy (`crates/llm-api-proxy/`) advances scenario steps sequentially rather than matching on message content or tool calls.

**Deliverables:**

- [ ] Enhance the LLM API proxy to identify scenario steps by examining request payloads (user messages, tool invocations) and remove the sequential fallback so only content-based matching drives scenario progression; provide rule-based handlers for allowable extra requests.
- [ ] Provide configuration for handling out-of-scenario requests (allow, deny with diagnostics, or record for scenario updates).
- [ ] Re-enable real-agent integration tests using scenario fixtures so Claude/Codex/etc. run deterministically against the proxy.
- [ ] Document the matching logic and update [Scenario-Format.md](Scenario-Format.md) and any proxy docs to describe the new rules.

**Verification (automated):**

- [ ] CI suite running restored real-agent integration tests via `just test-rust`, confirming deterministic outcomes.
- [ ] Scenario fixtures proving the proxy matches steps by content/tool usage and delivers the scripted responses.
- [ ] Manual validation recipe (`just manual-test-real-agent-scenarios`) demonstrating end-to-end real-agent runs through the proxy.

## R22. Restore hosted setup scripts and workspaces for hosted agent platforms

**Status:** Not started — the repository still ships the legacy `codex-setup` (and similar) scripts that assume Ruby helpers and do not surface the modern `ah agent` workflows or the Nix flake packaging flow described in `specs/Public/CLI.md` and the current `README.md`.

**Context:**

- `codex-setup`, `common-pre-setup`, and friends invoke Ruby binaries (`install-extras`, `download-internet-resources`) that are no longer part of the supported developer workflow now that `ah agent get-task`, `ah agent get-setup-env`, and `ah agent start` exist.
- Hosted IDEs (Codex, Jules, Cursor, Copilot CLI, etc.) still rely on cloning this repository and running the legacy scripts, so they never pick up the Nix flake and the new CLI commands.
- The README advertises remote setup instructions, but there is no automated build that produces pre-initialized workspaces or verifies that the scripts stay in sync with the spec.
- Cloud agent environments already run inside provider-managed sandboxes, so only the minimal Agent Harbor tooling (task retrieval, prompt capture, credentials) needs to be ported. Advanced local-only features (multiplexer orchestration, AgentFS mounting, etc.) should remain disabled to avoid redundant work.

**Deliverables:**

- [ ] Rewrite `codex-setup`, `jules-setup`, `cursor-setup`, and the shared helpers under `scripts/remote-setup/` to bootstrap Agent Harbor exclusively through the Nix flake (`nix profile install .#ah` by default, with `nix run .#ah -- agent ...` fallbacks when profiles are unavailable).
- [ ] Replace all Ruby helper calls with the equivalent `ah agent` subcommands: fetch tasks with `ah agent get-task`, environment extraction via `ah agent get-setup-env`, and invoke `ah agent starting-cloud-task` (successor to `start-work`) to capture developer prompts before handing control to the hosted agent; delete or archive unused Ruby binaries while documenting the reduced feature set that hosted sandboxes make unnecessary.
- [ ] Migrate the existing `just legacy-test-codex-setup-integration` harness to invoke the new scripts via the `ah` binary (updating assertions, fixtures, and naming it `just test-cloud-setup` once stable) so hosted setup regressions continue to be caught automatically.
- [ ] Update `README.md` and `specs/Public/CLI.md` to reference the new setup flow.
- [ ] Integrate hosted setup validation into CI: add a GitHub Actions job (and local `just test-remote-setup`) that spawns a clean container, runs each setup script, executes `ah agent get-task --repo <tmp workspace>`, and asserts that `ah agent start` succeeds with the seeded prompt.
- [ ] Add developer-facing documentation (`docs/Hosted-Agent-Setup.md`) covering how the scripts map to CLI spec primitives, how to refresh the workspace artifacts, and how to test changes locally.

**Verification (automated):**

- [ ] `just test-cloud-setup` (migrated from `just legacy-test-codex-setup-integration`) executes each hosted setup in an ephemeral directory, verifies that `ah agent get-task`, `ah agent get-setup-env`, `ah agent starting-cloud-task`, and `ah agent start` all succeed, and asserts that expected configuration files and logs are produced for each platform harness.

## R23. REST API alignment with `LocalTaskManager`

**Status:** Not started — the REST specification in `specs/Public/REST-Service/API.md` and the `ah-rest-api-contract` crate expose fields and lifecycle semantics that drifted from what the TUI and `ah-core::TaskManager` actually require.

**Context:**

- TUI flows call into `GenericLocalTaskManager`/`TaskLaunchParams`, but the REST API still expects legacy concepts (`runtime.delivery.mode`, `workspace.snapshotPreference`, webhook arrays) that are unused by the dashboard or CLI and complicate contract maintenance.
- The REST client (`crates/ah-rest-client`) contains multiple TODOs translating between REST types and core domain enums (e.g., agent types, session statuses), leading to subtle mismatches in feature toggles like multiplexer selection, AgentFS flags, or multi-model counts.
- Several endpoints duplicate state that already lives in the SQLite database or the recorder (e.g., `recent_events`, diff APIs) yet lack parity tests ensuring responses match what the local task manager produces.
- Without strict contract tests, trimming unused fields risks breaking the WebUI/TUI. Conversely, keeping unused fields increases maintenance overhead and diverges from LocalTaskManager behavior.

**Deliverables:**

- [ ] Produce a field-by-field comparison between `TaskLaunchParams`/`AgentExecutionConfig` and the REST `CreateTaskRequest`/`Session` schemas; document the minimal set of fields required by the TUI, CLI, and recorder, and deprecate everything else.
- [ ] Refactor `ah-rest-api-contract` and `ah-rest-server` so `POST /api/v1/tasks` accepts the same structures that the local path builds (multi-model counts, sandbox config, FS snapshot selection, MCP flags) and reject unsupported combinations early with Problem+JSON errors.
- [ ] Remove or consolidate endpoints and response fields that duplicate local behavior (e.g., trim file/diff endpoints if the recorder already serves them) and update the spec + clients accordingly.
- [ ] Update `ah-rest-client` and TUI data sources to consume the new slim contract, ensuring enums and structs are shared via `ah-domain-types` rather than redefined locally.
- [ ] Expand the API documentation (`specs/Public/REST-Service/API.md`) with explicit parity tables that map each LocalTaskManager operation to its REST equivalent, highlighting any intentional differences.

**Verification (automated):**

- [ ] All existing tests should pass after the reforms

## R24. SQLite schema alignment and migration reset

**Status:** Not started — the SQLite schema used by the CLI/TUI and the REST server still matches the authoritative design in `specs/Public/State-Persistence.md`, but the implementation is scattered across layered migrations (`MigrationManager::apply_migration_*`), handwritten schema snippets, and code-specific structs. With no production databases yet, we have an opportunity to collapse the migrations while proving that every persisted column remains justified and correctly wired through the application and specs.

**Context:**

- `State-Persistence.md` is the normative reference for persistent state (repos, sessions, tasks, events, drafts, executor metadata, etc.), and other specs (e.g., `TUI.status.md`, `CLI.md`, `ah-agent-record.md`, `REST-Service/API.md`) rely on fields such as `browser_automation`, `codex_workspace`, executor capability rows, and event payloads.
- Multiple crates duplicate the schema in slightly different shapes (`ah-local-db`, `ah-rest-server`, `ah-rest-api-contract`, TUI persistence code). Any schema change requires chasing each copy, increasing the risk of drift.
- The current migration chain (versions 1–4) replays large schema batches on every initialization, and lints/tests cannot easily confirm which code paths depend on which columns.
- Before flattening migrations we must catalogue every column and provide evidence (spec citations + code usage) so we do not accidentally remove still-needed functionality.

**Deliverables:**

- [ ] Build a cross-reference document (stored under `specs/Implementation-Progress/First.Release/R24.schema-matrix.md`) that, for every table/column in `State-Persistence.md`, lists:
  - Which specs describe the field’s intent,
  - Which crates/modules read or write it (include search evidence or code references),
  - Whether REST, CLI, recorder, or TUI scenarios cover it today.
- [ ] Update `specs/Public/State-Persistence.md` (and any dependent specs) to resolve gaps or outdated descriptions discovered during the audit, ensuring the spec remains the single source of truth.
- [ ] Introduce a canonical schema artifact (e.g., `crates/ah-local-db/schema.sql`) generated from the spec mapping and referenced by both `ah-local-db` and `ah-rest-server`, so migrations/tests can diff against one authoritative definition.
- [ ] Replace the incremental migrations with a single baseline migration that materializes the canonical schema when `user_version = 0`, plus a lightweight verification path for developers to reset local DBs (e.g., `ah local-db reset` or `just reset-local-db`). Preserve a compatibility shim that refuses to run if unexpected user data/migrations exist, guiding contributors through the reset.
- [ ] Adjust ORM/model code and REST persistence helpers to consume the shared definitions (e.g., derive column lists from the canonical schema, add typed helpers for JSON fields) while keeping all existing columns intact.
- [ ] Add developer documentation capturing the migration reset procedure, including guidance for contributors on how to add new columns (update the schema artifact + cross-reference doc) going forward.

**Verification (automated):**

- [ ] Schema conformance test in `crates/ah-local-db/tests/` that runs the baseline migration on a temp DB, queries `sqlite_master`/`PRAGMA table_info`, and compares the result to the canonical schema artifact (fail on missing or extra columns).
- [ ] CI job (`just test-db-schema`) invoking the schema conformance test plus an automated ripgrep audit that ensures every column documented in the schema matrix still appears in other specs; fail if a column loses coverage.
- [ ] Lint or doc-test that regenerates the schema matrix and fails if the committed matrix is outdated, keeping the cross-reference synchronized with the code.
- [ ] All existing tests should pass after the schema changes.

## R25. Consolidate shared enums into `ah-domain-types`

**Status:** Not started — multiple crates define their own versions of core enums (`TaskStatus`, `SessionStatus`, `AgentType`, `MultiplexerKind`, etc.), leading to repetitive `From`/`TryFrom` glue code, serialization drift between REST and local modes, and inconsistent Clap/Serde implementations.

**Context:**

- `ah-core`, `ah-local-db`, `ah-rest-api-contract`, `ah-rest-client`, and the TUI all declare separate enums for session/task statuses, agent types, delivery modes, sandbox policies, and multiplexer identifiers. These are semantically identical but live in different modules with divergent derives.
- Some enums (e.g., `TaskStatus`) only derive `Serialize/Deserialize` in certain crates, forcing ad-hoc conversions or lossy mappings when sending data over REST vs. local DB vs. UI.
- The CLI needs Clap parsing for the same values that REST requires Serde support and that the database layer needs SQL mapping. Maintaining separate implementations increases bug surface and slows down feature updates like multi-model counts or new sandbox policies.
- The `ah-domain-types` crate already exposes foundational structs but does not yet serve as the canonical home for these enums.

**Deliverables:**

- [ ] Catalog all duplicated enums across crates (status enums, agent/multiplexer identifiers, delivery modes, sandbox approval states, logging levels) and specify the canonical variants + documentation in `specs/Public/Subcrates-Pattern.md`.
- [ ] Move the canonical definitions into `ah-domain-types`, implementing required traits: `Clone`, `Copy` (where applicable), `Serialize/Deserialize`, `Display`, `FromStr`, Clap `ValueEnum`, and `sqlx::Type`/`rusqlite` helpers where the values hit storage.
- [ ] Update downstream crates (`ah-core`, `ah-local-db`, `ah-rest-api-contract`, `ah-rest-client`, `ah-tui`, `ah-cli`) to use the shared enums, removing local copies and conversion glue; ensure REST OpenAPI generation, database bindings, and TUI view models compile.
- [ ] Provide conversion shims only where legacy formats remain (e.g., mapping REST `queued` to `SessionStatus::Queued`), confined to boundary modules with exhaustive tests.
- [ ] Document the enum consolidation rules and usage examples in `docs/Contributor-Guide.md`, emphasizing when to extend `ah-domain-types` vs. defining crate-local enums.

**Verification (automated):**

- [ ] All existing tests should pass after the refactoring.

## R26. MCP tool enablement for `ah agent start`

**Status:** Not started — `ah agent start` exposes no CLI flags for Model Context Protocol (MCP) servers, and the agent launch pipeline does not verify that MCP tooling is wired when running through the LLM API proxy or local executor. The `ah-agents` abstraction already supports MCP server lists, but the CLI, TUI, and REST stack never populate them.

**Context:**

- `ah_agents::AgentLaunchConfig` carries an `mcp_servers: Vec<String>`, yet neither the CLI nor TUI populate it, so downstream agent integrations (Claude Code, Gemini, Copilot CLI, Cursor) run without the configured MCP tools.
- MCP server configuration today is manual (users edit agent-specific config files). We need a unified mechanism to pass `--mcp-server` definitions via `ah agent start`, `ah task`, and REST requests.
- Without automated tests, regressions in MCP wiring go undetected; we rely on manual verification through hosted agents.
- The LLM API proxy needs to emulate MCP capabilities in integration tests to ensure the agent loads the expected MCP manifests.

**Deliverables:**

- [ ] Extend `ah agent start` (CLI + spec) with flags for MCP enablement (`--mcp-server <name=uri>` and `--mcp-config <path>`), wiring them into `AgentLaunchConfig::mcp_servers`. Propagate the same settings through `TaskLaunchParams` so TUI and REST launches gain parity.
- [ ] Update configuration loading (`ah-config-types`, `.ah/config.toml`) to support persistent MCP server definitions (per repo, per user) and make them discoverable by both CLI and TUI.
- [ ] Ensure agent-specific adapters (`ah-agents::claude`, `::gemini`, `::copilot_cli`, `::cursor_cli`) translate the canonical MCP configuration into the agent's native CLI/environment variables, including temporary config files when required.
- [ ] Document MCP usage in `specs/Public/CLI.md` and relevant agent specs (Gemini, Copilot, Cursor) with examples showing local + remote workflows.
- [ ] Update TUI workflow so the agent launch modal surfaces MCP selections (at minimum, read-only indicator for active MCP toolchains; stretch goal: selector UI tied to config).

**Verification (automated):**

- [ ] Add integration tests in `ah-agents` (or `ah-core`) that launch agents via a fake MCP server (embedded HTTP server) and assert the agents attempt to connect using the provided configuration.
- [ ] Extend the LLM API proxy test suite to include scenarios where MCP tool definitions are expected to be provided by the client agent and are suggested as tool use by the mock LLM API server; The response modules used in the LLM API Proxy should be shared between proxy mode and test server mode, so this tests would ensure that our response formats are compatible with the actual third-party agents (if our responses are compatible, the agent will perform a call to our fake MCP server, as instructed by our mock LLM API server which is driven by a scenario file).
- [ ] Snapshot tests covering CLI help output to ensure MCP flags remain documented (fail CI when accidentally removed).

## R27. Logging guidelines and runtime defaults

**Status:** Not started — logging practices vary widely across crates (mix of `tracing`, `log`, direct `println!`), there is no shared policy for log levels, and release builds still include expensive `trace!` instrumentation. Developers lack guidance on what to log for end-user diagnostics.

**Context:**

- Some crates (e.g., `ah-core`, `ah-rest-server`) use `tracing::info!` while others still rely on `println!` or `eprintln!`, leading to inconsistent formatting and lack of correlation IDs.
- Default log levels differ between binaries; the TUI suppresses logs entirely while the REST server may emit verbose debug output. There is no centralized configuration for `RUST_LOG` defaults or filtering per component.
- Trace-level logs remain compiled into release artifacts, increasing binary size and risking accidental sensitive output on end-user machines.
- No documentation explains when to use `info` vs `warn` vs `error`, how to redact secrets, or how to capture logs for support cases.

**Deliverables:**

- [ ] Author a dedicated specification (`specs/Public/Logging-Guidelines.md`) that defines log-level semantics, field naming conventions, structured logging expectations, default log levels for debug vs release builds, and the requirement that `trace!` instrumentation is compiled out in release configurations. The guidelines should also cover how to decide at which log level to log a certain message, considering that the main purpose of the logs is to diagnose issues on end user machines and cloud agent environments.
- [ ] Introduce a small `ah-logging` helper crate (or module) that standardizes initialization (`tracing_subscriber` setup, JSON/plaintext format selection, correlation IDs) and exposes macros that automatically insert component metadata.
- [ ] Update binaries (`ah`, `ah agent start`, `ah rest-server`, recorder, AgentFS daemons) to use the shared logging initialization and adopt the documented defaults (e.g., `info` in release, `debug` in dev shells, `trace` behind `debug_assertions`).
- [ ] Replace remaining `println!/eprintln!` diagnostics with structured logging calls, ensuring secrets/tokens are redacted via helper utilities.
- [ ] Provide cookbook examples in the spec that show how to log sandbox denials, MCP tool loading, AgentFS failures, etc., to guide developers and coding agents.

**Verification (automated):**

- [ ] Add unit tests for the logging helper to confirm compile-time stripping of `trace!` in release builds (e.g., using `cfg!(debug_assertions)` assertions or compiletests).
- [ ] Extend integration tests (REST server, AgentFS daemon, CLI smoke tests) to capture logs and assert default levels/formatting (e.g., JSON fields present, correlation IDs included).
- [ ] Include a lint (`just lint-logs`) that scans for stray `println!/eprintln!` or direct `tracing::subscriber::set_global_default` calls outside the helper crate.
- [ ] CI smoke test launching primary binaries with `RUST_LOG` unset to ensure they emit at the documented default level and respect redaction helpers.
