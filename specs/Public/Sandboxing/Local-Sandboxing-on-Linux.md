## 1) Scope & goals (Linux‑specific)

- Provide a **developer‑illusion** environment: paths, tools, and configs appear in their familiar locations; host build caches are immediately usable under the same paths.

- Maintain a **hardened posture** with **no publicly known escape paths** (defense‑in‑depth across namespaces, mount policy, seccomp, cgroups). Kernel 0‑days are out of scope but we require a current, patched LTS.

- **Dynamic read allow‑list** by default (block‑until‑approved); **static RO/overlay** mode as an opt‑in simplification.

- **Debugging enabled** for in‑sandbox processes; configuration allows disabling it.

- **Internet egress disabled** by default; opt‑in enablement; no inbound unless explicitly configured.

- Prefer **no‑sudo startup** via user namespaces; provide a minimal privileged fallback where required.

---

## 2) Security & threat model

- Threat: potentially hostile tooling/code (from an AI agent or dependencies) attempting to exfiltrate secrets, modify the host, or interfere with other tenants.

- Objective: prevent writes outside approved locations; prevent reads of sensitive data without explicit approval; prevent process and network visibility/collision with the host.

- Requirement: deploy on a **supported LTS kernel** with timely security fixes; enable distro settings needed for unprivileged user namespaces.

---

## 3) Kernel/OS prerequisites

- **User namespaces** enabled (for no‑sudo path): `kernel.unprivileged_userns_clone=1` or distro equivalent.

- **cgroup v2** available and mounted.

- **Seccomp** with user notifications supported.

- **`openat2(2)`** (for `RESOLVE_*` flags) available; recommend ≥ 5.11.

- **Overlayfs** available for optional writable overlays.

- **slirp4netns** (or equivalent) present for unprivileged egress when enabled.

---

## 4) Isolation requirements (namespaces)

- **PID namespace**: processes inside must only see/affect each other. A private `/proc` is mounted inside the PID ns; commands like `ps`, `kill` are scoped to the sandbox.

- **Network namespace**: sockets and ports belong to the sandbox only; binding `:PORT` cannot collide with host daemons; `ss/netstat` reflect only the sandbox stack.

- **Mount namespace**: a private VFS view; global tree presented mostly **read‑only** with explicit writable carve‑outs/overlays. Apply `nodev`, `nosuid`, `noexec` where appropriate.

- **UTS namespace**: custom hostname to signal session context.

- **IPC namespace**: isolate SysV IPC/POSIX MQ.

- **Time namespace** (optional): reproducible time per session.

- **devpts instance**: private ptys (`newinstance`).

---

## 5) Filesystem policy requirements

### 5.1 Baseline

- Present the host filesystem in its usual locations to preserve developer muscle memory.

- Flip the majority of the tree to **read‑only** (recursively) inside the mount namespace.

- Bind **explicit writable areas**:
  - Project working directory/ies.

  - Language/tool caches (e.g., `$CARGO_HOME`, `$GOCACHE`, `$PIP_CACHE_DIR`, `$HOME/.cache/sandbox/*`).

  - Additional team/project‑specific paths as required.

- Optionally use **overlayfs** to make select paths appear writable while persisting to a per‑session upperdir.

### 5.2 Secrets & sensitive paths

- Default‑deny access to known sensitive locations (e.g., `$HOME/.ssh`, `$HOME/.gnupg`, cloud credential directories, password/keyring stores). Access only via explicit approval.

### 5.3 Dynamic read allow‑list (default mode)

- First access to a non‑allowed path **blocks**; the supervisor prompts the human to **approve/deny**.

- Approval takes effect **without restarting** the target process; the access unblocks and succeeds. Denial returns `EACCES/EPERM`.

- Approvals can be persisted to **user / project / organization** policy stores.

### 5.4 Static RO/overlay mode (opt‑in)

- Provide a **read‑only** view with a configurable **blacklist** of sensitive directories and a configurable set of **writable overlays**.

- No interactive gating; intended for trusted or non‑interactive sessions.

---

## 6) Process & debugging requirements

- **Debugging enabled by default**; must be confined to **in‑sandbox processes only**.

