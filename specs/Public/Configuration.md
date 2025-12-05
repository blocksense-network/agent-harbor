## AH Configuration

### Overview

- `ah config` subcommand with Git-like interface for reading and updating configuration.
- Schema validation on both config file loading and CLI-based modification.
- Precedence for `~/.config` over `%APPDATA%` on Windows only when both are present.
- Motivation and support for tracking the origin of each configuration value, with use cases such as: debug-level log reporting, enforced setting explanation, and editor pre-fill mes
  sages.

Layered configuration supports system, user, repo, and repo-user scopes. Values can also be supplied via environment variables and CLI flags. See [CLI](CLI.md) for flag mappings.

### Locations (by scope)

- System (OS‑level):
  - Linux: `/etc/agent-harbor/config.toml`
  - macOS: `/Library/Application Support/agent-harbor/config.toml`
  - Windows: `%ProgramData%/agent-harbor/config.toml`
- User:
  - Linux: `$XDG_CONFIG_HOME/agent-harbor/config.toml` or `$HOME/.config/agent-harbor/config.toml`
  - macOS: `$HOME/Library/Application Support/agent-harbor/config.toml`
  - Windows: `%APPDATA%/agent-harbor/config.toml` (precedence is given to `~/.config` when both exist as noted below)
  - Custom (when `AH_HOME` is set): `$AH_HOME/config.toml`
- Repo: `<repo>/.agents/config.toml`
- Repo‑user: `<repo>/.agents/config.user.toml` (ignored by VCS; add to `.gitignore`)

Paths are illustrative; the CLI prints the exact search order in `ah config --explain` and logs them at debug level.

The `AH_HOME` environment variable can override the default user configuration and data directory locations. When set, it changes the user configuration file to `$AH_HOME/config.toml` and the local SQLite database to `$AH_HOME/state.db` (see [State-Persistence.md](State-Persistence.md)).

### Admin‑enforced values

Enterprise deployments may enforce specific keys at the System scope. Enforced values are read‑only to lower scopes. The CLI surfaces enforcement in `ah config <key> --explain` output and prevents writes with a clear error. See the initial rationale in [Configuration](../Initial-Developer-Input/Configuration.md).

Use a single key `ui` (not `ui.default`) to control the default UI.

### Configuration Layers and Precedence

Configuration values are resolved from multiple sources with the following precedence order (highest to lowest):

1. **CLI flags** - Command-line arguments override all other sources
2. **CLI --config** - Additional configuration file specified via `--config` flag
3. **Environment variables** - `AH_*` prefixed variables
4. **Repo-user scope** - `<repo>/.agents/config.user.toml`
5. **Repo scope** - `<repo>/.agents/config.toml`
6. **User scope** - `~/.config/agent-harbor/config.toml` (or `$AH_HOME/config.toml`)
7. **System scope** - `/etc/agent-harbor/config.toml` (or equivalent platform location)

### Mapping Rules (Flags ↔ Config ↔ ENV/JSON)

To keep things mechanical and predictable:

- TOML sections correspond to subcommand groups (e.g., `[repo]` for `ah repo ...`).
- CLI option keys preserve dashes in TOML (e.g., `default-mode`, `task-runner`). The name of the options should be chosen to read well both on the command-line and inside a configuration file.
- There are options that are available only within configuration files (e.g. `[[fleet]]` as described below).
- JSON and environment variables replace dashes with underscores. ENV vars keep the `AH_` prefix.
- CLI options that participate in this precedence chain **must not rely on Clap `default_value`**. Declare them as `Option<T>` and apply defaults manually after configuration has been merged. Otherwise a Clap-supplied default is indistinguishable from an explicit CLI override and will incorrectly stomp on repo/user/system values. Example: `--log-level` is parsed as `Option<CliLogLevel>`; release builds fall back to `info`, while debug builds opt into `debug`, but that decision happens after `load_config` merges every layer.
- When a flag accepts a filesystem path, store it as a string in the serialized JSON (e.g. `log-dir = "/tmp/logs"`). The CLI layer can still convert the string into a `PathBuf` when it needs to resolve the final log location, but the serialized overrides should remain UTF-8 strings so they round-trip cleanly through JSON/TOML and the precedence engine.

