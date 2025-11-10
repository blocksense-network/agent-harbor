# Agent Harbor Component Architecture

This document provides an overview of the major components in the Agent Harbor system, their responsibilities, key implementation files, and communication patterns.

**Note:** The described component breakdown shows how Agent Harbor's different processes and services are organized, but the majority of these roles are handled by the same `ah` binary, launched with different flags. The key internal components of the `ah` binary include the AH core crate (`crates/ah-core/`) with its TaskManager object that orchestrates most operational workflows.

## 1. Agent Harbor TUI Dashboard

**Description:** A terminal-based dashboard for launching and monitoring agent tasks, integrated with terminal multiplexers (tmux, zellij, screen). Provides a task-centric interface with real-time activity streaming and keyboard-driven navigation.

**See also:** [TUI-PRD.md](Public/TUI-PRD.md) for detailed UI specifications.

**Key Implementation Files:**

- `crates/ah-tui/` - Main TUI implementation using Ratatui
- `crates/ah-tui-multiplexer/` - Multiplexer integration layer
- `crates/ah-mux/` - Low-level multiplexer abstractions

**Communication:**

- **Local Mode:** Directly calls `ah-core` crate for task execution
- **Remote Mode:** Uses REST API to communicate with access point daemon
- **Multiplexers:** Uses `ah-mux` traits to control tmux/zellij/screen sessions
- **TaskManager:** The LocalTaskManager in `ah-core` is responsible for starting the agent starter under the agent recorder (see below) in both local mode and on the server.

## 2. Supported Multiplexers

**Description:** Terminal multiplexers that provide windowing environment for agent sessions. Support tmux, Zellij, GNU Screen, iTerm2, Kitty, WezTerm, Ghostty, Tilix, Windows Terminal, Vim/Emacs, and other terminal apps with split-pane capabilities.

**See also:** [Terminal-Multiplexers/TUI-Multiplexers-Overview.md](Public/Terminal-Multiplexers/TUI-Multiplexers-Overview.md) for multiplexer integration details.

**Key Implementation Files:**

- `crates/ah-mux/` - Core multiplexer abstraction with trait definitions
- `crates/ah-mux/backend-*` - Individual backend implementations

**Communication:**

- **AH Adapter:** Translates Agent Harbor layouts into multiplexer primitives
- **TUI:** Auto-attaches to multiplexer sessions and creates split-pane windows
- **Agent Starter:** Creates new windows/tabs for agent execution

## 3. Agent Starter (`ah agent start` command)

**Description:** Orchestrates the complete agent execution workflow, including workspace preparation, sandboxing, and agent launching. Handles filesystem snapshots, runtime selection, integration with various execution environments, recreation of user credentials in the selected sandbox environment, agent setup for starting from particular snapshots, and agent configuration for work with the LLM API proxy.

**See also:** [CLI.md](Public/CLI.md) for detailed command specifications.

**Key Implementation Files:**

- `crates/ah-core/` - Core business logic and orchestration
- `crates/ah-cli/` - CLI command implementation

**Communication:**

- **Sandbox Core:** Integrates with Linux sandboxing (`ah-sandbox`) for isolation
- **Execs the Target Agent:** The agent starter process is very short-lived. It replaces itself with the target agent process through execve.

## 4. Agent Session Recorder (`ah agent record` command)

**Description:** Records terminal output from agent sessions with byte-perfect fidelity using PTY capture and vt100 parsing. Provides live viewing, snapshot coordination, and compressed storage in .ahr format. Can stream live events to the TUI and the WebUI access point and provides real-time session viewer with agent forking support.

**See also:** [ah-agent-record.md](Public/ah-agent-record.md) for detailed recording specifications.

**Key Implementation Files:**

- `crates/ah-recorder/` - Main recording implementation
- `crates/ah-cli/` - CLI integration

**Communication:**

- **Filesystem Snapshots:** Receives IPC notifications when snapshots are created
- **TUI/WebUI:** Streams events via unnamed pipes or SSE for real-time monitoring
- **Local Database:** Stores compressed .ahr files and metadata

## 5. Agent Harbor Access Point

**Description:** REST API server that orchestrates agent execution across multiple machines and serves as a hub for connected worker machines acting as followers in multi-OS testing. Provides centralized task management, authentication, real-time event streaming, and fleet coordination for cross-platform validation workflows.

