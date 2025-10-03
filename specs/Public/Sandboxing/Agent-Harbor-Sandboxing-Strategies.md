> **Scope:** Blocksense **agents‑workflow** project. The primary "developer" is an **AI agent**, with humans able to enter sessions for manual testing. This document captures the **cross‑strategy, platform‑agnostic requirements** for sandboxed development. Strategy‑specific designs live in separate documents (e.g., _Local Sandboxing on Linux_).

## Strategies in scope

- **Local Sandbox** (namespaces/OS primitives; no Docker): detailed separately.
- **Docker Devcontainer** (containerized dev env; VS Code/`devcontainer.json` compatible).
- **Virtual Machine** (lightweight VM or desktop VM image for dev).
- **No Sandbox** (operate in a pre‑sandboxed environment such as a corporate VDI, managed VM, or hosted Codespaces‑style setup).

---

## High‑level requirements (common to all strategies)

### 1) Security posture

- **No publicly known sandbox escapes**: defense‑in‑depth; apply least privilege everywhere.
- Depend on **up‑to‑date** OS/runtime with security patches.
- Clear **threat model**: potentially hostile tools/code; protect host, secrets, and other tenants.

### 2) Developer illusion & UX

- Environment should feel like the machine of the developer. Files and programs should be in their familiar location. Utilities should work with the usual configuration that the developer uses outside of the sandbox. Cached build artifacts on the host system should be immediately available in the sandbox under the same path.
- **Fast start/stop**, minimal friction; all flows automated by project tooling.
- **Interactive approvals** for first‑time resource access, with optional **persisted policy** for later sessions.

### 3) File-system semantics

- **Read‑only baseline** of the host/system image.
- **Writable working copy** and **writable package manager caches** which will be discarded when the session ends.
- **Sensitive areas shielded** by default (e.g., credentials); access only on explicit approval.
- **Dynamic read allow‑list (default)**: first access to a non‑allowed path **blocks** until the develop interactively approves or denies the access; Access can be granted without restarting the sandbox session. The developer choice can be saved in a user, project or company configuration file.
- **Relaxed model with blacklists (opt-in):** read‑only view with a configurable **blacklist** for sensitive directories and a configurable set of writable overlays; no interactive gating. We provide reasonable defaults for both lists.

### 4) Process & debugging

- **Process isolation** so tools see only session processes in commands like `ps` and `kill`.
- **Debugging supported**: allow tracing/inspection **within the sandbox** only; This is enabled by default, but a configuration option might disable it.

### 5) Networking

- **Isolated networking** to avoid port clashes and cross‑tenant visibility.
- **Egress off by default** (policy‑controlled) with simple **opt‑in** to enable; **no inbound** unless explicitly configured.

### 6) Resource governance

- Enforce **CPU/memory/pids/IO** limits on a per‑session basis to contain runaway builds/tests.

### 7) Nested Virtualization

- Allow launching **containers** and **VMs** _inside_ the session without exposing host control sockets or excessive devices; We provide separate configuration options for each.

### 8) Privilege model

- Prefer **no‑sudo startup**; any privileged steps happen in a short‑lived helper that drops privileges immediately.
- Drop **capabilities**/elevations for workloads; keep the supervisor **outside** the sandboxed context.

### 9) Platform integrations

- Safe package installation mechanisms (e.g., daemon‑mediated installers such as the ones used in Nix and Spack) may be **allowed** when they do not compromise host integrity.
- Opt‑in bindings for developer agents (SSH/GPG), GUIs, or IDE integrations.
- **Deterministic teardown** that removes mounts, processes, and limits.

---

## Acceptance criteria (applies to any chosen strategy)

- Launch/teardown completes in seconds; leaves no host residue.
- Default policy prevents writes outside approved locations; **no secrets readable** without explicit approval.
- Debugging of in‑session processes works; host processes remain invisible/inaccessible.
- Enabling internet egress or container/VM support is a **conscious, explicit action** documented by the tooling.
- Clear test matrix validating isolation (process, filesystem, network) and resource limits.
