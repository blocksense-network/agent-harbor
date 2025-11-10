# Cursor CLI Agent — Implementation Plan and Status

This document proposes a comprehensive, review‑first plan to add a `cursor-cli` agent to Agent Harbor, aligned with the agent‑abstraction patterns established in the `agent-abstractions` and `feat/agent-abstraction/gemini` branches. No implementation is included; this is a design and execution plan for review.

## Goals

- Unify Cursor CLI integration under the `ah-agents` abstraction with a clean `AgentExecutor` implementation.
- Support running via `ah agent start --agent cursor-cli` (interactive and non‑interactive), environment isolation, and optional proxy routing.
- Implement robust credential handling, version detection, session export/import, and normalized output parsing consistent with other agents.
- Ship automated tests and documentation, without regressing existing agents.

## Out of Scope (Phase 1)

- Deep E2E parsing of Cursor structured logs if not publicly documented; we will start with a conservative line‑based normalizer, then iterate.
- Packaging Cursor in Nix. Phase 1 assumes Cursor CLI is user‑installed and available on PATH.

## Deliverables

- New `CursorAgent` in `ah-agents` with feature `cursor-cli`.
- Facade crate `ah-agent-cursor` mirroring the Claude/Codex facades.
- CLI wiring to use `CursorAgent` instead of legacy fallback for `--agent cursor-cli`.
- Credential copy support for Cursor config (platform aware), session export/import skeletons, version detection, and minimal output normalizer.
- Unit tests (version parse, name, config paths), integration tests gated on binary availability, and CLI tests ensuring dispatch works.
- Documentation updates (AGENTS.md, CLI help notes, and troubleshooting).

## Reference Patterns Observed

- Abstraction entrypoint: `crates/ah-agents/src/lib.rs` with feature‑gated modules and `agent_by_name`/`available_agents`.
- Concrete agents: `claude.rs`, `codex.rs`, `gemini.rs` implement `AgentExecutor` with:
  - `detect_version` executing `<binary> --version` and `parse_version` via regex.
  - `prepare_launch` configuring HOME isolation, API server/key envs, and stdio.
  - `copy_credentials` using common helpers in `credentials.rs`.
  - `export_session`/`import_session` using `session::{export_directory, import_directory}` (when available/known) or stubbed with TODO and clear error.
  - Simple line‑based `parse_output` mapping to `AgentEvent`.
- CLI integration: `crates/ah-cli/src/agent/start.rs` selects an agent and builds `AgentLaunchConfig` with per‑agent model overrides and exec replacement.
- Core typing/helpers: `ah-core::agent_types::AgentType::CursorCli`, `ah-core::agent_binary::AgentBinary` maps to binary name `cursor`.
- Credentials helper already defines `cursor_credential_paths()`.

## Architecture and Code Changes

- `crates/ah-agents/Cargo.toml`
  - Add feature `cursor-cli` (off by default initially; can be enabled in CI later).
- `crates/ah-agents/src/lib.rs`
  - `#[cfg(feature = "cursor-cli")] mod cursor;`
  - `pub fn cursor() -> cursor::CursorAgent` constructor.
  - `agent_by_name("cursor-cli")` mapping to `CursorAgent` behind the feature.
  - `available_agents()` pushes `"cursor-cli"` behind the feature.
- `crates/ah-agents/src/cursor.rs` (new)
  - Struct `CursorAgent { binary_path: String }` with `binary_path = "cursor"`.
  - `parse_version()` to extract semantic version from `cursor --version` stdout/stderr.
  - `prepare_launch()` implementing:
    - HOME isolation (`HOME` env) and working dir.
    - Credential copy via `cursor_credential_paths()` when `copy_credentials` and custom HOME.
    - Optional `api_server`/`api_key` env wiring:
      - If Cursor supports provider‑specific envs, map from `AgentLaunchConfig` (OpenAI/Gemini/Anthropic). If not, pass generic `OPENAI_API_KEY` as reasonable default.
    - Model flag: `--model <model>` when provided.
    - Interactive vs non‑interactive stdio behavior (inherit vs piped), and add non‑interactive subcommand/flag only if Cursor CLI supports it.
    - Optional `json_output` mapping if supported; otherwise keep text (normalize later).
  - `copy_credentials()` using `credentials::copy_files(cursor_credential_paths(), ...)`, platform‑aware fallbacks.
  - `export_session()`/`import_session()` using `session::export_directory/import_directory` on `~/.cursor` (or reasonable state dir) if safe; else return `AgentError::Other` with TODO note and link.
  - `parse_output()` minimal, line‑based mapping to `AgentEvent::{Thinking, ToolUse, Output, Error}` similar to Codex.
  - `config_dir(&self, home) -> ~/.cursor` and `credential_paths()` returning `cursor_credential_paths()`.
  - Unit tests for version parsing, `name()`, and `config_dir()`.
- Facade crate: `crates/ah-agent-cursor/`
  - `Cargo.toml` and `src/lib.rs` re‑export `CursorAgent` and common traits, with a `cursor()` convenience function, mirroring `ah-agent-claude`/`ah-agent-codex`.
- `crates/ah-cli/src/agent/start.rs`
  - In agent selection, switch `AgentType::CursorCli` to use `Box::new(ah_agents::cursor())` when feature enabled; otherwise keep legacy fallback behind cfg.
  - Extend `build_agent_config()` model selection precedence with a `--cursor-model` override, similar to `--gemini-model`, if Cursor supports explicit models.
  - Update `build_home_dir()` to return `.../agents/cursor-cli` for Cursor.
