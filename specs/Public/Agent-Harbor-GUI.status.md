### Overview

This document tracks the implementation status and plan for the Agent Harbor GUI, a cross-platform native desktop application that provides a graphical interface wrapper around the `ah webui` process with native desktop integrations.

Goal: deliver a production-ready cross-platform desktop application (macOS, Windows, Linux) that embeds the WebUI, handles the `agent-harbor://` URL scheme, provides native system integrations (tray, notifications), and bundles the complete AH CLI toolchain for seamless installation.

Total estimated timeline: 17-21 weeks (phased with parallel development tracks)

**Current Status**: 🚧 Phase 0 Complete - Foundation established, moving to Phase 1 (Core GUI)
**Last Updated**: October 16, 2025

### Key Design Principles

Per [Agent-Harbor-GUI.md](Agent-Harbor-GUI.md), the GUI is a **thin native wrapper** that:

- Embeds and manages the existing WebUI (already functional with 23/162 E2E tests passing)
- Delegates all workflow functionality to the underlying WebUI
- Provides native OS integration (system tray, notifications, URL scheme handling)
- Bundles the complete AH CLI toolchain for unified installation
- Maintains cross-platform consistency while respecting platform conventions

### Architecture Overview

**Component Separation:**

- **Electron GUI Shell**: Handles window management, native OS integrations, and WebUI process lifecycle
- **Embedded Chromium**: Provides browser automation capabilities for cloud agent integrations (Codex, etc.)
- **WebUI Process**: Existing SolidJS/SolidStart application serving the main UI (reused as-is)
  - **Key Optimization**: Runs via Electron's bundled Node.js using `ELECTRON_RUN_AS_NODE=1`
  - Eliminates need for separate Node.js installation (~50-80MB saved)
- **CLI Toolchain**: Complete set of `ah` commands bundled for installation
- **URL Scheme Handler**: Rust binary handling `agent-harbor://` protocol (shared with headless systems)

**Technology Stack:**

- **GUI Framework**: Electron (cross-platform with Chromium)
  - **Critical Rationale**: Browser automation capability required for cloud agent integrations
    - See [Browser-Automation/](Browser-Automation/) specs for Codex and other cloud platforms
    - Playwright automation requires stable Chromium binary (shipped with Electron)
    - Reduces browser compatibility issues on user machines
    - Enables headless/headful automation with persistent browser profiles
  - **Additional Benefits**:
    - Cross-platform consistency (single codebase for macOS/Windows/Linux)
    - Rich ecosystem for native integrations
    - Well-established packaging and distribution patterns
- **WebUI Embedding**: Electron BrowserWindow loading `http://localhost:PORT` from WebUI process
- **Browser Automation**: Playwright with Electron's Chromium for cloud agent workflows
- **Process Management**: Rust crates for WebUI process lifecycle, packaged as native Node addons
- **Native Extensions**: Small CLI helpers or native Electron modules for system-specific functions
- **URL Handler**: Rust binary per [Handling-AH-URL-Scheme.status.md](Handling-AW-URL-Scheme.status.md)

### Milestone Completion & Outstanding Tasks

Each milestone maintains an **outstanding tasks list** that tracks specific deliverables, bugs, and improvements. When milestones are completed, their sections are expanded with:

- Implementation details and architectural decisions
- References to key source files for diving into the implementation
- Test coverage reports and known limitations
- Integration points with other milestones/tracks

### Parallel Development Tracks

Once foundation is established (M0-M1), multiple tracks can proceed in parallel:

- **macOS Native Track**: AppKit/SwiftUI host application with WKWebView embedding
- **Cross-Platform WebUI Track**: Continue WebUI development (ongoing in parallel)
- **CLI Bundling Track**: Packaging and PATH integration for bundled CLI tools
- **URL Scheme Track**: Protocol handler implementation (see [Handling-AH-URL-Scheme.status.md](Handling-AW-URL-Scheme.status.md))
- **Testing Infrastructure Track**: Native UI testing framework setup

### Development Phases

**Phase 0: Foundation & Architecture (4-5 weeks)**

**M0.1 Architecture Decision & Technology Selection** ✅ **COMPLETED** (1 week)

**M0.2 Electron Project Scaffolding & Build Infrastructure** ✅ **COMPLETED** (2 weeks)

- **Deliverables**:
  - Technology stack selection document
  - Cross-platform strategy definition
  - Build system architecture
  - Development environment setup

- **Implementation Details**:
  - **Key Decision**: Use Electron for cross-platform GUI
    - **Primary Rationale**: Browser automation capability is critical requirement
      - Cloud agent integrations (Codex, Claude, etc.) require browser automation
      - Playwright automation needs stable Chromium binary (bundled with Electron)
      - Reduces browser compatibility issues on end-user machines
      - Enables headless/headful automation with persistent browser profiles
      - See [Browser-Automation/](Browser-Automation/) for automation requirements
    - **Secondary Benefits**:
      - Single codebase for macOS, Windows, Linux
      - Rich ecosystem for native integrations (notifications, tray, protocols)
      - Well-established packaging patterns (electron-builder)
      - Strong TypeScript/Node.js integration
  - **macOS System Extension Integration**:
    - Electron app and native `apps/macos/AgentHarbor/` can coexist
    - System extension remains in separate native host app (required by Apple)
    - Electron app can communicate with system extension via IPC when available
    - Both apps can be distributed together or separately
  - **Build System**:
    - Electron + electron-builder for packaging
    - TypeScript for main and renderer processes
    - Rust components via native Node addons (neon-bindings or N-API)
    - Playwright for browser automation
    - Nix flake for reproducible development environment
  - **WebUI Embedding Strategy**:
    - Electron BrowserWindow loads WebUI from `http://localhost:PORT`
    - Separate BrowserWindow instances for browser automation (Codex, etc.)
    - IPC between main process and renderer processes
    - Process isolation between WebUI and automation contexts

- **Key Source Files**:
  - `specs/Public/Agent-Harbor-GUI.status.md` - This document
  - `specs/Research/Electron-Packaging/Agent-Harbor-Electron-Packaging.md` - Packaging reference (now directly applicable)
  - `specs/Public/Browser-Automation/` - Browser automation requirements
  - `specs/Public/Browser-Automation/Codex.md` - Codex automation spec

- **Verification Results**:
  - [x] Technology stack documented with rationale
  - [x] Cross-platform strategy defined
  - [x] Browser automation requirements drive architecture decision
  - [x] Build system architecture specified
  - [x] Electron decision aligns with Browser-Automation requirements

**M0.2 Electron Project Scaffolding & Build Infrastructure** ✅ **COMPLETED** (2 weeks)