Examples:

- Flag `--remote-server` ↔ TOML `remote-server` ↔ ENV `AH_REMOTE_SERVER`
- Per-server URLs are defined under `[[server]]` entries; `remote-server` may refer to a server `name` or be a raw URL.
- WebUI-only: key `service-base-url` selects the REST base URL used by the browser client when the WebUI is hosted persistently at a fixed origin.
- Flag `--task-runner` ↔ TOML `repo.task-runner` ↔ ENV `AH_REPO_TASK_RUNNER`

### Keys

- `ui`: string — default UI to launch with bare `ah` (values: `"tui"` | `"webui"`).
- `browser-automation`: `boolean` — enable/disable site automation.
- `browser-profile`: string — preferred agent browser profile name.
- `chatgpt-username`: string — optional default ChatGPT username used for profile discovery.
- `codex-workspace`: string — default Codex workspace to select before pressing "Code".
- `remote-server`: string — either a known server `name` (from `[[server]]`) or a raw URL. If set, AH uses REST; otherwise it uses local SQLite state.
- `tui-font-style`: string — TUI symbol style (values: `"nerdfont"` | `"unicode"` | `"ascii"`). Auto-detected based on terminal capabilities.
- `tui-font`: string — TUI font name for advanced terminal font customization.
- `acp.daemonize`: string — controls `ah acp` behavior when no access point is running (`auto` = start daemon with idle timeout, default; `never` = run inline; `disabled` = fail if absent).
- `acp.socket-path` / `acp.uds_path`: string — preferred Unix-domain socket path for ACP access point discovery (overrides platform default). When set, the access point listens on this socket in addition to WebSocket.
- `acp.transport`: `websocket` (default) or `stdio`. Stdio is only used by embedded/inline launchers (e.g. `ah acp --daemonize=never`) and is not exposed by `ah agent access-point` CLI flags.
- `acp.ws-url`: string — preferred WebSocket URL for ACP access point discovery.

### Behavior

- CLI flags override environment, which override repo-user, repo, user, then system scope.
- On Windows, `~/.config` takes precedence over `%APPDATA%` only when both are present.
- The CLI can read, write, and explain config values via `ah config`.
- Backend selection: if `remote-server` is set (by flag/env/config), AH uses the REST API; otherwise it uses the local SQLite database.
- Repo detection: when `--repo` is not specified, AH walks parent directories to find a VCS root among supported systems; commands requiring a repo fail with a clear error when none is found.

### Validation

- The configuration file format is TOML, validated against a single holistic JSON Schema:
  - Schema: `specs/schemas/config.schema.json` (draft 2020-12)
  - Method: parse TOML → convert to a JSON data model → validate against the schema
  - Editors: tools like Taplo can use the JSON Schema to provide completions and diagnostics

- DRY definitions: the schema uses `$defs` for shared `enums` and shapes reused across the CLI (e.g., `Mode`, `Multiplexer`, `Vcs`, `DevEnv`, `TaskRunner`, `AgentName`, `SupportedAgents`).

Tools in the dev shell:

- `taplo` (taplo-cli): TOML validation with JSON Schema mapping
- `ajv` (ajv-cli): JSON Schema `validator` for JSON instances
- `docson` (via shell function): local schema viewer using `yarn dlx` (no global install)

Examples (use Just targets inside the Nix dev shell):

```bash
# Validate all JSON Schemas (meta-schema compile)
just conf-schema-validate

# Check TOML files with Taplo
just conf-schema-taplo-check

# Preview the schemas with Docson (serves http://localhost:3000)
just conf-schema-docs
```

Tip: from the host without entering the shell explicitly, you can run any target via:

```bash
nix develop --command just conf-schema-validate
```

### Servers, Fleets, and Sandboxes

AH supports declaring remote servers, fleets (multi-environment presets), and sandbox profiles.