- `crates/ah-core/src/agent_binary.rs`
  - Optional: add version extraction for Cursor in `AgentBinary::from_agent_type` if `cursor --version` output is known. Otherwise, leave `unknown`.
- Docs: Update `AGENTS.md` and CLI help to include Cursor usage and env hints.

## Testing Strategy

Follow repo testing principles: each test produces its own log file, success keeps output minimal, failures point to log paths.

- Unit tests (Rust):
  - `ah-agents`:
    - `cursor.rs::test_parse_version_*` covering common outputs (e.g., `cursor 1.2.3`, `1.2.3`).
    - `test_agent_name()` returns `"cursor-cli"` or `"cursor"` per `name()` decision (see Open Questions), consistent with `available_agents()` key.
    - `test_config_dir()` equals `<home>/.cursor`.
  - `ah-core` (optional): add a test ensuring `AgentType::CursorCli` maps to `"cursor"` binary and `tools_profile()` returns `"cursor-cli"` consistently.
- Integration tests (conditional):
  - Gate on `which cursor` availability; otherwise `ignore`.
  - Start local proxy if useful (see Gemini/Codex tests) and run a smoke `ah agent start --agent cursor-cli --prompt "noop" --non-interactive` with a temporary HOME.
  - Validate process exit and minimal parsing via normalized events if JSON requested and supported.
- CLI tests:
  - Ensure `--agent cursor-cli` dispatches to abstraction (when feature enabled) and uses `exec()` path.
  - Ensure model precedence if a `--cursor-model` flag is added.
- Logging discipline:
  - Each integration test writes to a dedicated `target/test-logs/cursor/<testname>.log` and only prints the path on failure.

## Tooling, Lints, and CI

- Ensure `just lint-rust` and `just test-rust` pass locally with feature flags:
  - `cargo test -p ah-agents --features cursor-cli` for unit tests.
  - Maintain clippy compliance; fix code instead of disabling lints.
- Do not enable `cursor-cli` in workspace defaults until green locally; then progressively add to CI lanes.

## Rollout Plan

1. Land `ah-agents` Cursor module + unit tests, behind `cursor-cli` feature; update docs (hidden/preview).
2. Add facade crate `ah-agent-cursor` and wire up `ah-cli` dispatch under feature flag.
3. Add integration tests gated by binary existence; stabilize parsing and environment mapping.
4. Optionally add version detection in `ah-core::agent_binary` and enable feature by default once stable.
5. Iterate on output normalization and session export/import fidelity.

## Security and Privacy

- Credential copying honors the minimal required files (`cursor_credential_paths()`), preserves directory structures, and never overwrites unless the destination differs from system HOME and the user explicitly opts into copying (already defaulted in `AgentLaunchConfig` but constrained to custom HOME).
- Avoid logging secrets; gate any sensitive values behind redaction.
- When adding proxy routing, ensure only intended env vars are forwarded.

## Open Questions / Research

- Cursor CLI flags parity:
  - Does Cursor provide non‑interactive mode flags similar to `codex exec`?
  - Does Cursor support `--output-format json` or equivalent structured output? If not, define minimal safe normalizer.
  - Exact model flag naming and provider selection (OpenAI/Anthropic/Gemini) and how best to map `llm_api`/`llm_api_key`.
- Onboarding skip:
  - Whether Cursor CLI requires any onboarding skip config for first‑run in isolated HOME.
- Session export/import:
  - Confirm `~/.cursor` contains sufficient state; assess any additional directories.
- Nix packaging:
  - If we later add Cursor to the flake, verify license constraints; otherwise document manual install.

## Verification Checklist (Phase 1)

- Feature‑gated build: `cargo check -p ah-agents --features cursor-cli` succeeds.
- Unit tests for Cursor pass locally: `cargo test -p ah-agents --features cursor-cli`.
- `ah agent start --agent cursor-cli --non-interactive` uses `exec()` and returns 0 in a sandboxed temp HOME when `cursor` is installed.
- Lints green via `just lint-rust`.
- Docs updated: AGENTS overview and CLI usage mention Cursor with feature flag note.

## Proposed File/Line Touch Points

- Add: `crates/ah-agents/src/cursor.rs`
- Edit: `crates/ah-agents/src/lib.rs` (module, constructor, registry lists)
- Edit: `crates/ah-agents/Cargo.toml` (feature `cursor-cli`)
- Add: `crates/ah-agent-cursor/Cargo.toml`, `crates/ah-agent-cursor/src/lib.rs`
- Edit: `crates/ah-cli/src/agent/start.rs` (dispatch, model flag, home dir mapping)
- Optional Edit: `crates/ah-core/src/agent_binary.rs` (version extraction)
- Docs: `AGENTS.md`, CLI help strings
- Tests: new unit and optional integration tests under `crates/ah-agents/tests/` and CLI tests as applicable

## Implementation Notes

- Keep implementation focused and consistent with Claude/Codex/Gemini structure to minimize divergence.
- Prefer small, testable helpers over monolithic methods.
- Treat unknowns conservatively; clearly mark TODOs with references to public docs or issue links, and wire errors as typed `AgentError` variants.
- Avoid breaking defaults: only wire Cursor into default features after stability passes.

---

Status: Pending review. This plan reflects the repo’s existing agents design and will be refined once Cursor CLI flag/behavior details are confirmed.
