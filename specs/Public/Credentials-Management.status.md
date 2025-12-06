### Overview

Goal: Implement the Credentials Management system described in [Credentials-Management.md](Credentials-Management.md), covering secure multi-account storage, encryption, acquisition workflows, account selection across CLI/TUI, health reporting, and migration from legacy single-account setups. The plan aligns with configuration paths and layering rules in [Configuration.md](Configuration.md) and UI expectations from the TUI/CLI specs.

**Responsibilities by crate (current design):**

- `ah-credentials`: library-first crate for registry/storage, file format (generic key-value payloads), encryption, key management, and passphrase caching. It knows nothing about agent-specific credential shapes.
- `ah-agents`: provides agent-specific acquisition helpers (launch + extract credentials into key-value maps) built on its AgentExecutor implementations.
- `ah-credentials-tests`: shared fixtures/mocks for storage/encryption and acquisition harness wiring.
- `ah-cli`: glue surface that drives acquisition flows by invoking `ah-agents` extractors and persisting results via `ah-credentials`.

### Planned Crates (repository-aligned)

- `crates/ah-credentials/`: library-first crate for registry/storage, file format, encryption/key management, verification hooks, and passphrase caching APIs consumed by `ah-cli`, `ah-agent start`, and TUI. No agent-specific extraction logic lives here.
- `crates/ah-credentials-tests/` (optional): shared fixtures and mock-agent harnesses for integration tests to keep `ah-credentials` deps slim; mirrors the workspace pattern of test helper crates.

### Milestones

**M1. Storage Registry & Config Wiring**

- **Deliverables:**
  - Define `accounts.toml` schema and load/store layer (metadata, aliases, status, timestamps, `encrypted` flag) in a reusable crate (library-first, binary-free).
  - Implement file layout under `{config-dir}/credentials/` with `keys/` and `temp/` subdirectories; enforce owner-only permissions.
  - Integrate config defaults (`[credentials]` block) honoring precedence rules from Configuration.md, including `storage-path`, `default-accounts`, and `auto-verify-*`.
  - Validation routines for account names/aliases, agent type matching, and stale metadata cleanup.
- **Verification:**
  - [ ] Unit tests for TOML round-trip and schema validation (valid/invalid cases).
  - [ ] Permission tests ensuring created files/dirs are `0600`/`0700` on supported platforms.
  - [ ] Config precedence tests covering AH_HOME override and repo/user/system layering.
  - [ ] In-memory registry operations (add/update/delete/list) fuzzed for alias collisions and duplicate agents.

**M2. Encryption & Key Management** _(Status: Complete – Dec 4, 2025)_

- **Deliverables:**
  - AES-256-GCM encryption module with Argon2id passphrase derivation (PHC strings stored per account) and authenticated envelopes for credential payloads.
  - Session unlock cache (per-process) with secure zeroization and configurable inactivity timeout surfaced via config.
  - Key rotation flow supporting re-encryption of existing `*.enc` files with new passphrases/ciphers.
  - Optional plaintext support maintained for non-sensitive accounts; mixed encrypted/unencrypted coexistence.
- **Library guidance:** Use RustCrypto `aes-gcm` crate (audit by NCC Group) and pin to >=0.10.3 (or >=0.11 once stable) to avoid recent AEAD CVEs; enable the `zeroize` feature.
  - **KDF guidance:** Derive keys with RustCrypto `argon2` (Argon2id default) and store PHC strings with per-account salt; expose tunable params with secure defaults (e.g., m≥64 MiB, t≥3, p=1) informed by current hardening guidance.
- **Verification:**
  - [x] Crypto property tests (encrypt→decrypt round-trip, tag tampering detection, salt/iteration variation).
  - [x] Rotation tests migrating sample payloads between ciphers/keys without data loss.
  - [x] Memory hygiene tests (drop wipes buffers, no plaintext persisted on disk after operations).
  - [x] CLI unlock prompt mocked to ensure no repeated passphrase asks within a session cache window.

**M3. Credential Acquisition Pipelines** _(Status: Complete – Dec 5, 2025)_

- **Deliverables:**
  - Agent-specific acquisition helpers in `ah-agents` (Codex, Claude, Cursor) that create isolated temp HOME, launch the agent binary, wait for login completion, and return extracted credentials as key-value maps (per 3rd-Party-Agents specs).
  - `ah-credentials` remains agent-agnostic; it only provides the temp directory utilities and storage hooks used by the glue layer.
  - `ah-cli` glue that orchestrates acquisition: calls `ah-agents` helpers, enforces temp workspace cleanup (no residual credentials), and persists the returned key-value pairs via `ah-credentials`.
  - Pluggable extraction interface in `ah-agents` to support future agents plus detectors for success/failure and expiry metadata.
  - Verification hook (lives in `ah-agents` or `ah-cli` glue) to probe freshly acquired credentials against provider APIs/CLIs to populate `status/plan/limits`.
