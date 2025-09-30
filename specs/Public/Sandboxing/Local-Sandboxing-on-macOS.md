## 1) Scope & goals (macOS‑specific)

- Provide a developer‑illusion environment on macOS: familiar paths, tools, and configs; host build caches usable in place.

- Maintain a hardened posture with no publicly known escape paths (defense‑in‑depth across AgentFS, chroot, Seatbelt, and Endpoint Security).

- Default to a dynamic read allow‑list for filesystem access (block‑until‑approved); offer a static RO/overlay mode for trusted sessions.

- Debugging enabled for in‑sandbox processes; configurable to disable.

- Internet egress disabled by default; opt‑in enablement; no inbound unless explicitly configured.

— macOS mechanisms —

- Filesystem isolation: FSKit‑backed AgentFS overlay with per‑process branch binding, chroot into the AgentFS root, and Seatbelt (SBPL) profile applied.

- Dynamic policy: Endpoint Security (ES) authorization for filesystem opens/execs, signals/trace (debug), and outbound network connects.

- Networking isolation options for localhost semantics (see section 7): fail with error, rewrite to alternative loopback device, or rewrite to alternative port.

---

## 2) Security & threat model

- Threat: potentially hostile tooling/code (from an AI agent or dependencies) attempting to exfiltrate secrets, modify the host, or interfere with other tenants.

- Objective: prevent writes outside approved locations; prevent reads of sensitive data without explicit approval; prevent process and network visibility/collision with the host.

- Defense‑in‑depth: AgentFS (view and write policy), chroot (path confinement), Seatbelt (deny‑by‑default capability surface), and ES (dynamic gating + auditing).

---

## 3) macOS prerequisites

- macOS 12+ recommended (Big Sur+ supported) with current security patches.

- Endpoint Security client entitlement (system extension) approved on the machine; extension runs as root and is enabled by the user.

- Container/host app for system extension activation; Full Disk Access granted to enable comprehensive file event visibility.

- Code signing and hardened runtime as required by Apple for system extensions; development provisioning profiles during development.

- FSKit host app + filesystem extension installed (for AgentFS mount control), or equivalent mechanism to mount AgentFS.

---

## 4) Isolation requirements (process view)

- Process actions are constrained so tools can interact primarily with in‑session processes.

- Seatbelt profile restricts process‑info APIs and inter‑process effects across the boundary.

- ES authorization enforces: signals allowed inside→inside only; debug/trace allowed only within the same cohort when debug mode is enabled.

- Supervisor maintains a cohort/session registry to classify processes (inside vs outside) for ES policy.

---

## 5) Filesystem policy requirements

### 5.1 Baseline

- Present the host filesystem at its usual locations to preserve developer muscle memory, overlaid by AgentFS.

- Flip the majority of the tree to read‑only within the AgentFS/seatbelt view; writable carve‑outs provided explicitly.

- Writable areas:
  - Project working directory/ies.
  - Language/tool caches (e.g., `$CARGO_HOME`, `$GOCACHE`, `$PIP_CACHE_DIR`, `$HOME/.cache/sandbox/*`).
  - Additional team/project‑specific paths as required.

### 5.2 Secrets & sensitive paths

- Default‑deny access to sensitive locations (e.g., `$HOME/.ssh`, `$HOME/.gnupg`, cloud credential dirs, keychains). Access only via explicit approval.

### 5.3 Dynamic read allow‑list (default mode)

- First access to a non‑allowed path blocks via ES AUTH_OPEN/AUTH_EXEC; supervisor prompts the human to approve/deny.

- Approval takes effect without restarting the target process; denial returns `EACCES/EPERM`.

- Approvals can persist at user / project / organization scopes with TTL and deterministic precedence.

### 5.4 Static RO/overlay mode (opt‑in)

- Provide a read‑only view with a configurable blacklist of sensitive directories and a configurable set of writable overlays.

- No interactive gating; intended for trusted or non‑interactive sessions.

---

## 6) Process & debugging requirements

- Debugging enabled by default, confined to in‑sandbox processes only (same cohort). Optional `--no-debug` disables it.

- ES gates AUTH_SIGNAL and AUTH_TRACE/GET_TASK; inside→outside signals/attach are denied. Same‑cohort actions allowed (when debug enabled).

- Seatbelt profile hardens process‑info and task access to complement ES.

---

## 7) Networking requirements and localhost strategies

### 7.1 Baseline networking policy

- Default: loopback only; no egress and no inbound.

- Opt‑in egress: allow outbound network (still default‑deny via ES with approvals and caching). No inbound unless explicitly configured.

- ES subscriptions: AUTH_SOCKET_CONNECT (outbound) and AUTH_SOCKET_BIND (listening/bind).

### 7.2 Localhost isolation strategies

