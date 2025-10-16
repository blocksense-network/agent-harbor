# Claude Code — Integration Notes

## Overview

Claude Code is Anthropic's official command-line interface for interacting with Claude, providing an AI-powered coding assistant with MCP (Model Context Protocol) support.

- **Website**: <https://docs.anthropic.com/en/docs/agents-and-tools/claude-code>
- **Documentation**: <https://docs.anthropic.com/en/docs/agents-and-tools/claude-code/overview>
- **GitHub**: <https://github.com/anthropics/claude-code>
- **Version**: 1.0.98 (as of this writing)

### Task start-up command

Claude Code can be started with a specific task prompt in several ways:

1. **Direct prompt**:

   ```bash
   claude "Implement a REST API endpoint for user management"
   ```

2. **Interactive session with prompt**:

   ```bash
   claude --continue "Continue working on the authentication system"
   ```

3. **Resume specific session**:

   ```bash
   claude --resume <session-id>
   ```

4. **Non-interactive mode** (for automation):

   ```bash
   claude --print "Generate unit tests for this function"
   ```

5. **With specific model**:
   ```bash
   claude --model sonnet "Refactor this legacy code"
   ```

### How to skip the initial onboarding screens on first launch of the agent?

To bypass Claude Code's initial onboarding and authentication prompts (essential for automated testing and programmatic integration), you need to set up a minimal configuration in the agent's HOME directory:

1. **Create `.claude.json` file** in HOME directory with basic user configuration:
   ```json
   {
     "installMethod": "test",
     "autoUpdates": false,
     "firstStartTime": "2025-09-23T00:00:00.000Z",
     "userID": "test-user-id",
     "projects": {}
   }
   ```

2. **Create `.claude/settings.json` file** for session-specific settings:
   ```json
   {
     "hooks": {}
   }
   ```

3. **Set environment variables** to provide API credentials and skip interactive prompts:
   ```bash
   export ANTHROPIC_API_KEY="your-api-key"
   # Use --dangerously-skip-permissions to bypass permission prompts
   claude --dangerously-skip-permissions "Your prompt here"
   ```

4. **For custom HOME directory** (useful for isolated testing):
   ```bash
   # Set up config files in custom HOME
   export HOME=/path/to/custom/home
   # Create .claude.json and .claude/settings.json as above
   claude --dangerously-skip-permissions "Your prompt here"
   ```

**Key flags for automation:**
- `--dangerously-skip-permissions`: Bypass file operation permission prompts
- `--print`: Non-interactive output mode (prints results without TUI)
- `--yes`: Automatically confirm prompts (when supported)

**Note:** The `--dangerously-skip-permissions` flag should only be used in controlled testing environments, as it allows Claude to execute file operations without user confirmation.

### Checkpointing (point-in-time restore of chat + filesystem)

Claude Code does not provide an official checkpointing mechanism that restores both chat and filesystem state to a specific moment in time. There is no built‑in facility to create file‑system snapshots or to roll back workspace changes via a checkpoint identifier.

- Scope: Chat only (see Session continuation below). No filesystem restore.
- Granularity: N/A (no checkpoints for files). No per‑step file snapshots.
- Restore semantics: N/A for filesystem. Conversation can be resumed, but not rewound to an arbitrary message boundary.

### Session continuation (conversation resume)

Claude Code supports resuming conversations across sessions. This is distinct from checkpointing and does not restore filesystem state.

- **Session resumption**: Use `--resume [sessionId]` to continue from a previous conversation
- **Continue mode**: `--continue` resumes the most recent conversation
- **Session IDs**: Each conversation has a unique session ID for targeted resumption via `--session-id <uuid>`
- **Automatic session tracking**: Conversations are automatically saved and can be resumed
- **Granularity**: No official support to resume at an arbitrary message/tool step inside a session

### Where are chat sessions stored?

- Claude Code writes conversation transcripts to per‑project storage; hook inputs expose an absolute `transcript_path` pointing to a JSONL transcript file (example shape: `~/.claude/projects/<project-id>/<session-id>.jsonl`). Exact locations may vary by OS and configuration.
- `<project-id>`: An internal identifier that maps to the current project context (often derived from the working directory or internal project registry). It is not user‑set; infer it empirically by starting a short session and inspecting the parent directory of `transcript_path`.

Empirical steps to determine `<project-id>` and validate trimming:

- Start a minimal session in this repo (e.g., ask Claude to list repo files) and enable `--debug` to surface paths in logs.
- Add a simple `PostToolUse` hook that prints its input JSON to a temp file to capture `transcript_path`.
- Inspect the path to extract `<project-id>` and `<session-id>`. Back up the transcript, trim a trailing message block, and resume to observe behavior.

### What is the format of the persistent chat sessions?

