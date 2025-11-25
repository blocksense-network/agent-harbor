# ACP Protocol Extensions for Agent Harbor

This document captures the custom ACP surface we expose so IDEs can drive Harbor-specific features (filesystem snapshots, time-travel branching, multi-agent orchestration, timeline playback, and pipeline introspection). The extensions follow ACP’s extensibility rules: method names are namespaced (`_ah/*`, leading underscore per spec), capabilities are advertised during `initialize`, and all requests/notifications use JSON-RPC 2.0 semantics.

References:

- `specs/Public/FS-Snapshots/FS-Snapshots-Overview.md`
- `specs/Public/ah-agent-record.md`
- `specs/ACP.server.status.md`

---

## Capability Advertisement

During `initialize`, Harbor advertises its standard ACP capabilities and uses the `_meta` namespace (per ACP spec) to declare extension bundles. All Harbor-specific extensions live under `_meta.agent.harbor`.

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "result": {
    "agentCapabilities": {
      "loadSession": true,
      "_meta": {
        "agent.harbor": {
          "snapshots": {
            "version": 1,
            "supportsTimelineSeek": true,
            "supportsBranching": true,
            "supportsFollowPlayback": true
          },
          "pipelineIntrospection": {
            "version": 1,
            "supportsStepStreaming": true
          },
          "workspace": {
            "version": 1,
            "supportsDiffs": true
          }
        }
      }
    }
  }
}
```

Clients that do not recognize `agent.harbor` simply ignore the `_meta` section. Clients that do recognize it can call the `_ah/*` methods below (including the pipeline APIs when `pipelineIntrospection.supportsStepStreaming` is `true`).

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

{
  "pipelineStep": {
    "pipelineId": "pipe-01HV…",
    "stepId": "pipe-01HV…:2",
    "index": 2,
    "command": "grep ERROR",
    "argv": ["grep", "ERROR"],
    "pid": 12345,
    "stdoutBytes": 4096,
    "stderrBytes": 0,
    "inputBytes": 8192,
    "startedAt": "2025-02-01T12:35:01.000Z",
    "endedAt": "2025-02-01T12:35:04.500Z",
    "executionId": "exec-01HV…",
    "byteRanges": {
      "stdout": { "start": 210000, "len": 4096 },
      "stderr": null
    }
  }
}
```

Branches extend the above with `branchId`, `name`, `status`, `snapshotId`, `mountPath`, `parentSessionId`, and `createdBy`.

---

> **Note:** REST counterparts for pipeline introspection (`GET /api/v1/sessions/{sessionId}/executions/{executionId}/pipelines…`) are documented in `specs/Public/REST-Service/API.md`. This section focuses on the ACP extensions that mirror those capabilities.

## Custom Methods (Client → Harbor)

### `_ah/workspace/info`

Returns metadata about the Harbor-managed workspace (provider, working copy mode, exec path, disk usage).

```json
{
  "jsonrpc": "2.0",
  "id": 50,
  "method": "_ah/workspace/info",
  "params": {
    "sessionId": "sess-01HV…"
  }
}
```

Response:

```json
{
  "jsonrpc": "2.0",
  "id": 50,
  "result": {
    "provider": "zfs",
    "workingCopy": "cow-overlay",
    "execPath": "/workspace",
    "usageBytes": 2147483648,
    "snapshotCount": 12,
    "lastSnapshotId": "snap-01HV…"
  }
}
```

### `_ah/workspace/list`

Browse the workspace tree (mirrors `GET /workspace/files`).

```json
{
  "jsonrpc": "2.0",
  "id": 51,
  "method": "_ah/workspace/list",
  "params": {
    "sessionId": "sess-01HV…",
    "path": "src",
    "recursive": false,
    "modifiedOnly": true
  }
}
```

Response matches the REST structure: `{ "path": "src", "type": "directory", "children": [ … ] }`.

### `_ah/workspace/file`

Fetch file metadata/content (without using `fs/read_text_file`). Mirrors `GET /files/{filePath}` and `GET /files/{filePath}/content`.

```json
{
  "jsonrpc": "2.0",
  "id": 52,
  "method": "_ah/workspace/file",
  "params": {
    "sessionId": "sess-01HV…",
    "path": "src/auth.rs",
    "includeContent": true
  }
}
```

Response includes metadata (`status`, `lines_added`, `lines_removed`, `last_modified`, etc.) and optional full content.

### `_ah/workspace/diff`

Fetch file diff(s) (mirrors `GET /diff` endpoints).

```json
{
  "jsonrpc": "2.0",
  "id": 53,
  "method": "_ah/workspace/diff",
  "params": {
    "sessionId": "sess-01HV…",
    "files": ["src/auth.rs"],
    "context": 3,
    "format": "unified"
  }
}
```

Response matches the REST diff payload (per-file diffs, line counts, etc.). Large diffs may be truncated as documented in the REST spec; clients can request the full diff via the REST endpoint referenced in the payload.

### `_ah/session/new`

Harbor-specific analogue of ACP `session/new` that lets the client describe multiple model/agent pairs and spawn several sessions in one call.

```json
{
  "jsonrpc": "2.0",
  "id": 9,
  "method": "_ah/session/new",
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

For compatibility, the client can still call the standard ACP `session/new` if it only needs a single session; `_ah/session/new` is optional and gated by the `agentHarborSnapshots` capability block (a client that doesn’t know about it won’t call it). Harbor mirrors the underlying behavior to REST (`POST /api/v1/tasks`), so the IDE sees the same lifecycle as other Agent Harbor entry points.

### `_ah/snapshot_create`

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

### `_ah/snapshot_list`

Read-only listing with pagination/filtering (`branchable`, `provider`, `createdAfter`, etc.).

### `_ah/branch_create`

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

### `_ah/tool_pipeline_list`

Returns the pipelines (and steps) detected for a specific execution.

```json
{
  "jsonrpc": "2.0",
  "id": 40,
  "method": "_ah/tool_pipeline_list",
  "params": {
    "sessionId": "sess-01HV…",
    "executionId": "exec-01HV…"
  }
}
```

Response:

```json
{
  "jsonrpc": "2.0",
  "id": 40,
  "result": {
    "pipelines": [
      {
        "pipelineId": "pipe-01HV…",
        "startedAt": "2025-02-01T12:35:00Z",
        "endedAt": "2025-02-01T12:35:05Z",
        "bytes": { "stdout": 8192, "stderr": 0 },
        "steps": [
          { "stepId": "pipe-01HV…:1", "command": "npm run build", "stdoutBytes": 4096, "stderrBytes": 0 },
          { "stepId": "pipe-01HV…:2", "command": "grep ERROR", "stdoutBytes": 4096, "stderrBytes": 0 }
        ]
      }
    ]
  }
}
```

### `_ah/tool_pipeline_stream`

Streams stdout/stderr for a specific pipeline step. Works like the REST streaming endpoint but returns a stream handle.

```json
{
  "jsonrpc": "2.0",
  "id": 41,
  "method": "_ah/tool_pipeline_stream",
  "params": {
    "sessionId": "sess-01HV…",
    "executionId": "exec-01HV…",
    "pipelineId": "pipe-01HV…",
    "stepId": "pipe-01HV…:2",
    "stream": "stdout",
    "follow": true
  }
}
```

Response:

```json
{
  "jsonrpc": "2.0",
  "id": 41,
  "result": {
    "streamId": "stream-01HV…"
  }
}
```

The client consumes the `streamId` using ACP’s streaming transport (SSE/WebSocket). Payloads are base64-encoded byte chunks with timestamps and step metadata so IDEs can render the same modal pipeline explorer that SessionViewer exposes.

---

## Notifications (Harbor → Client)

These ride on the regular `session/update` SSE/WebSocket stream with `update.type = "custom"` and `customType = "harbor/*"`:

| Notification                   | Payload                                                         | Purpose                                                                                                                                  |
| ------------------------------ | --------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------- |
| `harbor/snapshot_created`      | `{ "sessionId", "snapshot": {…}, "reason": "auto \| manual \| branch_point" }` | Fired whenever Harbor captures a snapshot (auto or manual). Enables IDE indicators for branch points. |
| `harbor/branch_created`        | `{ "branch": {…}, "newSessions": ["sess-…", …] }`               | Notifies UI that a branch + new sessions exist.                                                                                          |
| `harbor/branch_updated`        | `{ "branch": {…} }`                                             | Status changes (running, paused, merged).                                                                                                |
| `harbor/branch_deleted`        | `{ "branchId" }`                                                | Cleanup confirmation.                                                                                                                    |
| `harbor/timeline_checkpoint`   | `{ "sessionId", "executionId", "byteOffset", "label", "kind" }` | Mirrors recorder `REC_SNAPSHOT` / `REC_MARK` events so clients can draw timeline handles even before a full FS snapshot is materialized. |
| `harbor/pipeline_detected`     | `{ "sessionId", "executionId", "pipelineId", "steps": ["pipe-…:1", …] }` | Notifies clients that a new pipeline (and its steps) has been recorded so they can populate UI menus immediately. |
| `harbor/pipeline_step_updated` | `{ "sessionId", "executionId", "pipelineId", "step": {…} }`     | Sends incremental updates (bytes, status, timestamps) for a step so IDEs can update progress bars and completion state. |

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
