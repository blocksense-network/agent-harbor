# Configuration Keys Inventory

This document contains a comprehensive inventory of all configuration keys mentioned across Agent Harbor specifications, organized by subsystem/crate based on Repository-Layout.md.

## Inventory Methodology

- Systematically scanned all spec files in `specs/Public/` directory
- Extracted keys from Configuration.md, CLI.md command options, JSON schema definitions, and other relevant specs
- Mapped keys to subsystems based on Repository-Layout.md crate responsibilities
- Identified primary crate ownership for each configuration subsystem
- Included source file references for each configuration key

## Configuration Keys by Subsystem

### 1. Startup Configuration (ah-cli crate)

**Primary Crate**: `ah-cli` (early startup decisions)

Configuration keys:

- `ui` (string: "tui"|"webui") - Default UI to launch (consulted before UI initialization)
  - **Sources**: Configuration.md (lines 35, 67, 163), CLI.md (lines 65, 127, 372), Handling-AH-URL-Scheme.md (line 314)
- `remote-server` (string) - Remote server name/URL (determines local vs remote mode)
  - **Sources**: Configuration.md (lines 72, 123, 161), CLI.md (lines 67, 88, 372)

**Subsystem Config Type**: `StartupConfig`

### 2. UI/Interface Configuration (ah-tui crate)

**Primary Crate**: `ah-tui` (TUI-specific config)

Configuration keys:

- `terminal-multiplexer` (string: "tmux"|"zellij"|"screen") - Terminal multiplexer
  - **Sources**: Configuration.md (lines 197, 199), CLI.md (line 70)
- `editor` (string) - Default editor command
  - **Sources**: Configuration.md (line 199)
- `tui-font-style` (string: "nerdfont"|"unicode"|"ascii") - TUI symbol style
  - **Sources**: Configuration.md (line 73), TUI-PRD.md (line 100)
- `tui-font` (string) - TUI font name
  - **Sources**: Configuration.md (line 74)

**Subsystem Config Type**: `TuiConfig`

### 3. Repository/Project Configuration (ah-cli crate)

**Primary Crate**: `ah-cli` (repo initialization behavior)

Configuration keys:

- `repo.supported-agents` (string|"all"|array) - Supported agent types
  - **Sources**: Configuration.md (lines 212, 224)
- `repo.init.vcs` (string: "git"|"hg"|"bzr"|"fossil") - Version control system
  - **Sources**: Configuration.md (line 215)
- `repo.init.devenv` (string: "nix"|"spack"|"bazel"|"none"|"no"|"custom") - Development environment
  - **Sources**: Configuration.md (line 216)
- `repo.init.devcontainer` (boolean) - Enable devcontainer
  - **Sources**: Configuration.md (line 217)
- `repo.init.direnv` (boolean) - Enable direnv
  - **Sources**: Configuration.md (line 218)
- `repo.init.task-runner` (string: "just"|"make") - Task runner tool
  - **Sources**: Configuration.md (line 219)

**Subsystem Config Type**: `RepoInitConfig`

### 3. Browser Automation Configuration (ah-cli with browser automation)

**Primary Crate**: `ah-cli` (integrates browser automation features)

Configuration keys:

- `browser-automation` (boolean) - Enable/disable browser automation
  - **Sources**: Configuration.md (lines 68, 204), CLI.md (lines 115, 449, 1290, 1734, 1743)
- `browser-profile` (string) - Browser profile name
  - **Sources**: Configuration.md (lines 69, 205), CLI.md (line 116)
- `chatgpt-username` (string) - ChatGPT username for profile discovery
  - **Sources**: Configuration.md (lines 70, 206), CLI.md (line 117)
- `codex-workspace` (string) - Codex workspace identifier
  - **Sources**: Configuration.md (lines 71, 209), CLI.md (lines 118, 170)

**Subsystem Config Type**: `BrowserAutomationConfig`

### 4. Filesystem/Sandbox Configuration (agentfs-_, sandbox-_ crates)

**Primary Crates**: `agentfs-core`, `sandbox-core`

Configuration keys:

- `fs-snapshots` (string: "auto"|"zfs"|"btrfs"|"agentfs"|"git"|"disable") - Filesystem snapshot provider
  - **Sources**: Configuration.md (lines 172, 183), CLI.md (line 77)
- `working-copy` (string: "auto"|"cow-overlay"|"worktree"|"in-place") - Working copy strategy
  - **Sources**: Configuration.md (lines 173, 184), CLI.md (line 78)

**Subsystem Config Types**: `FsSnapshotsConfig`, `SandboxConfig`

### 5. Logging/Debug Configuration (ah-logging crate)

**Primary Crate**: `ah-logging`

Configuration keys:

- `log-level` (string: "debug"|"info"|"warn"|"error") - Logging verbosity level
  - **Sources**: Configuration.md (lines 195, 228), CLI.md (line 75)

**Subsystem Config Type**: `LoggingConfig`

## Fleet and Server Configuration

Additional configuration structures for multi-environment setups:

### Fleet Configuration

- `[[fleet]]` entries with name and member arrays
- Each member has `type` ("container"|"remote"), `profile` (sandbox profile name), `url` (remote server URL)

### Server Configuration

- `[[server]]` entries with name and url for remote server aliases

## Implementation Notes

1. **Configuration Loading**: The `ah-cli` crate is responsible for loading configuration from all layers (system, user, repo, repo-user, environment, CLI flags) and creating the unified `Config` struct.

2. **Subsystem Distribution**: Each subsystem receives its typed configuration object during initialization, ensuring type safety and encapsulation.

3. **Schema Validation**: All configuration is validated against the JSON schema in `specs/schemas/config.schema.json`.

4. **Environment Variables**: All keys support `AH_*` prefixed environment variables with underscores instead of dots/dashes.

5. **CLI Flags**: Most keys support `--flag-name` equivalents for command-line override.

## Next Steps

This inventory should be used to:

1. Update the JSON schema if any keys are missing
2. Create the subsystem-specific configuration types in `ah-config-types`
3. Implement configuration loading and distribution in `ah-cli`
4. Add validation and error handling for each subsystem's config