- Conversation history is persisted as line‑delimited JSON (JSONL) transcripts. While it is technically possible to trim transcripts, there is no documented, supported procedure to manually edit transcripts for partial restores; prefer built‑in resume options.

### How is the use of MCP servers configured?

Claude Code provides comprehensive MCP server configuration through multiple methods:

**Command-line options:**

- `--mcp-config <configs...>`: Load MCP servers from JSON files or strings (space-separated)
- `--strict-mcp-config`: Only use MCP servers from --mcp-config, ignoring all other MCP configurations
- `--mcp-debug`: [DEPRECATED] Enable MCP debug mode (shows MCP server errors)
- `--settings <file-or-json>`: Load session-specific settings including MCP configuration

**MCP management commands:**

- `claude mcp serve`: Start the Claude Code MCP server
- `claude mcp add <name> <commandOrUrl> [args...]`: Add a server (stdio or URL-based)
- `claude mcp remove <name>`: Remove an MCP server
- `claude mcp list`: List configured MCP servers
- `claude mcp get <name>`: Get details about an MCP server
- `claude mcp add-json <name> <json>`: Add an MCP server with a JSON string
- `claude mcp add-from-claude-desktop`: Import MCP servers from Claude Desktop (Mac and WSL only)
- `claude mcp reset-project-choices`: Reset all approved and rejected project-scoped (.mcp.json) servers

**Configuration files:**

- Project-specific: `.mcp.json` files for project-scoped MCP configurations
- Global MCP server configurations stored in Claude Code's configuration directory

**Environment variables:**

- No specific MCP-related environment variables documented in help screens

### Support for custom hooks

**YES, Claude Code supports custom hooks for the Agent Time Travel feature!** Claude Code has a comprehensive hook system that allows executing custom commands after every agent step (tool use).

#### Hook Configuration

Hooks are configured through settings files with the following hierarchy:

- `~/.claude/settings.json` - User global settings
- `.claude/settings.json` - Project-specific settings
- `.claude/settings.local.json` - Local project settings (not committed)
- Enterprise managed policy settings

#### Session-Specific Hook Configuration

**Yes, hooks can be enabled for particular sessions using the `--settings` option:**

```bash
# Use session-specific settings file
claude --settings /path/to/session-hooks.json "Start coding session"

# Or use inline JSON
claude --settings '{"hooks":{"PostToolUse":[{"matcher":"*","hooks":[{"type":"command","command":"echo \"Tool executed\" >> /tmp/agent-log.txt"}]}]}}' "Start session"
```

#### Hook Events for Time Travel

The most relevant hook event for Agent Time Travel is `PostToolUse`:

```json
{
  "hooks": {
    "PostToolUse": [
      {
        "matcher": "*",
        "hooks": [
          {
            "type": "command",
            "command": "$CLAUDE_PROJECT_DIR/.claude/hooks/time-travel-log.sh",
            "timeout": 30
          }
        ]
      }
    ]
  }
}
```

#### Available Hook Events

- **PreToolUse**: Runs before tool execution
- **PostToolUse**: Runs after tool execution (perfect for Agent Time Travel)
- **UserPromptSubmit**: Runs when user submits a prompt
- **Stop**: Runs when agent stops
- **SubagentStop**: Runs when subagent stops
- **SessionStart**: Runs when session starts
- **SessionEnd**: Runs when session ends
- **Notification**: Runs on notifications

#### Hook Input/Output

Hooks receive JSON data via stdin and can return structured responses:

- **Input**: Session info, tool details, event data
- **Output**: Exit codes (0=success, 2=blocking error) or JSON responses
- **Environment**: `$CLAUDE_PROJECT_DIR` available for project-relative paths

#### Example Time Travel Hook

```bash
#!/bin/bash
# .claude/hooks/time-travel-log.sh
echo "$(date): Tool ${TOOL_NAME} executed in session ${SESSION_ID}" >> /tmp/agent-time-travel.log
```

This demonstrates that Claude Code has **full support** for the Agent Time Travel feature through its comprehensive hook system.

### Credentials

Claude Code stores credentials and configuration in the following precise locations:

**Authentication methods:**

- **Token setup**: `claude setup-token` for long-lived authentication (requires Claude subscription)
- **OAuth/browser authentication**: Automatic browser-based authentication for Claude API access

**Configuration files:**

