# How to Add Support for New Agents

This guide explains the minimal steps to integrate a third‑party agent into Agent Harbor (AH): documenting the agent, running it locally, understanding AH CLI’s abstraction, and implementing the adapter in `ah-agents`.

> Paths below are relative to the repo root.

---

## Definition of Done (DoD)

An agent is “done” when:

- Can be started via: `cargo run --bin ah agent start --agent <your-agent> --prompt "write simple hello world python program" < --additional-flags>`.
- Adapter runs with a synthetic HOME directory for isolation and security (see [Appendix 2: Synthetic HOME Environment](#appendix-2-synthetic-home-environment)).
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

> ADVICE:
>
> This is best done by uploading the template in a deep research prompt in ChatGPT.
>
> Once the initial draft of the document is ready, you can ask a local agent to validate the information by interacting with the CLI of the targeted software (or by directly examining its source code when available).

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

> ADVICE:
>
> Some steps can be done by asking local agent to run `<your-agent> --help` command to gather information about its capabilities and requirements.

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

### ACP client quickstart (bridging external binaries)

- Use `--agent acp` when invoking `ah agent start` to wrap an external ACP-compliant binary.
- Flag: `--acp-agent-cmd "<binary and args>"` (env override `AH_ACP_AGENT_CMD`)
- Resolution order: CLI flag → env var → PATH search for `acp-agent`/`mock-agent` → fallback
  `acp-agent`. Version detection runs `<binary> <args> --version` and accepts noisy strings like
  `mock-agent version v0.2.3`.
- Environment forwarded to the external binary includes `ACP_LLM_API`, `ACP_LLM_API_KEY`,
  `ACP_INITIAL_PROMPT`, `ACP_OUTPUT=json` (when `--output json*`), and `ACP_SNAPSHOT_CMD`.
- Quick verification: `just test-acp-client` runs the ACP unit tests (version detection + RPC stub).

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

> ADVICE:
>
> A local agent might be very helpful for this task. You can provide the specification you wrote for the agent and the content of the `crates/ah-agents` folder for reference implementations of other agents. Then ask it to implement the new agent. Then you need to review and test the implementation thoroughly.

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

> Advice
>
> This step too can be performed by local agent.

---

# Appendix 1: Quick Implementation Checklist

This appendix provides a condensed checklist for agent integration. Refer to sections 5-6 for detailed implementation guidance.

## Phase 1: Core Implementation

### Files to Create

- [ ] `specs/Public/3rd-Party-Agents/YourAgent.md` (see section 1)
- [ ] `crates/ah-agents/src/your_agent.rs` (see section 5.2)

### Files to Modify

- [ ] `crates/ah-agents/Cargo.toml` - Add feature flag
- [ ] `crates/ah-agents/src/lib.rs` - Add registry integration (5 points)
- [ ] `crates/ah-cli/src/agent/start.rs` - Add CLI integration (8 points)
- [ ] `crates/ah-core/src/agent_types.rs` - Add enum variant (if needed)

## Phase 2: Implementation Requirements

### AgentExecutor Trait (9 required methods)

Refer to section 5.2 for detailed requirements:

- [ ] Basic methods: `name()`, `detect_version()`, `config_dir()`
- [ ] Launch: `prepare_launch()` with env vars, stdio, flags
- [ ] Credentials: `credential_paths()`, `get_user_api_key()`
- [ ] Sessions: `export_session()`, `import_session()`
- [ ] Output: `parse_output()`

### Minimal Tests (5 required tests)

- [ ] Version parsing, agent name, config paths - see section 5.2

## Phase 3: Verification

### Quick Commands

```bash
cargo build -p ah-agents     # Verify agent builds
cargo build -p ah-cli        # Verify CLI builds
cargo run --bin ah agent start --agent your-agent --prompt "hello"
```

### Definition of Done

- [ ] Agent starts: `cargo run --bin ah agent start --agent your-agent --prompt "hello"`
- [ ] Appears in help: `cargo run --bin ah agent start --help`
- [ ] All tests pass, no clippy warnings, rustfmt clean

## References

- **Implementation details**: Sections 5-6 of this document
- **Code examples**: `crates/ah-agents/src/{gemini,claude,codex}.rs`
- **AgentExecutor trait**: `crates/ah-agents/src/traits.rs`

---

# Appendix 2: Synthetic HOME Environment

Agent Harbor can run third-party agents with a **synthetic HOME directory** instead of the user's real `$HOME` when isolation and security are required. This design choice provides several critical benefits:

## Why Synthetic HOME?

1. **Security Isolation**: Prevents agents from accessing or modifying sensitive files in the user's actual home directory (SSH keys, browser data, personal documents, etc.).

2. **Reproducible Sessions**: Each agent invocation gets a clean, controlled environment that can be exported/imported for debugging and collaboration.

3. **Credential Management**: Agent Harbor can precisely control which credentials and configuration files are available to each agent, preventing credential leakage between different tools.

4. **Sandboxing**: Limits the attack surface by containing agent file system access to a controlled directory tree.

5. **Session Portability**: The synthetic HOME can be packaged and transferred between machines, enabling consistent agent behavior across environments.

## Implementation Requirements

When implementing an agent adapter to support synthetic HOME environments, you should:

- **Respect the synthetic HOME**: When `AgentLaunchConfig.home_dir` is provided, ensure all file operations use this directory instead of the real `$HOME`.
- **Handle credential copying**: Use the credential management APIs to selectively copy required auth files to the synthetic HOME when `copy_credentials` is enabled.
- **Support config isolation**: Allow the agent to read configuration from the synthetic environment when isolation is requested.
- **Implement session export/import**: Provide `export_session()` and `import_session()` methods to package the synthetic HOME for portability.

While not all cases require this level of isolation, supporting synthetic HOME environments enables Agent Harbor's advanced security features like workspace sharing and reproducible agent sessions.

> TODO: Add examples of how to run the agents with `just manual-test-agent-start` once it is stable