```toml
remote-server = "office-1"  # optional; can be a name from [[server]] or a raw URL

[[server]]
name = "office-1"
url  = "https:/ah.office-1.corp/api"

[[server]]
name = "office-2"
url  = "https://ah.office-2.corp/api"

# Fleets define a combination of local testing strategies and remote servers
# to be used as presets in multi-OS or multi-environment tasks.

[[fleet]]
name = "default"  # chosen when no other fleet is provided

  [[fleet.member]]
  type = "container"   # refers to a sandbox profile by name (see [[sandbox]] below)
  profile = "container"

  [[fleet.member]]
  type = "remote"      # special value; not a sandbox profile
  url  = "https://ah.office-1.corp/api"  # or `server = "office-1"`

[[sandbox]]
name = "container"
type = "container"      # predefined types with their own options

# Examples (type-specific options are illustrative and optional):
# [sandbox.options]
# engine = "docker"           # docker|podman
# image  = "ghcr.io/ah/agents-base:latest"
# user   = "1000:1000"        # uid:gid inside the container
# network = "isolated"         # bridge|host|none|isolated
```

Flags and mapping:

- `--remote-server <NAME|URL>` selects a server (overrides `remote-server` in config).
- `--fleet <NAME>` selects a fleet; default is the fleet named `default`.
- Bare `ah` uses `ui` to decide between TUI and WebUI (defaults to `tui`).

### Filesystem Snapshots

Control snapshotting and working‑copy strategy. Defaults are `auto`.

TOML keys (top‑level):

```toml
fs-snapshots = "auto"        # auto|zfs|btrfs|agentfs|git|disable
working-copy = "auto"        # auto|cow-overlay|worktree|in-place

# Provider‑specific (optional; may be organized under a [snapshots] section in the future)
# git.includeUntracked = false
# git.worktreesDir = "/var/tmp/ah-worktrees"
# git.shadowRepoDir = "/var/cache/ah/shadow-repos"
```

Flag and ENV mapping:

- Flags: `--fs-snapshots`, `--working-copy`
- ENV: `AH_FS_SNAPSHOTS`, `AH_WORKING_COPY`

Behavior:

- `auto` selects the highest‑score provider for the repo and platform. Users can pin to `git` (or any provider) even if CoW is available.
- `cow-overlay` requests isolation at the original repo path (Linux: namespaces/binds; macOS/Windows: AgentFS). When impossible, the system falls back to `worktree` with a diagnostic.
- `in-place` runs the agent directly on the original working copy. Isolation is disabled, but FsSnapshots may still be available when the chosen provider supports in‑place capture (e.g., Git shadow commits, ZFS/Btrfs snapshots). Use `fs-snapshots = "disable"` to turn snapshots off entirely.

### Sandbox Configuration

Control Linux local sandboxing behavior and resource limits. These options apply to `ah agent sandbox` commands.

TOML keys (top-level section):

```toml
[sandbox]
mode = "dynamic"         # dynamic|static - Interactive read allow-list vs RO with blacklists
debug = true             # Enable debugging/ptrace inside sandbox (default: true)
allow-network = false    # Enable internet egress via slirp4netns (default: false)
containers = false       # Allow rootless containers inside sandbox (default: false)
vm = false               # Allow VMs inside sandbox (default: false)
allow-kvm = false        # Expose /dev/kvm for VM acceleration (default: false)
tmpfs-size = "256m"      # Size limit for isolated /tmp tmpfs mount (default: "256m")
rw-paths = []            # List of read-write path carve-outs
overlay-paths = []       # List of overlay mount paths
blacklist-paths = []     # List of blocked/hidden paths (for static mode)

  [sandbox.limits]
  pids-max = 1024        # Maximum PIDs for fork-bomb protection
  memory-max = "2G"      # Maximum memory limit
  memory-high = "1G"     # Memory high watermark
  cpu-max = "80000 100000"  # CPU quota/period (e.g., "80000 100000" for 80% of one core)
  io-max = ""            # I/O throttle settings (optional)
```

Flag and ENV mapping:

- Flags: `--mode`, `--debug`, `--allow-network`, `--containers`, `--vm`, `--allow-kvm`, `--tmpfs-size`, `--rw`, `--overlay`, `--blacklist`, `--pids-max`, `--memory-max`, `--memory-high`, `--cpu-max`
- ENV: `AH_SANDBOX_MODE`, `AH_SANDBOX_DEBUG`, `AH_SANDBOX_ALLOW_NETWORK`, etc.

