### Overview

Goal: Implement a comprehensive, strongly-typed configuration system for the agent-harbor project that provides layered configuration support, schema validation, provenance tracking, and enforcement capabilities. The system will support TOML configuration files across multiple scopes (system, user, repo, repo-user) with environment variables and CLI flags, while maintaining backward compatibility and providing clear error messages.

Total estimated timeline: 4-6 weeks (distributed across config-core crate, ah-config-types crate, and CLI integration)

**Timeline Breakdown**:

- **Foundation Layer**: Weeks 1-2 (core config engine and schema generation)
- **Core Functionality Layer**: Weeks 3-4 (validation, merging, enforcement, extraction)
- **Integration Layer**: Weeks 5-6 (CLI integration and testing)

### Milestone Completion & Outstanding Tasks

Each milestone maintains an **outstanding tasks list** that tracks specific deliverables, bugs, and improvements. When milestones are completed, their sections are expanded with:

- Implementation details and architectural decisions
- References to key source files for diving into the implementation
- Test coverage reports and known limitations
- Integration points with other milestones/tracks

### Configuration System Feature Set

The configuration system focuses on these core capabilities:

- **Schema-Driven Validation**: Single strongly-typed definition generates JSON Schema for runtime validation
- **Layered Configuration**: System, user, repo, and repo-user scopes with environment variables and CLI flags
- **Provenance Tracking**: Complete origin tracking for debug-level logging and enforced setting explanation
- **Enforcement Support**: Enterprise deployments can enforce specific keys at system scope
- **Distributed Types**: Modules own their typed views instead of monolithic config structs
- **TOML-First Design**: Configuration operates on JSON data model for generic field-agnostic operations

### Parallel Development After Bootstrapping

Once the config-core crate skeleton and schema generation are complete, multiple development tracks can proceed in parallel:

- **Schema Generation Track**: Build-time schema diff and validation infrastructure
- **Layer Processing Track**: TOML loading, environment overlays, and CLI flags
- **Merge/Enforcement Track**: JSON-level merging, provenance tracking, and enforcement masking
- **CLI Integration Track**: ah-cli integration and config command implementation
- **Testing Track**: Comprehensive tests covering all functionality

### Approach

- **Single Strong Type**: SchemaRoot defines the canonical shape for schema generation and validation
- **Generic JSON Operations**: All merging, enforcement, and provenance operate on serde_json::Value for field-agnostic processing
- **Distributed Ownership**: Modules extract their typed views from final JSON using path-based extraction
- **Build-Time Validation**: Schema generation compares against spec at compile time to catch drift
- **Comprehensive Testing**: Integration tests validate end-to-end workflows with schema validation, merging, and enforcement

### Development Phases (with Parallel Tracks)

**Phase 0: Infrastructure Bootstrap**

**0.1 Config Crates Skeleton** COMPLETED

- Deliverables:
  - Create `config-core` crate with basic structure and dependencies
  - Create `ah-config-types` crate with distributed config type definitions
  - Set up basic Cargo.toml workspace configuration for config crates
  - Add essential dependencies: schemars, serde_json, serde, toml, config-rs, jsonschema, etc.

- Implementation Details:
  - Created `crates/config-core/` with lib.rs skeleton and basic module structure
  - Created `crates/ah-config-types/` with distributed type modules (ui.rs, repo.rs, server.rs)
  - Added workspace dependencies for schema generation and JSON processing
  - Configured schemars for JSON Schema generation from Rust types

- Key Source Files:
  - `crates/config-core/Cargo.toml` - Config core crate configuration
  - `crates/ah-config-types/Cargo.toml` - Distributed types crate configuration
  - `crates/config-core/src/lib.rs` - Core library structure
  - `crates/ah-config-types/src/lib.rs` - Distributed types library

- Verification Results:
  - [x] Cargo workspace compiles with new config crates
  - [x] Basic crate structure matches Repository-Layout.md
  - [x] Essential dependencies (schemars, serde_json, toml, etc.) properly configured
  - [x] Distributed type modules created for each subsystem

**Phase 1: Schema Generation & Build-Time Validation**

**1.1 SchemaRoot Definition & Schema Generation** COMPLETED

- Deliverables:
  - Define `SchemaRoot` struct in `config-core` with comprehensive config shape
  - Implement schemars JsonSchema derive for automatic schema generation
  - Create build.rs that generates schema and compares against spec
  - Set up expected schema file in `specs/config.schema.expected.json`

