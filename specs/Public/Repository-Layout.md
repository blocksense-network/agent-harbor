## Repository Layout (post‑Rust migration)

This document defines the ideal repository structure after the migration to Rust, aligning with CLI and sandbox specs in [CLI.md](CLI.md) and [Sandbox-Profiles.md](Sandbox-Profiles.md). It emphasizes library‑first design, thin binaries, strong testability, and clear platform boundaries, while temporarily preserving the existing Ruby implementation under `legacy/` and keeping current cloud setup scripts at their present paths.

### Principles

- Libraries first; binaries are thin entry points.
- Clear prefixes per domain: `ah-*` (application), `agentfs-*` (filesystem), `sandbox-*` (isolation).
- Cross‑platform adapters are minimal shims to a shared Rust core.
- Specs remain authoritative in `specs/`; runtime JSON schemas are vendored under `schemas/` for code to consume.
- Temporary coexistence: legacy Ruby kept intact under `legacy/` during transition.

### Top‑level layout

```text
agent-harbor/
├─ Cargo.toml                  # [workspace] with all crates
├─ rust-toolchain.toml         # pinned toolchain
├─ .cargo/config.toml          # linker/rpath, per-target cfgs
├─ Justfile
├─ flake.nix / flake.lock      # Nix dev shells and CI builds
├─ .devcontainer/              # Devcontainer definitions
├─ .github/workflows/          # CI: build, test, lint, package
├─ specs/                      # Product/spec documents (source of truth)
│  └─ Public/
├─ docs/                       # Developer docs (how-to, runbooks)
├─ scripts/                    # Small repo scripts (non-build)
├─ schemas/                    # JSON schemas used at runtime (mirrors specs/Public/Schemas)
│  └─ agentfs/
├─ tests/                      # Cross-crate integration & acceptance tests
│  ├─ integration/
│  │  └─ recording/              # `ah agent record` integration tests with Scenario-Format.md
│  ├─ acceptance/
│  ├─ fixtures/
│  └─ tools/                     # Testing and debugging utilities
│     └─ inspect_ahr.rs          # Tool for inspecting .ahr recording files
├─ examples/                   # Small runnable examples per subsystem
├─ electron-app/               # Electron GUI application (cross-platform)
│  ├─ package.json             # Node.js dependencies and scripts
│  ├─ electron-builder.yml     # Packaging configuration for all platforms
│  ├─ src/
│  │  ├─ main/                 # Electron main process (Node.js)
│  │  │  ├─ index.ts           # Main process entry point
│  │  │  ├─ window-manager.ts  # Window lifecycle management
│  │  │  ├─ webui-manager.ts   # WebUI process management (spawns via ELECTRON_RUN_AS_NODE)
│  │  │  ├─ tray-manager.ts    # System tray integration
│  │  │  ├─ notification-manager.ts # Native notifications
│  │  │  ├─ protocol-handler.ts # agent-harbor:// URL scheme
│  │  │  ├─ shortcut-manager.ts # Global keyboard shortcuts
│  │  │  ├─ browser-automation/ # Browser automation subsystem
│  │  │  │  ├─ profile-manager.ts   # Agent browser profile management
│  │  │  │  ├─ playwright-manager.ts # Playwright integration
│  │  │  │  ├─ codex-automation.ts  # Codex browser automation
│  │  │  │  └─ selectors.ts         # UI selectors for automation
│  │  │  └─ ipc-handlers.ts    # IPC API for renderer processes
│  │  └─ renderer/             # Electron renderer process (optional, WebUI handles most UI)
│  │     └─ preload.ts         # Preload script for secure IPC
│  ├─ assets/                  # Application icons and resources
│  │  ├─ icon.png / icon.icns / icon.ico
│  │  ├─ tray-icon-Template.png    # macOS menu bar icon
│  │  └─ tray-icon.png             # Windows/Linux tray icon
│  └─ resources/               # Bundled resources (packed by electron-builder)
│     ├─ webui/                # WebUI server files (runs via ELECTRON_RUN_AS_NODE=1)
│     │  └─ server.js          # SolidStart server entry point (built from webui/app/)
│     └─ cli/                  # Bundled CLI tools (from Rust workspace)
├─ apps/                       # Platform-specific application bundles
│  └─ macos/
│     └─ AgentHarbor/              # Native macOS app for system extension hosting
│        ├─ AgentHarbor.xcodeproj/ # Xcode project for native host app
│        ├─ AgentHarbor/           # Host app source (SwiftUI/AppKit)
│        │  ├─ AppDelegate.swift
│        │  ├─ MainMenu.xib
│        │  └─ Info.plist
│        └─ PlugIns/                  # Embedded system extensions (PlugIns/)
│           └─ AgentFSKitExtension.appex/ # FSKit filesystem extension bundle
├─ adapters/
│  └─ macos/
│     └─ xcode/
│        ├─ AgentFSKitExtension/      # FSKit filesystem extension source code
│        │  ├─ AgentFSKitExtension/   # Extension source files
│        │  │  ├─ AgentFsUnary.swift
│        │  │  ├─ AgentFsVolume.swift
│        │  │  ├─ AgentFsItem.swift
│        │  │  └─ Constants.swift
│        │  ├─ Package.swift          # Swift Package Manager configuration
│        │  └─ build.sh               # Build script for Rust FFI integration
│        └─ (legacy Swift package artifacts and build scripts)
├─ webui/                      # WebUI and related JavaScript/Node.js projects
│  ├─ app/                     # Main SolidStart WebUI
│  ├─ mock-server/             # Mock REST API server for development/testing
│  ├─ e2e-tests/               # Playwright E2E test suite
│  └─ shared/                  # Shared utilities and types between WebUI components
├─ bins/                       # Packaging assets/manifests per final binary
│  ├─ ah/                      # CLI packaging, completions, manpages
│  ├─ agentfs-fuse/            # FUSE host packaging (Linux/macOS dev)
│  ├─ agentfs-winfsp/          # WinFsp host packaging (Windows)
│  └─ sbx-helper/              # Sandbox helper packaging
├─ crates/                     # All Rust crates
│  ├─ ah-cli/                  # Bin: `ah` (Clap subcommands; TUI glue, logging integration)
│  ├─ ah-tui/                  # TUI widgets/flows (Ratatui)
│  ├─ tui-testing/             # TUI testing framework with ZeroMQ IPC
│  ├─ ah-core/                 # Task/session lifecycle orchestration
│  ├─ ah-domain-types/         # Shared domain types (CLI args, task states, enums)
│  ├─ ah-logging/              # Centralized logging utilities with format selection and platform paths
│  ├─ ah-mux-core/             # Low-level, AH-agnostic multiplexer trait + shared types
│  ├─ ah-mux/                  # Monolith crate: AH adapter + all backends as feature-gated modules
│  ├─ config-core/             # Generic config engine (schema/validation/merging)
│  ├─ ah-config-types/         # Distributed strongly-typed config structs
│  ├─ ah-state/                # Local state (SQLite models, migrations)
│  ├─ ah-repo/                 # VCS operations (Git/Hg/Bzr/Fossil)
│  ├─ ah-rest-api-contract/    # Schema types, input validation, etc (shared between mock servers and production server)
│  ├─ ah-rest-client/          # Client for remote REST mode
│  ├─ ah-rest-server/          # Agent Harbor REST service (lib + mock_server binary)
│  ├─ ah-scenario-format/      # Scenario-Format.md parser and playback utilities
│  ├─ ah-connectivity/         # SSH, relays, followers, rendezvous
│  ├─ ah-notify/               # Cross-platform notifications
│  ├─ ah-fleet/                # Multi-OS fleet orchestration primitives
│  ├─ ah-workflows/            # Workflow expansion engine (`/cmd`, dynamic instructions)
│  ├─ ah-schemas/              # Load/validate JSON schemas (e.g., AgentFS control)
│  ├─ agentfs-core/            # Core FS: VFS, CoW, snapshots/branches, locks, xattrs/ADS
│  ├─ agentfs-proto/           # Control plane types + validators
│  ├─ agentfs-client/          # High-level daemon client (handshake/config + control RPCs)
│  ├─ agentfs-fuse-host/       # Bin: libfuse host → `agentfs-core`
│  ├─ agentfs-winfsp-host/     # Bin: WinFsp host → `agentfs-core`
│  ├─ agentfs-ffi/             # C ABI (FFI) for FSKit/Swift bridging
│  ├─ agentfs-backstore-macos/ # Lib: macOS backstore (APFS snapshots, reflink) → `agentfs-core`
│  ├─ sandbox-core/            # Namespaces/lifecycle/exec
│  ├─ sandbox-fs/              # Mount planning (RO seal, overlays)
│  ├─ sandbox-seccomp/         # Dynamic read allow-list (seccomp notify)
│  ├─ sandbox-cgroups/         # cgroup v2 limits + metrics
│  ├─ sandbox-net/             # Loopback/slirp/veth; nftables glue
│  ├─ sandbox-proto/           # Helper⇄supervisor protocol types
│  ├─ sbx-helper/              # Bin: PID 1 inside sandbox; composes sandbox-* crates
│  ├─ ah-command-trace-shim/   # Lib: Cross-platform interpose shim for command capture (DYLD/LD_PRELOAD)
│  ├─ ah-command-trace-proto/  # Lib: SSZ protocol types for shim↔recorder communication
│  ├─ ah-command-trace-server/ # Lib: Tokio-based server for command trace protocol
│  ├─ ah-command-trace-e2e-tests/ # Lib/bin: End-to-end tests for shim injection (prevents cargo test contamination)
│  ├─ ah-recorder/             # Bin/lib: `ah agent record` implementation
│  │  ├─ src/format.rs         # .ahr file format with Brotli compression and record serialization
│  │  ├─ src/viewer.rs         # Ratatui viewer rendering from vt100 model
│  │  ├─ src/ipc.rs            # IPC server for instruction injection with SSZ marshaling
│  │  └─ src/lib.rs            # Core recording functionality (PTY capture, vt100 parsing)
  ├─ ah-gui-core/             # Lib: Shared GUI logic (native Node.js addon via N-API)
│  ├─ ah-gui-webui-manager/    # Lib: WebUI process lifecycle management (native addon)
│  └─ platform-helpers/        # Per-OS helpers (paths, perms, names)
├─ legacy/                     # Temporary home for the Ruby implementation
│  └─ ruby/
│     ├─ bin/                  # existing Ruby entrypoints (kept intact)
│     ├─ lib/
│     ├─ test/
│     ├─ Gemfile / *.gemspec
│     └─ README.md
├─ bin/                        # Thin wrappers/launchers (may exec Rust bins)
└─ (root scripts preserved; see below)
```