Behavior:

- `dynamic` mode provides interactive read allow-listing with supervisor prompts for file access
- `static` mode makes the filesystem read-only with blacklists, no interactive gating
- `--tmpfs-size` accepts size suffixes (k, m, g) or "0" to disable `/tmp` isolation entirely
- Resource limits apply to the cgroup v2 subtree created for each sandbox session

### Example TOML (partial)

```toml
log-level = "info"

terminal-multiplexer = "tmux"

editor = "nvim"

service-base-url = "https://ah.office-1.corp/api"  # WebUI fetch base; browser calls this URL

# Browser automation (no subcommand section; single keys match CLI flags)
browser-automation = true
browser-profile = "work-codex"
chatgpt-username = "alice@example.com"

# Codex workspace (single key)
codex-workspace = "main"

[repo]
supported-agents = "all" # or ["codex","claude","cursor"]

  [repo.init]
  vcs = "git"
  devenv = "nix"
  devcontainer = true
  direnv = true
  task-runner = "just"

[sandbox]
mode = "dynamic"
debug = true
allow-network = false
tmpfs-size = "512m"

  [sandbox.limits]
  pids-max = 2048
  memory-max = "4G"
  memory-high = "2G"
```

Notes:

- `supportedAgents` accepts "all" or an explicit array of agent names; the CLI may normalize this value internally.
- `devenv` accepts values like `nix`, `spack`, `bazel`, `none`/`no`, or `custom`.

ENV examples:

```
AH_REMOTE_SERVER=office-1
AH_REPO_SUPPORTED_AGENTS=all
```

### Rust Configuration Patterns

The Agent Harbor configuration system follows a **distributed ownership with centralized composition** pattern that enables clean separation of concerns while maintaining a unified configuration interface.

#### Distributed Configuration Types

Each subsystem/crate defines its own configuration type locally (e.g., in `$subsystem_config.rs`), owning the complete type definition for its configuration needs:

```rust
// In ah-tui/src/tui_config.rs
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct TuiConfig {
    pub font_style: FontStyle,
    pub font: Option<String>,
    // ... other TUI-specific fields
}

// In ah-cli/src/cli_config.rs
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct CliConfig {
    pub json_output: bool,
    pub non_interactive: bool,
    // ... other CLI-specific fields
}
```

This approach ensures:

- **Type Safety**: Each subsystem has strongly-typed access to its configuration
- **Encapsulation**: Subsystems control their own configuration schema and validation
- **Testability**: Subsystem-specific configuration can be tested in isolation
- **Flexibility**: Subsystems can evolve their configuration independently

#### Centralized Configuration Composition

The main application (`ah-cli`) defines a single root configuration struct that composes all subsystem configurations using Serde attributes for flattening and field mapping:

```rust
// In ah-cli/src/config.rs or ah-config-types/src/lib.rs
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct Config {
    // Top-level fields that don't belong to specific subsystems
    pub remote_server: Option<String>,

    // Subsystem configurations composed via flattening
    #[serde(flatten)]
    pub ui: UiConfig,

    #[serde(flatten)]
    pub repo: RepoConfig,

    // Rename fields to match CLI spec conventions
    #[serde(rename = "browser-automation")]
    pub browser_automation: bool,

    #[serde(rename = "browser-profile")]
    pub browser_profile: Option<String>,
    // ... other top-level and flattened fields
}
```

#### Configuration Loading and Distribution

One of the first steps after launching `ah` is loading the complete configuration across all layers (system, user, repo, repo-user, environment, CLI flags). This produces a fully populated `Config` instance from which subsystem-specific configuration objects are extracted and passed to respective subsystems:

```rust
// In ah-cli/src/main.rs or initialization code
fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Load configuration from all layers
    let config = config_core::load_config()?;

    // Extract and pass subsystem configurations
    let tui = TuiSubsystem::new(config.ui)?;
    let cli = CliSubsystem::new(config.cli)?;
    let repo = RepoSubsystem::new(config.repo)?;

    // Initialize application with configured subsystems
    App::new(tui, cli, repo).run()
}
```