- **Verification:**
  - [x] Integration tests using mock agents and fixture credential stores to assert extraction paths and cleanup (in `ah-agents` + glue).
  - [x] Expiry detection tests (expired tokens rejected; fresh tokens marked active).
  - [x] Failure-path tests (login aborted, missing binaries) with actionable error messages.
  - [x] Concurrency test ensuring two acquisitions for different agents do not leak temp dirs or cross-contaminate HOME.

**M4. Account Management CLI Surface**

- **Deliverables:**
  - Implement CLI commands: `ah credentials add/list/remove/verify/reauth/encrypt/decrypt/encrypt-status`, following the formatting rules in CLI.md and confirmation/backup prompts in the spec.
  - JSON and human output with color/status indicators; `--compact` flag support.
  - Alias handling, default account inference, and clear errors for unknown/expired accounts.
  - Test log file generation per AGENTS testing guidelines.
- **Verification:**
  - [ ] Snapshot tests for help/usage output and list rendering (standard + `--compact`).
  - [ ] Command integration tests covering happy paths and error handling (missing account, double-remove, expired).
  - [ ] Encryption/decryption CLI tests with mocked passphrase input and status reporting.
  - [ ] Reauth flow test asserting updated `last_used` and refreshed credentials after acquisition rerun.

**M5. Agent Start/TUI/Health Integration**

- **Deliverables:**
  - Account resolution engine per spec (explicit account, task metadata, config defaults, most-recent active, interactive prompt).
  - `ah agent start` integration: account implies agent type; passes credentials to agent HOME setup and sandbox isolation hooks.
  - TUI advanced launch options dropdown wired to registry with status colors and preferred-account handling.
  - `ah health` augmentation reporting counts, per-account usage/limits, and recommendations.
- **Verification:**
  - [ ] Integration tests for `ah agent start` selecting correct account across combinations of flags/config/defaults.
  - [ ] TUI component tests (headless) ensuring dropdown lists filtered by agent and honors `preferred` rule.
  - [ ] Health command snapshot tests verifying grouping, status colors, and recommendation generation.
  - [ ] Sandbox/network policy test to confirm credentials only mounted/copied to intended session directories.

**M6. Auto-Verification & Monitoring**

- **Deliverables:**
  - Background scheduler using config-driven intervals (`auto-verify-interval`, `auto-verify-on-start`) to refresh status/limits.
  - Rate-limit/expiry telemetry collection and storage in registry metadata for UI/CLI display.
  - Audit logging of credential access, verify attempts, and failures with redaction per Logging-Guidelines.
- **Verification:**
  - [ ] Timer-driven tests simulating interval elapse; ensures throttling/backoff on failures.
  - [ ] Telemetry persistence tests (status and limits updated atomically; survives process restart).
  - [ ] Log redaction tests asserting no secrets appear in structured logs.

**M7. Migration & Compatibility**

- **Deliverables:**
  - Migration tool to import legacy single-account credentials into new layout, preserving paths/permissions and creating default labels.
  - Backward-compatible reads from legacy locations with deprecation warnings until migrated.
  - User prompts/guides integrated into CLI/TUI when legacy creds detected.
- **Verification:**
  - [ ] Migration integration test converting legacy files to new format with idempotent reruns.
  - [ ] Backward-compatibility test ensuring legacy paths still usable pre-migration.
  - [ ] UX snapshot tests for migration prompts in CLI and TUI.

### Cross-Spec Dependencies

- **Configuration.md**: config directory resolution, precedence, `AH_HOME` override, and credential-related keys.
- **CLI.md**: command formatting conventions and flag mapping.
- **TUI-PRD.md / Agent-Harbor-GUI.md**: account selectors in advanced launch options.
- **3rd-Party-Agents specs**: per-agent credential storage locations and login flows.
- **Logging-Guidelines.md & Sandboxing specs**: redaction rules and isolation requirements when handling credentials.

### Outstanding tasks

- Relocate the removed acquisition orchestration into `ah-agents` (helpers) and `ah-cli` (glue) per the new layering; add corresponding tests there.
- Wire `ah-cli` to call `ah-agents` extractors and persist via `ah-credentials`, then reintroduce acquisition integration tests in their new home.