- Support `ptrace`/debuggers under constraints:
  - PID + user namespaces scope tracing to the sandbox.

  - Host `yama` setting respected (e.g., `ptrace_scope=1`); `PR_SET_PTRACER` used as needed.

  - Option to disable debugging globally (`--no-debug`) or per session.

- Prevent tracing/inspection of the supervisor/helper (they run outside and are non‑dumpable).

---

## 7) Networking requirements

- **Default:** loopback only; **no egress** and **no inbound**.

- **Opt‑in egress:** provide NAT via **slirp4netns** (no‑sudo) or veth/bridge (privileged setup); still no inbound by default.

- Policy must allow per‑session enabling/disabling and optional egress filtering (domain/IP allow‑lists).

---

## 8) Nix & package manager integration

- In **multi‑user Nix** setups, bind `/nix/store` **read‑only** and the nix‑daemon socket into the sandbox; allow egress to substituters when egress is enabled. Installing additional packages during a session **must not** risk store integrity.

- Similar principle for other daemon‑mediated content‑addressed stores (e.g., Spack with a shared cache): allow only the safe daemon pathways.

---

## 9) Nested virtualization (optional capability)

- **Containers inside** the sandbox (rootless runtimes): allowed via an explicit toggle; must **not** bind the host Docker socket. Provide `/dev/fuse`, delegate a cgroup v2 subtree, and pre‑allow storage directories.

- **VMs inside** the sandbox: allowed via an explicit toggle; prefer QEMU user‑mode networking. If exposing `/dev/kvm`, call out the increased kernel attack surface; keep other devices blocked.

---

## 10) Privilege model & hardening

- **No‑sudo path** preferred: perform namespace/mount setup inside a user namespace; install seccomp with `NO_NEW_PRIVS`.

- **Fallback:** a minimal setuid or one‑shot `sudo` helper may be used when userns is unavailable or for veth/advanced mount APIs; must drop privileges immediately after setup.

- **Seccomp policy (allow‑list):**
  - Permit common dev syscalls; **deny/notify** risky ones (e.g., module load, kexec, `bpf`, raw mount APIs, `open_by_handle_at`, namespace creation beyond initial setup, `process_vm_*` except in debug mode).

- **Capabilities:** drop to minimal or empty set for workloads after setup; clear ambient.

- **/sys** and `/proc/sys`: not mounted or mounted RO; kernel tunables not writable from inside.

- **/dev**: minimal nodes only; mount flags `nodev,nosuid,noexec` widely.

---

## 11) Resource governance & quotas

- Place sandbox processes in a **dedicated cgroup v2** subtree with limits:
  - `pids.max` to prevent fork bombs.

  - `memory.max`/`memory.high` to contain runaway builds.

  - `cpu.max` and optional IO throttling for fairness.

- Expose session metrics to the supervisor for observability.

---

## 12) Policy, supervisor & audit

- **Supervisor** (outside the sandbox) mediates approvals, maintains policy files, and writes an **append‑only audit log** (who/what/when/decision).

- Policy stores: per‑user, per‑project, and organization scopes with predictable merge order.

- Provide import/export and dry‑run modes for policy changes.

---

## 13) CLI & configuration defaults (Linux)

- **Defaults:**
  - Mode = **Secure‑Dynamic** (interactive read allow‑list).

  - Debugging = **enabled** (scoped to sandbox).

  - Internet egress = **disabled**.

  - Containers/VMs = **disabled**.

- **Toggles:** `--no-debug`, `--allow-network`, `--containers`, `--vm`, `--static` (enables RO/overlay mode), `--rw <path>`, `--overlay <path>`, `--blacklist <path>`, `--tmpfs-size <SIZE>`.

- Config files: user‑level, project‑level (checked into repo as needed), org‑level baseline.

---

## 14) Acceptance criteria (Linux)

- Process isolation verified: host PIDs invisible/inaccessible; signals confined.

- Network isolation verified: same‑port binds possible without conflict; `ss` shows only sandbox sockets; egress/inbound toggles behave as configured.

- Filesystem policy verified: writes outside approved areas fail; secrets unreadable without approval; dynamic gating blocks/unblocks correctly; static mode honors blacklists/overlays.

- Debugging verified: `ptrace` works **only** in‑sandbox; supervisor non‑attachable.