- Strategy A — Fail with error (policy‑enforced):
  - Each sandbox session is assigned a listening port range on startup.
  - Environment exposes `LISTENING_BASE_PORT` (and optionally `LISTENING_PORT_COUNT`).
  - Optionally restrict listening to a specific loopback device exposed via `LISTENING_LOOPBACK_DEVICE` (e.g., `127.0.0.2`).
  - On `bind(2)` events outside the allowed port window or on disallowed loopback devices, ES denies the operation and the supervisor surfaces a concise error to the agent (and audit log). No rewrite occurs.

- Strategy B — Rewrite to alternative loopback device (dynamic loader injection via environment):
  - The launcher sets an environment variable to preload a small interpose library for `bind()` and `connect()`.
  - When programs attempt to listen on or connect to localhost, the interposed library rewrites the destination to the configured device (e.g., `127.0.0.2`).
  - The target device is provided via `LISTENING_LOOPBACK_DEVICE`; the library reads this on process start.
  - ES remains subscribed for defense‑in‑depth and auditing; localhost rewrites continue to satisfy policy without prompting.

- Strategy C — Rewrite to alternative port (dynamic port mapping):
  - The same injected library interposes `bind()`/`connect()` and consults a sandbox‑managed port map to rewrite to an alternative port.
  - The sandbox runtime maintains a shared memory file with `{original_port → mapped_port}` entries; the interposed library reads this mapping on demand.
  - The supervisor/launcher assigns and manages the port map per session (aligned with the session’s allocated port window). ES audits outcomes.

Notes:
  - Strategies B/C are mutually exclusive with A; they can be toggled via CLI/config.
  - Loopback device aliases (e.g., `lo0` alias `127.0.0.2`) must be preconfigured by the launcher when device‑based isolation is enabled.
  - For outbound non‑localhost traffic, ES default‑deny with user approvals (domain/IP caching) still applies when egress is enabled.

---

## 8) Package manager integration (Nix and others)

- For multi‑user Nix setups, bind `/nix/store` read‑only and the nix‑daemon socket into the sandbox; allow egress to substituters only when egress is enabled.

- Installing additional packages during a session must not risk store integrity; prefer daemon‑mediated, content‑addressed stores.

---

## 9) Nested virtualization (optional capability)

- Containers inside the sandbox: allowed via explicit toggle; never expose host control sockets; pre‑allow storage directories.

- VMs inside the sandbox: allowed via explicit toggle; prefer user‑mode networking; exposing hardware acceleration is opt‑in and called out explicitly.

---

## 10) Privilege model & hardening

- Prefer no‑sudo startup for the developer workflow; privileged steps are performed by short‑lived helpers or system extensions that drop privileges immediately.

- Seatbelt profile is applied after chroot and before `exec(2)` of the workload.

- ES extension enforces dynamic policy and is resilient to supervisor faults (failsafe default‑deny with audit).

---

## 11) Resource governance & quotas (macOS)

- Apply per‑session governance using a combination of:
  - POSIX resource limits for file descriptors, core size, and address space.
  - CPU throttling via process priority/QoS adjustments and periodic supervision.
  - Memory pressure monitoring with soft/hard thresholds; supervisor can terminate on breach.
  - Optional I/O throttling via controlled access to heavy I/O paths (AgentFS policy) and back‑pressure in the supervisor.

- Expose live metrics to the supervisor for observability.

---

## 12) Policy, supervisor & audit

- Supervisor mediates approvals, maintains policy stores (org → project → user → session) with deterministic precedence, and writes an append‑only audit log.

- Directory‑granularity prompt coalescing; LRU caches with TTL to minimize prompt noise.

- Non‑interactive mode defaults to deny.

---

## 13) CLI & configuration defaults (macOS)

```
aw sandbox [OPTIONS] -- CMD [ARGS...]

DESCRIPTION: Launch a macOS sandboxed session (AgentFS + chroot + Seatbelt + ES) and run CMD.

OPTIONS:
  --mode <dynamic|static>          Dynamic approvals (default) or static RO/overlay mode
  --no-debug                       Disable in‑sandbox debugging/attach
  --allow-network                  Enable outbound network (still policy‑gated)
  --network-strategy <strategy>    Localhost strategy: fail|rewire-device|rewire-port
  --listening-base-port <port>     Start of the allowed listening port window
  --listening-port-count <n>       Size of the listening port window (optional)
  --listening-loopback <ip>        Allowed loopback device (e.g., 127.0.0.2)
  --rw <PATH>...                   Additional read‑write paths
  --overlay <PATH>...              Paths made writable via overlay
  --blacklist <PATH>...            Sensitive directories hidden in static mode

ARGUMENTS:
  -- CMD [ARGS...]                 Command and arguments to run in the sandbox
```

Environment exposed to the workload:

- `LISTENING_BASE_PORT` and optional `LISTENING_PORT_COUNT` (when a port window is configured).

- `LISTENING_LOOPBACK_DEVICE` (when device‑based isolation is active).

---