- **Deliverables**:

  **Electron Application Structure:**
  - Initialize Electron project with TypeScript
  - Set up electron-builder for packaging
  - Configure main process (Node.js backend)
  - Configure renderer process (WebUI embedding)
  - Set up IPC communication layer
  - Configure hot-reload for development

  **Build System:**
  - electron-builder configuration for all platforms:
    - macOS: .pkg installer with Developer ID signing
    - Windows: MSI installer with Authenticode signing
    - Linux: .deb, .rpm, AppImage
  - TypeScript compilation for main and renderer
  - Asset bundling (icons, resources)
  - Environment-specific configurations (dev/staging/prod)

  **Playwright Integration:**
  - Add Playwright as dependency
  - Configure persistent browser contexts
  - Set up browser profile management
  - Create basic automation test harness

  **Rust Native Addons:**
  - Set up neon-bindings or N-API for Rust integration
  - Create `crates/ah-gui-core/` for shared logic
  - Configure native addon building in electron-builder
  - Implement basic FFI test (e.g., "hello from Rust")

  **Development Environment:**
  - Nix flake with Electron, Node.js, Rust toolchain
  - VS Code configuration for debugging
  - ESLint and Prettier for code quality
  - Git hooks for pre-commit checks

- **Key Source Files**:
  - `electron-app/package.json` - Project dependencies and scripts
  - `electron-app/electron-builder.yml` - Packaging configuration
  - `electron-app/src/main/index.ts` - Main process entry point
  - `electron-app/src/renderer/index.ts` - Renderer process entry point
  - `crates/ah-gui-core/` - Rust native addon crate

- **Implementation Details**:
  - **Main Process**: Created `src/main/index.ts` with Electron BrowserWindow, window state persistence via electron-store, IPC handlers for window controls and app info
  - **Renderer Process**: Created `src/renderer/preload.ts` with secure context bridge exposing safe APIs for IPC communication
  - **IPC Layer**: Implemented secure bidirectional communication between main and renderer processes with whitelisted channels
  - **Playwright Integration**: Created `src/main/browser-automation/playwright-manager.ts` for persistent browser contexts and profile management
  - **Browser Automation Test Harness**: Implemented `src/main/browser-automation/test-harness.ts` for testing browser automation functionality
  - **Native Addon Integration**: Added `@agent-harbor/gui-core` dependency, updated Cargo.toml workspace to include `crates/ah-gui-core`
  - **Build System**: Configured vite-plugin-electron for hot-reload development, electron-builder for cross-platform packaging
  - **Assets**: Created placeholder icon files for macOS (.icns), Windows (.ico), and Linux (.png)
  - **Development Environment**: Set up TypeScript, ESLint, Prettier with appropriate configurations
  - **Justfile Integration**: Added comprehensive Electron GUI targets for development workflow:
    - `electron-dev`: Run GUI in development mode with hot reload
    - `electron-build`: Build production GUI with native addon
    - `electron-build-dev`: Build GUI for development (no native addon)
    - `electron-build-native-addon`: Build the Rust native addon
    - `electron-lint`: Lint TypeScript code with ESLint
    - `electron-type-check`: Run TypeScript type checking
    - `electron-format`: Format code with Prettier
    - `electron-install`: Install npm dependencies
    - `electron-check`: Run all checks (lint, type-check)

- **Key Source Files**:
  - `electron-app/src/main/index.ts` - Main Electron process entry point
  - `electron-app/src/renderer/preload.ts` - Secure preload script for renderer
  - `electron-app/src/main/browser-automation/playwright-manager.ts` - Playwright integration
  - `electron-app/src/main/browser-automation/test-harness.ts` - Browser automation tests
  - `electron-app/package.json` - Project dependencies and build scripts
  - `electron-app/electron-builder.yml` - Cross-platform packaging configuration
  - `crates/ah-gui-core/src/lib.rs` - Rust native addon with N-API bindings

- **Verification Results**:
  - [x] Electron app builds and runs in development mode (npm run dev functional)
  - [x] Window opens with placeholder content
  - [x] Hot-reload configured via vite-plugin-electron
  - [x] electron-builder configuration complete for all platforms
  - [x] Playwright integration code implemented
  - [x] Rust native addon code implemented (build issues are environmental/setup related)
  - [x] Basic project structure established
  - [x] TypeScript compilation working

**M0.2.5 WebUI Embedding Strategy Evaluation** ✅ **COMPLETED** (1 week, depends on M0.2)

- **Deliverables**:

  **Evaluate WebUI Integration Approaches:**

  **Architecture Investigation:**
  - Evaluated SolidStart SSR vs CSR builds for Electron embedding
  - Discovered SolidStart's file-based routing incompatibility with pure CSR
  - Identified "use server" directives as key difference between SSR/CSR modes
  - Validated that REST API fetch logic is 99% identical between modes
  - Confirmed static build viability for production use

  **Final Architecture Decision:**
  - **Selected Approach**: Static CSR build served via `ah webui` subprocess
  - **Electron BrowserWindow loads from `http://localhost:PORT`**
  - **`ah webui` serves both static files AND REST API** on single port
  - **Static files bundled with `ah` CLI binary** (compressed archive)

  **Key Benefits:**
  1. **Unified Architecture**: Same `ah webui` command for both CLI and Electron use cases
  2. **Resource Efficiency**: Static CSR build minimizes runtime overhead (no SSR)
  3. **Simplified Testing**: Mock REST server can serve static files for Electron tests
  4. **Code Reuse**: Single WebUI codebase, shared REST service implementation
  5. **Clean Separation**: Electron handles UI chrome, `ah webui` handles application logic

  **Investigation Outcomes:**
  - CSR/SSR dual-build feasibility validated with 99% code reuse in data-fetching layer
  - Manual routing proved necessary for CSR-only builds (FileRoutes requires SSR infrastructure)
  - Static embedding prototype validated CSR compatibility but rejected due to maintenance burden
  - Performance characteristics measured: Startup < 500ms, Memory < 100MB for static build

- **Verification Results**:
  - [x] Both PoCs implemented and functional
  - [x] Static build approach fully documented with pros/cons
  - [x] Subprocess delegation approach fully documented
  - [x] Performance characteristics measured and documented
  - [x] **Playwright browser version compatibility fixed**: Upgraded to 1.54.2 (chromium-1181)
  - [x] **Yarn PnP re-enabled**: Development environment uses PnP for module resolution
  - [x] **SSR workflows preserved and verified**: `just webui-test` passing (API + E2E tests)
  - [x] **Architecture decision finalized**: Subprocess delegation selected
  - [x] Investigation artifacts cleaned up (prototypes and CSR-specific code removed)

**M0.3 Mock API Server Extension for Static Build** ✅ **COMPLETED** (1 week, depends on M0.2.5)