- Nix integration verified: installing packages does not modify `/nix/store` directly and does not require writable system paths; daemon mediation works when egress enabled.

- Containers/VMs (if enabled) verified: storage devices limited; no host control sockets; resource limits enforced.

---

## 15) Design & Mechanics — Architecture

**Components**

- **Rust helper (launcher):** Creates namespaces, mounts, cgroups; installs seccomp; starts the target (or shell) as PID 1 of the sandbox; proxies dynamic file approvals.

- **Supervisor (Ruby UI/daemon):** Presents interactive prompts; persists policy (user/project/org scopes); emits audit logs; can attach/inspect running sessions.

- **Target workload:** AI agent tools and developer shells; confined to the sandbox.

**High‑level flow**

1. `sandbox run` parses config (defaults + project overrides).

2. Helper unshares namespaces and sets up mounts/devices/cgroups.

3. Helper installs seccomp filters and opens the **user‑notif** listener.

4. Helper execs the target entrypoint; supervisor listens for events and mediates approvals.

---

## 16) Namespace bootstrap (step‑by‑step)

**Note:** User namespaces are created first and enable unprivileged processes to become "root" within the namespace, allowing subsequent privileged operations (mounts, etc.) without requiring CAP_SYS_ADMIN in the parent namespace.

1. **Userns**: `clone3(CLONE_NEWUSER)` → write UID/GID maps (using `newuidmap/newgidmap` if required). Set `no_new_privs` early. **The process now has root privileges within this namespace.**

2. **Mount ns**: `unshare(CLONE_NEWNS)` → mark `/` **private** (no propagation).

3. **PID ns**: `unshare(CLONE_NEWPID)` → Execute fork(). The child process becomes **PID 1** inside the user namespace; Parent remains as orchestrator until hand‑off.

4. **UTS/IPC/Time**: `unshare(CLONE_NEWUTS|CLONE_NEWIPC|CLONE_NEWTIME)`; set hostname; optionally skew time.

5. **Net ns**: `unshare(CLONE_NEWNET)`; bring up `lo`. If `--allow-network`, start **slirp4netns** attached to the target PID; optional port‑forwards.

6. **/proc**: mount a **new procfs** inside the child (which still holds CAP_SYS_ADMIN as PID 1 in the user namespace). This is needed for correct `ps/kill`. Use `hidepid=2,subset=pid` if supported.

7. **devpts & /dev**: Similarly, mount **devpts** in the child (`newinstance,gid=tty,mode=0620,ptmxmode=0666`); mount a minimal **/dev** (tmpfs) and populate required nodes (`null,zero,urandom,tty,ptmx,pts/*`).

8. **tmpfs** (in child): mount tmpfs for `/tmp` and `/run` (per‑session private state). Use configured `--tmpfs-size` for `/tmp` capacity (default 256MB).

9. **Filesystem sealing** (executed within user namespace context):
   - Make the existing mount tree **read‑only** recursively using `mount_setattr(AT_RECURSIVE, MS_RDONLY)` if available; otherwise bind‑remount each subtree RO.

   - Apply `nodev,nosuid,noexec` broadly (except where toolchains require exec).

10. **Writable carve‑outs** (executed within user namespace context): bind‑mount project dirs and caches **read‑write**; create optional **overlayfs** upperdirs for paths that need in‑place writes (e.g., `/usr/local`, `~/.local/share`).

11. **Cgroup v2** (executed within user namespace context): create a per‑session subtree; set `pids.max`, `memory.high/max`, `cpu.max`, optional IO throttles. If `--containers`, delegate controllers to allow rootless runtimes to create children.

12. **Drop privileges**: clear ambient caps; tighten bounding set; set securebits (keep caps off across exec); remain root‑in‑ns only if needed.

13. **Exec entrypoint**: replace PID 1 with the target or a login shell.

---

## 17) Filesystem plan — algorithm

- **Inputs**: list of RW paths, overlay paths, blacklist (static mode), secrets list (default‑deny), policy store(s).

- **RO sealing**: compute mount graph; flip to RO with a single recursive call when possible.

- **RW binds**: for each RW path, create directories, ensure ownership (ID‑mapped mounts optional), then `--bind` and remount `rw`.

