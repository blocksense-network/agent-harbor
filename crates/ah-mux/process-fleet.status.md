### Overview

Goal: Implement a `process-fleet` binary in the `ah-mux` package that provides advanced process orchestration capabilities using terminal multiplexers. The tool will serve as a bridge between configuration-driven process management (inspired by process-compose and tmuxinator) and the agent-harbor multiplexer infrastructure, enabling sophisticated process fleets with health checks, dependency management, and template-based configuration.

The `process-fleet` tool will integrate with the existing `ah-mux` crate's multiplexer backends (tmux, kitty, wezterm, zellij, screen) while adding configuration file parsing, environment variable expansion via envsubst, and template processing capabilities.

### Deliverables

#### Core Binary Implementation

- [ ] Create `process-fleet` binary target in `ah-mux` crate's `Cargo.toml`
- [ ] Implement main CLI entry point with subcommands for different operation modes
- [ ] Add integration with existing `ah-mux-core` traits for multiplexer operations
- [ ] Support all multiplexers enabled by `ah-mux` crate features (tmux, kitty, wezterm, zellij, screen)

#### Process-Compose Configuration Support

- [ ] Parse process-compose YAML/JSON configuration files (reference: `resources/process-compose/www/docs/configuration.md`)
- [ ] Implement process dependency graph resolution and execution ordering
- [ ] Support process health checks and restart policies (`availability.restart`, `availability.backoff_seconds`)
- [ ] Handle process replicas with `PC_REPLICA_NUM` environment variable injection
- [ ] Support `is_dotenv_disabled` flag to prevent environment variable injection conflicts
- [ ] Implement shutdown ordering and graceful termination sequences

#### Environment Variable Expansion (envsubst Integration)

- [ ] Integrate envsubst functionality from `resources/envsubst/` for advanced variable expansion
- [ ] Support envsubst functions: `${VAR^^}` (uppercase), `${HOST%:8000}` (suffix removal), `${VAR/old/new}` (pattern replacement)
- [ ] Load and process `.env` files as specified in process-compose configuration
- [ ] Support multiple environment file loading with `-e` flag equivalents
- [ ] Implement `--disable-dotenv` functionality to skip automatic `.env` loading
- [ ] Handle `.pc_env` files for Process Compose-specific environment settings

#### Tmuxinator Configuration Support

- [ ] Parse tmuxinator YAML configuration files (reference: `resources/tmuxinator/README.md`)
- [ ] Support tmuxinator project hooks: `on_project_start`, `on_project_first_start`, `on_project_restart`, `on_project_exit`, `on_project_stop`
- [ ] Implement window and pane layout specifications from tmuxinator configs
- [ ] Handle tmuxinator's `pre_window` commands for setup in each window/pane
- [ ] Support tmuxinator's `startup_window` and `startup_pane` selection
- [ ] Implement `attach: false` mode for detached operation

#### Template Processing Support

- [ ] Detect template files by extension (`.erb`, `.jinja`, `.j2`, etc.)
- [ ] Implement ERB template processing for Ruby-based templates
- [ ] Add Jinja2 template processing for Python-based templates
- [ ] Support custom template delimiters and processing engines
- [ ] Process templates before parsing configuration files
- [ ] Handle template variables and context passing

#### Convenient CLI Interface

- [ ] Implement direct process launching without configuration files: `process-fleet run <command> [args...]`
- [ ] Support `--name`, `--env`, `--health-check` flags for ad-hoc process management
- [ ] Add `--multiplexer` flag to specify target multiplexer (tmux, kitty, etc.)
- [ ] Implement `--attach`/`--detach` modes for session attachment control
- [ ] Support `--replicas` flag for launching multiple instances of the same process

#### Multiplexer Integration with Health Checks

- [ ] Create launcher processes that implement waiting based on health checks
- [ ] Integrate with multiplexer pane management for process isolation
- [ ] Implement process health monitoring within multiplexer sessions
- [ ] Support process restart within multiplexer panes on health check failures
- [ ] Handle multiplexer-specific process lifecycle events (pane creation, destruction, etc.)

### Verification

#### Unit Test Scenarios

- [ ] Parse valid process-compose configuration files without errors
- [ ] Parse valid tmuxinator configuration files without errors
- [ ] Process envsubst variable expansion correctly for all supported functions
- [ ] Detect and process different template file types appropriately
- [ ] Validate configuration schema compliance for both formats

#### Integration Test Scenarios

- [ ] Launch single process via CLI and verify multiplexer pane creation
- [ ] Launch process fleet from process-compose config and verify dependency ordering
- [ ] Launch tmuxinator-style session and verify window/pane layout
- [ ] Test environment variable injection and expansion in running processes
- [ ] Test health check monitoring and automatic restart functionality
- [ ] Verify graceful shutdown and cleanup across all multiplexer backends
- [ ] Test template processing pipeline from file to running processes
- [ ] Validate cross-platform multiplexer support (where applicable)

#### End-to-End Test Scenarios

- [ ] Create complete development environment using process-compose config with databases, APIs, and frontend services
- [ ] Simulate complex tmuxinator project with multiple windows, panes, and startup commands
- [ ] Test environment variable precedence and .env file loading behavior
- [ ] Verify process dependency resolution with circular dependency detection
- [ ] Test replica scaling and `PC_REPLICA_NUM` environment variable handling
- [ ] Validate template processing with real project configurations

### Implementation Details

_(To be filled after implementation begins)_

### Key Source Files

_(To be filled after implementation begins)_

### Outstanding Tasks

_(To be filled after implementation begins)_

### Resources for Implementation

#### Process-Compose References

- Configuration documentation: `resources/process-compose/www/docs/configuration.md`
- Example configurations: `resources/process-compose/fixtures/` and `resources/process-compose/fixtures-code/`
- Schema definition: `resources/process-compose/schemas/process-compose-schema.json`
- Source code structure: `resources/process-compose/src/` (Go implementation reference)

#### Envsubst References

- Core implementation: `resources/envsubst/` (Go package)
- Function documentation: `resources/envsubst/readme.md`
- Test cases: `resources/envsubst/*_test.go`

#### Tmuxinator References

- Configuration documentation: `resources/tmuxinator/README.md`
- Example configurations: `resources/tmuxinator/spec/fixtures/`
- Ruby implementation: `resources/tmuxinator/lib/tmuxinator/`

#### Related Agent-Harbor Components

- Multiplexer core traits: `crates/ah-mux-core/src/lib.rs`
- Existing multiplexer implementations: `crates/ah-mux/src/tmux.rs`, `crates/ah-mux/src/kitty.rs`
- Terminal multiplexer specifications: `specs/Public/Terminal-Multiplexers/`