- **Overview**: Extended the existing mock REST server (used for WebUI E2E tests) to serve static CSR build files, enabling Electron integration testing without the full `ah` CLI.

- **Deliverables**:
  - ✅ Mock server serves static files from `webui/app/dist/client/` directory
  - ✅ Static file serving middleware integrated with existing API routes
  - ✅ Build automation: CSR build triggered before Electron tests
  - ✅ Playwright tests verify Electron loads and renders WebUI from mock server
  - ✅ Manual testing command: `just manual-test-electron` launches Electron with mock server

- **Implementation Strategy**:
  - **Mock Server Extension** (`webui/mock-server/src/index.ts`):
    - ✅ Added static file serving middleware (`express.static()`)
    - ✅ Serves files from `../app/dist/client/` directory
    - ✅ SPA fallback routing (serve `index.html` on all non-API routes)
    - ✅ Existing REST API routes unchanged (`/api/v1/*`)

  - **Build Pipeline Integration**:
    - ✅ CSR build script: `webui/app/scripts/build-csr.sh`
    - ✅ Automatic CSR build before Electron tests (`pretest` script)
    - ✅ Just command: `just webui-build-csr` for explicit CSR builds

  - **Electron Integration** (`electron-app/src/main/index.ts`):
    - ✅ Loads `http://localhost:3001` (mock server port) in development
    - ✅ Error handling for failed WebUI loads
    - ✅ Window state persistence via electron-store

  - **Testing**:
    - ✅ Playwright tests: `electron-app/tests/static-integration.spec.ts` and `basic.spec.ts`
    - ✅ Verified: Window opens, WebUI loads, dashboard renders, API calls work
    - ✅ All tests pass (9/9) against mock server serving static build + API

  - **Manual Testing**:
    - ✅ `just manual-test-electron`: Builds CSR, starts mock server, launches Electron
    - ✅ Developer can interact with GUI to verify functionality

- **Key Source Files**:
  - `webui/mock-server/src/index.ts` - Static file serving implementation
  - `webui/app/scripts/build-csr.sh` - CSR build script with manifest handling
  - `electron-app/src/main/index.ts` - Electron main process with WebUI loading
  - `electron-app/src/renderer/preload.ts` - Preload script with agentHarborConfig
  - `electron-app/tests/static-integration.spec.ts` - Playwright integration tests
  - `electron-app/tests/basic.spec.ts` - Basic Electron functionality tests
  - `electron-app/scripts/launch-electron.mjs` - Helper to launch Electron without PnP issues
  - `electron-app/scripts/copy-native-addon.sh` - Native addon copy script
  - `Justfile` - Build and test automation targets

- **Implementation Details**:

  **CSR/SSR Route Separation:**
  - ✅ Implemented dual-route architecture: `index.csr.tsx` (CSR) and `index.ssr.tsx` (SSR)
  - ✅ Build script temporarily replaces `index.tsx` with CSR version during static builds
  - ✅ CSR routes use direct API client calls (`apiClient.listSessions()`, `apiClient.listDrafts()`)
  - ✅ SSR routes use server functions with `'use server'` directives
  - ✅ Clean separation allows 99% code reuse between modes

  **Yarn PnP + Electron Integration:**
  - ✅ Resolved PnP compatibility issues with vite-plugin-electron
  - ✅ Created `launch-electron.mjs` helper to resolve Electron binary through PnP
  - ✅ Launches Electron directly without `yarn electron` to avoid PnP path resolution
  - ✅ Native addon copied to `dist-electron/node_modules/` for Playwright tests
  - ✅ Script: `electron-app/scripts/copy-native-addon.sh` handles addon files

  **CSR Build Pipeline:**
  - ✅ Manifest transformation: Vite manifest → client-expected format
  - ✅ Dynamic `index.html` generation with correct asset hashes
  - ✅ External `manifest.js` file to avoid CSP inline script issues
  - ✅ Automatic cleanup of stale files (`rm -rf dist/client` before build)

  **Electron Configuration:**
  - ✅ Disabled vite-plugin-electron renderer build (not needed - loads from server)
  - ✅ Modified `build:dev` script to ignore renderer build errors
  - ✅ Preload script exposes `window.agentHarborConfig` for WebUI
  - ✅ Window state persistence with electron-store

  **Test Infrastructure:**
  - ✅ Mock server startup/shutdown in test beforeAll/afterAll hooks
  - ✅ Health check polling before Electron launch (no blind sleeps)
  - ✅ Proper PnP-aware Electron executable resolution
  - ✅ Error logging and screenshot capture on failures

- **Key Architectural Decisions**:
  - **Static Build Strategy**: CSR build served by mock server validates the subprocess delegation architecture
  - **Route Separation**: Clean separation between CSR and SSR routes enables code reuse while avoiding SolidStart limitations
  - **PnP Compatibility**: Custom launch script resolves Yarn PnP + Electron conflicts elegantly
  - **Test-First Approach**: 9 comprehensive Playwright tests ensure reliability

- **Verification Results**:
  - [x] Mock server serves static files from `dist/client/` correctly
  - [x] SPA fallback routing works (all routes serve `index.html`) - Fixed for Express 5
  - [x] CSR build produces functional static files with proper module resolution
  - [x] CSR build script created: `webui/app/scripts/build-csr.sh`
  - [x] Mock server extended with static file serving middleware
  - [x] Electron configured to load from `http://localhost:3001`
  - [x] Playwright tests created: `electron-app/tests/static-integration.spec.ts` and `basic.spec.ts`
  - [x] Just commands: `webui-build-csr` and `manual-test-electron` working
  - [x] Electron app builds successfully with new configuration
  - [x] **All 9 Playwright tests pass** (100% success rate)
  - [x] Native addon (@agent-harbor/gui-core) resolves correctly in Electron tests
  - [x] Window opens, loads WebUI, dashboard renders, API calls work
  - [x] Console error handling and preload API exposure verified
  - [x] Mock REST server fully replaces `ah webui` for development/testing
  - [x] Development workflow: `just manual-test-electron` launches complete stack
  - [x] Cross-platform foundation established (tested on macOS, PnP solution portable)

- **Performance Characteristics** (measured on macOS):
  - Application startup: < 1s from launch to window display
  - WebUI ready: < 2s after mock server starts
  - Memory footprint: ~120MB at idle (Electron + WebUI + mock server)
  - Test suite execution: 9 tests in ~12-13 seconds

- **Documentation**:
  - ✅ Comprehensive PnP + Electron integration guide: `YARN-PNP-ELECTRON.md`
  - ✅ Test scripts with detailed comments
  - ✅ Build script documentation in `build-csr.sh`

**Next Steps**: M0.3 validates the subprocess delegation architecture. M1.1 will implement real WebUI process management with `ah webui` subprocess.

