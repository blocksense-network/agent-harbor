# Agent Harbor Component Architecture

This document provides an overview of the major components in the Agent Harbor system, their responsibilities, key implementation files, and communication patterns.

**Note:** The described component breakdown shows how Agent Harbor's different processes and services are organized, but the majority of these roles are handled by the same `ah` binary, launched with different flags. The key internal components of the `ah` binary include the AH core crate (`crates/ah-core/`) with its TaskManager object that orchestrates most operational workflows.

## 1. Agent Harbor TUI Dashboard

**Description:** A terminal-based dashboard for launching and monitoring agent tasks, integrated with terminal multiplexers (tmux, zellij, screen). Provides a task-centric interface with real-time activity streaming and keyboard-driven navigation.

**See also:** [TUI-PRD.md](../specs/Public/TUI-PRD.md) for detailed UI specifications.

**Key Implementation Files:**

- `crates/ah-tui/` - Main TUI implementation using Ratatui
- `crates/ah-tui-multiplexer/` - Multiplexer integration layer
- `crates/ah-mux/` - Low-level multiplexer abstractions

**Manual Testing (for humans):**

- `just run-mock-tui-dashboard` - Run the TUI dashboard with fully synthetic data, produced by a mock task manager.
- `just manual-test-tui [--repo NAME] [--fs TYPE]` - Run a manual test of the full TUI launched over the selected example repo, placed in the test filesystem.
- `just manual-test-tui-remote [--mode {rest,mock}] [--scenario NAME]` - Launch the remote-mode harness that boots a REST or mock server, seeds demo data, and attaches the dashboard via `ah tui --remote-server ...`. Mock mode now reuses the Rust REST server with an embedded dataset that includes active/running sessions, so the TUI always has live activity to display. Any `--scenario` flag is copied alongside the run for reference, but the curated dataset is available out of the box. Copy `manual-tests/env.remote-example` to `.env` to pre-populate sample API keys and tenant metadata if you need authentication flags.

