## Prompt Engineering — Automatic Instruction Injection

### Purpose

Document when and how Agent Harbor automatically injects instructions into agent prompts to ensure consistent behavior across features like Time‑Travel, Multi‑OS testing, credential use, and safe execution.

### Core Principles

- Keep injections minimal, explicit, and idempotent.
- Prefer declarative guidance (what to do, not how) with concrete command examples.
- Surface all injected text to the user (auditability) via logs and session recording.

### Injection Sites

1. Session Start (Boot Strap)
   - Goal: establish conventions (working directory, shell, editor policy), and safety guarantees (sandbox, no destructive host ops outside workspace).
   - Content (illustrative):
     - "Always operate within the workspace root."
     - "Use provided commands/scripts; do not re‑invent environment setup."
     - Credential usage rules (see per‑agent docs in `docs/agents/`).

2. Before Command Execution (Time‑Travel Hooks)
   - Goal: bind edits and commands to SessionMoments/FsSnapshots for reproducibility.
   - Content: "Between editing and running tools, a filesystem snapshot will be taken automatically; do not run concurrent background processes that modify the tree during snapshot."

3. Multi‑OS Testing (run‑everywhere)
   - Trigger: When multi‑OS is enabled, after `fs_snapshot_and_sync` the agent is instructed to use `ah agent followers run`.
   - Content (illustrative excerpt):
     - "To validate across OSes, run `.agents/run_everywhere <action> [--host <name>|--tag <k=v>|--all]`."
     - "Examples: `.agents/run_everywhere test --all`; `.agents/run_everywhere build --tag os=windows`."
     - "Do not invoke platform‑specific commands directly on followers; `run_everywhere` returns the per‑host outputs to you."

4. Delivery Policy (PR/branch/patch)
   - Goal: align with chosen delivery mode.
   - Content: strict steps for commit hygiene, test gating, and PR formatting.

5. Credential Usage
   - Goal: ensure tools pick up auth from env/hosts files mounted by the executor; avoid asking the user to paste secrets.
   - Content: name the env vars and file paths (read‑only) approved for use.

6. Safety and Idempotence
   - Goal: avoid destructive operations outside the workspace; favor idempotent scripts.
   - Content: "Do not edit files outside the workspace root. Do not alter global system settings."

### Injection Mechanics

- The runner composes a prompt preamble from active features and project config and prepends it to the user’s instruction.
- Injections are tagged in the session timeline and recorded for audit.
- Preamble is cached per session and updated when features toggle (e.g., enabling multi‑OS).

### Examples (Abbreviated)

Multi‑OS enabled session preamble snippet:

```
You are working in a sandboxed workspace on the leader. After edits, a filesystem snapshot and sync fence will run. To validate across OSes, use:
  ah agent followers run [--host <name>|--tag <k=v>|--all] [--] <command>
Examples:
  ah agent followers run -- test
  ah agent followers run --tag os=windows -- build
Do not execute platform-specific commands directly on followers; the followers run command returns per-host output here.
```

Time‑Travel snapshotting snippet:

```
Between editing and running tools, the system captures an FsSnapshot tied to the upcoming command. Avoid background mutations during this phase.
```

### Open Items

- Per‑agent tailored injections (see `docs/agents/`), including auth scopes and rate‑limit guidance.
- Localization of injections for international teams.
