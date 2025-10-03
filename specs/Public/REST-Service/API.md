## Agent Harbor REST Service — Purpose and Specification

### Purpose

- **Central orchestration**: Provide a network API to create and manage isolated agent coding sessions on demand, aligned with the filesystem snapshot model described in [FS-Snapshots-Overview](FS%20Snapshots/FS-Snapshots-Overview.md).
- **On‑prem/private cloud ready**: Designed for enterprises running self‑managed clusters or single hosts.
- **UI consumers**: Back the WebUI and TUI dashboards; enable custom internal portals and automations.
- **Uniform abstraction**: Normalize differences between agents (Claude Code, OpenHands, Copilot, etc.), runtimes (devcontainer/local), and snapshot providers (ZFS/Btrfs/Overlay/copy).

### Non‑Goals

- Replace VCS or CI systems.
- Store long‑term artifacts or act as a package registry.
- Provide full IDE functionality; instead, expose launch hooks and connection info.

### Architecture Overview

- **API Server (stateless)**: Exposes REST + SSE/WebSocket endpoints to localhost only by default. Optionally persists state in a database. Also known as the "access point daemon" (same code path as `ah agent access-point`).
- **WebUI Integration**: When launched via `ah webui`, the SSR server acts as a proxy for all `/api/v1/*` requests, forwarding them to the access point daemon. This enables the SSR server to implement user access policies and security controls. The daemon runs either as an in-process component (local mode) or as a subprocess/sidecar.
- **Executors**: One or many worker processes/hosts that provision workspaces and run agents.
- **Workspace provisioning**: Uses ZFS/Btrfs snapshots when available; falls back to OverlayFS or copy; can orchestrate devcontainers.
- **Transport**: JSON over HTTPS; events over SSE (preferred) and WebSocket (optional).
- **Identity & Access**: API Keys, OIDC/JWT, optional mTLS; project‑scoped RBAC.
- **Observability**: Structured logs, metrics, traces; per‑session logs streaming.

### Core Concepts

- **Task**: A request to run an agent with a prompt and runtime parameters.
- **Session**: The running instance of a task with lifecycle and logs; owns a per‑task workspace.
- **Workspace**: Isolated filesystem mount realized by snapshot provider or copy, optionally inside a devcontainer.
- **Runtime**: The execution environment for the agent (devcontainer/local), plus resource limits.
- **Agent**: The tool performing the coding task.

### Lifecycle States

`queued → provisioning → running → pausing → paused → resuming → stopping → stopped → completed | failed | cancelled`

### Security and Tenancy

- **AuthN**: API Keys, OIDC (Auth0/Keycloak), or JWT bearer tokens.
- **AuthZ (RBAC)**: Roles `admin`, `operator`, `viewer`; resources scoped by `tenantId` and optional `projectId`.
- **Network policy**: Egress restrictions and allowlists per session.
- **Secrets**: Per‑tenant secret stores mounted as env vars/files, never logged.

### API Conventions

- **Base URL**: `/api/v1`
- **Content type**: `application/json; charset=utf-8`
- **Idempotency**: Supported via `Idempotency-Key` header on POSTs.
- **Pagination**: `page`, `perPage` query params; responses include `nextPage` and `total`.
- **Filtering**: Standard filters via query params (e.g., `status`, `agent`, `projectId`).
- **Errors**: Problem+JSON style:

```json
{
  "type": "https://docs.example.com/errors/validation",
  "title": "Invalid request",
  "status": 400,
  "detail": "repo.url must be provided when repo.mode=git",
  "errors": { "repo.url": ["is required"] }
}
```

### Data Model (high‑level)

- **Session**
  - `id` (string, ULID/UUID)
  - `tenantId`, `projectId` (optional)
  - `task`: prompt, attachments, labels
  - `agent`: type, version/config
  - `runtime`: type, devcontainer config reference, resource limits
  - `workspace`: snapshot provider, mount path, host, devcontainer details
  - `vcs`: repo info and delivery policy (PR/branch/patch)
  - `status`, `startedAt`, `endedAt`
  - `links`: SSE stream, logs, IDE/TUI launch helpers

### Endpoints

#### Create Task / Session

- `POST /api/v1/tasks`

Request:

```json
{
  "tenantId": "acme",
  "projectId": "storefront",
  "prompt": "Fix flaky tests in checkout service and improve logging.",
  "repo": {
    "mode": "git",
    "url": "git@github.com:acme/storefront.git",
    "branch": "feature/agent-task",
    "commit": null
  },
  "runtime": {
    "type": "devcontainer",
    "devcontainerPath": ".devcontainer/devcontainer.json",
    "resources": { "cpu": 4, "memoryMiB": 8192 }
  },
  "workspace": {
    "snapshotPreference": ["zfs", "btrfs", "overlay", "copy"],
    "executionHostId": "executor-a"
  },
  "agent": {
    "type": "claude-code",
    "version": "latest",
    "settings": { "maxTokens": 8000 }
  },
  "delivery": {
    "mode": "pr",
    "targetBranch": "main"
  },
  "labels": { "priority": "p2" },
  "webhooks": [
    { "event": "session.completed", "url": "https://hooks.acme.dev/agents" }
  ]
}
```

Response `201 Created`:

```json
{
  "id": "01HVZ6K9T1N8S6M3V3Q3F0X5B7",
  "status": "queued",
  "links": {
    "self": "/api/v1/sessions/01HVZ6K9T1...",
    "events": "/api/v1/sessions/01HVZ6K9T1.../events",
    "logs": "/api/v1/sessions/01HVZ6K9T1.../logs"
  }
}
```

Notes:

- `repo.mode` may also be `upload` (use pre‑signed upload flow) or `none` (operate on previously provisioned workspace template).
- `runtime.type` may be `local` (no container) or `disabled` (explicitly allowed by policy).

#### List Sessions

- `GET /api/v1/sessions?status=running&projectId=storefront&page=1&perPage=20`

Response `200 OK` includes array of sessions and pagination metadata.

**Session Object Structure:**

Each session object includes:
- `id`, `status`, `prompt`, `repo`, `runtime`, `agent`, `delivery`, `createdAt`, `updatedAt`
- `recent_events`: Array of the last 3 events for active sessions (for SSR pre-population)
  - Only included for active sessions (`running`, `queued`, `provisioning`, `paused`)
  - Empty array `[]` for completed/failed/cancelled sessions
  - Format matches SSE event structure (see Event Types below)

Example session with recent events:
```json
{
  "id": "01HVZ6K9T1N8S6M3V3Q3F0X5B7",
  "status": "running",
  "prompt": "Fix authentication bug",
  "repo": { ... },
  "runtime": { ... },
  "agent": { ... },
  "recent_events": [
    { "thought": "Analyzing authentication flow", "ts": "2025-09-30T17:10:00Z" },
    { "tool_name": "read_file", "tool_output": "File read successfully", "tool_status": "success", "ts": "2025-09-30T17:10:05Z" },
    { "file_path": "src/auth.ts", "lines_added": 5, "lines_removed": 2, "ts": "2025-09-30T17:10:10Z" }
  ],
  ...
}
```

**Purpose:** The `recent_events` field enables SSR to pre-populate active task cards with the last 3 events, ensuring cards never show "Waiting for agent activity" and maintain fixed height from initial page load.

#### Get Session

- `GET /api/v1/sessions/{id}` → session details including current status, workspace summary, and recent events.

#### Stop / Cancel

- `POST /api/v1/sessions/{id}/stop` → graceful stop (agent asked to wrap up).
- `DELETE /api/v1/sessions/{id}` → force terminate and cleanup.

#### Pause / Resume

- `POST /api/v1/sessions/{id}/pause`
- `POST /api/v1/sessions/{id}/resume`

#### Logs and Events

- `GET /api/v1/sessions/{id}/logs?tail=1000` → historical logs.
- `GET /api/v1/sessions/{id}/events` (SSE) → live status, logs, and milestones.

Event payload (SSE `data:` line):

```json
{
  "type": "log",
  "level": "info",
  "message": "Running tests...",
  "ts": "2025-01-01T12:00:00Z"
}
```

#### Event Ingestion (leader → server)

The server does not initiate any connections to executors. Multi‑OS execution (sync‑fence, run‑everywhere) is performed by the leader over SSH. For connectivity, clients use the access point’s HTTP CONNECT tunnel to reach each executor’s local sshd, as defined in Executor‑Enrollment. To keep the UI and automations informed, the leader pushes timeline events to the server.

- Control‑plane event flow: Session timeline events (`fence*`, `host*`, etc.) are delivered over the QUIC control channel from the leader to the access point server and rebroadcast on the session SSE stream. No REST ingestion endpoint is exposed for these events.

Accepted event types (minimum set):

```json
{ "type": "followersCatalog", "hosts": [{"name":"win-01","os":"windows","tags":["os=windows"]}] }
{ "type": "fenceStarted",  "snapshotId": "snap-01H...", "ts": "...", "origin": "leader", "transport": "ssh" }
{ "type": "fenceResult",   "snapshotId": "snap-01H...", "hosts": {"win-01": {"state": "consistent", "tookMs": 842}}, "ts": "..." }
{ "type": "hostStarted",    "host": "mac-02", "ts": "..." }
{ "type": "hostLog",        "host": "win-01", "stream": "stdout", "message": "Running tests...", "ts": "..." }
{ "type": "hostExited",     "host": "mac-02", "code": 0, "ts": "..." }
{ "type": "summary",        "passed": ["mac-02","lin-03"], "failed": ["win-01"], "ts": "..." }
{ "type": "note",           "message": "optional annotation", "ts": "..." }
```

