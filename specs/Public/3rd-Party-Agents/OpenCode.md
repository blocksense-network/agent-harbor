# OpenCode — Integration Notes

> Usually this information can be obtained by checking out command-line help screens or man pages for the agent software.

## Overview

OpenCode is an open-source AI coding assistant CLI tool with TUI and headless server capabilities, supporting multiple AI providers and custom agents.

- **Website**: <https://opencode.dev/>
- **Documentation**: <https://docs.opencode.dev/>
- **GitHub**: <https://github.com/opencode-ai/opencode>
- **Version**: 0.6.3 (as of this writing)

### Task start-up command

OpenCode can be started with a specific task prompt in several ways:

1. **Interactive TUI with prompt**:

   ```bash
   opencode --prompt "Implement a user authentication system"
   ```

2. **Run command with message**:

   ```bash
   opencode run "Generate unit tests for this function"
   ```

3. **Continue last session**:

   ```bash
   opencode --continue
   ```

4. **Continue specific session**:

   ```bash
   opencode --session <session-id>
   ```

5. **With specific model**:

   ```bash
   opencode --model openai/gpt-4 "Refactor this legacy code"
   ```

6. **With custom agent**:
   ```bash
   opencode --agent my-custom-agent "Implement feature X"
   ```

### Checkpointing (point-in-time restore of chat + filesystem)

No official checkpointing mechanism is documented for OpenCode that restores both chat and filesystem state to a specific moment in time.

- Scope: No file‑system snapshotting documented.
- Restore: N/A. Use external VCS or custom workflows if rollback is required.

### Session continuation (conversation resume)

OpenCode materials reference sessions and exports, which concern conversation state rather than workspace snapshots.

- **Session continuation**: `--continue` resumes the last session
- **Session ID resumption**: `--session <id>` resumes a specific session by ID
- **Session export**: `opencode export [sessionID]` exports session data as JSON
- **Session sharing**: `--share` for sharing sessions
- **Automatic session tracking**: Sessions are automatically saved with unique IDs

### Where are chat sessions stored?

- Export format: JSON via `opencode export`
- Persistence: Sessions persist across restarts; storage directory is platform‑specific under the app’s config data directory

### What is the format of the persistent chat sessions?

- Exported JSON structure; trimming to a specific point is not documented. Prefer built‑in export and resume semantics.

### How is the use of MCP servers configured?

OpenCode does not appear to have built-in MCP (Model Context Protocol) server support based on available help documentation:

**Command-line options:**

- No MCP-related command-line options documented

**MCP management commands:**

- No MCP management commands available

**Configuration files:**

- No MCP configuration files documented

**Environment variables:**

- No MCP-related environment variables documented

Note: OpenCode may support MCP indirectly through provider integrations, but no direct MCP server configuration is documented.

### Support for custom hooks

For Agent Time Travel feature (commands executed after every agent step), OpenCode does not appear to have built-in support for custom step-level hooks. However, it does provide customization through:

1. **Custom Agents**:
   - **Agent creation**: `opencode agent create` to create new custom agents
   - **Agent selection**: `--agent` flag to specify which agent to use
   - **Agent configuration**: Agents can be customized with different behaviors and capabilities

2. **Provider Integration**:
   - **Multi-provider support**: Supports various AI providers through the model system
   - **GitHub integration**: `opencode github` command for GitHub-specific agent management
   - **Model selection**: `-m, --model` to specify provider/model combinations

3. **Server Mode**:
   - **Headless server**: `opencode serve` starts a headless server
   - **Network configuration**: `--port` and `--hostname` for server configuration
   - **API access**: Server mode enables programmatic access to OpenCode functionality

Note: While custom agents and server mode provide extensibility, there is no documented support for automatic execution of custom commands after every agent step as required for the Agent Time Travel feature.

### Credentials

OpenCode uses a provider-based authentication system with the following precise storage locations:

**Authentication methods:**

- **Provider login**: `opencode auth login [url]` to authenticate with providers
- **Logout**: `opencode auth logout` to remove credentials
- **List providers**: `opencode auth list` to see configured providers

**Configuration files:**

- **Main config directory**: Stored in OS-specific application data directories
- **Provider-specific configs**: Individual provider authentication stored within main config
- **Local configuration**: Project-specific or user-specific configuration files

**Environment variables:**

- Provider-specific environment variables for API keys (varies by provider)
- Standard API key environment variables for supported providers

**Precise storage paths** (platform-dependent):

- **Linux/macOS**: `~/.config/opencode/` or `~/.local/share/opencode/`
- **Windows**: `%APPDATA%\opencode\` or `%LOCALAPPDATA%\opencode\`
- **Secure storage**: Authentication tokens stored in OS keychain/keyring systems when available

**Provider-specific storage:**

- Different AI providers may use different credential storage mechanisms
- Some providers may use API keys, OAuth tokens, or other authentication methods

### Known Issues

- **Early development**: As a newer tool (v0.6.3), some features may be in active development
- **Provider compatibility**: Feature availability may vary across different AI providers
- **Network dependency**: Requires internet access for AI provider communication
- **TUI limitations**: Terminal user interface may have limitations on some systems
- **Agent customization**: Custom agent creation and management features are still evolving

## Findings — Session File Experiments (2025-09-10)

- Tooling: `specs/Research/Tools/SessionFileExperiments/opencode.py` runs a minimal interactive session (pexpect‑only).
- Storage (observed): No sessions persisted yet on this machine; `opencode export` reports "No sessions found". Logs present under `~/.local/share/opencode/log/` with per‑run logs like `2025-09-09T233616.log`.
- Export: Use `opencode export [sessionID]` to export session JSON when sessions exist. Back up and trim mid‑array with `trim_json_array_midpoint()` when experimenting.

How to produce session files (recommended procedure):

- Start: `opencode --prompt "Create a file 'experiment.tmp' with one line, then print it."`
- Follow-up: "Append a second line and show the file again."
- After activity, run: `opencode export` (or `opencode export <sessionID>`) to write JSON to stdout; save it to a file.
- Then back up and trim mid‑array using `trim_json_array_midpoint()`.