## 14) Acceptance criteria (macOS)

- Filesystem policy verified: writes outside approved areas fail; secrets unreadable without approval; dynamic gating blocks/unblocks correctly; static mode honors blacklists/overlays.

- Process isolation verified: inside→outside signals denied; same‑cohort signals allowed; debug attach limited to in‑sandbox when enabled.

- Networking isolation verified:
  - Default: no egress; localhost only.
  - Strategy A: `bind()` outside the assigned port window or on disallowed device is denied with a clear error; audit recorded.
  - Strategy B: localhost binds/connects are rewritten to the configured loopback device; applications operate normally; audit records the rewritten destination.
  - Strategy C: binds/connects are rewritten per port map; connections succeed on remapped ports; audit records mapping.

- CLI E2E: run, approve, deny, persist; teardown leaves no residue (mounts, processes, and limits removed deterministically).

---

## 15) Design & mechanics — architecture

Components

- Launcher (Rust): Mounts AgentFS via FSKit, chroots to AgentFS root, applies Seatbelt profile, sets environment, and execs the workload. Manages dynamic loader injection when rewrite strategies are enabled.

- FSKit host app + filesystem extension: Provides AgentFS overlay with per‑process branch binding and XPC control.

- Endpoint Security system extension: Subscribes to AUTH_OPEN, AUTH_EXEC, AUTH_SIGNAL, AUTH_TRACE/GET_TASK, AUTH_SOCKET_CONNECT, and AUTH_SOCKET_BIND; enforces policy and communicates with supervisor for approvals.

- Supervisor (UI/daemon): Prompts the human, merges policy stores, maintains caches and audit logs, and manages the port mapping shared memory file for Strategy C.

High‑level flow

1. CLI parses config (defaults + project overrides) and determines localhost strategy.

2. Launcher mounts AgentFS, chroots, applies Seatbelt, sets env (including `LISTENING_*`), configures dynamic loader injection if needed, and execs the workload.

3. ES mediates gated operations; supervisor prompts and records decisions; caches reduce prompt volume.

4. Teardown removes mounts, processes, and limits; audit preserved.

---

## 16) AgentFS + Seatbelt integration

- AgentFS presents a mostly read‑only view with explicit writable carve‑outs; per‑process branches ensure isolation across concurrent sessions.

- Launcher applies a minimal SBPL profile (deny by default) with filesystem allowances for AgentFS paths and essential system operations; optional loopback‑only network when egress is disabled.

---

## 17) Dynamic read allow‑list — implementation (macOS)

- ES blocks the calling thread for AUTH_OPEN/EXEC.

- Path canonicalization is performed relative to the AgentFS root to defeat symlink/`..` tricks.

- Supervisor consults merged policy and LRU caches; unknown paths prompt the user; allow/deny decisions unblock the thread.

- Directory‑granularity approvals coalesce prompts; kernel caching flags remain off in favor of user‑space caches.

---

## 18) Networking mechanics — ES + rewrite library

- ES enforcement:
  - Outbound: AUTH_SOCKET_CONNECT default‑deny (except loopback); approvals can persist by domain/IP with TTL; loopback auto‑allow.
  - Listening: AUTH_SOCKET_BIND policy enforces the session’s port window and (optionally) allowed loopback device.

- Rewrite library (when enabled):
  - Injected via the dynamic loader into all workload processes launched by the sandbox.
  - Interposes `bind()` and `connect()` to apply either device rewrites (Strategy B) or port rewrites (Strategy C).
  - Reads `LISTENING_LOOPBACK_DEVICE` and the session port map shared memory file; fails closed if the map is unavailable.
  - Ad‑hoc code signing acceptable for development; production uses hardened signing aligned with the launcher.

---

## 19) Port management

- The supervisor allocates a non‑overlapping port window per session; conflicts are prevented across concurrent sessions.

- For Strategy C, a per‑session port map is created and kept in shared memory backed by a file under the session state directory; updates are atomic.

- Collisions with host daemons are avoided by assigning windows from a reserved high‑port pool.

---

## 20) Supervisor protocol (events relevant to macOS)

Transport: UNIX domain socket; newline‑delimited JSON.

Messages (additions):

- `event.net_request { id, pid, exe, op: "connect|bind", addr, port, is_loopback }`

- `cmd.approve { id, scope: "once|project|user|org", persist: bool }`

- `cmd.deny { id }`

- `event.audit { id, decision, scope, ts }`

- `event.net_rewrite { pid, op, from: {addr,port}, to: {addr,port} }` (when B/C are enabled)

Timeouts yield default‑deny with a clear UI message; all decisions are audited.

---

## 21) Logging & telemetry

- Structured logs (JSON) for launcher, ES extension, and supervisor including session id, pid, op, path/addr, decision, latency.

- Metrics: ES decision rates, cache hit rates, prompt counts, resource usage; exposed to the CLI and UI.