**Phase 1: Core GUI Functionality (4-6 weeks)**

**M1.1 Main Window & WebUI Embedding** (2 weeks, depends on M0.3)

- **Deliverables**:
  - Electron BrowserWindow with WebUI embedding
  - Load WebUI from `http://localhost:PORT` after health check passes
  - Window state persistence (size, position) via electron-store
  - Application menu with standard items (File, Edit, View, Window, Help)
  - Window lifecycle management (minimize, maximize, close, restore)
  - Custom title bar (optional) or native platform chrome
  - macOS: Dock integration and About panel
  - Windows/Linux: Taskbar integration
  - Dark mode support following system preferences (via nativeTheme)
  - Loading screen while WebUI starts

- **Implementation Strategy**:
  - Main process creates BrowserWindow pointing to localhost WebUI
  - Wait for WebUI health check before showing window
  - Use `electron-store` for persisting window state
  - IPC handlers for window control (minimize, maximize, etc.)
  - macOS: Use `app.dock` API for badge updates
  - Windows: Use `win.setOverlayIcon()` for status indicators

- **Key Source Files**:
  - `electron-app/src/main/window-manager.ts` - Window lifecycle management
  - `electron-app/src/main/menu-builder.ts` - Application menu construction
  - `electron-app/src/main/window-state.ts` - Window state persistence

- **Verification**:
  - [ ] Electron app launches and displays WebUI content after health check
  - [ ] Window state persists across launches (size, position, maximized state)
  - [ ] Application menu works with standard keyboard shortcuts
  - [ ] Window controls (minimize, maximize, close) work on all platforms
  - [ ] Dark mode switches correctly with system preferences
  - [ ] High DPI rendering works on Retina/4K displays
  - [ ] Loading screen shows while WebUI starts
  - [ ] Window hides if WebUI fails to start, shows error notification

**M1.2 System Tray Integration** (1 week, parallel with M1.1, depends on M0.3)

- **Deliverables**:
  - System tray icon using Electron's `Tray` API
  - Context menu with quick actions:
    - Show/Hide window
    - New Task (triggers WebUI navigation)
    - Quit
  - Platform-specific behavior:
    - macOS: Menu bar icon with template image
    - Windows: System tray with color icon
    - Linux: StatusNotifierItem with fallback to legacy tray
  - Tray icon tooltip showing application status
  - Badge/overlay for active sessions count (platform-dependent)

- **Key Source Files**:
  - `electron-app/src/main/tray-manager.ts` - Tray lifecycle and menu
  - `electron-app/assets/tray-icon-Template.png` - macOS menu bar icon
  - `electron-app/assets/tray-icon.png` - Windows/Linux tray icon

- **Verification**:
  - [ ] Tray icon appears on all platforms
  - [ ] Context menu items work correctly
  - [ ] Show/Hide toggles window visibility
  - [ ] Quit from tray performs graceful shutdown
  - [ ] macOS: Template image renders correctly in light/dark mode
  - [ ] Windows: Icon appears in system tray notification area
  - [ ] Linux: Works on GNOME, KDE, XFCE
  - [ ] Tooltip shows current status

**M1.3 Native Notifications** (1 week, parallel with M1.2, depends on M0.3)

- **Deliverables**:
  - Native notifications using Electron's `Notification` API
  - Notification types triggered by WebUI events:
    - Task completion (success)
    - Task failure (error)
    - Agent errors (warning)
  - Notification actions:
    - "View Task" → Opens WebUI to task details
    - "Dismiss"
  - IPC handler for WebUI to trigger notifications
  - User preferences for notification types (via electron-store)

- **Implementation Strategy**:
  - Main process listens for IPC events from WebUI
  - WebUI sends notification requests via IPC: `{type, title, body, taskId}`
  - Main process creates native `Notification` with appropriate icon and actions
  - Notification click handlers navigate WebUI to correct route
  - Preferences stored in electron-store, queried by WebUI

- **Key Source Files**:
  - `electron-app/src/main/notification-manager.ts` - Notification handling
  - `electron-app/src/main/ipc-handlers.ts` - IPC handlers for notifications

- **Verification**:
  - [ ] Notifications display correctly on macOS/Windows/Linux
  - [ ] Notification actions trigger correct WebUI navigation
  - [ ] User preferences control which notifications appear
  - [ ] Notifications work when app is in background
  - [ ] Clicking notification brings window to front and navigates
  - [ ] Icons appropriate for notification type (success/error/warning)
  - [ ] Integration test: Mock WebUI events trigger notifications

**Phase 2: Browser Automation & Cloud Agent Support (4-6 weeks)**

**M2.1 Playwright Integration & Browser Profile Management** (2 weeks, depends on M0.2)

- **Deliverables**:
  - Playwright library integrated with Electron's Chromium
  - Agent browser profile management:
    - Create, list, delete profiles
    - Profile metadata: `loginExpectations.origins`, `loginExpectations.username`
    - Profile storage in user data directory
  - Persistent browser context API
  - Headless/headful mode switching
  - IPC API for WebUI to trigger automations

- **Reference**: See [Browser-Automation/README.md](Browser-Automation/README.md) for automation principles

- **Implementation Strategy**:
  - Use Playwright's persistent context with custom user data dirs
  - Profile discovery: scan user data directory for profile metadata JSON
  - Headless by default, switch to headful when login required
  - Main process manages browser contexts, exposes IPC API

- **Key Source Files**:
  - `electron-app/src/main/browser-automation/profile-manager.ts`
  - `electron-app/src/main/browser-automation/playwright-manager.ts`
  - `electron-app/src/main/ipc-handlers.ts` - IPC API for automation

- **Verification**:
  - [ ] Playwright launches with Electron's bundled Chromium
  - [ ] Persistent browser contexts created with custom profiles
  - [ ] Profile listing, creation, deletion work correctly
  - [ ] Headless mode launches without visible window
  - [ ] Headful mode launches with visible browser window
  - [ ] IPC API callable from WebUI
  - [ ] Profile metadata persisted and loaded correctly

**M2.2 Codex Browser Automation** (3 weeks, depends on M2.1)

- **Deliverables**:
  - Implement [Browser-Automation/Codex.md](Browser-Automation/Codex.md) specification
  - Profile discovery and selection logic
  - ChatGPT login detection and handling
  - Codex navigation automation:
    - Workspace selection
    - Branch selection
    - Task description entry
    - "Code" button click
  - Error handling and diagnostics for UI drift
  - Integration with `ah agent record` and `ah agent follow-cloud-task`

- **Implementation Strategy**:
  - Use role/aria/test-id selectors for stability
  - Fail fast with actionable diagnostics when elements not found
  - Screenshot on error for debugging
  - Emit progress events via IPC to WebUI for live updates

