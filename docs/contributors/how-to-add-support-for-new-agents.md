# How to Add Support for New Agents

This guide explains the minimal steps to integrate a third‑party agent into Agent Harbor (AH): documenting the agent, running it locally, understanding AH CLI’s abstraction, and implementing the adapter in `ah-agents`.

> Paths below are relative to the repo root.

---

## Definition of Done (DoD)

An agent is “done” when:

- Can be started via: `cargo run --bin ah agent start --agent <your-agent> --prompt "write simple hello world python program" < --additional-flags>`.
- Adapter runs with a synthetic HOME (no reads/writes to the user’s real `$HOME`).
- Credentials resolve without leaking secrets; session export/import works.
- `agent_by_name("your-agent")` and `available_agents()` return it.
- Appears in CLI help `cargo run --bin ah agent start --help`.
- Unit tests pass; no clippy warnings; code is rustfmt-clean.
- Spec document exists under `specs/Public/3rd-Party-Agents/`.

## 1) Research the new agent and write its spec

Create a new document in:

```
specs/Public/3rd-Party-Agents/<Agent-Name>.md
```

Use the template at: [3rd-Party-Agent-Description-Template.md](../../specs/Public/3rd-Party-Agents/3rd-Party-Agent-Description-Template.md)

Populate the following sections (bulleted here for quick copy/paste):

- **Overview**: What the agent is, maturity, license, upstream repo/version.
- **Primary capabilities**: Tasks, modalities, notable strengths/limits.
- **Interfaces**: CLI, HTTP/gRPC, SDKs. Entry points and invocation patterns.
- **Runtime model**: Long‑lived process vs. one‑shot CLI. Concurrency model.
- **I/O Contracts**: Input schema(s), output schema(s), streaming vs. batch, exit codes.
- **Auth**: Credential types (API key, OAuth, service account, keypair), token lifetime, scopes.
- **Config**: Supported config files/paths, env vars, flags; precedence rules.
- **State & storage**: Where it writes cache, logs, models, temp files.
- **Networking**: Required outbound domains/ports; proxy support; TLS requirements.
- **Performance**: Typical latency, throughput, resource profile; known bottlenecks.
- **Limits & quotas**: Rate limits, payload size, cost considerations.
- **Security**: PII handling, sandboxing, isolation needs; CVEs if any.
- **Local run instructions**: Minimal command(s) to run a smoke test.
- **Observability**: Log format, verbosity flags, metrics endpoints, health checks.
- **Error model**: Common failure modes, retry‑ability, backoff, idempotency.
- **Compatibility matrix**: OS/arch support, min versions of runtimes.
- **Versioning**: Upstream version pin, checksum/signature validation (if releases).
- **Maintenance**: How to detect upstream breaking changes; watch releases/changelog.
- **References**: Links to official docs, examples, API refs.

Commit the spec as part of your PR.

---

## 2) Run the agent’s own CLI locally

Goal: prove you can invoke the agent reliably on your machine.

1. **Install** the agent. Best case is adding it to the nix environment.
2. **Discover commands**: `--help`, `version`, and any init/bootstrap commands.
3. **Create a minimal config** (if required). Save it under a temporary workspace.
4. **Provide credentials** by logging into your account or setting ENV vars.
5. **Understand how to skip onboarding screens** (if applicable).
6. **Smoke test**: Run a minimal invocation that returns a deterministic result.
7. **Capture artifacts**: Note stdout/stderr, exit codes, created files, and logs.

Record the exact commands and expected outputs in the spec’s _Local run instructions_ section.

---

## 3) Locate credentials and config

You must know exactly where the agent reads its secrets and settings. Check, in order:

- **Env vars** (e.g., `AGENT_API_KEY`, `AGENT_CONFIG`, `HTTP_PROXY`)
- **CLI flags** (e.g., `--api-key=...`, `--config=/path/to/file`)
- **Config files** and precedence. Common locations:
  - Project local: `./agent.yaml`, `.env`
  - User: `~/.config/<agent>/config.(yaml|toml|json)`
  - System: `/etc/<agent>/config.*`

- **Credential stores**: macOS Keychain, Windows Credential Manager, `pass`, `gnome-keyring`.
- **Runtime writes**: caches (`~/.cache/<agent>`), logs, and tmp dirs.

Document:

- The minimal set of **required** credentials.
- The **recommended** way to pass them (env, file, keychain).
- How to **validate** they’re loaded (e.g., `--diagnostics`, verbose logs, or dry‑run).

---

## 4) Understand AH CLI and `AgentStartArgs`

Read the code in `crates/ah-cli` and identify the abstraction used to start agents (the `AgentStartArgs` struct). This captures the parameters the AH CLI passes down to any agent adapter. Typical fields include:

- **Agent identity** (name/type, version)
- **Paths** (working dir, config path(s), artifacts dir)
- **Credential handles** (env var map or secret ref)
- **Runtime flags** (verbosity, dry‑run, network policy)
- **I/O wiring** (stdin/stdout mode, streaming)