### Electron GUI Application Architecture

The `electron-app/` directory contains the **Agent Harbor GUI** - a cross-platform Electron application that provides the primary graphical interface for Agent Harbor on macOS, Windows, and Linux. This GUI embeds the WebUI, manages browser automation for cloud agents, and provides native OS integrations.

#### GUI Responsibilities

- **WebUI Process Management**: Launches and monitors the `ah webui` process
  - **Key Optimization**: WebUI server runs via Electron's bundled Node.js using `ELECTRON_RUN_AS_NODE=1`
  - Eliminates need for separate Node.js installation (~50-80MB saved)
- **Browser Automation**: Provides Playwright-based automation for cloud agents (Codex, Claude, etc.)
- **Native OS Integration**: System tray, notifications, global shortcuts, URL scheme handling
- **CLI Bundling**: Packages complete AH CLI toolchain for unified installation

#### Key Architecture Decisions

- **Electron + TypeScript**: Cross-platform GUI framework with Node.js main process
- **Bundled Chromium**: Provides consistent browser automation environment via Playwright
- **Rust Native Addons**: Process management and core logic via N-API/neon-bindings
- **WebUI Embedding**: BrowserWindow loads WebUI from `http://localhost:PORT`
- **Node.js Runtime Reuse**: WebUI server executes via `ELECTRON_RUN_AS_NODE=1` environment variable
  - See [Using-Electron-As-NodeJS.md](../../specs/Research/Electron-Packaging/Using-Electron-As-NodeJS.md) for implementation details