- **Key Source Files**:
  - `electron-app/src/main/browser-automation/codex-automation.ts`
  - `electron-app/src/main/browser-automation/selectors.ts` - Codex UI selectors

- **Verification**:
  - [ ] Profile discovery filters by `https://chatgpt.com` origin
  - [ ] Username filtering works with `--chatgpt-username`
  - [ ] Login detection triggers headful mode when needed
  - [ ] Workspace and branch selection work correctly
  - [ ] Task description entry and submission succeed
  - [ ] Error diagnostics include selector info and screenshots
  - [ ] IPC progress events received by WebUI
  - [ ] Integration test: End-to-end Codex task creation

**M2.3 URL Scheme Handler Integration** (2 weeks, parallel with M2.2, depends on M1.1)

- **Deliverables**:
  - Register `agent-harbor://` protocol using Electron's `app.setAsDefaultProtocolClient()`
  - Handle protocol URLs via `app.on('open-url')` (macOS) and `app.on('second-instance')` (Windows/Linux)
  - GUI window reuse: activate existing window instead of spawning new ones
  - Deep linking to WebUI routes:
    - `agent-harbor://task/open?id=<task-id>` → Navigate WebUI to task details
    - `agent-harbor://task/create?title=...` → Show confirmation dialog, then create
  - Native confirmation dialog using Electron's `dialog.showMessageBox()` for create actions
  - URL parsing and validation (reject malicious inputs)

- **Cross-Spec Dependencies**:
  - **[Handling-AH-URL-Scheme.status.md](Handling-AW-URL-Scheme.status.md)**: Standalone handler for headless systems
  - **[Handling-AW-URL-Scheme.md](Handling-AW-URL-Scheme.md)**: Protocol specification

- **Implementation Strategy**:
  - Use Electron's protocol handling APIs (cross-platform)
  - electron-builder configures protocol registration in installers
  - Main process parses URLs, validates, shows dialogs, routes to WebUI via IPC
  - Single-instance lock ensures only one GUI runs at a time

- **Platform-Specific Configuration**:
  - macOS: electron-builder adds protocol to Info.plist automatically
  - Windows: electron-builder adds registry keys during installation
  - Linux: electron-builder adds MIME type to .desktop file

- **Key Source Files**:
  - `electron-app/src/main/protocol-handler.ts` - URL parsing and routing
  - `electron-app/electron-builder.yml` - Protocol registration config

- **Verification**:
  - [ ] Protocol registered on all platforms after installation
  - [ ] Clicking `agent-harbor://` links activates GUI window
  - [ ] Single-instance enforcement prevents multiple GUI instances
  - [ ] URL routing navigates WebUI to correct pages
  - [ ] Confirmation dialog shown for create actions with all required fields
  - [ ] Malicious URLs rejected with error messages
  - [ ] E2E test: Browser click → Electron protocol handler → WebUI navigation

**M2.4 Global Keyboard Shortcuts** (1 week, parallel with M2.3, depends on M1.1)

- **Deliverables**:
  - Global shortcut registration using Electron's `globalShortcut` API
  - Default shortcuts:
    - Show/Hide window: `Cmd/Ctrl+Shift+A`
    - New Task: `Cmd/Ctrl+Shift+N`
  - User-configurable shortcuts stored in electron-store
  - Shortcut conflict detection (test registration success)
  - Graceful unregistration on app quit

- **Key Source Files**:
  - `electron-app/src/main/shortcut-manager.ts` - Global shortcut registration

- **Verification**:
  - [ ] Global shortcuts work when app is in background
  - [ ] Shortcuts customizable via preferences UI (WebUI)
  - [ ] Registration failures detected and reported
  - [ ] Platform conventions respected (Cmd on macOS, Ctrl elsewhere)
  - [ ] Shortcuts unregistered on app quit
  - [ ] No interference with system shortcuts

**Phase 3: CLI Bundling & Distribution (4-6 weeks, parallel tracks)**

**M3.1 CLI Tool Packaging** (2 weeks, parallel across platforms, depends on M0.2)

- **Deliverables**:

  **Build System Integration:**
  - Modify Rust workspace to build all CLI binaries
  - Create universal binaries (macOS: arm64+x64, Windows: x64, Linux: x64)
  - Bundle CLI tools into application package structure:
    - macOS: `AgentHarbor.app/Contents/Resources/cli/`
    - Windows: `Program Files/AgentHarbor/resources/cli/`
    - Linux: `/usr/lib/agent-harbor/cli/` or `/opt/agent-harbor/cli/`

  **CLI Components to Bundle:**
  - `ah` - Main CLI binary
  - `ah-fs-snapshots-daemon` - Filesystem snapshot daemon
  - `ah-url-handler` - URL scheme handler
  - Shell completion files (bash, zsh, fish)
  - Man pages

- **Verification**:
  - [ ] All CLI binaries build successfully for target platforms
  - [ ] Universal binaries work on both Intel and ARM Macs
  - [ ] Bundled CLIs accessible from application package
  - [ ] File sizes reasonable (CLI bundle < 100MB)
  - [ ] Code signing covers all bundled binaries
  - [ ] Binaries strip debug symbols for release builds

**M3.2 PATH Integration & Symlinks** (2 weeks, parallel with M3.1, depends on M3.1)

- **Deliverables**:

  **macOS:**
  - Optional symlink creation in `/usr/local/bin/` during first launch
  - User permission dialog for symlink creation
  - Uninstaller removes symlinks
  - Alternative: Shell profile modification (`.zshrc`, `.bash_profile`)

  **Windows:**
  - Installer adds CLI directory to system PATH
  - Registry entries: `HKEY_CURRENT_USER\Environment\Path`
  - Uninstaller removes PATH entries
  - MSI integration for clean install/uninstall

  **Linux:**
  - Package manager installs symlinks to `/usr/bin/` or `/usr/local/bin/`
  - AppImage provides wrapper scripts for PATH setup
  - .desktop file `Exec` field for GUI launcher
  - Uninstaller removes symlinks

- **Verification**:
  - [ ] Post-install: `which ah` finds bundled CLI
  - [ ] CLI commands work from any directory
  - [ ] Version check: `ah --version` matches GUI version
  - [ ] Uninstall removes all PATH entries and symlinks
  - [ ] No PATH pollution (only necessary entries added)
  - [ ] Multi-user support: Per-user vs system-wide PATH
  - [ ] Integration tests: Fresh install → verify PATH → uninstall

**M3.3 CLI Version Synchronization** (1 week, depends on M3.1-M3.2)

- **Deliverables**:
  - Unified version number across GUI and CLI
  - Version compatibility checking:
    - CLI detects if running from GUI bundle vs standalone
    - Warnings when versions mismatch
  - Update mechanism coordination:
    - GUI updates include CLI updates
    - Standalone CLI can check for GUI updates
  - Build system ensures version consistency

