---
status: Early-Draft, Needs-Expansion
---

Sandbox profiles define how local executions are isolated. They are orthogonal to UI and to local/remote mode. The profile is resolved from config or flags and determines the executor that hosts the agent and its per‑task workspace.

See [agent-harbor-Sandboxing-Strategies](Sandboxing/agent-harbor-Sandboxing-Strategies.md) for cross‑platform requirements and design principles that apply to all sandboxing approaches.

Why sandboxes (threats and safety):

- Accidental breakage of the host (e.g., `rm -rf /`, package manager changes, daemon starts).
- Prompt‑injection induced exfiltration or persistence beyond the per‑task workspace.
- Network egress controls and secret hygiene (limit where credentials are visible and what endpoints are reachable).
- Determinism: immutable base layers with copy‑on‑write upper layers make runs reproducible and easy to clean up.

Baseline requirements:

- Per‑task workspace must be isolated from the real working tree (snapshot + CoW or equivalent).
- No writes outside the workspace; only approved read‑only mounts (e.g., credential stores) when needed.
- Non‑root execution whenever possible; explicit elevation required and audited when unavoidable.

Profile types (predefined):

- container: OCI container (Docker/Podman). Options include image, user/uid, mounts, network, seccomp/apparmor.
- vm: Lightweight Linux VM (Lima/Colima, Apple Virtualization.framework, WSL2/Hyper‑V). Options include image, resources, networking.
- local: Local process sandbox using OS namespaces and primitives (Linux: user namespaces, cgroups v2, seccomp with dynamic file access control). See [Local-Sandboxing-on-Linux](Sandboxing/Local-Sandboxing-on-Linux.md) for detailed Linux implementation. Cross-platform support via equivalent isolation primitives.
- firejail: Linux Firejail profile with caps/seccomp filters.
- disabled: Run directly on host (policy‑gated, for already isolated environments like dedicated VMs).

Configuration:

- See [Configuration](Configuration.md) for `[[sandbox]]` entries (name, type, and options) and selecting a profile via `--sandbox`/fleet or by name in config.

Notes:

- Snapshot preference and workspace mounting are described in [FS-Snapshots/FS-Snapshots-Overview](FS-Snapshots/FS-Snapshots-Overview.md). In fleets, snapshots are taken on the leader host only.