#### Capability Discovery

- `GET /api/v1/agents` → List supported agent types and configurable options.
  - Response:

  ```json
  {
    "items": [
      {
        "type": "openhands",
        "versions": ["latest"],
        "settingsSchemaRef": "/api/v1/schemas/agents/openhands.json"
      },
      {
        "type": "claude-code",
        "versions": ["latest"],
        "settingsSchemaRef": "/api/v1/schemas/agents/claude-code.json"
      }
    ]
  }
  ```

- `GET /api/v1/runtimes` → Available runtime kinds and images/templates.
  - Response:

  ```json
  {
    "items": [
      {
        "type": "devcontainer",
        "images": ["ghcr.io/acme/base:latest"],
        "paths": [".devcontainer/devcontainer.json"]
      },
      { "type": "local", "sandboxProfiles": ["default", "disabled"] }
    ]
  }
  ```

- `GET /api/v1/executors` → Execution hosts (terminology aligned with CLI.md).
  - Response entries include: `id`, `os`, `arch`, `snapshotCapabilities` (e.g., `zfs`, `btrfs`, `overlay`, `copy`), and health.
  - Long‑lived executors:
    - Executors register with the Remote Service when `ah serve` starts and send heartbeats including overlay status and addresses (MagicDNS/IP).
    - The `GET /executors` response includes `overlay`: `{ provider, address, magicName, state }` and `controller` hints (typically `server`).

#### Draft Task Management

Drafts allow users to save incomplete task configurations for later completion and persistence across browser sessions.

- `POST /api/v1/drafts` → Create a new draft task

  Request:
  ```json
  {
    "prompt": "Implement user authentication...",
    "repo": {
      "mode": "git",
      "url": "https://github.com/user/repo.git",
      "branch": "main"
    },
    "agent": {
      "type": "claude-code",
      "version": "latest"
    },
    "runtime": {
      "type": "devcontainer"
    },
    "delivery": {
      "mode": "pr"
    }
  }
  ```

  Response `201 Created`:
  ```json
  {
    "id": "draft-01HVZ6K9T1N8S6M3V3Q3F0X5B7",
    "createdAt": "2025-01-01T12:00:00Z",
    "updatedAt": "2025-01-01T12:00:00Z"
  }
  ```

- `GET /api/v1/drafts` → List user's draft tasks

  Response `200 OK`:
  ```json
  {
    "items": [
      {
        "id": "draft-01HVZ6K9T1N8S6M3V3Q3F0X5B7",
        "prompt": "Implement user authentication...",
        "repo": { "mode": "git", "url": "...", "branch": "main" },
        "agent": { "type": "claude-code", "version": "latest" },
        "runtime": { "type": "devcontainer" },
        "delivery": { "mode": "pr" },
        "createdAt": "2025-01-01T12:00:00Z",
        "updatedAt": "2025-01-01T12:00:00Z"
      }
    ]
  }
  ```

- `PUT /api/v1/drafts/{id}` → Update a draft task

- `DELETE /api/v1/drafts/{id}` → Delete a draft task

- Optional helper endpoints used by CLI completions and WebUI forms:
  - `GET /api/v1/git/refs?url=<git_url>` → Cached branch/ref suggestions for `--target-branch` UX.
  - `GET /api/v1/projects` → List known projects per tenant for filtering.
  - `GET /api/v1/repos?tenantId=<id>&projectId=<id>` → Returns repositories the service has indexed (from historical tasks or explicit imports). Each item includes `id`, `displayName`, `scmProvider`, `remoteUrl`, `defaultBranch`, and `lastUsedAt`, mirroring common REST patterns for repository catalogs.
  - `GET /api/v1/workspaces?status=active` → Lists provisioned workspaces with metadata.
  - `GET /api/v1/workspaces/{id}` → Detailed view including workspace repository URLs, storage usage, task history, etc.

CLI parity:

```
ah remote repos [--tenant <id>] [--project <id>] [--json]
ah remote workspaces [--status <state>] [--json]
ah remote workspace show <WORKSPACE_ID>
```

The `ah remote` subcommands call the endpoints above and surface consistent column layouts (name, provider, branch for repos; workspace state, executor, age for workspaces). They support `--json` for scripting and respect the CLI’s existing pager/formatting options.