- Implementation Details:
  - SchemaRoot includes all top-level config sections (ui, repo, server, fleet, sandbox)
  - References distributed types from `ah-config-types` crate
  - Build script uses schemars::schema_for! to generate JSON Schema
  - Canonicalizes both generated and expected schemas for comparison

- Verification:
  - [x] SchemaRoot compiles and generates valid JSON Schema
  - [x] Build fails when schema differs from expected
  - [x] Schema includes all config sections from spec
  - [x] Generated schema matches Draft 2020-12 specification

**1.2 TOML to JSON Conversion & Validation**

- Deliverables:
  - Implement TOML parsing to JSON conversion functions
  - Add runtime JSON Schema validation for each config layer
  - Create layer loading infrastructure with error handling
  - Validate TOML type safety through JSON schema validation

- Implementation Details:
  - Round-trip TOML -> serde_json::Value for schema validation
  - Uses jsonschema crate for Draft 2020-12 validation
  - Comprehensive error messages with JSON Schema diagnostics
  - Layer validation happens before any merging operations

- Verification:
  - [x] TOML parsing preserves all data types correctly
  - [x] Invalid TOML produces clear schema validation errors
  - [x] Valid TOML layers pass schema validation
  - [x] Error messages include specific field and constraint information

**Phase 2: Layer Processing & Merging** COMPLETED

**2.1 Environment & CLI Flag Overlays** COMPLETED

- Deliverables:
  - Environment variable overlay with `AH_*` prefix and kebab-case conversion
  - CLI flag dynamic overlay with dotted path support (--set key=value)
  - Flag validation against schema before merging
  - Integration with config-rs for environment processing

- Implementation Details:
  - config-rs handles `AH_*` environment variables with separator and case conversion
  - Dotted path insertion for CLI flags (repo.task-runner=just)
  - JSON overlays validated against schema before use
  - Generic field-agnostic overlay construction

- Verification:
  - [x] Environment variables converted to correct JSON structure
  - [x] CLI flags inserted at correct dotted paths
  - [x] Invalid overlays rejected with schema validation
  - [x] Overlay precedence works correctly in merge order

**2.2 JSON-Level Merging & Provenance** COMPLETED

- Deliverables:
  - Generic JSON merging with object deep-merge and array replacement
  - Provenance tracking for each key's winning scope and change history
  - Precedence order: system < user < repo < repo-user < env < flags
  - Change history recording for explain functionality

- Implementation Details:
  - Recursive merge_two_json function for serde_json::Value
  - Provenance tracking with Scope enum and change history
  - Associativity and precedence property testing
  - Winner tracking for each dotted key path

- Verification:
  - [x] Deep object merging works correctly
  - [x] Array replacement policy implemented
  - [x] Precedence order respected in merge results
  - [x] Provenance tracking captures complete history

**2.3 Enforcement Masking** COMPLETED

- Deliverables:
  - Enforcement key extraction from system layer
  - Layer masking for non-system scopes when keys are enforced
  - Enforcement provenance marking for explain functionality
  - Write-path rejection for enforced keys in non-system layers

- Implementation Details:
  - System layer can define "enforced" array of dotted key paths
  - mask_layer function removes enforced keys from lower scopes
  - Provenance tracks enforced status for CLI explain commands
  - CLI commands reject writes to enforced keys

- Verification:
  - [x] Enforced keys masked from lower scope layers
  - [x] System layer can still modify enforced keys
  - [x] Provenance correctly marks enforced keys
  - [x] CLI rejects writes to enforced keys with clear errors

**Phase 3: Typed Extraction & CLI Integration** COMPLETED

**3.1 Distributed Typed Extraction** COMPLETED

- Deliverables:
  - Generic extraction utilities for root and path-based access
  - Typed extraction functions for distributed config modules
  - Error handling with path context for extraction failures
  - Round-trip fidelity validation between JSON and typed values

- Implementation Details:
  - get::<T>() for whole-root extraction
  - get_at::<T>(path) for subsection extraction
  - serde_path_to_error for detailed error reporting
  - Extraction preserves missing field semantics (None vs missing)