### macOS System Extension Host Application Architecture

The `apps/macos/AgentHarbor/` directory contains a separate **native macOS host application** required by Apple for system extension registration. This is distinct from the Electron GUI and serves a specific macOS-only purpose.

#### Host App Responsibilities (macOS-specific)

- **Extension Hosting**: Contains and manages system extensions (AgentFSKitExtension)
- **Extension Registration**: Handles PlugInKit registration with macOS System Extensions framework
- **Lifecycle Management**: Manages extension loading, unloading, and system approval workflows
- **Minimal UI**: Provides basic UI for extension status monitoring and approval

#### Relationship to Electron GUI

- **Separate Applications**: Host app and Electron GUI are independent macOS applications
- **Distinct Purposes**: Host app for system extensions only; Electron GUI for main user interface
- **Optional IPC**: Electron GUI can communicate with system extension via IPC when needed
- **Distribution**: Can be bundled together or distributed separately

#### Extension Architecture

- **AgentFSKitExtension**: FSKit-based filesystem extension for user-space AgentFS implementation
- **Extension Sources**: Extension source code is developed in `adapters/macos/xcode/AgentFSKitExtension/`
- **Built Extensions**: Compiled extensions are embedded in the host app's `PlugIns/` directory
- **Future Extensions**: Additional system extensions (network filters, device drivers, etc.) will follow the same pattern

