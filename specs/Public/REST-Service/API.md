## Agent Harbor REST Service — Purpose and Specification

### Purpose

- **Central orchestration**: Provide a network API to create and manage isolated agent coding sessions on demand, aligned with the filesystem snapshot model described in [FS-Snapshots-Overview](../FS-Snapshots/FS-Snapshots-Overview.md).
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
  "webhooks": [{ "event": "session.completed", "url": "https://hooks.acme.dev/agents" }]
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

##### Task Execution Workflow

The server implements a sophisticated task execution workflow designed for performance and incremental builds:

1. **Snapshot Cache Management**: The server maintains a global LRU cache of filesystem snapshots for each repository and recent commit. Snapshots are created after a full build and test cycle completes successfully.

2. **Workspace Provisioning Logic**:
   - When a task starts, the server first checks if a snapshot exists for the task's starting commit
   - If a snapshot exists, it's mounted as the workspace for the agent
   - If no snapshot exists, the server acquires a global mutex to prevent concurrent provisioning, checks out the starting commit, and runs `ah agent start` directly (which will create the initial snapshot)
   - Once the agent process starts, the mutex is released since the agent handles snapshot creation

3. **Configuration Policy**:
   - The server does not specify policy flags (e.g., sandbox, runtime) to launched processes, relying instead on the configuration system
   - If the server is launched with `--config <path>`, it forwards this parameter to both `ah agent start` and `ah agent record`
   - This allows server administrators to specify consistent configuration across all agent executions via config files
   - Individual task requests cannot override server-level configuration policies

4. **Agent Command Integration**:
   - Tasks are executed using `ah agent record --session-id <id> -- <agent_command>`
   - The agent command is constructed as `ah agent start --agent <type> --cwd <workspace_path> --from-snapshot <snapshot_id> --non-interactive --prompt <task_prompt>`
   - When a cached snapshot is available, `--from-snapshot <snapshot_id>` enables fast workspace restoration
   - Configuration parameters are forwarded: `--config <server_config>` is added to both the record and start commands
   - Process output is captured by the recorder and stored in the database