**Log Files:** See the [Log Files and Debugging](#log-files-and-debugging) section above for comprehensive logging information.

**Communication:**

- **Local Mode:** Directly uses the `LocalTaskManager` from `ah-core` crate to launch agent coding tasks
- **Remote Mode:** Uses REST API to communicate with access point daemon
- **Multiplexers:** Uses `ah-mux` traits to control tmux/zellij/screen sessions

## 2. Supported Multiplexers

**Description:** Terminal multiplexers that provide windowing environment for agent sessions. Support tmux, Zellij, GNU Screen, iTerm2, Kitty, WezTerm, Ghostty, Tilix, Windows Terminal, Vim/Emacs, and other terminal apps with split-pane capabilities.

**See also:** [Terminal-Multiplexers/TUI-Multiplexers-Overview.md](../specs/Public/Terminal-Multiplexers/TUI-Multiplexers-Overview.md) for multiplexer integration details.

**Key Implementation Files:**

- `crates/ah-mux/` - Core multiplexer abstraction with trait definitions
- `crates/ah-mux/src/$multiplexer.rs` - Individual backend implementations

**Manual Testing:**

- The multiplexer detection logic can be tested by running `ah health`
- The multiplexer operations can be experienced in `just manual-test-tui`

**Communication:**

- **AH Adapter:** Translates Agent Harbor layouts into multiplexer primitives
- **TUI:** Auto-attaches to multiplexer sessions and creates split-pane windows
- **Agent Starter:** Creates new windows/tabs for agent execution

## 3. Agent Starter (`ah agent start` command)

**Description:** Orchestrates the complete agent execution workflow, including workspace preparation, sandboxing, and agent launching. Integrates with AgentFS daemon for filesystem snapshots and per-process mounting, runtime selection, integration with various execution environments, recreation of user credentials in the selected sandbox environment, agent setup for starting from particular snapshots, and agent configuration for work with the LLM API proxy.

**See also:** [CLI.md](../specs/Public/CLI.md) for detailed command specifications.

**Key Implementation Files:**

- `crates/ah-core/` - Core business logic and orchestration
- `crates/ah-cli/` - CLI command implementation

**Manual Testing:**

- `just manual-test-agent-start [--agent AGENT_TYPE] [--prompt TEXT] [--scenario NAME]` - Starts an agent process in the foreground without any extra gizmos. This is useful for testing our handling of agent CLI flags, credentials, MCP and proxy configuration, etc.

**Log Files:** See the [Log Files and Debugging](#log-files-and-debugging) section above for comprehensive logging information.

**Communication:**

- **Sandbox Core:** Integrates with Linux sandboxing (`ah-sandbox`) for isolation
- **Execs the Target Agent:** The agent starter process is very short-lived. It replaces itself with the target agent process through execve.

## 4. Agent Session Recorder (`ah agent record` command)

**Description:** Records terminal output from agent sessions with byte-perfect fidelity using PTY capture and vt100 parsing for persistent storage in our compressed AHR format. The native UI of the agent software can be displayed in real-time during recording with added snapshot indicators in a minimalistic gutter/footer UI. Supports launching new session forks through interactive UI displayed over the recorded agent UI.

**See also:** [ah-agent-record.md](../specs/Public/ah-agent-record.md) for detailed recording specifications.

**Key Implementation Files:**

- `crates/ah-recorder/` - Main recording implementation
- `crates/ah-cli/` - CLI integration

**Manual Testing (for humans):**

- `just manual-test-ah-agent-record [--agent AGENT_TYPE] [--prompt TEXT] [--scenario NAME]` - Manual testing of agent recording functionality

`AGENT_TYPE` can be `mock` and `mock-simple` which execute custom programs that simulate the behavior of a real agent. This is useful for testing the Session Recorder/Viewer UI in a simpler, more controlled environment.

**Log Files:** See the [Log Files and Debugging](#log-files-and-debugging) section above for comprehensive logging information.

**Communication:**

- **Filesystem Snapshots:** Receives IPC notifications when snapshots are created
- **TUI/WebUI:** Streams events via unnamed pipes or SSE for real-time monitoring
- **Local Database:** Stores compressed .ahr files and metadata

## 5. AgentFS Daemon

**Description:** Core filesystem service that provides cross-platform filesystem snapshots, per-process mounting, and overlay filesystem capabilities. Implements copy-on-write snapshots, writable branches from snapshots, and process-scoped filesystem views. On macOS, integrates with the AgentFS interpose shim for zero-overhead data I/O while maintaining overlay semantics through API interposition.

**See also:** [AgentFS/AgentFS.md](../specs/Public/AgentFS/AgentFS.md) for complete filesystem specifications and [AgentFS/macOS-FS-Hooks.md](../specs/Public/AgentFS/macOS-FS-Hooks.md) for macOS interposition details.

**Key Implementation Files:**

- `crates/agentfs-core/` - Core Rust library implementing filesystem logic, snapshots, and branch management
- `crates/agentfs-daemon/` - Daemon service that manages filesystem state and provides control plane APIs
- `crates/agentfs-interpose-shim/` - macOS interposition library that hooks filesystem APIs for zero-overhead I/O
- `crates/agentfs-interpose-e2e-tests/` - End-to-end testing for interposition functionality

**Testing:**

- `just test-daemon-integration` - Automated integration tests for daemon functionality
- `just test-fuse-basic <mountpoint>` - Manual testing of FUSE filesystem mounting

**Log Files:** See the [Log Files and Debugging](#log-files-and-debugging) section above for comprehensive logging information. Integration tests follow standard Rust test output patterns.

**Communication:**

- **Agent Starter:** Requests filesystem snapshots and branch creation for workspace isolation
- **Interpose Shim (macOS):** Uses CFMessagePort for control-plane communication; forwards file descriptors via SCM_RIGHTS for data I/O bypass
- **Platform Glue Layers:** Linux uses FUSE, Windows uses WinFsp, macOS uses FSKit + optional interposition
- **Backstore Management:** Provisions RAM disks or host filesystem directories for upper layer storage
- **Snapshot Export:** Supports native filesystem snapshots (ZFS/BtrFS/ReFS) when available, with fallback to selective file copying

## 6. Agent Harbor Access Point

**Description:** REST API server that orchestrates agent execution across multiple machines and serves as a hub for connected worker machines acting as followers in multi-OS testing. Provides centralized task management, authentication, real-time event streaming, and fleet coordination for cross-platform agent coding tasks.

**See also:** [REST-Service/API.md](../specs/Public/REST-Service/API.md) for API specifications and [Multi-OS-Testing.md](../specs/Public/Multi-OS-Testing.md) for fleet coordination details.

**Key Implementation Files:**

- `crates/ah-rest-server/` - REST API implementation
- `crates/ah-rest-client/` - Client library for API communication
- `crates/ah-core/` - Shared business logic

**Communication:**

- **WebUI:** Receives API calls and serves web interface
- **TUI:** Connects via REST client for remote mode operations
- **Executors:** Uses QUIC control plane and SSH tunneling for remote execution
- **Database:** Persists state in SQLite or external databases

## 7. Agent Harbor WebUI

**Description:** Browser-based interface for task creation, monitoring, and management. Provides graphical dashboard with real-time activity streaming and IDE integration.

**See also:** [WebUI-PRD.md](../specs/Public/WebUI-PRD.md) for detailed UI specifications.

**Key Implementation Files:**

- `webui/` - SolidJS-based web application
- `crates/ah-rest-client/` - API communication layer

**Automated Testing (for agents):**

- `just webui-test` - Run all WebUI tests
- `just webui-test-api` - API integration tests
- `just webui-test-headed` - Headed browser tests (with UI)
- `just webui-test-debug` - Run tests in debug mode

**Manual Testing (for humans):**

- `just manual-test-webui` - Manual testing of WebUI functionality

**Log Files:** See the [Log Files and Debugging](#log-files-and-debugging) section above for comprehensive logging information.

**Communication:**

- **Access Point:** Uses REST API for all operations
- **SSE/WebSocket:** Receives real-time events from server
- **Agent Harbor GUI:** Can be embedded in Electron wrapper
- **External IDEs:** Launches VS Code, Cursor, Windsurf with workspace connections

## 8. Agent Harbor Electron GUI App

**Description:** Cross-platform Electron application providing native desktop wrapper around WebUI with system tray, custom URL scheme handling, and native notifications.

**See also:** [Agent-Harbor-GUI.md](../specs/Public/Agent-Harbor-GUI.md) for GUI application specifications.

**Key Implementation Files:**

- `electron-app/` - Electron application with Node.js build system
- `apps/macos/AgentHarbor/` - macOS-specific native components

**Automated Testing (for agents):**

- `just electron-test` - Run Electron tests
- `just electron-test-headed` - Run Electron tests with headed browser

**Manual Testing (for humans):**

- `just manual-test-electron` - Manual testing of Electron GUI functionality

**Log Files:** See the [Log Files and Debugging](#log-files-and-debugging) section above for comprehensive logging information.

**Communication:**

- **WebUI Process:** Launches and monitors `ah webui` subprocess
- **CLI Tools:** Bundles complete AH CLI toolchain
- **URL Scheme Handler:** Registers `agent-harbor://` protocol and routes to WebUI
- **System Integration:** Provides native notifications and system tray

## 9. LLM API Proxy

**Description:** Optional proxy service for routing LLM API requests, providing session management, credential discovery, and provider abstraction.

**Key Implementation Files:**

- `crates/llm-api-proxy/` - Proxy server implementation

**Manual Testing:**

- Testing is integrated with agent startup testing (`just manual-test-agent-start`)

**Log Files:** See the [Log Files and Debugging](#log-files-and-debugging) section above for comprehensive logging information.

**Communication:**

- **Agent Starter:** Routes LLM API calls through proxy when configured
- **External APIs:** Forwards requests to OpenAI, Anthropic, and other LLM providers
- **Session Management:** Maintains user sessions with automatic credential discovery

## 10. Follower Agent Node

**Description:** Distributed execution nodes for running agents across multiple machines in coordinated fleets. Development has not yet started.

**Key Implementation Files:**

- Not yet implemented

**Testing:**

- Not yet implemented

**Log Files:** See the [Log Files and Debugging](#log-files-and-debugging) section above for comprehensive logging information.

**Communication:**

- **Access Point:** Registers as executor and receives task assignments
- **Leader Node:** Participates in multi-OS testing with filesystem synchronization
- **SSH Tunneling:** Uses HTTP CONNECT for connectivity without public IPs

## 11. Browser with Agent Browser Profile

**Description:** Automated browser instances with persistent profiles for cloud-based agents requiring web authentication. Uses Playwright for automation with shared profile convention.

**See also:** [Agent-Browsers/Agent-Browser-Profiles.md](../specs/Public/Agent-Browsers/Agent-Browser-Profiles.md) for browser profile specifications.

**Key Implementation Files:**

- Browser automation scripts integrated with agent implementations
- Profile storage following `Agent-Browser-Profiles.md` specification

**Communication:**

- **Agent Starter:** Launches browser automation when `--browser-automation true`
- **Cloud Agents:** Handles authentication and task submission for web-based platforms
- **Profile Storage:** Reads/writes browser data to user-specific profile directories

## 12. Lima/Docker/Other VM Environments

**Description:** Virtualized execution environments for agent isolation, including Lima VMs for macOS multi-OS testing and Docker containers for sandboxing.

**See also:** [Lima-VM-Images.md](../specs/Public/Lima-VM-Images.md) for VM image specifications.

**Key Implementation Files:**

- `crates/ah-sandbox/` - Sandbox core functionality
- `crates/sandbox-*` - Platform-specific sandbox implementations
- Lima VM images and configuration

**Testing:**

- `just test-vms` - Automated tests for VM environments
- `just test-containers` - Automated tests for container environments

**Log Files:** See the [Log Files and Debugging](#log-files-and-debugging) section above for comprehensive logging information.

**Communication:**

- **Agent Starter:** Configures execution environment based on `--sandbox` flag
- **Filesystem Providers:** Integrates with snapshot providers for workspace isolation
- **Host System:** Uses namespaces, cgroups, and virtualization for containment

## Log Files and Debugging

All Agent Harbor components write logs to help with debugging and monitoring. Here's a comprehensive guide to where logs are written:

### Manual Testing Scripts

- **Script logs:** Written to `working-directory/user-home/script.log` where `working-directory` is auto-generated based on test parameters (`--fs` and `--repo`)
- **Request/response logs:** Written to `working-directory/user-home/session.log` (when logging is enabled)
- **Remote harness logs:** The remote orchestration script stores structured artifacts under `manual-tests/runs/<run-id>/` and `manual-tests/logs/<run-id>/`. Server stdout/stderr is captured in `manual-tests/logs/<run-id>/rest-server.log` for both REST and mock modes, making it easy to inspect API interactions post-run.

### Remote Manual Mode Troubleshooting

- **Ports already in use:** Both the REST and mock servers default to `127.0.0.1:38080`/`38180`. Use `just manual-test-tui-remote --port 39080` when another process occupies the default sockets.
- **Missing dependencies:** The harness delegates to `cargo run -p ah-rest-server` (or the prebuilt binary when `--no-build` is set). Run `nix develop` before invoking the script so the Rust toolchain and target binaries are available.
- **Multiplexer focus issues:** When launching inside an existing tmux/zellij session, the dashboard may detect and reuse the parent multiplexer. Pass extra arguments after `--`, e.g. `just manual-test-tui-remote -- --multiplexer tmux`, to force a fresh tmux window and avoid stealing focus from the controlling pane.

### Mock TUI Dashboard

- **Trace logs:** Written to `tests/tools/mock-tui-dashboard/tui-mvvm-trace.log` (when `RUST_LOG` environment variable is set)

### WebUI Automated Tests

- **Test logs:** Written to `webui/e2e-tests/test-results/logs/test-run-*/` directories
- **Analysis command:** `just webui-test-failed`

### AH Command Logs

The main `ah` CLI command writes logs to platform-standard locations:

- **Linux:** `~/.local/share/agent-harbor/agent-harbor.log`
- **macOS:** `~/Library/Logs/agent-harbor.log`
- **Windows:** `%APPDATA%\agent-harbor\agent-harbor.log`

### AH Command Database/State

Session state and database files are stored separately:

- **Linux:** `${XDG_STATE_HOME:-~/.local/state}/agent-harbor/state.db` (or `$AH_HOME/state.db` if `AH_HOME` is set)
- **macOS:** `~/Library/Application Support/agent-harbor/state.db` (or `$AH_HOME/state.db` if `AH_HOME` is set)
- **Windows:** `%LOCALAPPDATA%\agent-harbor\state.db` (or `$AH_HOME/state.db` if `AH_HOME` is set)

### Live Log Monitoring

- **Command:** `ah session logs -f` to follow live session logs in real-time
