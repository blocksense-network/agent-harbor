# Codex Session File Format Specification

## Overview

Codex stores session data in two distinct file formats:

1. **Rollout Files** (`.jsonl`) - Core conversation persistence for resuming sessions
2. **Session Log Files** (`.jsonl`) - Detailed UI interaction logs

Both formats use JSON Lines (JSONL) format where each line is a valid JSON object.

## 1. Rollout Files (Core Session Persistence)

### Rollout File Location and Naming

```
~/.codex/sessions/YYYY/MM/DD/rollout-YYYY-MM-DDThh-mm-ss-<uuid>.jsonl
```

- Stored in date-organized subdirectories under `~/.codex/sessions/`
- Filename includes timestamp and conversation UUID
- Used for session resumption and conversation replay

### Rollout File Structure

Each line in a rollout file is a JSON object with this structure:

```json
{
  "timestamp": "2025-08-10T03:12:26.519Z",
  "type": "session_meta",
  "payload": { ... }
}
```

### Data Types

#### RolloutLine (Root Structure)

```rust
pub struct RolloutLine {
    pub timestamp: String,  // ISO 8601 timestamp with milliseconds
    #[serde(flatten)]
    pub item: RolloutItem,  // The actual data (flattened into root object)
}
```

#### RolloutItem (Content Types)

Rollout files contain these types of items:

##### 1. SessionMeta (Session Metadata)

**Purpose**: Stores session initialization information
**Frequency**: First line of every rollout file

```json
{
  "timestamp": "2025-08-10T03:12:26.519Z",
  "type": "session_meta",
  "payload": {
    "meta": {
      "id": "8f7c4ac2-6141-42da-b4d5-7032a8e8df3b",
      "timestamp": "2025-08-10T03:12:26.519Z",
      "cwd": "/Users/user/projects/my-app",
      "originator": "codex-cli",
      "cli_version": "0.1.0",
      "instructions": null
    },
    "git": {
      "commit_hash": "abc123...",
      "branch": "main",
      "repository_url": "https://github.com/user/repo.git"
    }
  }
}
```

**Fields**:

- `meta.id`: UUID of the conversation session
- `meta.timestamp`: When the session was created
- `meta.cwd`: Working directory path
- `meta.originator`: Component that created the session
- `meta.cli_version`: Version of Codex CLI
- `meta.instructions`: Optional initial instructions
- `git`: Git repository information (optional)

##### 2. ResponseItem (Conversation Content)

**Purpose**: Stores all conversation messages, tool calls, and responses
**Frequency**: Multiple times throughout session

```json
{
  "timestamp": "2025-08-10T03:12:27.123Z",
  "type": "message",
  "payload": {
    "role": "user",
    "content": [
      {
        "type": "text",
        "text": "Hello, can you help me with this code?"
      }
    ]
  }
}
```

**ResponseItem Variants**:

###### Message

```json
{
  "type": "message",
  "role": "user|assistant|system",
  "content": [
    {
      "type": "text",
      "text": "..."
    }
  ]
}
```

###### LocalShellCall (Command Execution)

```json
{
  "type": "local_shell_call",
  "call_id": "call_abc123",
  "status": "in_progress|completed|failed",
  "action": {
    "command": "ls -la",
    "cwd": "/path/to/dir",
    "env": { "KEY": "value" }
  }
}
```

###### FunctionCall (Tool Usage)

```json
{
  "type": "function_call",
  "name": "read_file",
  "arguments": "{\"path\": \"file.txt\"}",
  "call_id": "call_xyz789"
}
```

###### Reasoning (AI Thought Process)

```json
{
  "type": "reasoning",
  "summary": [
    {
      "type": "reasoning_text",
      "text": "I need to analyze this code..."
    }
  ],
  "content": [...]
}
```

##### 3. EventMsg (Protocol Events)

**Purpose**: Records protocol-level events during conversation
**Frequency**: Throughout session as events occur

```json
{
  "timestamp": "2025-08-10T03:12:28.456Z",
  "type": "agent_message",
  "payload": {
    "message": "I'll help you analyze this code.",
    "id": "msg_123"
  }
}
```

**Common EventMsg Types**:

- `session_configured`: Session initialization
- `agent_message`: AI responses
- `user_message`: User inputs
- `exec_command_begin|end`: Command execution
- `task_started|complete`: Task lifecycle
- `token_count`: Usage statistics

##### 4. CompactedItem (Conversation Summarization)

**Purpose**: Summarized conversation content for context management
**Frequency**: When conversations are compacted to save tokens

```json
{
  "timestamp": "2025-08-10T03:15:30.789Z",
  "type": "compacted",
  "payload": {
    "message": "Previous discussion summarized: User asked about code analysis, assistant provided detailed review..."
  }
}
```

##### 5. TurnContextItem (Turn Metadata)

**Purpose**: Context information for each conversational turn
**Frequency**: At the start of each turn

```json
{
  "timestamp": "2025-08-10T03:12:26.520Z",
  "type": "turn_context",
  "payload": {
    "cwd": "/Users/user/projects/my-app",
    "approval_policy": "on_failure",
    "sandbox_policy": "workspace_write",
    "model": "gpt-5",
    "effort": "medium",
    "summary": "concise"
  }
}
```