- **Overlays**: for each overlay path, build `upper,work` dirs under the session state; mount overlay with lowerdir=host path.

- **Secrets**: ensure blacklists/hidden paths are not reachable via alternate bind points (defend against path aliases); avoid `/proc/self/mounts` remount tricks by keeping the mount ns sealed.

---

## 18) Dynamic read allow‑list — implementation

**Intercepted syscalls (notify)**: `open, openat, openat2, stat, statx, fstatat, access, faccessat2, execve, execveat` (plus optional `linkat/renameat` when reads imply metadata traversals).

**Event handling**

1. When a gated syscall fires, the kernel **blocks the calling thread** and delivers a message to the helper’s seccomp listener.

2. Helper resolves a **canonical path** relative to the sandbox root using `openat2()` on `/proc/<pid>/cwd` with `RESOLVE_BENEATH|RESOLVE_NO_MAGICLINKS|RESOLVE_IN_ROOT`.

3. Helper consults merged policy (org → project → user) and an in‑memory **LRU cache** of recent decisions.

4. If unknown, helper sends `{pid, exe, cwd, op, path}` to the supervisor and **waits**.

5. **Approve**:
   - For `open*`: perform **proxy open** using `openat2()` with the same flags against the sandbox root; inject the FD into the blocked task via `SECCOMP_IOCTL_NOTIF_ADDFD` (with `SEND` if supported) so the original syscall returns success with that FD.

   - For `stat*/access/execve*`: reply **allow**; kernel replays and completes the syscall.

6. **Deny**: reply with `errno=EACCES` (or `EPERM`).

7. **Persist** (optional): supervisor writes the rule to the chosen scope.

**Performance controls**

- Coalesce prompts at **directory granularity** when appropriate (approve `/opt/sdk/include/**`).

- Pre‑seed allow‑lists for language runtimes (e.g., `/usr/lib`, dynamic linker, compiler toolchains).

- Exempt high‑churn dirs (e.g., `/proc`, `/sys`, caches) from gating.

**Correctness notes**

- Avoid trusting user pointers or string paths from the tracee; always resolve via `openat2()` anchored at the tracee’s root/cwd.

- Handle **shebang** and dynamic loader: ensure `execve*` gating looks up the interpreter path and loader reads.

- Once an FD is granted, subsequent `read()`/`mmap()` on that FD are not re‑gated (by design); policy must be enforced at **open time**.

---

## 19) Debugging mechanics

- **Default**: ptrace allowed **within** the sandbox. Enforce with PID + user namespaces; mount `/proc` inside; host processes remain invisible.

- **Yama**: keep `ptrace_scope=1`; when attaching to non‑children, require the tracee to set `PR_SET_PTRACER`.

- **Seccomp toggles**:
  - Normal: deny `ptrace`, `process_vm_*`.

  - Debug mode: allow `ptrace` (and optionally `process_vm_*`) for the session; keep other risky syscalls denied.

- **Supervisor/launcher**: outside the sandbox and **non‑dumpable** to prevent attachment.

---

## 20) Networking mechanics

- **Default**: only `lo` is up; no routes except local.

- **Egress (no‑sudo)**: spawn `slirp4netns <sandbox-pid>`; set DNS (e.g., bind‑mount `/etc/resolv.conf` into the netns) and optional per‑session egress allow‑list enforced by slirp or by in‑ns nftables rules.

- **Egress (privileged option)**: set up `veth` → bridge → NAT on host; keep firewall rules scoped to the netns; optionally expose specific inbound ports via DNAT.

- **Observability**: `ss`, `/proc/net/*`, and `iptables` inside the ns reflect only session sockets/routes.

---

## 21) Cgroups v2 layout

- Create `/sys/fs/cgroup/sbx/<id>/` as the session root.

- Set `pids.max`, `memory.max`/`memory.high`, `cpu.max`; optional `io.max` for image/VM directories.

- If `--containers`, enable delegation by writing `+pids +cpu +memory` to `cgroup.subtree_control` and chowning the subtree to the mapped UID.

- Expose usage metrics via procfs/cgroupfs to the supervisor for live charts and kill thresholds.

---

## 22) Nix integration details

- Bind `/nix/store` **read‑only**; bind the nix‑daemon socket inside the sandbox.