- **Global config**: Stored in OS-specific application data directories (e.g., `~/.config/claude-code/` on Linux, `%APPDATA%\claude-code\` on Windows)
- **Local/project config**: Project-specific configuration files in the working directory
- **Settings files**: Custom JSON settings files loaded via `--settings` flag

**Environment variables:**

- Various `CLAUDE_*` environment variables for API keys and settings
- Standard Anthropic API environment variables

**Precise storage paths** (platform-dependent):

- **Linux/macOS**: `~/.config/claude-code/` or `~/.local/share/claude-code/`
- **Windows**: `%APPDATA%\claude-code\` or `%LOCALAPPDATA%\claude-code\`
- **Token storage**: Authentication tokens stored securely in OS keychain/keyring systems

The configuration system supports hierarchical settings (global → local → command-line overrides).

### Known Issues

- **Subscription requirement**: Some features require a Claude Pro subscription
- **Network dependency**: Requires internet access for Claude API communication
- **Rate limiting**: Subject to Anthropic's API rate limits
- **MCP compatibility**: Not all MCP servers may be fully compatible
- **Platform limitations**: Some features (like Claude Desktop import) are platform-specific

## Custom API Server Support

**YES, Claude Code supports custom API servers** via environment variables, contrary to some outdated assumptions. This enables testing integrations and using Claude Code with alternative LLM providers.

### Environment Variables for Custom API Servers

Claude Code respects these environment variables to redirect API calls to custom servers:

```bash
# Point Claude Code to your custom API server
export ANTHROPIC_BASE_URL="http://127.0.0.1:18080"  # Base URL without /v1 suffix
export ANTHROPIC_API_KEY="mock-key"                   # API key for authentication

# Alternative for Bearer token authentication
export ANTHROPIC_AUTH_TOKEN="your-bearer-token"

# Launch Claude Code
claude "Create hello.py that prints Hello, World!"
```

### Windows Setup

```powershell
$env:ANTHROPIC_BASE_URL = "http://127.0.0.1:18080"
$env:ANTHROPIC_API_KEY = "mock-key"
claude "Create hello.py that prints Hello, World!"
```

### API Compatibility Requirements

Your custom server should implement Anthropic's Messages API:

- **Endpoint**: `POST /v1/messages`
- **Headers**: `anthropic-version`, `x-api-key` or `Authorization: Bearer`
- **Request/Response**: Anthropic Messages API format with role/content blocks and usage counters

### Testing with Mock Servers

The agent-harbor project includes a mock API server that can be used for testing Claude Code integrations:

```bash
# Start the mock server
python tests/tools/mock-agent/start_test_server.py --host 127.0.0.1 --port 18080

# In another terminal, set environment and run Claude Code
export ANTHROPIC_BASE_URL="http://127.0.0.1:18080"
export ANTHROPIC_API_KEY="mock-key"
claude "Create hello.py"
```

### Enterprise Gateway Support

Claude Code also supports enterprise gateways via additional environment variables:

- **AWS Bedrock**: `CLAUDE_CODE_USE_BEDROCK=1`, `ANTHROPIC_BEDROCK_BASE_URL`, `CLAUDE_CODE_SKIP_BEDROCK_AUTH=1`
- **Google Vertex**: Similar flags available for Vertex AI gateways

This makes Claude Code highly flexible for enterprise deployments with custom LLM infrastructure.

## Findings — Session File Experiments (2025-09-10)

- Tooling: `specs/Research/Tools/SessionFileExperiments/claude.py` runs a minimal session (pexpect‑only) and sets up a temporary `PostToolUse` hook. It also includes a filesystem fallback to discover transcripts without relying on hooks.
- Transcript paths (observed): JSONL transcripts under `~/.claude/projects/<project-id>/<session-id>.jsonl`. On this machine and repo, the `<project-id>` resolved to:
  - `~/.claude/projects/-home-zahary-blocksense-agent-harbor-specs/<UUID>.jsonl`
- Minimal JSONL structure (observed): one JSON object per line; fields include `type` ("user"|"assistant"|"summary"), `sessionId`, `uuid`, `timestamp`, `cwd`, `version`, and a `message` object with `role`, `content` (array of parts), and optional `usage`. API errors appear as assistant entries with `isApiErrorMessage=true` and text like "Credit balance is too low".
- Hook capture: `PostToolUse` hooks fire after a successful tool step. If authentication/permissions prevent tools from running, the fallback locates the latest transcript for the current working directory and proceeds.
- Trimming: `trim_jsonl_midpoint()` safely trims sufficiently long transcripts after creating a timestamped backup. If a transcript is too short (<4 lines), the script skips trimming and advises letting the session run longer.
- Resume: After trimming, `claude --resume` continues from the truncated conversation (chat‑only). Filesystem state is not restored.

Trim test (this machine):

- Source: `~/.claude/projects/-home-zahary-blocksense-agent-harbor-specs/95d9929f-a314-472f-89d8-135f9d1c4ffc.jsonl`
- Backup: same path with `.bak-YYYYMMDD-HHMMSS`
- Output: same path with `.trimmed` (keeps first half of lines)

Reproduce locally:

- Ensure `tmux` and Python `pexpect` are installed; run from repo root:
  - `python3 specs/Research/Tools/SessionFileExperiments/claude.py`
- If hooks don’t capture `transcript_path`, the script reports the latest transcript discovered under `~/.claude/projects/<sanitized-cwd>/` and makes a backup before attempting a trim.