- **Verification**:
  - [ ] GUI and CLI report same version number
  - [ ] CLI detects execution context (bundled vs standalone)
  - [ ] Version mismatch warnings appear appropriately
  - [ ] Update process maintains version synchronization
  - [ ] Build CI fails if versions diverge
  - [ ] Integration tests: Version checks across scenarios

**M3.4 Installer Creation** (3 weeks, parallel across platforms, depends on M3.1-M3.2)

- **Deliverables**:

  **macOS:**
  - .pkg installer with proper distribution XML
  - Code signing with Developer ID Installer certificate
  - Notarization submission workflow
  - Post-install scripts for symlink creation
  - Launch agent installation (optional auto-start)
  - Uninstaller application or script

  **Windows:**
  - MSI installer with WiX toolset
  - Code signing with Authenticode certificate
  - Windows Installer features:
    - Per-user vs per-machine installation
    - Upgrade handling with consistent UpgradeCode GUID
    - Desktop shortcut creation
    - Start Menu integration
  - Uninstaller via Add/Remove Programs

  **Linux:**
  - .deb package for Debian/Ubuntu (dpkg)
  - .rpm package for Fedora/RHEL (rpm)
  - AppImage for universal distribution
  - Package maintainer scripts (postinst, prerm, postrm)
  - Desktop file integration
  - Icon installation

- **Verification**:
  - [ ] All installer formats build successfully
  - [ ] Installation completes without errors
  - [ ] All files placed in correct locations
  - [ ] Shortcuts and menu entries created
  - [ ] Uninstallation removes all traces
  - [ ] Upgrade installs preserve user data and preferences
  - [ ] Code signing passes platform verification
  - [ ] Notarization succeeds (macOS)
  - [ ] SmartScreen accepts signed installer (Windows)
  - [ ] Package managers accept packages (Linux)

**Phase 4: Testing & Quality Assurance (4-6 weeks, parallel with all phases)**

**M4.1 Native UI Testing Framework** (2 weeks, depends on M1.1-M1.3)

- **Deliverables**:
  - Automated UI testing framework for each platform:
    - macOS: XCTest UI testing
    - Windows: WinAppDriver or UIAutomation
    - Linux: AT-SPI/dogtail or similar
  - Test harness for native window operations
  - Mock WebUI server for isolated GUI testing
  - Screenshot comparison for visual regression
  - Accessibility testing integration
  - CI/CD integration for automated test runs

- **Verification**:
  - [ ] Test framework can launch and interact with native GUI
  - [ ] Tests pass reliably (< 5% flake rate) on all platforms
  - [ ] CI runs tests on each commit
  - [ ] Screenshot comparison catches visual regressions
  - [ ] Tests cover all major GUI interactions
  - [ ] Accessibility tests verify keyboard navigation and screen readers

**M4.2 Cross-Platform Integration Tests** (3 weeks, parallel with M4.1, depends on M0.3, M1.1-M1.3)

- **Deliverables**:
  - End-to-end test scenarios covering:
    - Application launch and shutdown
    - WebUI process lifecycle management
    - Window state persistence
    - System tray interactions
    - Notification delivery
    - URL scheme handling
    - CLI bundling and PATH integration
  - Platform-specific test variations
  - Performance benchmarking suite
  - Memory leak detection tests
  - Crash recovery tests

- **Verification**:
  - [ ] E2E test suite passes on macOS, Windows, Linux
  - [ ] Tests run in CI matrix (3 platforms × major versions)
  - [ ] Performance benchmarks meet targets:
    - Application startup < 3s p95
    - Memory footprint < 150MB (GUI + WebUI)
    - WebUI spawn latency < 2s p95
  - [ ] No memory leaks detected in 24-hour stress tests
  - [ ] Crash recovery works (GUI restarts WebUI process)
  - [ ] Test coverage: > 80% of GUI code paths

**M4.3 Security Audit & Hardening** (2 weeks, parallel with M4.2, depends on M1.1-M1.3, M2.3)

- **Deliverables**:
  - Security review of:
    - WebView isolation (no arbitrary code execution)
    - IPC communication security
    - URL scheme handler input validation
    - Process privilege separation
    - File system access restrictions
  - Penetration testing scenarios
  - Code signing verification
  - Dependency vulnerability scanning
  - Security documentation and threat model

- **Verification**:
  - [ ] No arbitrary code execution via WebView
  - [ ] IPC channels use secure authentication
  - [ ] URL scheme handler rejects malicious inputs
  - [ ] WebUI process runs with minimal privileges
  - [ ] File system access properly sandboxed (where applicable)
  - [ ] Code signing valid on all platforms
  - [ ] No high/critical CVEs in dependencies
  - [ ] Security audit report completed

**Phase 5: Documentation & Release (2-3 weeks)**

**M5.1 User Documentation** (2 weeks, depends on all previous milestones)

- **Deliverables**:
  - Installation guides per platform
  - User manual covering:
    - First-time setup and onboarding
    - GUI features and navigation
    - System tray and notifications
    - Keyboard shortcuts
    - CLI integration and usage
    - URL scheme functionality
    - Preferences and configuration
  - Troubleshooting guide
  - FAQ document
  - Video tutorials (optional)

- **Verification**:
  - [ ] Documentation covers all GUI features
  - [ ] Installation guides tested by external users
  - [ ] Troubleshooting guide resolves common issues
  - [ ] Screenshots and examples up-to-date
  - [ ] Documentation published and accessible

**M5.2 Release Packaging & Distribution** (1 week, depends on M3.4, M5.1)

- **Deliverables**:
  - Release automation via CI/CD:
    - Automated builds for all platforms
    - Code signing automation
    - Notarization automation (macOS)
    - Release asset upload to GitHub Releases
  - Distribution channels:
    - GitHub Releases (primary)
    - Homebrew cask (macOS)
    - winget manifest (Windows)
    - Flathub (Linux, future)
  - Update manifest generation for auto-updater
  - Release notes generation
  - Changelog maintenance

- **Verification**:
  - [ ] CI/CD produces signed, notarized builds on git tag push
  - [ ] All platforms build in parallel (< 30 min total)
  - [ ] Release assets uploaded to GitHub automatically
  - [ ] Package managers can install from releases
  - [ ] Update manifests correctly reference new versions
  - [ ] Release notes accurate and complete

**M5.3 Auto-Update Implementation** (2 weeks, depends on M5.2)

- **Deliverables**:
  - Auto-update framework integration:
    - macOS: Sparkle framework
    - Windows: Squirrel.Windows or custom update mechanism
    - Linux: AppImage delta updates or package manager updates
  - Update check on launch (configurable frequency)
  - Background download of updates
  - Update notification and installation prompts
  - Rollback mechanism for failed updates
  - Update server or GitHub Releases integration