- When egress is enabled, allow connections to substituters/cache endpoints.

- Ensure builds don’t require global writable paths; point `$XDG_CACHE_HOME` and tool caches to RW carve‑outs.

---

## 23) Containers & VMs inside the sandbox — mechanics

**Containers mode**

- Use rootless Podman/Docker; require `/dev/fuse`, delegated cgroup subtree, pre‑allowed storage dirs (`$XDG_DATA_HOME/containers` or rootless Docker dirs`).

- Whitelist runtime binaries and their storage paths to minimize gating noise; still prohibit host control sockets.

**VM mode**

- QEMU with `-netdev user` by default; optional pass‑through of `/dev/kvm` when `--allow-kvm` is set; never expose arbitrary devices.

- Pre‑allow VM images dir; throttle via cgroups; keep snapshots in RW area.

---

## 24) Privilege & capability management

- After setup, drop all capabilities from the target’s bounding set (or to a minimal set if tools require specific caps in‑ns).

- Set securebits to prevent cap regain across exec; keep `no_new_privs` set.

- Keep the helper small; any privileged code path must drop to unprivileged immediately after completing its work.

---

## 25) Supervisor protocol (wire format)

**Transport**: UNIX domain socket; newline‑delimited JSON.

**Messages**

- `event.fs_request { id, pid, exe, cwd, op, path, flags }`

- `cmd.approve { id, scope: "file"|"dir", persist: bool }`

- `cmd.deny { id }`

- `event.audit { id, decision, scope, ts }`

- `cmd.policy.save { scope: user|project|org }`

**State machine**

- Requests are **exclusive** (one decision unblocks one thread). Timeouts → default‑deny (configurable) with a clear UI.

---

## 26) CLI — initial spec

```
sandbox run [--mode dynamic|static] [--no-debug] [--allow-network] \
            [--containers] [--vm] [--allow-kvm] \
            [--rw PATH ...] [--overlay PATH ...] [--blacklist PATH ...] -- CMD [ARGS...]

sandbox attach <SID>         # open shell in running session
sandbox ps <SID>             # list processes inside session
sandbox kill <SID>           # terminate session
sandbox policy import|export # manage policy stores
sandbox audit <SID>          # show decisions
```

---

## 27) Logging & telemetry

- Structured logs (JSON) for helper and supervisor; include session id, pid, op, path, decision, latency.

- Metrics: cgroup CPU/mem/IO usage, seccomp decision rates, prompt counts; expose via socket or files.

---

## 28) Test harness (mechanics‑focused)

- **Mount plan**: verify RO sealing and RW carve‑outs; overlays behave correctly.

- **Seccomp**: unit tests for `openat2` resolution and ADDFD injection; race/TOCTOU cases.

- **Ptrace**: ensure attach limited to in‑ns; supervisor non‑attachable.

- **Network**: same‑port bind tests; egress toggle; DNS resolution inside ns.

- **Cgroups**: limits enforced; OOM behavior graceful; fork‑bomb contained.

- **Nix/containers/VMs**: end‑to‑end scenarios respecting toggles and restrictions.

## Rationale

### Why binding the host Docker socket is dangerous

Binding `\/var\/run\/docker.sock` into the sandbox hands the sandboxed process **full, host‑level control** of the Docker daemon, which typically runs as **root**. Through the Docker API, a sandboxed process can:

- **Launch privileged containers** (`--privileged`, `--cap-add=ALL`), bypassing your sandbox’s seccomp/capability limits.

- **Mount the host filesystem** into a container (`-v /:/host`), read/write arbitrary host files, and exfiltrate secrets.

- **Join host namespaces** (`--pid=host`, `--net=host`) to inspect/affect host processes and networking, defeating PID/net isolation.

- **Access devices and kernel interfaces** (e.g., `--device`, `--security-opt`), potentially escalating to kernel compromise.

- **Bypass your gating logic:** all sensitive operations are performed by the **daemon outside the sandbox**; your in‑sandbox seccomp/fanotify policies do not apply to the daemon’s actions.

**Safer alternatives:** run **rootless containers** (Podman or Docker‑rootless) _inside_ the sandbox; or use a **policy‑enforced remote builder** with strict authentication and no privileged flags.