- Verification:
  - [x] Root extraction deserializes complete config correctly
  - [x] Path extraction works for all config subsections
  - [x] Missing optional fields become None correctly
  - [x] Type errors provide clear path and type information

**3.2 AH CLI Config Commands** COMPLETED

- Deliverables:
  - `ah config` subcommand with Git-like interface
  - `ah config --show-origin` with provenance display
  - `ah config <key> --explain` with change history
  - `ah config --set key=value` with enforcement checking
  - Dynamic key completion and validation

- Implementation Details:
  - Clap subcommand structure for config operations
  - Integration with config-core loading and provenance
  - Enforcement checking for write operations
  - Human-readable output formats for provenance

- Verification:
  - [x] Config commands show current values correctly
  - [x] --show-origin displays winning scope for each key
  - [x] --explain shows change history and enforcement status
  - [x] --set validates and rejects enforced keys
  - [x] Dynamic key paths work for all config sections

**Phase 4: Comprehensive Testing**

**4.1 Build-Time Schema Tests** COMPLETED

- Deliverables:
  - Build.rs schema comparison testing
  - Schema drift detection with helpful error messages
  - Expected schema file maintenance workflow
  - Schema validity testing against JSON Schema spec

- Implementation Details:
  - Build script compares generated vs expected schemas
  - Canonical JSON comparison to avoid formatting differences
  - Clear error messages directing to generated file location
  - Expected schema file updated when changes are approved

- Verification:
  - [x] Build fails on schema drift with clear instructions
  - [x] Expected schema file matches spec requirements
  - [x] Schema generation is deterministic and stable
  - [x] JSON Schema validates against meta-schema

**4.2 Runtime Validation Tests** COMPLETED

- Deliverables:
  - Per-layer TOML validation testing
  - Invalid config rejection with helpful errors
  - Environment and flag overlay validation
  - Schema validation error message quality testing

- Implementation Details:
  - Unit tests for TOML parsing and validation
  - Golden file tests for error messages
  - Edge case testing for malformed inputs
  - Performance testing for large config files

- Verification:
  - [x] All invalid TOML configurations rejected
  - [x] Valid configurations pass validation
  - [x] Error messages are clear and actionable
  - [x] Performance acceptable for large configs

**4.3 Merge & Provenance Integration Tests** COMPLETED

- Deliverables:
  - End-to-end merge testing across all layers
  - Provenance tracking validation
  - Enforcement integration testing
  - Property-based testing for merge associativity

- Implementation Details:
  - Integration tests with temporary config files
  - Environment isolation with custom AH_HOME
  - Property testing with proptest for merge properties
  - Golden file testing for expected merge results

- Verification:
  - [x] Layer precedence works correctly in all combinations
  - [x] Provenance tracking captures complete history
  - [x] Enforcement masking works across all scenarios
  - [x] Property tests pass for merge associativity

**4.4 CLI Integration Tests** COMPLETED

- Deliverables:
  - End-to-end CLI testing with config commands
  - Golden file testing for CLI output
  - Error case testing for CLI operations
  - Integration with real filesystem config files

- Implementation Details:
  - CLI integration tests with temporary directories
  - Snapshot testing with cargo-insta for output
  - Error message golden files
  - Real config file loading and validation

- Verification:
  - [x] CLI commands produce correct output
  - [x] Config file operations work end-to-end
  - [x] Error cases handled gracefully
  - [x] CLI help and completion work correctly

**4.5 Cross-Platform Compatibility Tests** COMPLETED

- Deliverables:
  - Platform-specific config path testing
  - Environment variable handling across platforms
  - File permission and encoding testing
  - Unicode and special character handling

- Implementation Details:
  - Platform-specific test configurations
  - Environment isolation testing
  - File encoding edge case testing
  - Path resolution testing for different platforms

- Verification:
  - [x] Config paths resolve correctly on all platforms
  - [x] Environment handling works across shells
  - [x] File encoding issues handled properly
  - [x] Special characters in paths/configs work

## New/Refined Milestones to Reach Full Configuration System

### Implementation Summary

- **Schema-First Design**: Single SchemaRoot drives both schema generation and runtime validation
- **Generic JSON Operations**: Field-agnostic merging, enforcement, and provenance on serde_json::Value
- **Distributed Types**: Modules own their strongly-typed views extracted from final JSON
- **Comprehensive Testing**: Integration tests validate complete workflows from TOML loading to typed extraction
- **Build-Time Safety**: Schema comparison catches spec drift at compile time

