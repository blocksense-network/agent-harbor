# Cursor CLI — Integration Notes

## Overview

Cursor CLI (binary name: `cursor-agent`) is Cursor's AI-powered command-line interface that brings AI coding assistance directly to the terminal.

- **Website**: <https://cursor.com/cli>
- **Documentation**: <https://cursor.com/docs/cli/installation>
- **GitHub**: Not publicly available (proprietary)
- **nixpkgs**: Available as `cursor-cli` in nixpkgs-unstable
- **Version tested**: 2025.09.18-39624ef (verified installed version)
- **Short description**: AI-driven terminal coding agent with multi-model support, MCP integration, and CI/CD capabilities

### Task start-up commands

Cursor CLI can be started with a specific task prompt in several ways:

1. **Direct prompt**:

   ```bash
   cursor-agent "implement user authentication"
   ```

2. **Interactive session**:

   ```bash
   cursor-agent
   ```

3. **Non-interactive/print mode** (for automation):

   ```bash
   cursor-agent --print "find bugs and fix them"
   ```

4. **Resume specific chat**:

   ```bash
   cursor-agent --resume <chatId>
   ```

5. **Resume latest chat**:

   ```bash
   cursor-agent resume
   ```

6. **List available chats**:

   ```bash
   cursor-agent ls
   ```

7. **Model selection**:

   ```bash
   cursor-agent --model gpt-5 "implement feature X"
   cursor-agent --model sonnet-4 "refactor code"
   cursor-agent --model sonnet-4-thinking "complex reasoning task"
   ```

**Interactive vs non-interactive**:

- Default: Interactive TUI mode
- `--print` flag: Non-interactive mode for scripts and automation
- Supports both `cursor-agent [prompt]` for quick tasks and `cursor-agent` alone for interactive sessions

**Machine-readable output**:

- `--output-format text | json | stream-json` (default: stream-json)
- Works only with `--print` mode
- Enables CI/CD integration and scripting

**Session resumption**:

- `--resume [chatId]`: Resume specific chat
- `cursor-agent resume`: Resume latest session
- `cursor-agent ls`: List available chats
- `cursor-agent create-chat`: Create new empty chat

**Model specification**:

- `--model <model>`: Specify model (gpt-5, sonnet-4, sonnet-4-thinking)
- Can be changed within interactive session via UI

### Support for custom hooks

**Status: NO built-in per-step hooks**

Cursor CLI does not appear to have documented support for custom hooks executed after every agent step. The CLI is designed for interactive use and automation via `--print` mode, but does not provide hook mechanisms similar to Claude Code's PostToolUse hooks.

Potential workarounds for Agent Time Travel:

- Monitor filesystem changes externally during `--print` mode execution
- Parse JSON output in `stream-json` mode to track tool executions
- Use MCP servers for some extensibility (though not true per-step hooks)

Investigation shows no equivalent to Claude Code hooks for Time Travel integration.

### How to skip the initial onboarding screens on first launch of the agent?

Cursor CLI provides several authentication methods:

1. **Environment variable**:

   ```bash
   export CURSOR_API_KEY=your-api-key
   cursor-agent --print "task description"
   ```

2. **Command-line flag**:

   ```bash
   cursor-agent --api-key your-key "task description"
   ```

3. **Pre-authenticate via login command**:

   ```bash
   cursor-agent login
   # Then use cursor-agent normally
   ```

4. **Check authentication status**:
   ```bash
   cursor-agent status
   ```

For automated/non-interactive use, set `CURSOR_API_KEY` environment variable or use `--api-key` flag.

### Checkpointing (point-in-time restore of chat + filesystem)

**Status: NO built-in checkpointing found**

- **Official checkpointing**: Not documented
- **Scope**: N/A
- **Restore semantics**: N/A
- **Operational notes**: Use external version control for filesystem state

### Session continuation (conversation resume)

Cursor CLI supports session management:

- **Resume capability**: `cursor-agent sessions` lists available sessions; documentation indicates sessions can be resumed
- **Persistence**: Sessions are tracked and can be listed
- **Limitations**: Exact resumption command and session format to be determined through testing

### Where are chat sessions stored?

**Confirmed locations:**