Action items:

- Note how `AgentStartArgs` is constructed by the CLI subcommands.
- Find how args are **validated** and **defaulted**.
- Identify where args are handed to the adapter in `ah-agents`.

---

## 5) Implement the agent in `ah-agents` (summary)

This section condenses the concrete integration work based on our Gemini integration and the `AgentExecutor` trait. Use it as the single source of truth while coding.

### 5.1 Files to create/modify (code layer only)

- `crates/ah-agents/src/your_agent.rs` — new module with full implementation (feature-gated).
- `crates/ah-agents/Cargo.toml` — add feature flag `your-agent`.
- `crates/ah-agents/src/lib.rs` — 5 integration points:
  - Module declaration with `#[cfg(feature = "your-agent")]`
  - Convenience constructor `your_agent()`
  - Add case in `agent_by_name()`
  - Add to `available_agents()`
  - Add tests for constructor/lookup

### 5.2 Implementation requirements (`your_agent.rs`)

#### Struct & constructors

- Define `pub struct YourAgent { binary_path: String }`
- Implement `new()` and `Default` (pointing to the canonical binary name).

#### Version parsing

- Implement `parse_version(output: &str) -> AgentResult<AgentVersion>`
  - Use a regex to extract semantic versions from stdout/stderr.
  - Handle multiple formats; prefer resilient parsing.

#### Implement `AgentExecutor` trait

- `name()` → static kebab-case agent id (e.g., `"your-agent"`).
- `detect_version()` → run `your-agent-binary --version`; parse stdout or stderr.
- `prepare_launch(config: AgentLaunchConfig)` builds a `tokio::process::Command`:
  - Copy/prepare credentials if needed (respect `config.copy_credentials`).
  - Set `HOME` to `config.home_dir` and `current_dir` to `config.working_dir`.
  - Wire env: API base (`YOUR_AGENT_API_BASE`), API key (`YOUR_AGENT_API_KEY`), any agent-specific env.
  - Configure stdio for interactive vs piped modes.
  - Add model selection (`--model <model>`), output format (`--output-format json` when requested), search/web flags, and any agent-specific flags.
  - Append the prompt/command payload as last argument when applicable.

- `credential_paths()` → return plausible locations under `~/[agent-dir]/` that the agent uses (`config.json`, `auth.json`, `credentials.json`, etc.).
- `get_user_api_key()` → resolve API key in priority order: environment var → file path in env → well-known credential files; return `Ok(None)` when absent.
- `export_session(home_dir)` / `import_session(archive, home_dir)` → tar.gz round‑trip of the agent’s config dir for portability.
- `parse_output(raw: &[u8])` → normalize agent output into `Vec<AgentEvent>` (Thinking/ToolUse/Output/Error) based on the agent’s stdout conventions.
- `config_dir(home)` → return `home.join(".[agent-config-dir]")`.

#### Tests

Add at least minimal tests for the new agent:

- `test_parse_version()` — prefix+version.
- `test_parse_version_simple()` — bare version.
- `test_agent_name()` — `name()` returns the expected id.
- `test_config_dir()` — correct path under provided HOME.
- `test_credential_paths()` — list includes expected files.

---

## 6) Wire it into the registry and CLI

**ah-agents**

- Feature flag in `crates/ah-agents/Cargo.toml`.
- `crates/ah-agents/src/lib.rs`:
  - `pub mod your_agent` gated on the feature
  - `pub fn your_agent() -> your_agent::YourAgent`
  - Add to `agent_by_name()` and `available_agents()`
  - Tests

**ah-core**

- `crates/ah-core/src/agent_types.rs`: add enum variant (if missing).
- Optional binary detection: `crates/ah-core/src/agent_binary.rs` (binary name + tools profile).

**ah-cli**

- `crates/ah-cli/src/agent/start.rs` (8 points):
  - Add to `CliAgentType`
  - Add to `From<CliAgentType> for AgentType`
  - `--your-agent-model` flag
  - Agent executor creation
  - Proxy configuration
  - Model selection logic
  - `build_home_dir()` naming
  - Ensure not in `run_legacy_agent()` fallback

---

# Appendix 1: New Agent Implementation Checklist

Use this quick reference when adding a new agent to Agent Harbor.

## Files to Create/Modify

### 1. Core Type (if needed)

- [ ] `crates/ah-core/src/agent_types.rs` - Add enum variant

### 2. Agent Implementation

- [ ] `crates/ah-agents/src/your_agent.rs` - Create new file with full implementation

### 3. Agent Integration

- [ ] `crates/ah-agents/Cargo.toml` - Add to features (if not present)
- [ ] `crates/ah-agents/src/lib.rs` - 5 integration points:
  - [ ] Module declaration with feature gate
  - [ ] Convenience constructor function
  - [ ] Add to `agent_by_name()`
  - [ ] Add to `available_agents()`
  - [ ] Add tests

### 4. CLI Integration