## 2. Session Log Files (UI Interaction Logs)

### Session Log File Location and Naming

```
~/.codex/logs/session-YYYYMMDDTHHMMSSZ.jsonl
```

- Optional detailed logging (enabled via `CODEX_TUI_RECORD_SESSION=1`)
- Captures all UI interactions and events
- Used for debugging and session analysis

### Session Log File Structure

Each line is a JSON object with this structure:

```json
{
  "ts": "2025-08-10T03:12:26.500Z",
  "dir": "to_tui|from_tui|meta",
  "kind": "event_type",
  "payload": { ... }
}
```

### Session Log Event Types

#### Meta Events

```json
// Session start
{
  "ts": "2025-08-10T03:12:26.500Z",
  "dir": "meta",
  "kind": "session_start",
  "cwd": "/Users/user/projects",
  "model": "gpt-5",
  "model_provider_id": "openai"
}

// Session end
{
  "ts": "2025-08-10T03:48:49.927Z",
  "dir": "meta",
  "kind": "session_end"
}
```

#### UI Events (to_tui)

```json
// Codex protocol events
{
  "ts": "2025-08-10T03:12:26.519Z",
  "dir": "to_tui",
  "kind": "codex_event",
  "payload": { "id": "0", "msg": { "type": "agent_message", ... } }
}

// History insertions
{
  "ts": "2025-08-10T03:12:26.520Z",
  "dir": "to_tui",
  "kind": "insert_history",
  "lines": 9
}

// App events
{
  "ts": "2025-08-10T03:12:26.500Z",
  "dir": "to_tui",
  "kind": "app_event",
  "variant": "RequestRedraw"
}

// Key events
{
  "ts": "2025-08-10T03:12:28.561Z",
  "dir": "to_tui",
  "kind": "key_event",
  "event": "KeyEvent { code: Char('h'), modifiers: KeyModifiers(0x0), kind: Press, state: KeyEventState(0x0) }"
}
```

#### User Actions (from_tui)

```json
// User operations
{
  "ts": "2025-08-10T03:12:30.123Z",
  "dir": "from_tui",
  "kind": "op",
  "payload": {
    "type": "send_message",
    "content": "Please analyze this file"
  }
}
```

## 3. Data Persistence Implementation

### RolloutRecorder

The `RolloutRecorder` manages conversation persistence:

```rust
pub struct RolloutRecorder {
    tx: Sender<RolloutCmd>,        // Async command channel
    rollout_path: PathBuf,         // File path
}
```

**Key Methods**:

- `new()`: Creates new recorder or resumes existing
- `record_items()`: Queues items for writing
- `flush()`: Ensures all writes are committed
- `shutdown()`: Cleans up resources

**Async Architecture**:

- Uses Tokio channels for non-blocking I/O
- Dedicated writer task handles file operations
- Bounded channel prevents memory issues

### SessionLogger

The `SessionLogger` handles UI event logging:

```rust
struct SessionLogger {
    file: OnceCell<Mutex<File>>,  // Thread-safe file access
}
```

**Key Features**:

- Optional logging (environment variable controlled)
- Thread-safe file operations
- Structured JSON logging
- Automatic timestamp generation

## 4. File Format Evolution

### Version Compatibility

- Files are forward-compatible within major versions
- New event types can be added without breaking old parsers
- Unknown event types are safely ignored

### Migration Strategy

- Old files remain readable
- New features add new event types
- Breaking changes require version bumps

## 5. Usage Examples

### Inspecting Rollout Files

```bash
# Pretty print with jq
jq -C . ~/.codex/sessions/2025/08/10/rollout-2025-08-10T03-12-26-8f7c4ac2.jsonl

# View with fx
fx ~/.codex/sessions/2025/08/10/rollout-2025-08-10T03-12-26-8f7c4ac2.jsonl
```

### Analyzing Session Logs

```bash
# Count events by type
jq -r '.kind' ~/.codex/logs/session-20250810T031226Z.jsonl | sort | uniq -c

# Find all user messages
jq 'select(.dir == "from_tui" and .kind == "op" and .payload.type == "send_message")' ~/.codex/logs/session-*.jsonl
```

### Programmatic Access

```rust
// Load conversation history
let history = RolloutRecorder::get_rollout_history(&path)?;

// List available conversations
let page = RolloutRecorder::list_conversations(&codex_home, 10, None)?;
```

## 6. Security Considerations

### File Permissions

- Session files created with restrictive permissions (0o600 on Unix)
- Contains potentially sensitive conversation data
- Should be encrypted for sensitive deployments

### Data Sanitization

- Command outputs may contain sensitive information
- File paths may reveal project structure
- Consider data classification for enterprise use

## 7. Performance Characteristics

### Storage Efficiency

- JSONL format is compact and streamable
- No redundant metadata per line
- Efficient for large conversation histories

### I/O Patterns

- Append-only writes for reliability
- Async I/O prevents UI blocking
- Buffered writes with explicit flushing

### Memory Usage

- Bounded async channels prevent memory leaks
- Lazy initialization of loggers
- Efficient serialization of complex objects