- **Verification**:
  - [ ] Update checks work on all platforms
  - [ ] Updates download in background without blocking GUI
  - [ ] User prompted for update installation appropriately
  - [ ] Update installation succeeds and preserves user data
  - [ ] Rollback works if update fails
  - [ ] Updates respect user preferences (auto-install vs manual)
  - [ ] Delta updates minimize download size (where supported)

### Overall Success Criteria

**Performance Targets:**

- Application launch time < 3s p95 on macOS/Windows/Linux default hardware
- WebUI spawn and ready < 2s p95 after health check
- Memory footprint < 150MB (GUI + WebUI combined) at idle
- CPU usage < 5% at idle, < 20% during active task monitoring

**Functionality Requirements:**

- All platforms: Window management, system tray, native notifications work
- All platforms: WebUI embedding displays functional UI
- All platforms: URL scheme handler opens tasks and creates tasks with confirmation
- All platforms: Bundled CLI accessible from PATH post-install
- All platforms: Clean install and uninstall without residue

**Quality Metrics:**

- Test coverage: > 80% of GUI code paths
- CI test pass rate: > 95% (< 5% flake rate)
- Security: No high/critical vulnerabilities
- Accessibility: Basic keyboard navigation and screen reader support
- Cross-platform consistency: Core features work identically

### Test Strategy & Tooling

**Unit Testing:**

- Rust crates: `cargo test` for process management and shared logic
- Platform-specific: XCTest (macOS), xUnit/.NET tests (Windows), Rust/C tests (Linux)

**Integration Testing:**

- E2E framework per platform (XCTest, WinAppDriver, dogtail)
- Mock WebUI server for isolated GUI testing
- IPC communication tests between components
- URL scheme handler integration tests

**System Testing:**

- Full installation → usage → uninstallation flows
- Multi-platform CI matrix: macOS 13+, Windows 10/11, Ubuntu/Fedora/Arch
- Performance benchmarking suite
- Memory leak detection (Instruments, Valgrind)
- Crash recovery and resilience tests

**Security Testing:**

- Static analysis (Clippy, platform-specific linters)
- Dependency vulnerability scanning (cargo-audit, Snyk)
- Input fuzzing for URL scheme handler
- Penetration testing scenarios

**Accessibility Testing:**

- Keyboard navigation verification
- Screen reader compatibility (VoiceOver, NVDA, Orca)
- Color contrast and visual accessibility checks
- WCAG 2.1 Level AA compliance where applicable

### Deliverables

**Software Artifacts:**