#### Build and Distribution

- Built as a standard macOS application bundle with embedded appex (extension) bundles
- Requires code signing and notarization for system extension approval
- Distributed as a single `.app` bundle containing all extensions
- Uses universal binaries for Intel + Apple Silicon compatibility

### Crate mapping (selected)

- CLI/TUI: `ah-cli`, `ah-tui`, `tui-testing`, `ah-core`, `ah-domain-types`, `ah-logging`, `config-core`, `ah-config-types`, `ah-state`, `ah-repo`, `ah-workflows`, `ah-rest-client`, `ah-rest-server`, `ah-scenario-format`, `ah-notify`, `ah-fleet`, `ah-agents`, `ah-agent-claude`, `ah-agent-codex`, `ah-schemas`.
- GUI (Electron native addons): `ah-gui-core`, `ah-gui-webui-manager`.
- AgentFS: `agentfs-core`, `agentfs-proto`, `agentfs-fuse-host`, `agentfs-winfsp-host`, `agentfs-ffi`.
- Sandbox (Local profile): `sandbox-core`, `sandbox-fs`, `sandbox-seccomp`, `sandbox-cgroups`, `sandbox-net`, `sandbox-proto`, `sbx-helper`.
- Command Trace (R9): `ah-command-trace-shim`, `ah-command-trace-proto`, `ah-command-trace-server`, `ah-command-trace-e2e-tests`.

### Electron GUI structure

