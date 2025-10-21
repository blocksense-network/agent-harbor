## Multi‑OS Testing — Leader/Followers, Sync, and run-everywhere

### Summary

Enable agents to validate builds and tests across multiple operating systems in parallel with a simple, reliable flow:

- One of the executors acts as the leader workspace (typically a Linux executor, due to the stronger support for [CoW Snapshots](FS-Snapshots/FS-Snapshots-Overview.md)).
- One or more follower workspaces (macOS, Windows, Linux) mirror the leader via Mutagen high‑speed file sync.
- Each execution cycle fences the filesystem state (FsSnapshot + sync).
- The agent on the leader can drive the execution of arbitrary commands (typically tests) on all executors via instructions to run the `ah agent run-everywhere` command.

### Goals

- Deterministic, low‑latency propagation of file changes from leader to followers.
- Atomic test execution view based on a consistent leader FsSnapshot.
- Simple project integration via a single `run-everywhere` entrypoint and tagging.
- Minimal OS‑specific logic inside agents; orchestration handled by the executor.
- Avoid the complexity of filesystem snapshots on followers. The snapshots of the leader are sufficient to restore any filesystem state on the followers as well.

### Terminology

- **End-User**: The user invoking the `ah task` command and the software components being part of this executable.
- **Executor Access Point**: A server started with `ah serve` that offers the end-user a set of APIs for launching agentic coding tasks on one or more executors. The end-user specifies the address of the access point through the `remote-server` configuration option.
- **Logical Coordinator** is the union of the end user’s `ah` client and the access point servers, listed in the fleet config. The fleet config fully determines which fleet members are managed directly by the end user and which are managed by the configured `remote-server` on the user’s behalf.
- **Executors**: Executors typically run `ah agent enroll`, registering themselves with the access point and maintaining a persistent QUIC control connection. See [Executor Enrollment](Executor-Enrollment.md) for mode details. The server running `ah serve` can also act as an executor. Executors are long‑lived and expected to be "always‑on" and ready to start tasks quickly, leveraging the FS Snapshot mechanisms defined in [FS Snapshots Overview](FS-Snapshots/FS-Snapshots-Overview.md).
- **Leader**: The primary executor in a fleet (Linux preferred) that owns FsSnapshots and initiates `sync-fence` and `run‑everywhere`.
- **Followers**: Secondary executors un a fleet (Windows/macOS/Linux) that execute commands and validate builds/tests.
- **Sync Fence**: An explicit operation ensuring all follower file trees match the leader FsSnapshot before execution.
- **run-everywhere**: Project command that runs an action (e.g., build/test) on selected hosts and returns output of the command execution to the agent running on the leader.
- **Fleet**: The set of one leader and one or more followers participating in a single multi‑OS session.

### Execution Cycle

- Session start preflight (leader):
- Establish persistent SSH masters (ControlMaster/ControlPersist) to all followers via HTTP CONNECT (see [Persistent SSH Connections](../Research/How-to-persistent-SSH-connections.md)).
- Write and start a Mutagen project describing sync/forward sessions to all followers (see [Mutagen Projects](../Research/Intro-to-Mutagen-Projects.md)).
- Sync ignores: `node_modules`, `.venv`, `target`, `build`, large caches unless explicitly needed; per‑project config supported via the standard configuration system (see `sync-ignores` in config.schema.json — string list; defaults as above; project‑specific overrides allowed in repo config).

- Agent edits files on the leader.
- Execute `fs_snapshot_and_sync`:
- Create a leader [FsSnapshot](FS-Snapshots/FS-Snapshots-Overview.md).
- Issue a sync fence: run `mutagen project flush` and wait until followers reflect the leader snapshot content.
- Invoke `ah agent followers run` with the project‑specific test commands, specified in the agent instructions.

Initiator:

- Default: The leader connects with SSH to followers using persistent connections. If a follower is directly reachable by TCP per policy, the leader may connect directly; otherwise it uses HTTP CONNECT via the access point.
- Hybrid: When the fleet spans multiple access points, the logical coordinator (ah client) relays bytes between CONNECT streams as needed.

### Targeting and Selectors (overview)

Fleet member selection (by host name or tag) is defined in Fleet config and can be overridden per task from `ah task`. See CLI (fleet options) for the exact flags; the leader’s followers run honors that selection.

### Project Contract: followers run