#### Followers and Multi‑OS Execution

- `GET /api/v1/sessions/{id}/info` → Session summary including current fleet membership (server view), health, and endpoints.

Notes:

- Sync‑fence and followers run are leader‑executed actions over SSH. They are not exposed as server‑triggered REST methods. The server observes progress via QUIC control‑plane events and rebroadcasts them on the session SSE stream.

### Authentication Examples

- API Key: `Authorization: ApiKey <token>`
- OIDC/JWT: `Authorization: Bearer <jwt>`

### Rate Limiting and Quotas

- Configurable per tenant/project/user; `429` responses include `Retry-After`.

### Observability

- Metrics: per‑session counts, durations, success rates.
- Tracing: provision → run → delivery spans with session id.

### Versioning and Compatibility

- Semantic API versioning via URL prefix (`/api/v1`).
- OpenAPI spec served at `/api/v1/openapi.json`.

### Deployment Topologies

- Single host: API + executor in one process.
- Scaled cluster: API behind LB; multiple executors with shared DB/queue; shared snapshot‑capable storage or local snapshots per host.

### Security Considerations

- Egress controls; per‑session network policies.
- Secret redaction in logs/events.

### Example: Minimal Task Creation

```bash
curl -X POST "$BASE/api/v1/tasks" \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "prompt": "Refactor build pipeline to reduce flakiness.",
    "repo": {"mode": "git", "url": "git@github.com:acme/storefront.git", "branch": "agent/refactor"},
    "runtime": {"type": "devcontainer"},
    "agent": {"type": "openhands"}
  }'
```

### Alignment with CLI.md (current)

- `ah task` → `POST /api/v1/tasks` (returns `sessionId` usable for polling and SSE).
- `ah session list|get|logs|events` → `GET /api/v1/sessions[/{id}]`, `GET /api/v1/sessions/{id}/logs`, `GET /api/v1/sessions/{id}/events`.
- `ah session run <SESSION_ID> <IDE>` → `POST /api/v1/sessions/{id}/open/ide`.
- `ah remote agents|runtimes|executors` → `GET /api/v1/agents`, `GET /api/v1/runtimes`, `GET /api/v1/executors`.
- `ah remote repos|workspaces` → `GET /api/v1/repos`, `GET /api/v1/workspaces` (and `GET /api/v1/workspaces/{id}` for detail views).
- `ah agent followers list` → QUIC `SessionFollowers` stream for real-time membership; the REST `GET /api/v1/sessions/{id}/info` endpoint remains available for static snapshots. QUIC keeps the connection open so membership and health changes arrive with minimal latency, matching the transport used elsewhere in the control plane.
- `ah agent sync-fence|followers run` → leader‑executed over SSH; server observes via the control plane (QUIC) and rebroadcasts on session SSE.

SSE event taxonomy for sessions:

```json
{ "type": "status",  "status": "provisioning", "ts": "..." }
{ "type": "log",     "level": "info", "message": "Running tests...", "ts": "..." }
{ "type": "moment",  "snapshotId": "snap-01H...", "note": "post-fence", "ts": "..." }
{ "type": "delivery", "mode": "pr", "url": "https://github.com/.../pull/123", "ts": "..." }
{ "type": "fenceStarted",  "snapshotId": "snap-01H...", "ts": "..." }
{ "type": "fenceResult",   "snapshotId": "snap-01H...", "hosts": {"...": {"state": "consistent", "tookMs": 842}}, "ts": "..." }
{ "type": "hostStarted",   "host": "...", "ts": "..." }
{ "type": "hostLog",       "host": "...", "stream": "stdout", "message": "...", "ts": "..." }
{ "type": "hostExited",    "host": "...", "code": 0, "ts": "..." }
{ "type": "summary",       "passed": ["..."], "failed": ["..."], "ts": "..." }
```

### Implementation and Testing Plan

Planning and status tracking for this spec live in [REST-Service.status.md](REST-Service.status.md). That document defines milestones, success criteria, and a precise, automated test plan per specs/AGENTS.md.

#### Session Info (summary)

- `GET /api/v1/sessions/{id}/info`

Response `200 OK`:

```json
{
  "id": "01HVZ6K9T1...",
  "status": "running",
  "fleet": {
    "leader": "exec-linux-01",
    "followers": [
      { "name": "win-01", "os": "windows", "health": "ok" },
      { "name": "mac-01", "os": "macos", "health": "ok" }
    ]
  },
  "endpoints": { "events": "/api/v1/sessions/01HV.../events" }
}
```

Notes:

- Health reflects the access point’s current view and recent QUIC/SSH checks.
- This is a read‑only summary used by UIs to render session topology.