- `electron-app/` — Cross-platform Electron GUI application
  - `src/main/` — Main process (Node.js): window management, WebUI process lifecycle, browser automation, native OS integrations
  - `src/renderer/` — Renderer process: preload scripts for secure IPC
  - `assets/` — Application icons and tray icons
  - `resources/webui/` — WebUI server files (executed via `ELECTRON_RUN_AS_NODE=1`)
    - **Key Optimization**: Reuses Electron's bundled Node.js runtime
    - Eliminates need for separate Node.js installation (~50-80MB saved)
  - `resources/cli/` — Bundled CLI binaries from Rust workspace
  - `package.json` — Node.js dependencies (Electron, Playwright, electron-builder)
  - `electron-builder.yml` — Packaging configuration for .pkg, MSI, .deb, .rpm, AppImage

### WebUI structure

- `webui/app/` — Main SolidJS application with server-side rendering support through SolidStart
- `webui/mock-server/` — Legacy TypeScript mock REST API server (deprecated in favor of Rust mock_server binary)
- `webui/e2e-tests/` — Playwright E2E test suite with pre-scripted scenarios controlling both mock server and UI interactions
- `webui/shared/` — Shared TypeScript utilities, API client code, and type definitions used across WebUI components

### Multiplexer crates structure

We use the [subcrates design pattern](Subcrates-Pattern.md) with a **monolith + facades strategy** to reduce compile times while preserving optional tiny crates:

- `ah-mux-core` — low‑level AH‑agnostic trait and shared types (no OS bindings).
- `ah-mux` (monolith) — contains the high‑level AH adapter and all concrete backends as modules gated by cargo features (e.g., `tmux`, `wezterm`, `kitty`, `iterm2`, `tilix`, `winterm`, `vim`, `emacs`). Only requested features are compiled.
- Optional facade crates (tiny re‑exports) to keep per‑backend packages when desired:
  - `ah-mux-tmux` depends on `ah-mux` with `features=["tmux"]` and `default-features=false`, then `pub use ah_mux::tmux::*;`
  - Same for `ah-mux-wezterm`, `ah-mux-kitty`, …

Usage

- Apps can depend directly on `ah-mux` and request the union of backends they need, compiling the monolith once.
- Or depend on multiple facades; cargo feature unification compiles `ah-mux` once with the union of features.

Why this helps

- One heavy compilation unit: all codegen happens in `ah-mux` once, even if multiple backends are used together.
- Keep or publish tiny crates: facades compile in milliseconds and maintain package boundaries.
- Flexible consumption: choose single‑dep monolith or per‑backend facades without N× compile cost.

Gotchas

- Visibility changes: code moved under `ah-mux` modules; adjust `pub(crate)`/paths accordingly.
- Proc‑macro crates cannot be merged; not applicable here.
- Tests/examples may need to move into `ah-mux/tests/` or remain in facades if they rely on crate boundaries.

Extra compile‑time wins

- Unify dependency versions/features across the workspace (consider a workspace‑hack crate).
- Use sccache/`RUSTC_WRAPPER` and check `CARGO_BUILD_TIMINGS` to validate improvements.

See [CLI.md](CLI.md) for command surface and [Sandbox-Profiles.md](Sandbox-Profiles.md) for isolation profiles and behavior.

### Cloud setup scripts (paths preserved)

The following existing setup scripts remain at the repository root to preserve current tooling and docs:

- `codex-setup`
- `copilot-setup`
- `jules-setup`
- `goose-setup`

Notes:

- These scripts are considered external helpers and may call into Rust binaries as migration proceeds.
- Additional provider scripts (if added later) should also live at the repository root for consistency.

### Legacy Ruby

- All current Ruby code is retained under `legacy/ruby/` without restructuring to minimize churn during migration.
- Existing Ruby `bin/` entrypoints are duplicated here; top‑level `bin/` may be thin shims that exec Rust `ah` as features roll over.
- Tests continue to run under `legacy/ruby/test/` until replaced by Rust acceptance tests under `tests/`.

### Testing and CI

- Unit tests live within each crate; cross‑crate tests in `tests/` mirror acceptance plans in AgentFS and CLI specs.
- CI fans out per crate (build/test/lint) and runs privileged lanes only where necessary (FUSE/WinFsp/FSKit).