- Option parsing precedes the forwarded command: `ah agent followers run [--host <name>]... [--tag <k=v>]... [--all] [--] <command> [args...]`.
- Supports (but does not require) `--` to delimit its own options from the forwarded command.
- Per‑host command adapters (what this means):
  - Purpose: A thin OS-specific shim that ensures the same logical command runs correctly on each follower.
  - Responsibilities per host:
    - Shell/launcher: choose the correct shell/invoker (Linux: bash/zsh; macOS: zsh; Windows: PowerShell or MSYS bash).
    - Working directory mapping: translate the leader’s workspace path to the follower’s mount/path (e.g., FSKit mount on macOS; `S:` drive mapping on Windows — even without WinFsp overlay in follower mode).
    - Quoting/escaping: apply OS-appropriate quoting so arguments/flags are preserved (POSIX vs PowerShell semantics).
    - Env/PATH normalization: export required env vars and ensure tool PATHs match the project runtime (container or native).
    - Exit/log streaming: return the follower’s exit code and stream stdout/stderr back to the leader.
  - Examples:
    - Linux follower:
      - `ssh lin-01 -- bash -lc 'cd /workspaces/proj && pytest -q'`
    - macOS follower (FSKit path):
      - `ssh mac-01 -- zsh -lc 'cd /Volumes/ah-overlays/proj && pytest -q'`
    - Windows follower (PowerShell with S: mapping):
      - `ssh win-01 powershell -NoProfile -Command "Set-Location S:\\; npm test"`
- Exit code aggregation: return non‑zero if any selected host fails.

Illustrative usage:

```bash
# Run tests on all followers (default)
ah agent followers run -- test

# Run build only on Windows hosts
ah agent followers run --tag os=windows -- build

# Run lint on a specific host
ah agent followers run --host win-12 -- lint
```

### REST Observability (control‑plane)

- `GET /api/v1/sessions/{id}/info` → session summary including current fleet membership (server view), health, and endpoints.
- `GET /api/v1/sessions/{id}/info` → session summary including current fleet membership (server view), health, and endpoints.
- Control‑plane: the leader uses QUIC to push `fence*` and `host*` events; the server rebroadcasts via the session SSE stream.

### Time‑Travel Integration

- The leader’s `fs_snapshot_and_sync` is inserted between edit operations and tool execution.
- SessionMoments are emitted before/after the fence; the FsSnapshot id is linked to the post‑fence SessionMoment.
- Seeking to that SessionMoment restores leader FsSnapshot; followers are re‑synced by issuing a fence before re‑execution.

### Failure Modes

- Fence timeout: abort run_everywhere; report lagging followers and suggest narrowing selectors.
- Partial host failure: aggregate failures and return non‑zero; provide per‑host logs and artifacts.
- Sync divergence: force rescan/rebuild of stale directories; optionally clear ignores for critical paths.

### Fetching files from followers

This is done through the `ah agent followers slurp` command which takes a list of file sets. Each file set has the following properties:

- name: string (consisting of alphanumeric characters, dashes and underscores)
- include: array of paths and blobs (in URL-encoded form can be repeated)
- exclude: array of paths and blobs to exclude (filtered from the selection of included files)
- required: optional boolean (yes|no|true|false|1|0)
- dest: destination folder path where the downloaded files will appear

`--fileset-data=<data>` accept a fileset in URL-encoded form
`--fileset <name>` expects the fileset to be registered by name in the configuration files
`--fileset-file=<path>` loads the fileset from a file. Supported file formats are TOML and JSON.

The files paths and globs specify files that will be fetched from the followers. File paths and globs MUST be relative to the workspace directory.

By default, each requested file set is considered optional, unless it's marked as required. Typically, these would be build artifacts which have unique names per platform, so a single follower will respond with the requested file set.

If multiple responses are present, the results are written in the destination folder under a sub-directory `<fileset-name>/<follower-name>`. Please note that a different destination folder can be specified for each requested file set. The downloaded files are first written to a temporary location until their unique/non-unique status is resolved and then they are moved to their final destination.

Compression is determined per-file by the sender by examining the format of the requested files; checksum verification runs after each host finishes.

Transfer concurrency follows the fleet limit (default 16) and retries once on transient SSH errors.

All metadata (host, mtime, checksum) is recorded in the session timeline so REST consumers can download per-host bundles later.

### Test sharding and orchestration

The execution of tests can be accelerated by adding more followers.

`ah agent followers run` accepts multiple `--command` entries and shards them across all matching executors. The scheduler builds a job queue and assigns tasks round-robin while respecting per-host concurrency=1 to avoid oversubscription. Each command is scheduled on exactly one executor per operating system. All outputs are collected before being returned to the agent.