#### Benefits of This Approach

- **Clean Separation**: Each subsystem owns its configuration while the main app handles composition
- **Spec Compliance**: Serde attributes ensure TOML field names match CLI.md specifications
- **Type Safety**: Compile-time guarantees that configuration is correctly structured
- **Testing Flexibility**: Alternative CLI tools can compose different subsets of configurations
- **Evolution Safety**: Subsystem configuration changes are isolated and don't break other components

#### Default Configuration Values

To promote maximum flexibility, crates should avoid providing default configuration values that are highly-specific to agent harbor. Instead, the default agent harbor configuration is provided in the `ah-configuration-types` module, which is referenced only in the `ah-cli` crate. This simplifies the creation of alternative CLI tools and entry points for the functionality of the crates for testing purposes.

### Subcommand Configuration Patterns

Subcommands that accept configuration-relevant options follow a structured pattern to automatically integrate with the CLI override system. This enables commands like `ah repo init --vcs git --devenv nix` to properly override configuration values.

#### SubcommandOverrides Trait

Subcommand argument structs that contain configuration options implement the `SubcommandOverrides` trait:

```rust
pub trait SubcommandOverrides {
    /// The configuration path where this subcommand's options should be placed
    /// (e.g., "tui" for TUI options, "repo.init" for repo init options)
    fn config_path(&self) -> &'static str;

    /// Convert this subcommand's config-relevant options to JSON
    /// Only include fields that were explicitly provided by the user
    fn to_config_json(&self) -> serde_json::Value;
}
```

#### Automatic CLI Override Integration

The main CLI structure automatically detects and integrates subcommand overrides:

```rust
impl ToJsonOverrides for Cli {
    fn to_json_overrides(&self) -> serde_json::Value {
        // Start with global options
        let mut global_json = serde_json::to_value(self).unwrap_or_default();

        // Extract subcommand options and merge them
        if let serde_json::Value::Object(ref mut map) = global_json {
            self.add_subcommand_overrides(map);
        }

        global_json
    }
}

impl Cli {
    fn add_subcommand_overrides(&self, json_map: &mut serde_json::Map<String, serde_json::Value>) {
        match &self.command {
            Commands::Tui(tui_args) => {
                self.merge_subcommand_config(json_map, tui_args);
            }
            Commands::Health(health_args) => {
                self.merge_subcommand_config(json_map, health_args);
            }
            // Add more subcommands as they implement SubcommandOverrides
            _ => {}
        }
    }
}
```

#### Subcommand Implementation Pattern

Subcommand argument structs implement the trait to specify their configuration mapping:

```rust
impl SubcommandOverrides for TuiArgs {
    fn config_path(&self) -> &'static str {
        "tui"  // Maps to [tui] section in TOML
    }

    fn to_config_json(&self) -> serde_json::Value {
        let mut config = serde_json::Map::new();

        // Only include options that were explicitly provided
        if let Some(ref remote_server) = self.remote_server {
            config.insert("remote-server".to_string(), serde_json::Value::String(remote_server.clone()));
        }

        if config.is_empty() {
            serde_json::Value::Null
        } else {
            serde_json::Value::Object(config)
        }
    }
}
```

#### Nested Path Support

For subcommands that map to nested configuration paths (like `repo.init`), the `config_path()` method returns dot-separated paths:

```rust
impl SubcommandOverrides for RepoInitArgs {
    fn config_path(&self) -> &'static str {
        "repo.init"  // Maps to [repo.init] section in TOML
    }
    // ... implementation
}
```

This automatically creates the nested JSON structure:

```json
{
  "repo": {
    "init": {
      "vcs": "git",
      "devenv": "nix"
    }
  }
}
```

#### Benefits of This Pattern

- **Automatic Integration**: Subcommands automatically participate in CLI override precedence
- **Type Safety**: Compile-time guarantees of correct field mapping
- **Selective Inclusion**: Only explicitly provided options override configuration
- **Extensible**: Easy to add configuration support to new subcommands
- **Testable**: Subcommand overrides can be tested in isolation
- **Future-Proof**: Works with any subcommand structure via the trait pattern