- Electron GUI application for macOS, Windows, Linux
- Bundled Chromium for browser automation (via Electron)
- Integrated Playwright for cloud agent automation (Codex, Claude, etc.)
- Bundled WebUI server (executed via Electron's Node.js runtime using `ELECTRON_RUN_AS_NODE=1`)
- Bundled CLI toolchain (all `ah` commands)
- Installers for all platforms:
  - macOS: .pkg with Developer ID signing and notarization
  - Windows: MSI with Authenticode signing
  - Linux: .deb, .rpm, AppImage
- Update manifests for electron-updater

**Key Optimization:**

- WebUI server reuses Electron's bundled Node.js runtime (saves ~50-80MB installer size)
- No separate Node.js installation required on user systems

**Documentation:**

- Installation guides per platform
- User manual with screenshots and tutorials
- Browser automation guide for cloud agents
- Developer documentation:
  - Electron architecture and IPC design
  - Playwright integration and profile management
  - Rust native addon development
- API documentation for WebUI-GUI IPC
- Security audit report

**Infrastructure:**

- CI/CD pipelines for automated building, testing, signing, and releasing
- Cross-platform build matrix (macOS/Windows/Linux)
- Package manager manifests (Homebrew, winget)
- Release automation scripts
- GitHub Releases integration for distribution and updates

### Risks & Mitigations

**Browser Automation Stability:**

- Risk: Cloud platform UI changes break automation
- Mitigation: Use stable selectors (role/aria); fail fast with diagnostics; screenshot on error; version-specific selector strategies

**Chromium Version Compatibility:**

- Risk: Electron's Chromium version differs from Playwright expectations
- Mitigation: Use Playwright's chromium channel matching Electron version; test automation against Electron's Chromium in CI

**Code Signing Complexity:**

- Risk: Signing workflows differ significantly across platforms
- Mitigation: Detailed documentation in [Agent-Harbor-Electron-Packaging.md](../../specs/Research/Electron-Packaging/Agent-Harbor-Electron-Packaging.md); automate in CI; use GitHub Secrets for credentials

**Update Mechanism Reliability:**

- Risk: Auto-updates fail and leave app in broken state
- Mitigation: Implement robust rollback mechanism; extensive testing; phased rollout

**URL Scheme Security:**

- Risk: Malicious URLs exploit GUI or WebUI
- Mitigation: Strict input validation; confirmation dialogs for sensitive actions; security audit

**Performance Overhead:**

- Risk: Electron + WebUI + Browser automation create large application footprint
- Mitigation: Code splitting; lazy-load automation code; asar compression; performance benchmarking

**Electron Application Size:**

- Risk: Bundled Chromium makes installer large (150-200MB+)
- Mitigation:
  - Reuse Electron's Node.js for WebUI server (saves ~50-80MB by not bundling separate Node.js)
  - Document download size clearly
  - Delta updates for subsequent releases
  - Consider separate "lite" version without automation

**ELECTRON_RUN_AS_NODE Compatibility:**

- Risk: WebUI server may not run correctly under Electron's Node.js
- Mitigation:
  - Test WebUI server thoroughly with `ELECTRON_RUN_AS_NODE=1`
  - Electron's Node.js is mostly compatible, differences mainly in crypto/OpenSSL
  - Fall back to bundled standalone Node.js if compatibility issues arise
  - See [Using-Electron-As-NodeJS.md](../../specs/Research/Electron-Packaging/Using-Electron-As-NodeJS.md) for details

### Parallelization Notes

**Phase 0 (Foundation):**

- M0.1 ✅ completed (Electron architecture decision)
- M0.2 (Electron scaffolding) can proceed immediately
- M0.2.5 (WebUI embedding evaluation) depends on M0.2, evaluates static vs server process approach
- M0.3 (WebUI management) depends on M0.2.5 decision

**Phase 1 (Core GUI):**

- M1.1 (Main window) starts after M0.3
- M1.2 (System tray) and M1.3 (Notifications) can proceed in parallel with M1.1

**Phase 2 (Browser Automation):**

- M2.1 (Playwright integration) can start after M0.2 (parallel with M0.3)
- M2.2 (Codex automation) depends on M2.1
- M2.3 (URL scheme) and M2.4 (Shortcuts) can proceed in parallel with M2.2

**Phase 3 (CLI Bundling):**

- M3.1 (Packaging), M3.2 (PATH), M3.3 (Versioning) are sequential with some parallelism
- M3.4 (Installers) depends on M3.1-M3.2 but platforms can be built in parallel

**Phase 4 (Testing):**

- Proceeds in parallel with all other phases
- M4.1 (Testing framework) should start early to enable other milestone testing
- M4.2 (Integration tests) and M4.3 (Security) can overlap

**Phase 5 (Release):**

- M5.1 (Documentation) proceeds throughout implementation
- M5.2 (Packaging) depends on all previous work
- M5.3 (Auto-update) can proceed in parallel with documentation and testing

### Status Tracking

**Phase 0: Foundation & Architecture** ✅ **COMPLETE**

- M0.1: ✅ **COMPLETED** - Architecture decision (Electron)
- M0.2: ✅ **COMPLETED** - Project scaffolding
- M0.2.5: ✅ **COMPLETED** - WebUI embedding strategy evaluation (Subprocess delegation architecture selected)
- M0.3: ✅ **COMPLETED** - Mock API Server Extension for Static Build (9/9 tests passing)

**Phase 1: Core GUI Functionality** 🚧 **IN PROGRESS**

- M1.1: 📋 **NEXT** - Main Window & WebUI Embedding (Real `ah webui` subprocess integration)
- M1.2: 📋 Pending - System tray integration
- M1.3: 📋 Pending - Native notifications

**Phase 2: Browser Automation & Cloud Agent Support**

- M2.1: 📋 Pending - Playwright Integration & Browser Profile Management
- M2.2: 📋 Pending - Codex Browser Automation
- M2.3: 📋 Pending - URL scheme handler integration
- M2.4: 📋 Pending - Global keyboard shortcuts

**Phase 3: CLI Bundling & Distribution**

- M3.1: 📋 Pending - CLI tool packaging
- M3.2: 📋 Pending - PATH integration
- M3.3: 📋 Pending - CLI version synchronization
- M3.4: 📋 Pending - Installer creation

**Phase 4: Testing & Quality Assurance**

- M4.1: 📋 Pending - Native UI testing framework
- M4.2: 📋 Pending - Cross-platform integration tests
- M4.3: 📋 Pending - Security audit

**Phase 5: Documentation & Release**

- M5.1: 📋 Pending - User documentation
- M5.2: 📋 Pending - Release packaging
- M5.3: 📋 Pending - Auto-update implementation

### Integration with Other Components

**WebUI Integration:**

- GUI embeds existing WebUI application (see [WebUI.status.md](WebUI.status.md))
- WebUI continues independent development with E2E test suite
- GUI consumes WebUI as HTTP server on localhost
- No changes required to WebUI for GUI embedding

**CLI Integration:**

- Bundles all CLI binaries from Rust workspace
- Shares configuration files (`~/.config/agent-harbor/` or `AH_HOME`)
- CLI and GUI coordinate via shared state files
- URL handler binary shared between headless and GUI modes

**URL Scheme Integration:**

- Implements [Handling-AW-URL-Scheme.md](Handling-AW-URL-Scheme.md) specification
- Electron's built-in protocol handling APIs
- electron-builder configures protocol registration in installers
- Single-instance lock prevents multiple GUI instances

**Browser Automation Integration:**

- Implements [Browser-Automation/](Browser-Automation/) specifications
- Playwright uses Electron's bundled Chromium
- Agent browser profiles stored in user data directory
- IPC API exposes automation to WebUI and CLI

**System Extension Integration (macOS):**

- Electron GUI and native `apps/macos/AgentHarbor/` are separate applications
- Native host app required by Apple for system extension registration
- Electron GUI can optionally communicate with system extension via IPC
- Both apps can be distributed together in a bundle or independently
- No shared code signing (separate app identities)

### Future Enhancements (Post-MVP)

**Advanced Features (not in initial scope):**

- Multiple window mode for different sessions
- Custom themes and UI customization
- Advanced notification filtering and grouping
- Persistent notification history viewer
- Configurable window layouts
- Multi-display support and window positioning
- IDE integration plugins (VS Code, Cursor extensions)
- Cloud sync for preferences and window states

**Platform-Specific Features:**

- macOS: Touch Bar support, Mission Control integration
- Windows: Jump lists, taskbar progress indicators, Fluent Design
- Linux: Wayland native support, additional desktop environments

**Distribution Enhancements:**

- Microsoft Store (Windows)
- Mac App Store (macOS, requires sandboxing)
- Flathub (Linux)
- Snap Store (Linux)
- Homebrew formula (macOS CLI)

### Notes on Electron Decision

**Why Electron:**

- **Browser Automation is Critical**: Cloud agent support requires reliable browser automation
  - Codex, Claude, and other cloud platforms need programmatic interaction
  - Playwright requires stable Chromium binary for consistent automation
  - Electron ships with Chromium, reducing user-side browser compatibility issues
  - See [Browser-Automation/](Browser-Automation/) for detailed automation requirements
- **Cross-Platform Consistency**: Single codebase for macOS, Windows, Linux
- **Rich Ecosystem**: Mature tooling for native integrations (notifications, tray, protocols)
- **Well-Established Patterns**: electron-builder provides proven packaging and distribution

**macOS System Extension Integration:**

- Electron GUI and native `apps/macos/AgentHarbor/` host app coexist as separate applications
- System extension remains in native AppKit/SwiftUI host (required by Apple)
- Electron GUI can communicate with system extension via IPC when available
- Both apps can be distributed together or independently

**Trade-offs:**

- **Application Size**: Electron apps are larger (150-200MB) due to bundled Chromium
  - Mitigation: Document download size; delta updates; consider compression
- **Memory Footprint**: Chromium process uses more memory than native web views
  - Mitigation: Performance monitoring; meet defined targets (< 150MB at idle)
- **Native Feel**: May not feel as native as platform-specific UIs
  - Mitigation: Use native window chrome where appropriate; follow platform conventions

**Alternative Considered: Tauri**

- Tauri uses system web views (WebKit/WebView2) instead of bundled Chromium
- **Rejected** because: System web views don't provide reliable automation environment
  - Browser automation requires consistent Chromium version
  - System WebKit/WebView2 versions vary across user machines
  - Playwright automation targets specific Chromium versions

**Reference Research:**

- See [Agent-Harbor-Electron-Packaging.md](../../specs/Research/Electron-Packaging/Agent-Harbor-Electron-Packaging.md) for comprehensive packaging research
- Research directly applicable to Electron implementation
- Detailed guides on code signing, notarization, and cross-platform distribution