## Connectivity Layer — QUIC Control + SSH over CONNECT

### Purpose

Provide reliable, low‑friction connectivity for run‑everywhere and Mutagen between the leader and follower hosts across Linux, macOS, and Windows — without dynamic VPNs. Control plane uses QUIC between executors and access points; data plane uses SSH tunneled via HTTP CONNECT (with optional client‑side multi‑hop relay in hybrid fleets).

Key properties:

- No dynamic VPNs required by the system.
- Prefer SSH as the execution transport; Mutagen spawns its agent over SSH.

### Assumptions

The following assumptions are normative and apply to all connectivity modes (local, remote, hybrid):

1. Coordinator reachability: The coordinator can execute control‑plane remote procedure calls against the access point (HTTPS/QUIC on :443). All executors are reachable for control and SSH tunneling through the access point.

2. Hybrid coordination: A hybrid mode is supported where the end user reaches some executors directly, while the remote server acts as coordinator on behalf of the user for others.

3. Logical coordinator: In all cases, the logical coordinator (the combination of the end user plus the remote server) can execute control‑plane RPCs via the access point; through it, the logical coordinator can reach all executors.

4. Data plane: The leader connects to followers over SSH. Direct TCP dials are preferred when reachable and allowed by policy; otherwise, use HTTP CONNECT via the access point. No dynamic VPNs are provisioned by the system. In hybrid fleets, relaying through the client (and possibly a second access point hop) is allowed.

5. Pre‑connected executors: A long‑lived executor may already be connected to the desired fleet peers; in such cases, connection steps are skipped and verification/probing proceeds directly.

6. Automatic fallback: If a direct connection is not possible, the system automatically falls back to communicating through the logical coordinator using the most efficient available rendezvous/relay mechanism defined in this spec.

7. Session transport: Once leader–followers connectivity is established, the coding session begins and all fleet operations communicate over SSH (and Mutagen over SSH) along the selected path.

### Transport Summary

- Control: QUIC (mutually‑authenticated, SPIFFE by default) between executors and access points.
- SSH/Mutagen: Direct SSH dials from the leader to followers are preferred when reachable and allowed by policy. Otherwise, use HTTP CONNECT through the access point, bridged over QUIC to each executor’s local sshd.
- Hybrid: When a fleet spans multiple access points, the client relays bytes between two CONNECT streams (multi‑hop) to stitch endpoints.

### Operational Guidance

- Standardize on SSH
  - Mutagen can run over SSH; run‑everywhere executes remote commands via SSH.
  - Keep follower SSH access non‑root; prefer short‑lived keys or SSO.

- Security
  - Disable password auth on SSH; prefer keys/SSO; limit to non‑privileged users.
  - QUIC identities follow Executor‑Enrollment; peer verification is always enforced.

- Performance
  - Co‑locate followers when possible; validate MTU; monitor sync‑fence latency in CI multi‑OS smoke tests.

### Mutagen Project (session setup)

At session start, the leader creates a Mutagen project file that enumerates all follower endpoints and any required forwardings. The project is started immediately so that SSH‑based syncs/forwards are established early. See [Intro-to-Mutagen-Projects.md](../Research/Intro-to-Mutagen-Projects.md).

- Project contents: one `sync:` entry per follower with `alpha`=leader path and `beta`=`ssh://<user>@<executor>/~/path` (resolved through CONNECT). Optional `forward:` entries for ports.
- Lifecycle: `mutagen project start` on session creation; `mutagen project flush` is used for sync‑fence; `mutagen project terminate` on session end.
- Non‑reachable peers become reachable through the QUIC CONNECT relay automatically; the CLI controls both CONNECT streams.

### Hybrid Multi‑Hop Forwarding (client‑relay)

### Handshake & Sync Confirmation

Goal: Confirm follower connectivity (CONNECT or client‑relay) before first run‑everywhere, with a short timeout.

Sequence (CONNECT path):

1. Establish CONNECT and QUIC control channels; no key distribution or VPN enrollment is performed by the system.

Fallback (hybrid relay): If endpoints live behind different access points, the client opens two CONNECT streams and relays bytes between them.

### Client-relay workflow

Followers do not initiate outbound connections to other executors.
The leader dials followers directly over SSH when reachable; otherwise it tunnels
over HTTP CONNECT via the relevant access point(s). When followers belong to
different access points, the access point coordinates and may instruct the
end‑user client to relay by opening two CONNECT streams and copying bytes between
them. If both executors belong to the same access point, the access point bridges
internally over QUIC. The relay is session‑scoped and ephemeral; the CLI tears it down when the action completes.