5. **Snapshot Cache Strategy**:
   - Cache is global across all repositories managed by the server
   - Each repository can have an optional lower quota than the global limit
   - LRU eviction ensures the cache stays within disk capacity limits
   - Snapshots are keyed by repository URL and commit hash

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
    { "type": "thought", "thought": "Analyzing authentication flow", "ts": "2025-09-30T17:10:00Z" },
    { "type": "tool_use", "tool_name": "read_file", "tool_args": {"target_file": "src/auth.ts"}, "tool_execution_id": "tool_exec_01H...", "status": "started", "ts": "2025-09-30T17:10:02Z" },
    { "type": "tool_result", "tool_name": "read_file", "tool_output": "File read successfully", "tool_execution_id": "tool_exec_01H...", "status": "completed", "ts": "2025-09-30T17:10:05Z" },
    { "type": "file_edit", "file_path": "src/auth.ts", "lines_added": 5, "lines_removed": 2, "ts": "2025-09-30T17:10:10Z" }
  ],
  ...
}
```

**Purpose:** The `recent_events` field enables SSR to pre-populate active task cards with the last 3 events, ensuring cards never show "Waiting for agent activity" and maintain fixed height from initial page load.

#### Get Session

- `GET /api/v1/sessions/{id}` → session details including current status, workspace summary, recent events, and change statistics.

  **Response includes change statistics for completed sessions:**

  ```json
  {
    "id": "01HVZ6K9T1N8S6M3V3Q3F0X5B7",
    "status": "completed",
    "prompt": "Fix authentication bug",
    "repo": { ... },
    "agent": { ... },
    "createdAt": "2025-01-01T11:00:00Z",
    "completedAt": "2025-01-01T12:15:00Z",
    "changes": {
      "files_changed": 3,
      "lines_added": 42,
      "lines_removed": 18
    },
    "recent_events": [ ... ]
  }
  ```

  **Purpose:** The `changes` field provides aggregated file change statistics for TUI completed/merged task cards to display VS Code-style summaries like "3 files changed (+42 -18)". Only included for completed sessions.

#### Stop / Cancel

- `POST /api/v1/sessions/{id}/stop` → graceful stop (agent asked to wrap up).
- `DELETE /api/v1/sessions/{id}` → force terminate and cleanup.

#### Pause / Resume

- `POST /api/v1/sessions/{id}/pause`
- `POST /api/v1/sessions/{id}/resume`

#### Logs and Events

- `GET /api/v1/sessions/{id}/logs?tail=1000` → historical logs.
- `GET /api/v1/sessions/{id}/events` (SSE) → live status, logs, and milestones.
- `GET /api/v1/sessions/{id}/events?type=thought,file_edit&page=1&perPage=50` → paginated historical events with filtering.
- `GET /api/v1/sessions/{id}/events?since=2025-01-01T12:00:00Z&until=2025-01-01T13:00:00Z` → events within time range.

Event payload (SSE `data:` line):

```json
{
  "type": "log",
  "level": "info",
  "message": "Running tests...",
  "tool_execution_id": "tool_exec_01H...",
  "ts": "2025-01-01T12:00:00Z"
}
```

Additional event types for agent activity:

```json
{
  "type": "thought",
  "thought": "Analyzing the codebase structure to understand the authentication flow",
  "reasoning": "Need to understand current auth implementation before making changes",
  "ts": "2025-01-01T12:00:00Z"
}
```

```json
{
  "type": "tool_use",
  "tool_name": "search_codebase",
  "tool_args": { "query": "authentication", "include_pattern": "*.ts" },
  "tool_execution_id": "tool_exec_01H...",
  "status": "started",
  "ts": "2025-01-01T12:00:05Z"
}
```

```json
{
  "type": "tool_result",
  "tool_name": "search_codebase",
  "tool_output": "Found 42 matches in 12 files",
  "tool_execution_id": "tool_exec_01H...",
  "status": "completed",
  "ts": "2025-01-01T12:00:08Z"
}
```

```json
{
  "type": "file_edit",
  "file_path": "src/auth.ts",
  "lines_added": 5,
  "lines_removed": 2,
  "description": "Enhanced error handling in authenticate function",
  "ts": "2025-01-01T12:05:00Z"
}
```

**Tool Execution ID (`tool_execution_id`)**: A unique identifier assigned to each tool execution, used to correlate `tool_use`, `tool_result`, and related `log` events when multiple tools are running concurrently. This field is optional for `log` events (set to `null` when the log is not associated with a specific tool execution).

Query parameters for events endpoint:

- `type`: Filter by event types (comma-separated: `thought,tool_use,file_edit,log,status`)
- `level`: Filter by log level (`debug`, `info`, `warn`, `error`)
- `since`, `until`: Time range filtering (ISO 8601 timestamps)
- `page`, `perPage`: Pagination (default: 50, max: 200)
- `sort`: Sort order (`asc`, `desc`) (default: desc for newest first)

#### File Operations and Diffs

Endpoints for TaskDetails page file browsing and diff viewing:

- `GET /api/v1/sessions/{id}/files` → List all files modified during the session.

  Response `200 OK`:

  ```json
  {
    "items": [
      {
        "path": "src/auth.ts",
        "status": "modified",
        "lines_added": 5,
        "lines_removed": 2,
        "last_modified": "2025-01-01T12:05:00Z",
        "size_bytes": 2048,
        "change_type": "content"
      },
      {
        "path": "tests/auth.test.ts",
        "status": "added",
        "lines_added": 25,
        "lines_removed": 0,
        "last_modified": "2025-01-01T12:10:00Z",
        "size_bytes": 1024,
        "change_type": "content"
      }
    ],
    "total": 2
  }
  ```

  Query parameters:
  - `status`: Filter by file status (`added`, `modified`, `deleted`, `renamed`)
  - `path`: Filter by file path (supports wildcards)
  - `page`, `perPage`: Pagination

- `GET /api/v1/sessions/{id}/files/{filePath}` → Get detailed file information and metadata.

  Response `200 OK`:

  ```json
  {
    "path": "src/auth.ts",
    "status": "modified",
    "lines_added": 5,
    "lines_removed": 2,
    "last_modified": "2025-01-01T12:05:00Z",
    "size_bytes": 2048,
    "change_type": "content",
    "encoding": "utf-8",
    "mime_type": "text/plain",
    "history": [
      {
        "timestamp": "2025-01-01T12:05:00Z",
        "event_type": "file_edit",
        "description": "Updated authentication logic"
      }
    ]
  }
  ```

- `GET /api/v1/sessions/{id}/diff/{filePath}?context=3&full=false` → Get file diff with configurable context.

  Response `200 OK`:

  ```json
  {
    "path": "src/auth.ts",
    "status": "modified",
    "diff": "@@ -10,7 +10,12 @@ function authenticate(user) {\n-  return user.isActive;\n+  if (!user) {\n+    throw new Error('User required');\n+  }\n+\n+  if (!user.isActive) {\n+    throw new Error('User not active');\n+  }\n+\n+  return true;\n",
    "lines_added": 5,
    "lines_removed": 2,
    "context_lines": 3,
    "full_file": false,
    "file_content": null,
    "hunks": [
      {
        "old_start": 10,
        "old_lines": 7,
        "new_start": 10,
        "new_lines": 12,
        "content": "@@ -10,7 +10,12 @@ function authenticate(user) {\n-  return user.isActive;\n+  if (!user) {\n+    throw new Error('User required');\n+  }\n+\n+  if (!user.isActive) {\n+    throw new Error('User not active');\n+  }\n+\n+  return true;\n"
      }
    ]
  }
  ```

  Query parameters:
  - `context`: Number of context lines before/after changes (default: 3, max: 10)
  - `full`: Include full file content instead of just diff (default: false)
  - `format`: Diff format (`unified`, `split`, `html`) (default: unified)

- `GET /api/v1/sessions/{id}/diff?files=src/auth.ts,tests/auth.test.ts&format=html` → Get diffs for multiple files.

  Response `200 OK`:

  ```json
  {
    "files": [
      {
        "path": "src/auth.ts",
        "status": "modified",
        "diff": "@@ -10,7 +10,12 @@ ...",
        "lines_added": 5,
        "lines_removed": 2,
        "context_lines": 3,
        "full_file": false
      },
      {
        "path": "tests/auth.test.ts",
        "status": "added",
        "diff": "@@ -0,0 +1,25 @@ ...",
        "lines_added": 25,
        "lines_removed": 0,
        "context_lines": 3,
        "full_file": false
      }
    ],
    "total_lines_added": 30,
    "total_lines_removed": 2
  }
  ```

  Query parameters:
  - `files`: Comma-separated list of file paths to include (if not specified, returns all modified files)
  - `context`: Context lines for all files (default: 3)
  - `full`: Include full file content for all files (default: false)
  - `format`: Diff format (`unified`, `split`, `html`) (default: unified)

- `GET /api/v1/sessions/{id}/workspace/files?path=src&recursive=true` → Browse workspace file tree.

  Response `200 OK`:

  ```json
  {
    "path": "src",
    "type": "directory",
    "children": [
      {
        "path": "src/auth.ts",
        "type": "file",
        "size_bytes": 2048,
        "last_modified": "2025-01-01T12:05:00Z",
        "is_modified": true
      },
      {
        "path": "src/utils",
        "type": "directory",
        "children": [...]
      }
    ]
  }
  ```

  Query parameters:
  - `path`: Directory path to list (default: root)
  - `recursive`: Include subdirectories (default: false)
  - `modified_only`: Show only files modified during session (default: false)

#### Chat and Context Management

Endpoints for interactive chat interface with agents:

- `GET /api/v1/sessions/{id}/chat` → Get chat history for the session.

  Response `200 OK`:

  ```json
  {
    "messages": [
      {
        "id": "msg-01HVZ6K9T1N8S6M3V3Q3F0X5B7",
        "role": "user",
        "content": "Please fix the authentication logic in src/auth.ts",
        "timestamp": "2025-01-01T12:00:00Z",
        "attachments": []
      },
      {
        "id": "msg-01HVZ6K9T1N8S6M3V3Q3F0X5B8",
        "role": "assistant",
        "content": "I'll analyze the authentication code and fix the issues I find.",
        "timestamp": "2025-01-01T12:00:05Z",
        "attachments": [],
        "tool_calls": [
          {
            "id": "call_01HVZ6K9T1N8S6M3V3Q3F0X5B9",
            "type": "function",
            "function": {
              "name": "read_file",
              "arguments": {
                "target_file": "src/auth.ts"
              }
            }
          }
        ]
      }
    ],
    "total": 2
  }
  ```

  Query parameters:
  - `limit`: Maximum messages to return (default: 50, max: 200)
  - `before`: Return messages before this message ID
  - `after`: Return messages after this message ID

- `POST /api/v1/sessions/{id}/chat/messages` → Send a message to the agent.

  Request:

  ```json
  {
    "content": "Please review and fix the authentication logic",
    "attachments": [
      {
        "type": "file",
        "file_path": "src/auth.ts",
        "content": "function authenticate(user) { ... }"
      }
    ],
    "context_files": ["src/auth.ts", "tests/auth.test.ts"],
    "tools_enabled": ["read_file", "search_codebase", "run_terminal_cmd"],
    "model_override": "claude-3-5-sonnet-20241022"
  }
  ```

  Response `201 Created`:

  ```json
  {
    "id": "msg-01HVZ6K9T1N8S6M3V3Q3F0X5B7",
    "role": "user",
    "content": "Please review and fix the authentication logic",
    "timestamp": "2025-01-01T12:00:00Z",
    "attachments": [...],
    "context_files": ["src/auth.ts", "tests/auth.test.ts"],
    "tools_enabled": ["read_file", "search_codebase", "run_terminal_cmd"],
    "model_override": "claude-3-5-sonnet-20241022"
  }
  ```

- `GET /api/v1/sessions/{id}/context` → Get current context window usage and configuration.

  Response `200 OK`:

  ```json
  {
    "context_window": {
      "total_tokens": 8192,
      "used_tokens": 3456,
      "remaining_tokens": 4736,
      "percentage_used": 42.2,
      "input_tokens": 2048,
      "output_tokens": 1408
    },
    "context_files": [
      {
        "path": "src/auth.ts",
        "tokens": 512,
        "last_modified": "2025-01-01T12:05:00Z"
      },
      {
        "path": "tests/auth.test.ts",
        "tokens": 256,
        "last_modified": "2025-01-01T12:10:00Z"
      }
    ],
    "enabled_tools": ["read_file", "search_codebase", "run_terminal_cmd"],
    "active_model": {
      "type": "claude-3-5-sonnet-20241022",
      "context_window": 8192,
      "input_pricing": 0.003,
      "output_pricing": 0.015
    }
  }
  ```

- `PUT /api/v1/sessions/{id}/context` → Update context configuration.

  Request:

  ```json
  {
    "add_files": ["src/utils.ts"],
    "remove_files": ["tests/old.test.ts"],
    "enable_tools": ["grep_search"],
    "disable_tools": ["run_terminal_cmd"],
    "model_override": "gpt-4"
  }
  ```

  Response `200 OK`:

  ```json
  {
    "context_window": { ... },
    "context_files": [ ... ],
    "enabled_tools": [ ... ],
    "active_model": { ... }
  }
  ```

- `GET /api/v1/sessions/{id}/models` → Get available models and their capabilities.

  Response `200 OK`:

  ```json
  {
    "models": [
      {
        "id": "claude-3-5-sonnet-20241022",
        "name": "Claude 3.5 Sonnet",
        "provider": "anthropic",
        "context_window": 8192,
        "input_pricing": 0.003,
        "output_pricing": 0.015,
        "capabilities": ["function_calling", "vision", "code_execution"],
        "status": "available"
      },
      {
        "id": "gpt-4",
        "name": "GPT-4",
        "provider": "openai",
        "context_window": 8192,
        "input_pricing": 0.03,
        "output_pricing": 0.06,
        "capabilities": ["function_calling", "vision"],
        "status": "available"
      }
    ]
  }
  ```

- `POST /api/v1/sessions/{id}/chat/messages/{messageId}/retry` → Retry a failed message or regenerate response.

  Response `200 OK`:

  ```json
  {
    "id": "msg-01HVZ6K9T1N8S6M3V3Q3F0X5B8",
    "role": "assistant",
    "content": "Let me try a different approach to fix the authentication logic.",
    "timestamp": "2025-01-01T12:01:00Z",
    "attachments": [],
    "tool_calls": [...]
  }
  ```

- `DELETE /api/v1/sessions/{id}/chat/messages/{messageId}` → Delete a message from chat history.

  Response `204 No Content`

#### Advanced Chat Features

Additional endpoints for sophisticated chat interface functionality:

- `GET /api/v1/sessions/{id}/workspace/search/files?q={query}&type={type}&limit={limit}` → Search and autocomplete file paths in workspace.

  Response `200 OK`:

  ```json
  {
    "query": "src/auth",
    "files": [
      {
        "path": "src/auth.ts",
        "type": "file",
        "size_bytes": 2048,
        "last_modified": "2025-01-01T12:05:00Z",
        "is_modified": true,
        "preview": "function authenticate(user) {\n  if (!user) {\n    throw new Error('User required');\n  }\n  // ...",
        "relevance_score": 0.95
      },
      {
        "path": "src/auth-utils.ts",
        "type": "file",
        "size_bytes": 1024,
        "last_modified": "2025-01-01T12:00:00Z",
        "is_modified": false,
        "preview": "export function validateToken(token) {\n  return token && token.length > 0;\n}",
        "relevance_score": 0.87
      }
    ],
    "directories": [
      {
        "path": "src/auth/",
        "type": "directory",
        "file_count": 3,
        "last_modified": "2025-01-01T12:05:00Z"
      }
    ],
    "total": 5
  }
  ```

  Query parameters:
  - `q`: Search query (supports fuzzy matching, partial paths)
  - `type`: Filter by type (`file`, `directory`, `both`) (default: both)
  - `limit`: Maximum results (default: 20, max: 100)
  - `include_preview`: Include file content preview (default: true for files under 10KB)
  - `modified_only`: Show only files modified during session (default: false)

- `GET /api/v1/sessions/{id}/files/{filePath}/preview` → Get file content preview for attachment consideration.

  Response `200 OK`:

  ```json
  {
    "path": "src/auth.ts",
    "content_preview": "function authenticate(user) {\n  if (!user) {\n    throw new Error('User required');\n  }\n\n  if (!user.isActive) {\n    throw new Error('User not active');\n  }\n\n  return true;\n}",
    "language": "typescript",
    "size_bytes": 2048,
    "lines": 15,
    "estimated_tokens": 128,
    "is_binary": false,
    "encoding": "utf-8"
  }
  ```

  Query parameters:
  - `max_lines`: Maximum lines to preview (default: 20, max: 50)
  - `max_bytes`: Maximum bytes to preview (default: 5000, max: 10000)

- `POST /api/v1/sessions/{id}/chat/messages/{messageId}/attachments` → Add file attachments to an existing message.

  Request:

  ```json
  {
    "attachments": [
      {
        "type": "file",
        "file_path": "src/auth.ts",
        "include_full_content": true,
        "preview_lines": 20
      },
      {
        "type": "image",
        "data_url": "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAA...",
        "filename": "screenshot.png"
      }
    ]
  }
  ```

  Response `201 Created`:

  ```json
  {
    "id": "msg-01HVZ6K9T1N8S6M3V3Q3F0X5B7",
    "attachments": [
      {
        "id": "att-01HVZ6K9T1N8S6M3V3Q3F0X5B8",
        "type": "file",
        "file_path": "src/auth.ts",
        "content": "function authenticate(user) { ... }",
        "size_bytes": 2048,
        "token_count": 128
      }
    ]
  }
  ```

- `GET /api/v1/sessions/{id}/chat/messages/{messageId}/attachments/{attachmentId}` → Get specific attachment content.

  Response `200 OK`:

  ```json
  {
    "id": "att-01HVZ6K9T1N8S6M3V3Q3F0X5B8",
    "type": "file",
    "file_path": "src/auth.ts",
    "content": "function authenticate(user) {\n  if (!user) {\n    throw new Error('User required');\n  }\n  // ... full file content",
    "size_bytes": 2048,
    "token_count": 128,
    "created_at": "2025-01-01T12:00:00Z"
  }
  ```

- `DELETE /api/v1/sessions/{id}/chat/messages/{messageId}/attachments/{attachmentId}` → Remove attachment from message.

  Response `204 No Content`

- `PUT /api/v1/sessions/{id}/chat/messages/{messageId}` → Edit an existing message.

  Request:

  ```json
  {
    "content": "Updated message content",
    "attachments": [
      {
        "id": "att-01HVZ6K9T1N8S6M3V3Q3F0X5B8",
        "include_full_content": false,
        "preview_lines": 10
      }
    ]
  }
  ```

  Response `200 OK`:

  ```json
  {
    "id": "msg-01HVZ6K9T1N8S6M3V3Q3F0X5B7",
    "content": "Updated message content",
    "edited_at": "2025-01-01T12:05:00Z",
    "attachments": [...]
  }
  ```

- `GET /api/v1/sessions/{id}/chat/threads` → Get conversation threads for branching discussions.

  Response `200 OK`:

  ```json
  {
    "threads": [
      {
        "id": "thread-01HVZ6K9T1N8S6M3V3Q3F0X5B7",
        "title": "Authentication fixes",
        "message_count": 5,
        "last_activity": "2025-01-01T12:10:00Z",
        "participants": ["user", "assistant"],
        "status": "active"
      }
    ],
    "total": 1
  }
  ```

- `POST /api/v1/sessions/{id}/chat/threads` → Create a new conversation thread.

  Request:

  ```json
  {
    "title": "Database optimization",
    "initial_message": {
      "content": "Let's discuss optimizing the database queries",
      "context_files": ["src/database.ts"]
    }
  }
  ```

  Response `201 Created`:

  ```json
  {
    "id": "thread-01HVZ6K9T1N8S6M3V3Q3F0X5B9",
    "title": "Database optimization",
    "created_at": "2025-01-01T12:15:00Z",
    "initial_message_id": "msg-01HVZ6K9T1N8S6M3V3Q3F0X5B10"
  }
  ```

- `GET /api/v1/sessions/{id}/chat/suggestions` → Get contextual suggestions for chat input.

  Response `200 OK`:

  ```json
  {
    "context_suggestions": [
      {
        "type": "file",
        "title": "Fix authentication logic",
        "description": "src/auth.ts has validation issues",
        "file_path": "src/auth.ts",
        "relevance": "high"
      },
      {
        "type": "action",
        "title": "Run tests",
        "description": "Execute the test suite to verify changes",
        "command": "npm test",
        "relevance": "medium"
      }
    ],
    "quick_actions": [
      {
        "id": "run_tests",
        "label": "Run Tests",
        "icon": "play",
        "action": "run_terminal_cmd",
        "params": { "command": "npm test" }
      },
      {
        "id": "format_code",
        "label": "Format Code",
        "icon": "format",
        "action": "run_terminal_cmd",
        "params": { "command": "prettier --write ." }
      }
    ]
  }
  ```

- `POST /api/v1/sessions/{id}/chat/stream` → Stream chat response in real-time (alternative to polling).

  Headers:

  ```
  Content-Type: application/json
  Accept: text/event-stream
  ```

  Request:

  ```json
  {
    "content": "Please analyze this code",
    "attachments": [...],
    "context_files": ["src/auth.ts"],
    "stream": true
  }
  ```

  SSE Response:

  ```
  event: message_start
  data: {"id": "msg-01HVZ6K9T1N8S6M3V3Q3F0X5B7", "timestamp": "2025-01-01T12:00:00Z"}

  event: content_delta
  data: {"delta": "I'll analyze the authentication code"}

  event: content_delta
  data: {"delta": " and fix the issues I find."}

  event: tool_call
  data: {"tool_name": "read_file", "tool_args": {"target_file": "src/auth.ts"}}

  event: message_complete
  data: {"id": "msg-01HVZ6K9T1N8S6M3V3Q3F0X5B7", "final_content": "I'll analyze..."}
  ```

- `GET /api/v1/sessions/{id}/files/search` → Advanced file search for context inclusion.

  Response `200 OK`:

  ```json
  {
    "results": [
      {
        "path": "src/auth.ts",
        "matches": [
          {
            "line": 5,
            "content": "function authenticate(user) {",
            "line_number": 5,
            "match_type": "function_definition"
          }
        ],
        "score": 0.95,
        "language": "typescript"
      }
    ],
    "facets": {
      "languages": { "typescript": 15, "javascript": 8, "json": 3 },
      "file_types": { ".ts": 15, ".js": 8, ".json": 3 },
      "directories": { "src": 20, "tests": 6 }
    },
    "total": 26
  }
  ```

  Query parameters:
  - `q`: Search query (supports regex, fuzzy matching)
  - `path`: Restrict search to specific paths
  - `type`: File type filter (e.g., `ts`, `js`, `json`)
  - `include_pattern`: Glob pattern for file inclusion
  - `exclude_pattern`: Glob pattern for file exclusion
  - `case_sensitive`: Case sensitive search (default: false)
  - `limit`: Maximum results (default: 50, max: 200)

#### Session Timeline and Time-Travel

Endpoints for session history navigation, branching, and time-travel functionality:

- `GET /api/v1/sessions/{id}/timeline` → Get session timeline with moments and snapshots for time-travel navigation.

  Response `200 OK`:

  ```json
  {
    "sessionId": "01HVZ6K9T1N8S6M3V3Q3F0X5B7",
    "durationSec": 1234.5,
    "currentTime": 567.8,
    "recording": {
      "format": "cast",
      "uri": "s3://bucket/session-recordings/01HVZ6K9T1N8S6M3V3Q3F0X5B7.cast",
      "width": 120,
      "height": 30,
      "hasInput": true
    },
    "moments": [
      {
        "id": "m1",
        "ts": 12.34,
        "label": "git clone",
        "kind": "auto",
        "type": "tool_boundary",
        "description": "Cloned repository"
      },
      {
        "id": "m2",
        "ts": 45.67,
        "label": "tests passed",
        "kind": "manual",
        "type": "milestone",
        "description": "All tests passing"
      }
    ],
    "fsSnapshots": [
      {
        "id": "s1",
        "ts": 12.4,
        "label": "post-clone",
        "provider": "btrfs",
        "snapshot": {
          "id": "repo@tt-001",
          "mount": "/.snapshots/repo@tt-001",
          "size_bytes": 10485760
        },
        "branchable": true
      },
      {
        "id": "s2",
        "ts": 45.7,
        "label": "tests-passed",
        "provider": "btrfs",
        "snapshot": {
          "id": "repo@tt-002",
          "mount": "/.snapshots/repo@tt-002",
          "size_bytes": 10485760
        },
        "branchable": true
      }
    ]
  }
  ```

- `POST /api/v1/sessions/{id}/fs-snapshots` → Create a manual filesystem snapshot at current time.

  Request:

  ```json
  {
    "label": "manual-snapshot",
    "description": "User-requested snapshot for time-travel branching"
  }
  ```

  Response `201 Created`:

  ```json
  {
    "id": "s3",
    "ts": 123.45,
    "label": "manual-snapshot",
    "provider": "btrfs",
    "snapshot": {
      "id": "repo@tt-003",
      "mount": "/.snapshots/repo@tt-003",
      "size_bytes": 10485760
    }
  }
  ```

- `POST /api/v1/sessions/{id}/moments` → Create a manual session moment at current time.

  Request:

  ```json
  {
    "label": "checkpoint",
    "description": "Manual checkpoint for time-travel",
    "type": "milestone"
  }
  ```

  Response `201 Created`:

  ```json
  {
    "id": "m3",
    "ts": 123.45,
    "label": "checkpoint",
    "kind": "manual",
    "type": "milestone"
  }
  ```

- `POST /api/v1/sessions/{id}/seek` → Seek session player to a specific timestamp or snapshot for inspection.

  Request:

  ```json
  {
    "ts": 45.67,
    "fsSnapshotId": "s2",
    "mountReadonly": true,
    "pausePlayer": true
  }
  ```

  Response `200 OK`:

  ```json
  {
    "seekTime": 45.67,
    "fsSnapshotId": "s2",
    "mountPath": "/tmp/ah-seek-01HVZ6K9T1N8S6M3V3Q3F0X5B7",
    "playerPaused": true,
    "workspaceView": "readonly"
  }
  ```

- `POST /api/v1/sessions/{id}/session-branch` → Create a new session branch from a timestamp or snapshot.

  Request:

  ```json
  {
    "fromTs": 45.67,
    "fsSnapshotId": "s2",
    "name": "fix-auth-alternative",
    "injectedMessage": "Try a different approach to fix the authentication logic",
    "autoSummarize": true
  }
  ```

  Response `201 Created`:

  ```json
  {
    "id": "01HVZ6K9T1N8S6M3V3Q3F0X5B8",
    "name": "fix-auth-alternative",
    "parentSessionId": "01HVZ6K9T1N8S6M3V3Q3F0X5B7",
    "branchFromTs": 45.67,
    "fsSnapshotId": "s2",
    "injectedMessageId": "msg-01HVZ6K9T1N8S6M3V3Q3F0X5B9",
    "workspaceMount": "/tmp/ah-branch-01HVZ6K9T1N8S6M3V3Q3F0X5B8",
    "status": "provisioning"
  }
  ```

- `GET /api/v1/sessions/{id}/fs-snapshots` → List all filesystem snapshots for the session.

  Response `200 OK`:

  ```json
  {
    "snapshots": [
      {
        "id": "s1",
        "ts": 12.4,
        "label": "post-clone",
        "provider": "btrfs",
        "snapshot": {
          "id": "repo@tt-001",
          "mount": "/.snapshots/repo@tt-001",
          "size_bytes": 10485760,
          "created_at": "2025-01-01T12:00:12Z"
        },
        "branchable": true,
        "used_in_branches": ["01HVZ6K9T1N8S6M3V3Q3F0X5B8"]
      }
    ],
    "total": 1
  }
  ```

  Query parameters:
  - `branchable`: Filter for snapshots that can be used for branching (default: true)
  - `limit`: Maximum snapshots to return (default: 50, max: 200)

- `POST /api/v1/sessions/{id}/summarize` → Generate a short summary name for a session or branch.

  Request:

  ```json
  {
    "prompt": "Fix authentication logic using JWT tokens instead of sessions",
    "maxLength": 30,
    "style": "kebab-case"
  }
  ```

  Response `200 OK`:

  ```json
  {
    "summary": "jwt-auth-refactor",
    "confidence": 0.85,
    "alternatives": ["fix-jwt-auth", "jwt-authentication", "auth-jwt-migration"]
  }
  ```

- `GET /api/v1/sessions/{id}/branches` → List all session branches (sub-sessions).

  Response `200 OK`:

  ```json
  {
    "branches": [
      {
        "id": "01HVZ6K9T1N8S6M3V3Q3F0X5B8",
        "name": "jwt-auth-refactor",
        "parentSessionId": "01HVZ6K9T1N8S6M3V3Q3F0X5B7",
        "branchFromTs": 45.67,
        "fsSnapshotId": "s2",
        "injectedMessageId": "msg-01HVZ6K9T1N8S6M3V3Q3F0X5B9",
        "createdAt": "2025-01-01T12:15:00Z",
        "status": "running"
      }
    ],
    "total": 1
  }
  ```

- `GET /api/v1/sessions/{id}/recording?startTime=0&endTime=100` → Get session recording data for playback.

  Response `200 OK`:

  ```json
  {
    "format": "cast",
    "width": 120,
    "height": 30,
    "events": [
      [12.34, "o", "git clone https://github.com/user/repo.git\r\n"],
      [12.45, "o", "Cloning into 'repo'...\r\n"],
      [45.67, "o", "Running tests...\r\n"]
    ],
    "moments": [
      {
        "ts": 12.34,
        "id": "m1",
        "label": "git clone"
      }
    ]
  }
  ```

  Query parameters:
  - `startTime`: Start time in seconds (default: 0)
  - `endTime`: End time in seconds (default: session duration)
  - `format`: Response format (`cast`, `ttyrec`) (default: cast)

- `GET /api/v1/sessions/{id}/files/{filePath}/content` → Get full file content for diff viewer.

  Response `200 OK`:

  ```json
  {
    "path": "src/auth.ts",
    "content": "function authenticate(user) {\n  if (!user) {\n    throw new Error('User required');\n  }\n\n  if (!user.isActive) {\n    throw new Error('User not active');\n  }\n\n  return true;\n}",
    "encoding": "utf-8",
    "size_bytes": 2048,
    "last_modified": "2025-01-01T12:05:00Z",
    "is_modified": true
  }
  ```

- `GET /api/v1/sessions/{id}/workspace/info` → Get workspace summary and metadata.

  Response `200 OK`:

  ```json
  {
    "id": "ws-01HVZ6K9T1N8S6M3V3Q3F0X5B7",
    "session_id": "01HVZ6K9T1N8S6M3V3Q3F0X5B7",
    "root_path": "/workspace",
    "snapshot_provider": "overlay",
    "size_bytes": 104857600,
    "file_count": 1247,
    "created_at": "2025-01-01T11:00:00Z",
    "mount_path": "/tmp/ah-workspace-01HVZ6K9T1...",
    "executor_id": "executor-linux-01"
  }
  ```

Additional workspace endpoints (for IDE integration and advanced features):

- `POST /api/v1/sessions/{id}/open/ide` → Launch IDE connected to workspace (existing endpoint enhanced).
- `GET /api/v1/sessions/{id}/workspace/download` → Download workspace as archive.
- `POST /api/v1/sessions/{id}/workspace/snapshot` → Create named snapshot of current workspace state.

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

#### Repository Branches

- `GET /api/v1/repositories/{id}/branches` → List branches for a specific repository.

  Response `200 OK`:

  ```json
  {
    "repositoryId": "repo_001",
    "branches": [
      {
        "name": "main",
        "is_default": true,
        "last_commit": "a1b2c3d4..."
      },
      {
        "name": "develop",
        "is_default": false,
        "last_commit": "e5f6g7h8..."
      },
      {
        "name": "feature/auth",
        "is_default": false,
        "last_commit": "i9j0k1l2..."
      }
    ]
  }
  ```

  The server retrieves branch information by querying the local VCS repository associated with the given repository ID. Results may be cached to improve performance for repeated queries.

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
  - `GET /api/v1/repositories/{id}/branches` → List branches for a specific repository. Returns branch information including names, default branch status, and last commit info.
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
- `ah session files <SESSION_ID>` → `GET /api/v1/sessions/{id}/files` (list modified files).
- `ah session diff <SESSION_ID> [FILE]` → `GET /api/v1/sessions/{id}/diff` (show file diffs).
- `ah session timeline <SESSION_ID>` → `GET /api/v1/sessions/{id}/timeline` (get session timeline and snapshots).
- `ah session branch <SESSION_ID> <TIMESTAMP|SNAPSHOT_ID>` → `POST /api/v1/sessions/{id}/session-branch` (create session branch).
- `ah session snapshots <SESSION_ID>` → `GET /api/v1/sessions/{id}/fs-snapshots` (list session snapshots).
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