- [ ] `crates/ah-cli/src/agent/start.rs` - 8 integration points:
  - [ ] Add to `CliAgentType` enum
  - [ ] Add to `From<CliAgentType>` impl
  - [ ] Add `--your-agent-model` flag
  - [ ] Add to agent executor creation
  - [ ] Add to proxy configuration
  - [ ] Add to model selection logic
  - [ ] Add to `build_home_dir()`
  - [ ] Remove from `run_legacy_agent()` (if present)

### 5. Binary Detection (optional)

- [ ] `crates/ah-core/src/agent_binary.rs` - Add to binary name and tools profile

### 6. Documentation

- [ ] `specs/Public/3rd-Party-Agents/Your-Agent.md` - Agent specification
- [ ] `README.md` - Add to supported agents list (if not present)

## Implementation Requirements

### Agent Implementation (`your_agent.rs`)

#### Struct and Constructor

- [ ] Create `YourAgent` struct with `binary_path` field
- [ ] Implement `new()` constructor
- [ ] Implement `Default` trait

#### Version Parsing

- [ ] Implement `parse_version()` helper
- [ ] Use regex to extract version numbers
- [ ] Handle multiple output formats

#### AgentExecutor Trait

- [ ] `name()` - Return agent name as static str
- [ ] `detect_version()` - Run `--version` and parse output
- [ ] `prepare_launch()` - Build command with all flags and env vars
  - [ ] Copy credentials if needed
  - [ ] Set HOME environment variable
  - [ ] Set working directory
  - [ ] Add API server/key env vars
  - [ ] Configure stdio (interactive vs piped)
  - [ ] Add agent-specific flags
  - [ ] Add prompt as argument
- [ ] `credential_paths()` - Return list of credential files
- [ ] `get_user_api_key()` - Resolve API key from env or files
- [ ] `export_session()` - Create tar.gz of config directory
- [ ] `import_session()` - Extract tar.gz to config directory
- [ ] `parse_output()` - Convert agent output to normalized events
- [ ] `config_dir()` - Return config directory path

#### Tests

- [ ] `test_parse_version()` - Test version with prefix
- [ ] `test_parse_version_simple()` - Test bare version number
- [ ] `test_agent_name()` - Verify name() returns correct value
- [ ] `test_config_dir()` - Verify config directory path
- [ ] `test_credential_paths()` - Verify credential paths list

#### Documentation

- [ ] Module-level documentation with:
  - [ ] Brief description
  - [ ] Configuration details
  - [ ] Authentication information
  - [ ] CLI interface documentation
  - [ ] Reference links (docs, GitHub)
- [ ] Inline comments explaining intentions
- [ ] Document all environment variables
- [ ] Document command-line flags

## Testing

- [ ] `cargo test -p ah-agents --features your-agent --lib` passes
- [ ] `cargo test -p ah-agents --all-features` passes
- [ ] `cargo build -p ah-agents --features your-agent` succeeds
- [ ] `cargo build -p ah-cli` succeeds
- [ ] `cargo clippy -p ah-agents --features your-agent -- -D warnings` passes
- [ ] `cargo fmt --package ah-agents` applied
- [ ] CLI help shows your agent: `cargo run -p ah-cli -- agent start --help`

## Code Quality

- [ ] All code properly formatted with rustfmt
- [ ] No clippy warnings specific to your code
- [ ] Module documentation is comprehensive
- [ ] Inline comments explain non-obvious code
- [ ] Error handling is defensive
- [ ] Tests cover edge cases

## Git Commits

- [ ] First commit: Agent abstraction layer implementation
- [ ] Second commit: CLI integration
- [ ] Commit messages follow conventional commits style
- [ ] Commit messages include detailed summary

## Quick Test Commands

```bash
# Test abstraction layer
cargo test -p ah-agents --features your-agent --lib

# Test all features
cargo test -p ah-agents --all-features

# Build and check
cargo build -p ah-agents --features your-agent
cargo build -p ah-cli

# Lint
cargo clippy -p ah-agents --features your-agent -- -D warnings

# Format
cargo fmt --package ah-agents

# Check CLI help
cargo run -p ah-cli -- agent start --help | grep your-agent
```

## Verification Points

Before committing, verify:

1. ✅ All tests pass
2. ✅ Code compiles without errors
3. ✅ No clippy warnings related to your changes
4. ✅ Code formatted with rustfmt
5. ✅ Documentation is complete and accurate
6. ✅ Agent appears in CLI help
7. ✅ All files mentioned above are modified/created
8. ✅ Commit messages are descriptive

## Example Reference

See Gemini CLI implementation:

- Agent: `crates/ah-agents/src/gemini.rs`
- Lib: `crates/ah-agents/src/lib.rs`
- CLI: `crates/ah-cli/src/agent/start.rs`

## Need Help?

1. Check existing implementations: `gemini.rs`, `claude.rs`, `codex.rs`
2. Look at the `AgentExecutor` trait in `crates/ah-agents/src/traits.rs`
