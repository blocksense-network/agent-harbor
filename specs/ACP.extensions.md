# ACP Protocol Extensions for Agent Harbor

This document captures the custom ACP surface we expose so IDEs can drive Harbor-specific features (filesystem snapshots, time-travel branching, multi-agent orchestration, and timeline playback). The extensions follow ACP’s extensibility rules: method names are namespaced (`ah/*`), capabilities are advertised during `initialize`, and all requests/notifications use JSON-RPC 2.0 semantics.

References:

- `specs/Public/FS-Snapshots/FS-Snapshots-Overview.md`
- `specs/Public/ah-agent-record.md`
- `specs/ACP.server.status.md`

---

## Capability Advertisement

During `initialize`, Harbor advertises:

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "result": {
    "agentCapabilities": {
      "agentHarborSnapshots": {
        "version": 1,
        "supportsTimelineSeek": true,
        "supportsBranching": true,
        "supportsFollowPlayback": true
      }
    }
  }
}
```

Clients that do not recognize `agentHarborSnapshots` ignore it. Clients that do recognize it can call the `ah/*` methods below.

---

## Data Structures

All APIs share the same snapshot/branch metadata:

```json
{
  "snapshot": {
    "id": "snap-01HV…",
    "label": "tests-green",
    "provider": "zfs|btrfs|agentfs|git",
    "workingCopy": "cow-overlay|worktree|in-place",
    "execPath": "/workspace",
    "createdAt": "2025-02-01T12:34:56.123Z",
    "branchable": true,
    "cleanupToken": "opaque",
    "anchor": {
      "executionId": "exec-01HV…", // Recorder execution id (aligns with `ah show-sandbox-execution`)
      "byteOffset": 198000, // REC_SNAPSHOT anchor
      "timelineTs": 123.45 // Seconds since session start
    }
  }
}
```

Branches extend the above with `branchId`, `name`, `status`, `snapshotId`, `mountPath`, `parentSessionId`, and `createdBy`.

---

## Custom Methods (Client → Harbor)

### `ah/session/new`

Harbor-specific analogue of ACP `session/new` that lets the client describe multiple model/agent pairs and spawn several sessions in one call.

```json
{
  "jsonrpc": "2.0",
  "id": 9,
  "method": "ah/session/new",
  "params": {
    "tenantId": "acme",
    "projectId": "webapp",
    "prompt": "Refactor logging and add unit tests",
    "repo": {
      "mode": "git",
      "url": "git@github.com:acme/webapp.git",
      "branch": "feature/logging"
    },
    "workspace": {
      "snapshotPreference": ["zfs", "btrfs", "agentfs", "git"],
      "workingCopy": "cow-overlay"
    },
    "agents": [
      {
        "model": {
          "id": "claude-3-5-sonnet-20241022",
          "provider": "anthropic",
          "settings": { "maxTokens": 8000 }
        },
        "count": 2,
        "delivery": { "mode": "pr", "targetBranch": "main" },
        "agentStartOptions": {
          "configPath": "/etc/ah/claude.toml",
          "sandboxFlags": ["network=isolated"]
        }
      },
      {
        "model": {
          "id": "openhands",
          "provider": "openhands",
          "settings": { "temperature": 0.1 }
        },
        "count": 1
      }
    ],
    "labels": { "priority": "p1" }
  }
}
```

- Each entry in `agents[]` describes a model + provider-specific settings plus optional Harbor start options.
- `count` spawns multiple identical sessions for that configuration (Harbor assigns separate session IDs). If omitted, defaults to 1.
- `agentStartOptions` let the client pass through the same flags it would normally provide to `ah agent start` (config path, sandbox profile, working copy overrides, etc.).

Response:

```json
{
  "jsonrpc": "2.0",
  "id": 9,
  "result": {
    "sessions": [
      {
        "sessionId": "sess-claude-01HV…",
        "model": { "id": "claude-3-5-sonnet-20241022" },
        "agentConfig": { "delivery": { "mode": "pr" } }
      },
      {
        "sessionId": "sess-claude-01HW…",
        "model": { "id": "claude-3-5-sonnet-20241022" },
        "agentConfig": { "delivery": { "mode": "pr" } }
      },
      {
        "sessionId": "sess-openhands-01HX…",
        "model": { "id": "openhands" }
      }
    ]
  }
}
```

For compatibility, the client can still call the standard ACP `session/new` if it only needs a single session; `ah/session/new` is optional and gated by the `agentHarborSnapshots` capability block (a client that doesn’t know about it won’t call it). Harbor mirrors the underlying behavior to REST (`POST /api/v1/tasks`), so the IDE sees the same lifecycle as other Agent Harbor entry points.

### `ah/snapshot_create`

Create a manual snapshot of the current workspace.

```json
{
  "jsonrpc": "2.0",
  "id": 10,
  "method": "harbor/snapshot_create",
  "params": {
    "sessionId": "sess-01HV…",
    "label": "before-upgrade",
    "anchor": {
      "executionId": "exec-01HV…"
    }
  }
}
```

Response: `{ "snapshot": { … } }`. Harbor flushes recorder buffers, invokes `snapshot_now()` on the active provider, and returns the metadata.

### `ah/snapshot_list`

Read-only listing with pagination/filtering (`branchable`, `provider`, `createdAfter`, etc.).

### `ah/branch_create`

Create a writable workspace or a new Harbor session from an existing snapshot or timeline moment.

```json
{
  "jsonrpc": "2.0",
  "id": 11,
  "method": "harbor/branch_create",
  "params": {
    "sessionId": "sess-01HV…",
    "from": {
      "snapshotId": "snap-01HV…" // OR { "timelineTs": 321.0 }
    },
    "branch": {
      "name": "fix-logging",
      "prompt": "Retry with better logging",
      "agents": [
        {
          "type": "claude-code",
          "version": "latest",
          "count": 2,
          "settings": { "maxTokens": 6000 }
        }
      ],
      "delivery": {
        "mode": "pr",
        "targetBranch": "main"
      },
      "agentStartOptions": {
        "configPath": "/etc/ah/claude.toml",
        "workingCopy": "cow-overlay",
        "sandboxFlags": ["network=isolated"]
      }
    },
    "prompt": "Continue from snapshot but disable flaky test suite."
  }
}
```

Behavior:

- Harbor clones/mounts the snapshot (`branch_from_snapshot()`), writes the provided prompt/task metadata, and launches a **new session** via `ah agent start` with the supplied agent list/settings. This maps Harbor’s “multi-agent per prompt” model to ACP by creating multiple ACP sessions (one per agent configuration) and returning their IDs.
- Response includes `branch` metadata **and** an array of `newSessions` so the client can attach to each ACP session independently.
- If a client wants to re-use the existing session instead of creating a sibling, it sets `branch.sessionReuse = true`; Harbor then swaps the current session workspace to the new branch and emits `harbor/snapshot_created` + `harbor/branch_updated` notifications (see below).

### `harbor/branch_list`, `harbor/branch_delete`

Enumerate / remove branches. Deletion is rejected if the branch is running, has dependent sessions, or the cleanup token cannot be honored.

### `harbor/session_seek` (optional)

Mount a read-only filesystem view for exploration:

```json
{
  "jsonrpc": "2.0",
  "id": 12,
  "method": "harbor/session_seek",
  "params": {
    "sessionId": "sess-01HV…",
    "snapshotId": "snap-01HV…",
    "mountReadonly": true
  }
}
```

Response: `{"mountPath": "/tmp/ah-snap-…", "executionId": "exec-01HV…", "timelineTs": 456.7 }`.

---

## Notifications (Harbor → Client)

These ride on the regular `session/update` SSE/WebSocket stream with `update.type = "custom"` and `customType = "harbor/*"`:

| Notification                 | Payload                                                         | Purpose                                                                                                                                  |
| ---------------------------- | --------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------- | ---------------- | ----------------------------------------------------------------------------------------------------- |
| `harbor/snapshot_created`    | `{ "sessionId", "snapshot": {…}, "reason": "auto                | manual                                                                                                                                   | branch_point" }` | Fired whenever Harbor captures a snapshot (auto or manual). Enables IDE indicators for branch points. |
| `harbor/branch_created`      | `{ "branch": {…}, "newSessions": ["sess-…", …] }`               | Notifies UI that a branch + new sessions exist.                                                                                          |
| `harbor/branch_updated`      | `{ "branch": {…} }`                                             | Status changes (running, paused, merged).                                                                                                |
| `harbor/branch_deleted`      | `{ "branchId" }`                                                | Cleanup confirmation.                                                                                                                    |
| `harbor/timeline_checkpoint` | `{ "sessionId", "executionId", "byteOffset", "label", "kind" }` | Mirrors recorder `REC_SNAPSHOT` / `REC_MARK` events so clients can draw timeline handles even before a full FS snapshot is materialized. |

Clients use these notifications to render gutter markers, time-travel scrubbers, and branch trees without polling.

---

## Multi-Agent Launch Semantics

Harbor can run multiple agents from the same prompt (e.g., two Claude Code workers plus one OpenHands). ACP already supports multiple concurrent sessions, so we map each Harbor agent instance to its **own** ACP session:

1. Client calls `harbor/branch_create` (or `POST /tasks` via the REST mirror) with `agents = […]`.
2. Harbor launches N agent processes (one per entry) and returns `newSessions: ["sess-A", "sess-B", …]`.
3. The IDE opens ACP connections for each session. Each session streams its own `session/update`, timeline, logs, etc.

This approach keeps each ACP session aligned with the protocol’s expectations (one agent per session) while still allowing Harbor to coordinate multiple agents behind the scenes. Clients that want to aggregate view state can group the returned session IDs in their UI.

---

## Policy & Security Hooks

- Every method enforces Harbor RBAC (e.g., only `operator` roles can create branches). Violations return Problem+JSON with `type = …/errors/forbidden`.
- Requests accept optional `tenantId` / `projectId` hints to support scoped views (mirrors REST).
- `harbor/snapshot_create` supports a `locked: true` flag for compliance use-cases where a snapshot must be immutable (Harbor then refuses branch/delete until the flag is cleared through a separate workflow).

---

## Future Extensions

- `harbor/snapshot_diff` for server-side diff rendering of two snapshots/branches.
- `harbor/branch_rebase` to reapply a branch on top of a newer snapshot (uses `FsSnapshotProvider` cleanup tokens).
- `harbor/agent_followers` to stream recorder metadata for all tool executions without starting playback immediately (useful for IDE dashboards).

These would follow the same namespacing and capability advertisement pattern established above.
