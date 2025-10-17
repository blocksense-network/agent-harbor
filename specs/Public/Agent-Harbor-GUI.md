# Agent Harbor GUI Specification

## Overview

The Agent Harbor GUI (AH GUI) is a cross-platform Electron application that provides a native desktop wrapper around the `ah webui` process. It adds desktop-specific features like system tray integration, custom URL scheme handling, and native notifications while delegating all workflow functionality to the underlying WebUI.

## Core Responsibilities

### WebUI Process Management

- Launches and monitors the `ah webui` process
- Handles process lifecycle (start, restart, shutdown)
- Manages port conflicts and service discovery
- Provides graceful error handling for WebUI failures

### Window Management

- **Single Window Mode**: Embeds WebUI in a single Electron window with native window controls
- **Multiple Window Mode**: Opens separate windows for different tasks/sessions
- Window state persistence and restoration

### Custom URL Scheme Handler

- Registers `agent-harbor://` protocol on installation and acts as the preferred handler when the GUI is present. Headless systems use the standalone AH URL Handler binary.
- Routes incoming URLs to appropriate WebUI pages or delegates to an existing GUI window via IPC to reuse the window/tab instead of spawning a new one.
- Handles URL scheme conflicts and fallbacks.

### Native OS Integration

- System tray presence with quick actions
- Native OS notifications for task completion
- Global keyboard shortcuts for common operations
- Platform-specific protocol association (registry, launch services, MIME types)

## CLI Tool Integration

### Bundled CLI Tools

The GUI application bundles the complete AH CLI toolchain, making all `ah` commands available without separate installation.

### Packaging Strategy

#### Distribution Methods

- **Standalone Installers**: Platform-specific installers that bundle both GUI and CLI
- **Package Manager Integration**: System packages that install both components
- **Portable Bundles**: Self-contained applications with embedded CLI tools

#### CLI Tool Availability

The bundled CLI tools are made available through multiple mechanisms:

1. **PATH Integration**
   - Installers add the GUI's bundled CLI directory to system PATH
   - Portable versions provide wrapper scripts that set up PATH temporarily
   - Package managers create symlinks in standard bin directories

2. **Embedded Execution**

   ```bash
   # GUI provides wrapper scripts that execute bundled CLI
   # Example: /Applications/AgentHarbor.app/Contents/Resources/cli/ah
   # This ensures CLI always uses the same version as the GUI
   ```

3. **Version Synchronization**
   - GUI and CLI versions are kept in sync through unified releases
   - CLI tools detect when run from GUI bundle vs standalone installation
   - Automatic version compatibility checking

#### Cross-Platform PATH Setup

**Windows:**

- Installer adds `%PROGRAMFILES%\AgentHarbor\resources\cli` to system PATH
- Registry entries for command completion
- MSI integration for Add/Remove Programs

**macOS:**

- App bundle contains CLI in `Contents/Resources/cli/`
- Optional symlink creation in `/usr/local/bin/` (with user permission)
- Launch Services integration for command discovery

**Linux:**

- Package installs CLI tools to `/usr/bin/` or `/usr/local/bin/`
- .desktop file integration for GUI launcher
- MIME type associations for URL scheme

### Command-Line Interface

When CLI tools are invoked from the command line, they:

1. **Detect GUI Context**: Check if running from GUI bundle vs standalone
2. **Configuration Sharing**: Use same configuration files as GUI
3. **Process Coordination**: Communicate with GUI process when available
4. **Unified State**: Share session state and task data with GUI

### Development and Testing

#### Development Workflow

- GUI and CLI developed as separate but coordinated components
- Shared build system ensures version alignment
- Integration tests verify CLI-GUI interoperability

#### Standalone vs Bundled Modes

- **Standalone CLI**: Full functionality without GUI dependency
- **Bundled CLI**: Optimized for GUI integration with enhanced features
- **Detection Logic**: Automatic mode detection based on installation context

## Configuration Integration

The GUI integrates with the layered configuration system defined in [Configuration.md](Configuration.md). GUI-specific settings are stored in GUI configuration files and accessed via `ah config` commands.

## Platform-Specific Implementation

### Windows

- MSI installer with CLI PATH integration
- Registry-based URL scheme registration
- Taskbar jump lists and badges
- Windows notification integration

### macOS

- .app bundle with embedded CLI tools
- Launch Services URL scheme registration
- Dock integration and badges
- macOS notification center

### Linux

- Native package formats (.deb, .rpm, AppImage)
- .desktop file integration
- System tray support
- FreeDesktop notification specification

## Security Considerations

- Renderer process sandboxing
- Secure IPC between main and renderer processes
- File access restrictions
- Certificate validation for WebUI communication

## Installation and Updates

### Installation Options

- **GUI-Only**: Install GUI with bundled CLI (recommended for most users)
- **CLI-Only**: Standalone CLI tools without GUI
- **Full Suite**: Both GUI and standalone CLI for advanced users

### Update Mechanism

- GUI handles its own updates via Electron's auto-updater
- Bundled CLI updated as part of GUI releases
- Standalone CLI updated separately via package managers

## Error Handling

### Process Management

- Automatic WebUI process restart on failure
- Clear error messages for missing CLI dependencies
- Fallback modes when WebUI is unreachable

### User Guidance

- Installation troubleshooting guides
- Configuration validation with helpful error messages
- Links to documentation for common issues

## Integration with Existing Specs

This specification focuses on GUI-specific concerns and delegates to:

- **[WebUI PRD.md](WebUI-PRD.md)**: All WebUI functionality and user interface details
- **[Configuration.md](../Initial-Developer-Input/Configuration.md)**: Configuration system and file formats
- **[CLI.md](CLI.md)**: CLI command specifications and behavior
  - **[Handling-AW-URL-Scheme.md](Handling-AW-URL-Scheme.md)**: URL scheme desired behavior. See the `.status.md` sibling for milestones and tests.