### Key Dependency Insights

- **Schema Generation** must complete before runtime validation can be implemented
- **Layer Processing** can proceed in parallel with merging infrastructure
- **CLI Integration** depends on complete config-core functionality
- **Testing** should be developed alongside each component for comprehensive coverage
- **Enforcement** depends on merge infrastructure being complete

### Risks & Mitigations

- **Schema Drift**: Mitigated by build-time comparison with clear error messages directing developers to update expected schema
- **JSON Schema Complexity**: Mitigated by using schemars for automatic generation and jsonschema for validation
- **Generic JSON Operations**: Mitigated by comprehensive testing of merge properties and provenance tracking
- **Distributed Types**: Mitigated by path-based extraction with clear error messages for type mismatches
- **TOML Round-trip**: Mitigated by validation that ensures TOML -> JSON preserves all information and types

## Subsystem Configuration Type Implementation

Now that the core configuration system is complete, we need to implement the distributed configuration types for each subsystem. Each milestone focuses on one subsystem's configuration, ensuring proper ownership and type safety.

### Milestone: Setup Configuration Loading in ah-cli

**Deliverables:**

- Integrate `config-core` and `ah-config-types` into `ah-cli` crate
- Implement configuration loading at application startup
- Create the root `Config` struct that composes all subsystem configs
- Add Serde attributes for proper flattening and field renaming
- Implement configuration distribution to subsystems during initialization
- Add error handling for configuration loading failures
- Create integration tests for end-to-end config loading

**Implementation Details:**

- The `ah-cli` crate becomes the central point for configuration loading
- Configuration is loaded once at startup and distributed as typed objects to subsystems
- Uses the distributed composition pattern with `#[serde(flatten)]` for subsystem configs
- Handles CLI flag overrides and environment variable processing
- Provides clear error messages for configuration issues

**Key Source Files:**

- `crates/ah-cli/src/config.rs` - Root Config struct and loading logic
- `crates/ah-cli/src/main.rs` - Configuration loading integration
- `crates/ah-config-types/src/lib.rs` - Root config composition

**Verification:**

- [x] Configuration loads correctly from all layers (system, user, repo, repo-user, env, CLI)
- [x] Subsystem configs are properly extracted and distributed
- [x] CLI flags override configuration correctly
- [x] Environment variables are processed correctly (inherited from config-core)
- [x] Error messages are clear for configuration failures
- [x] Integration tests pass for full config loading workflow

### Milestone: Startup Configuration (ah-cli crate)

**Subsystem**: Early application startup decisions
**Primary Crate**: `ah-cli`

**Deliverables:**

- Define `StartupConfig` struct for early application decisions
- Implement configuration for UI selection and remote server mode
- Integrate startup config into root Config struct with flattening
- Add accessor methods for startup configuration

**Configuration Keys:**

- `ui` - Default UI selection (tui/webui)
- `remote-server` - Remote server name/URL (determines local vs remote mode)

**Sources**: Configuration.md, CLI.md, Handling-AH-URL-Scheme.md

**Key Source Files:**

- `crates/ah-cli/src/config.rs` - Startup configuration type and root Config struct

**Implementation Details:**

- StartupConfig struct implemented with UI selection and remote server settings
- Serde serialization with proper field renaming
- Integration with root Config struct via flattening
- Accessor methods provided for subsystem extraction

**Verification:**

- [x] StartupConfig struct defined with required configuration options
- [x] Configuration integrated into root Config struct
- [x] Serde serialization with proper field naming
- [ ] Application selects correct UI based on startup config (pending UI selection logic)
- [ ] UI decision is made before UI subsystem initialization (pending application startup logic)
- [ ] Configuration validation rejects invalid UI values (pending validation implementation)

### Milestone: UI/Interface Configuration (ah-tui crate)

**Subsystem**: TUI-specific interface configuration
**Primary Crate**: `ah-tui`

**Deliverables:**

- Define `TuiConfig` struct in `ah-tui/src/tui_config.rs`
- Implement configuration for TUI settings, terminal multiplexer, editor
- Add validation for TUI configuration values
- Create unit tests for TUI config parsing
- Update root Config struct with TUI subsystem composition

**Configuration Keys:**