- **Linux/macOS**: `~/.cursor/` (verified through testing)
- **Windows**: `%USERPROFILE%\.cursor\` or `%APPDATA%\cursor\`
- **Project-local**: `.cursor/` in project directory

**Configuration files created:**

- `~/.cursor/cli-config.json`: CLI configuration (vim mode, permissions, network settings)
- `.cursor/mcp.json` or `~/.cursor/mcp.json`: MCP server configurations

**Session files**: The `cursor-agent ls` command lists available chats, and `--resume` accepts chat IDs. Session storage location within `~/.cursor/` to be determined through actual usage.

### What is the format of the persistent chat sessions?

**Status: UNKNOWN**

Session format not documented. Investigation required to determine:

- Serialization format (JSON/JSONL/binary)
- Structure and fields
- Whether trimming to specific points is feasible

### Reverse‑engineering policy for session formats

**Recommended procedure:**

1. Run minimal session: `cursor-agent chat "Create hello.py with one line"`
2. Locate session files in likely directories (`~/.cursor*`, `~/.config/cursor*`)
3. Inspect file format and structure
4. Test session resumption after manual edits
5. Document findings with version information

### How to run the agent with a set of MCP servers?

Cursor CLI supports MCP (Model Context Protocol) servers through configuration files:

**Configuration locations:**

- Project-scoped: `.cursor/mcp.json` (in project directory)
- Global: `~/.cursor/mcp.json` (user home directory)

**MCP management commands:**

```bash
# List configured MCP servers and their status
cursor-agent mcp list

# List available tools for a specific MCP server
cursor-agent mcp list-tools <identifier>

# Authenticate with an MCP server
cursor-agent mcp login <identifier>

# Disable an MCP server
cursor-agent mcp disable <identifier>
```

**Configuration file format** (`.cursor/mcp.json` or `~/.cursor/mcp.json`):

The exact JSON structure needs to be documented through testing, but likely includes:

- Server identifiers
- Connection details (stdio command, URL, etc.)
- Authentication settings
- Tool configurations

**Usage pattern:**

1. Create MCP configuration file in `.cursor/mcp.json` or `~/.cursor/mcp.json`
2. Authenticate if needed: `cursor-agent mcp login <identifier>`
3. List tools: `cursor-agent mcp list-tools <identifier>`
4. Use cursor-agent normally; MCP tools will be available

Further investigation needed to document the exact JSON configuration format.

### Credentials

Cursor CLI authentication methods:

**Primary authentication:**

- `cursor-agent login`: Interactive authentication with Cursor account
- `cursor-agent logout`: Sign out and clear stored authentication
- `cursor-agent status`: Check authentication status

**API key authentication:**

- `--api-key <key>`: Command-line flag
- `CURSOR_API_KEY`: Environment variable

**Storage locations:**

- **Configuration directory**: `~/.cursor/` (Linux/macOS)
- **Windows**: `%USERPROFILE%\.cursor\` or `%APPDATA%\cursor\`
- Credentials stored after `cursor-agent login`
- MCP server configs: `.cursor/mcp.json` (project) or `~/.cursor/mcp.json` (global)

**Settings files:**

- `~/.cursor/mcp.json`: MCP server configurations
- Other configuration files likely in `~/.cursor/` directory

### Known issues and quirks

- **Platform support**: Linux, macOS, Windows - cross-platform
- **Model availability**: Supports GPT-5, Sonnet-4, Sonnet-4-thinking; requires Cursor subscription
- **Proprietary**: Source code not publicly available
- **CI integration**: `--print` mode with `--output-format json` for automation
- **Security**: Can execute shell commands and modify files; `--force` flag allows commands unless explicitly denied
- **Background mode**: `--background` flag starts in background with composer picker
- **Shell integration**: `install-shell-integration` and `uninstall-shell-integration` commands for zsh
- **Auto-updates**: `cursor-agent update` or `cursor-agent upgrade` command available
- **MCP support**: Full MCP server integration via configuration files
- **nixpkgs availability**: Available as `cursor-cli` in nixpkgs-unstable (binary: `cursor-agent`)

## Additional Commands

**Shell integration:**

```bash
cursor-agent install-shell-integration    # Add to ~/.zshrc
cursor-agent uninstall-shell-integration  # Remove from ~/.zshrc
```

**Updates:**

```bash
cursor-agent update     # Update to latest version
cursor-agent upgrade    # Alias for update
```

**Session management:**

```bash
cursor-agent create-chat        # Create new empty chat, returns ID
cursor-agent ls                 # List available chats
cursor-agent resume             # Resume latest chat
cursor-agent --resume <chatId>  # Resume specific chat
```

## Integration with Agent Harbor

**Available in flake.nix**: Yes, as `pkgs.cursor-cli` (nixpkgs-unstable)

**Binary name**: `cursor-agent`

**Recommended for testing**:

- Use `--print` mode for non-interactive testing
- Use `--output-format json` for parsing
- Set `CURSOR_API_KEY` for automated authentication
- Monitor filesystem changes externally (no built-in hooks)

**Limitations for Agent Time Travel**:

- No per-step hooks like Claude Code
- Requires external monitoring for filesystem snapshots
- Can parse JSON output to track tool executions