Policy and security:

- The client authenticates to each access point with user OIDC/JWT; ACLs ensure it can open tunnels only to authorized executors.
- Host key verification remains end‑to‑end to each executor.

### Session setup details

#### Persistent SSH connections (multiplexing)

To minimize SSH handshake costs during a session, the leader (or the logical coordinator in hybrid mode) creates persistent SSH masters to all followers at session start, using OpenSSH ControlMaster/ControlPersist. Subsequent execs reuse the connection, dramatically reducing command latency. See [How-to-persistent-SSH-connections.md](../Research/How-to-persistent-SSH-connections.md).

#### Status and Observability

- Session setup emits standard `status`/`log`/`host*`/`fence*` events; there is no VPN enrollment telemetry in the system.

### Connectivity Algorithm (simplified)

This section specifies the algorithm the Coordinator (ah client or WebUI backend acting as coordinator) uses to establish connectivity between the leader and followers for a fleet. The server never dials followers; all data‑plane connections originate from the leader (or are stitched by the client in hybrid multi‑hop).

Inputs

- `fleet`: list of followers with metadata: `name`, `os`, `sshUser`, `sshPort` (default 22), and tags.
- `timeouts`: `{ connect: 5s, overall: 60s }` (configurable per policy).

High‑Level Stages

1. Session initialization
2. SSH path (Direct TCP preferred; CONNECT fallback; client‑relay for hybrid)
3. SSH Liveness Check
4. Mutagen project health (flush)
5. Monitoring and Re‑route on Failure

#### Stage 1: Session initialization

1. QUIC control connections are established (executors already run `ah agent enroll`).
2. The leader (or logical coordinator) creates persistent SSH masters to followers via CONNECT (multiplexing enabled).
3. The leader writes a Mutagen project file covering all followers and starts it.

#### Stage 2: SSH path

Use HTTP CONNECT via the relevant access point(s). In hybrid, stitch endpoints with client‑side multi‑hop relay. No dynamic VPNs are provisioned by the system.

#### Stage 3: SSH Liveness Check

Upon a successful probe, mark `sshPath` for the follower with details: `method` (P1..P7), `target` (host/port), and `proxy` (if any). Immediately perform a second liveness command to validate interactive command execution and latency budget:

```
ssh ... uname -a && id -u
```

Record `rtt_estimate_ms` as the measured round‑trip time.

#### Stage 4: Mutagen project health

Verify the Mutagen project is running and `mutagen project flush` completes (equivalent to sync‑fence). If a follower is temporarily unavailable, Mutagen will connect when the CONNECT relay is available.

#### Stage 5: Path Fixation and Persistence

- Persist in session state (SQLite/REST):
  - `follower.sshPath = { method, target, relay, rtt_estimate_ms }`
- Emit event `connect.path.fixed` for observability.

#### Stage 6: Monitoring and Re‑route on Failure

During the session:

- Health probe every 30s: `ssh ... echo ok` with timeout 1s; tolerate 3 consecutive failures before marking path degraded.
- On degradation, attempt seamless re‑route by re‑running Stage 2 starting from the last successful method, then earlier priorities. Emit `connect.path.degraded` and `connect.path.restored` events.
- If an access point changes addressing, reconnect CONNECT streams; SSH masters are re‑established automatically by ControlPersist.

#### Reverse path (rare)

If an executor cannot keep a QUIC control connection, the session cannot proceed. Reverse WS tunnels are not required; bring the executor back online or exclude it from the fleet.

#### Failure Handling and Telemetry

- Every failure includes `method`, `error`, and timestamps; the Coordinator aggregates and reports best diagnostics per follower.
- Common causes and guidance are included in messages (e.g., corporate proxy blocks CONNECT → use client relay across allowed endpoints).

#### Security & Cleanup

- SSH keys are short‑lived or use SSO. Password auth MUST be disabled.

#### Pseudocode (per follower)

```pseudo
function establish_paths(fleet):
  # Assume executors are connected over QUIC to access points
  for follower in fleet.followers:
    # Always use CONNECT via access point; in hybrid, open two CONNECT streams and relay
    open_connect_stream(fleet.leader, follower)
    start_ssh_control_master(fleet.leader, follower)  # ControlMaster/ControlPersist
  write_mutagen_project(fleet)
  mutagen_project_start()
  mutagen_project_flush()  # sync-fence
  return OK
```

This algorithm is normative for initial leader↔follower connectivity in fleets and is referenced by Multi‑OS Testing and CLI orchestration.