- `terminal-multiplexer` - Terminal multiplexer choice
- `editor` - Default editor command
- `tui-font-style` - TUI symbol style
- `tui-font` - TUI font name

**Sources**: Configuration.md, CLI.md, TUI-PRD.md

**Key Source Files:**

- `crates/ah-tui/src/tui_config.rs` - TUI configuration type
- `crates/ah-config-types/src/ui.rs` - UI config definitions

**Implementation Details:**

- TuiConfig struct implemented with comprehensive TUI configuration options from TUI-PRD.md
- Includes keyboard keymap configuration with all TUI operations
- Serde serialization with kebab-case naming convention
- Integration with root Config struct via flattening

**Verification:**

- [x] TuiConfig struct defined with comprehensive TUI settings
- [x] Keyboard keymap configuration implemented
- [x] Configuration integrated into root Config struct
- [x] Serde serialization with kebab-case naming
- [ ] Terminal multiplexer selection works (pending TUI integration)
- [ ] Editor configuration is respected (pending TUI integration)
- [ ] TUI font settings are applied correctly (pending TUI integration)
- [ ] Configuration validation rejects invalid values (pending validation implementation)

### Milestone: Repository Configuration (ah-cli crate)

**Subsystem**: Repository initialization behavior
**Primary Crate**: `ah-cli`

**Configuration Keys:**

- `repo.supported-agents` - Supported agent types
- `repo.init.vcs` - Version control system
- `repo.init.devenv` - Development environment
- `repo.init.devcontainer` - Devcontainer enablement
- `repo.init.direnv` - Direnv enablement
- `repo.init.task-runner` - Task runner tool

**Sources**: Configuration.md

**Key Source Files:**

- `crates/ah-cli/src/config.rs` - Repository configuration type and root Config struct

**Implementation Details:**

- RepoConfig and RepoInitConfig structs implemented with repository initialization settings
- Serde serialization with kebab-case naming convention
- Integration with root Config struct as optional field

**Verification:**

- [x] RepoConfig struct defined with required configuration options
- [x] Configuration integrated into root Config struct
- [x] Serde serialization with proper field naming
- [ ] VCS detection uses configured default VCS (pending repository initialization logic)
- [ ] Development environment setup respects config (pending repository initialization logic)
- [ ] Task runner commands use configured tool (pending repository initialization logic)
- [ ] Repository initialization applies config defaults (pending repository initialization logic)
- [ ] Configuration validation rejects invalid VCS/devenv values (pending validation implementation)

### Milestone: Browser Automation Configuration

**Subsystem**: Browser automation for cloud agents
**Primary Crate**: `ah-cli`

**Configuration Keys:**

- `browser-automation` - Enable/disable browser automation
- `browser-profile` - Browser profile name
- `chatgpt-username` - ChatGPT username
- `codex-workspace` - Codex workspace identifier

**Sources**: Configuration.md, CLI.md

**Key Source Files:**

- `crates/ah-cli/src/config.rs` - Browser automation config type and root Config struct

**Implementation Details:**

- BrowserAutomationConfig struct implemented with browser automation settings
- Serde serialization with proper field renaming
- Integration with root Config struct via flattening

**Verification:**

- [x] BrowserAutomationConfig struct defined with required configuration options
- [x] Configuration integrated into root Config struct
- [x] Serde serialization with proper field naming
- [ ] Browser automation enables/disables based on config (pending browser automation integration)
- [ ] Browser profile selection works correctly (pending browser automation integration)
- [ ] ChatGPT username is used for profile discovery (pending browser automation integration)
- [ ] Codex workspace configuration is applied (pending browser automation integration)

### Milestone: Filesystem/Sandbox Configuration

**Subsystem**: Filesystem snapshots and sandboxing
**Primary Crate**: `ah-fs-snapshots`

**Deliverables:**

- Define `FsSnapshotsConfig` struct for filesystem snapshot settings
- Define `SandboxConfig` struct for sandbox profile defaults
- Implement configuration for snapshot providers and working copy strategies
- Add validation for filesystem configuration values
- Create unit tests for FS/sandbox config parsing
- Update root Config struct with FS/sandbox composition

**Configuration Keys:**

- `fs-snapshots` - Filesystem snapshot provider
- `working-copy` - Working copy strategy

**Sources**: Configuration.md, CLI.md

**Key Source Files:**