**See also:** [REST-Service/API.md](Public/REST-Service/API.md) for API specifications and [Multi-OS-Testing.md](Public/Multi-OS-Testing.md) for fleet coordination details.

**Key Implementation Files:**

- `crates/ah-rest-server/` - REST API implementation
- `crates/ah-rest-client/` - Client library for API communication
- `crates/ah-core/` - Shared business logic

**Communication:**

- **WebUI:** Receives API calls and serves web interface
- **TUI:** Connects via REST client for remote mode operations
- **Executors:** Uses QUIC control plane and SSH tunneling for remote execution
- **Database:** Persists state in SQLite or external databases

## 6. Agent Harbor WebUI

**Description:** Browser-based interface for task creation, monitoring, and management. Provides graphical dashboard with real-time activity streaming and IDE integration.

**See also:** [WebUI-PRD.md](Public/WebUI-PRD.md) for detailed UI specifications.

**Key Implementation Files:**

- `webui/` - SolidJS-based web application
- `crates/ah-rest-client/` - API communication layer

**Communication:**

- **Access Point:** Uses REST API for all operations
- **SSE/WebSocket:** Receives real-time events from server
- **Agent Harbor GUI:** Can be embedded in Electron wrapper
- **External IDEs:** Launches VS Code, Cursor, Windsurf with workspace connections

## 7. Agent Harbor Electron GUI App

**Description:** Cross-platform Electron application providing native desktop wrapper around WebUI with system tray, custom URL scheme handling, and native notifications.

**See also:** [Agent-Harbor-GUI.md](Public/Agent-Harbor-GUI.md) for GUI application specifications.

**Key Implementation Files:**

- `electron-app/` - Electron application with Node.js build system
- `apps/macos/AgentHarbor/` - macOS-specific native components

**Communication:**

- **WebUI Process:** Launches and monitors `ah webui` subprocess
- **CLI Tools:** Bundles complete AH CLI toolchain
- **URL Scheme Handler:** Registers `agent-harbor://` protocol and routes to WebUI
- **System Integration:** Provides native notifications and system tray

## 8. LLM API Proxy

**Description:** Optional proxy service for routing LLM API requests, providing session management, credential discovery, and provider abstraction.

**Key Implementation Files:**

- `crates/llm-api-proxy/` - Proxy server implementation

**Communication:**

- **Agent Starter:** Routes LLM API calls through proxy when configured
- **External APIs:** Forwards requests to OpenAI, Anthropic, and other LLM providers
- **Session Management:** Maintains user sessions with automatic credential discovery

## 9. Follower Agent Node

**Description:** Distributed execution nodes for running agents across multiple machines in coordinated fleets. Development has not yet started.

**Key Implementation Files:**

- Not yet implemented

**Communication:**

- **Access Point:** Registers as executor and receives task assignments
- **Leader Node:** Participates in multi-OS testing with filesystem synchronization
- **SSH Tunneling:** Uses HTTP CONNECT for connectivity without public IPs

## 10. Browser with Agent Browser Profile

**Description:** Automated browser instances with persistent profiles for cloud-based agents requiring web authentication. Uses Playwright for automation with shared profile convention.

**See also:** [Agent-Browsers/Agent-Browser-Profiles.md](Public/Agent-Browsers/Agent-Browser-Profiles.md) for browser profile specifications.

**Key Implementation Files:**

- Browser automation scripts integrated with agent implementations
- Profile storage following `Agent-Browser-Profiles.md` specification

**Communication:**

- **Agent Starter:** Launches browser automation when `--browser-automation true`
- **Cloud Agents:** Handles authentication and task submission for web-based platforms
- **Profile Storage:** Reads/writes browser data to user-specific profile directories

## 11. Lima/Docker/Other VM Environments

**Description:** Virtualized execution environments for agent isolation, including Lima VMs for macOS multi-OS testing and Docker containers for sandboxing.

**See also:** [Lima-VM-Images.md](Public/Lima-VM-Images.md) for VM image specifications.

**Key Implementation Files:**

- `crates/ah-sandbox/` - Sandbox core functionality
- `crates/sandbox-*` - Platform-specific sandbox implementations
- Lima VM images and configuration

**Communication:**

- **Agent Starter:** Configures execution environment based on `--sandbox` flag
- **Filesystem Providers:** Integrates with snapshot providers for workspace isolation
- **Host System:** Uses namespaces, cgroups, and virtualization for containment