- `crates/ah-fs-snapshots/src/fs_snapshots_config.rs` - Filesystem snapshots config type

**Implementation Details:**

- FsSnapshotsConfig struct implemented with filesystem snapshot provider and working copy strategy settings
- Serde serialization with proper field renaming
- Integration with root Config struct via flattening

**Verification:**

- [x] FsSnapshotsConfig struct defined with required configuration options
- [x] Configuration integrated into root Config struct
- [x] Serde serialization with proper field naming
- [ ] Filesystem snapshot provider selection works (pending filesystem integration)
- [ ] Working copy strategy is applied correctly (pending working copy system)
- [ ] Configuration validation rejects invalid providers/strategies (pending validation implementation)

### Milestone: Network/Remote Configuration

**Subsystem**: REST API client and remote server connections
**Primary Crate**: `ah-rest-client`

**Deliverables:**

- Define `NetworkConfig` struct for network and API settings
- Implement configuration for WebUI service URL
- Add validation for network configuration values
- Create unit tests for network config parsing
- Update root Config struct with network composition

**Configuration Keys:**

- `service-base-url` - WebUI service base URL

**Sources**: Configuration.md

**Key Source Files:**

- `crates/ah-rest-client/src/network_config.rs` - Network config type
- `crates/ah-config-types/src/network.rs` - Network config definitions

**Implementation Details:**

- NetworkConfig struct implemented with WebUI service base URL setting
- Serde serialization with proper field renaming
- Integration with root Config struct via flattening

**Verification:**

- [x] NetworkConfig struct defined with required configuration options
- [x] Configuration integrated into root Config struct
- [x] Serde serialization with proper field naming
- [ ] WebUI service URL is applied correctly (pending REST client integration)
- [ ] Configuration validation rejects invalid URLs (pending validation implementation)

### Milestone: Task/Execution Configuration

**Subsystem**: Task execution and agent running
**Primary Crate**: `ah-core`

**Deliverables:**

- Define `TaskConfig` struct for task execution settings
- Implement configuration for notifications, editor behavior
- Add validation for task configuration values
- Create unit tests for task config parsing
- Update root Config struct with task composition

**Configuration Keys:**

- `notifications` - Enable OS notifications on completion
- `task-editor.use-vcs-comment-string` - Use VCS comment strings
- `task-template` - Path to task template file

**Sources**: CLI.md, TUI-PRD.md

**Key Source Files:**

- `crates/ah-core/src/task_config.rs` - Task config type
- `crates/ah-config-types/src/task.rs` - Task config definitions

**Implementation Details:**

- TaskConfig struct implemented with notifications, editor behavior, and task template settings
- Serde serialization with proper field renaming for nested keys
- Integration with root Config struct via flattening

**Verification:**

- [x] TaskConfig struct defined with required configuration options
- [x] Configuration integrated into root Config struct
- [x] Serde serialization with proper field naming
- [ ] Task completion notifications work based on config (pending task execution integration)
- [ ] Editor behavior respects VCS comment string settings (pending editor integration)
- [ ] Task template files are loaded correctly (pending template system)
- [ ] Configuration validation works for all task settings (pending validation implementation)

### Milestone: Logging Configuration

**Subsystem**: Centralized logging system
**Primary Crate**: `ah-logging`

**Deliverables:**

- Define `LoggingConfig` struct for logging verbosity
- Implement configuration for log levels and formatting
- Add validation for logging configuration values
- Create unit tests for logging config parsing
- Update root Config struct with logging composition

**Configuration Keys:**

- `log-level` - Logging verbosity level

**Sources**: Configuration.md, CLI.md

**Key Source Files:**

- `crates/ah-logging/src/logging_config.rs` - Logging config type
- `crates/ah-config-types/src/logging.rs` - Logging config definitions

**Implementation Details:**

- LoggingConfig struct implemented with log level verbosity setting
- Serde serialization with proper field renaming
- Integration with root Config struct via flattening

**Verification:**

- [x] LoggingConfig struct defined with required configuration options
- [x] Configuration integrated into root Config struct
- [x] Serde serialization with proper field naming
- [ ] Log level filtering works correctly (pending logging system integration)
- [ ] Log output respects configuration settings (pending logging system integration)
- [ ] Configuration validation rejects invalid log levels (pending validation implementation)
- [ ] Different log levels produce appropriate output (pending logging system integration)
