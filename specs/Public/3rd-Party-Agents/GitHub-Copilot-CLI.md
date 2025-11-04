# GitHub Copilot CLI — Integration Notes

## Overview

GitHub Copilot CLI is GitHub's command-line interface that brings AI-powered coding assistance directly to the terminal, featuring autonomous agentic capabilities.

- **Website**: <https://docs.github.com/en/copilot/how-tos/set-up/install-copilot-cli>
- **Documentation**: <https://docs.github.com/en/copilot/concepts/agents/about-copilot-cli>
- **GitHub**: <https://github.com/github/copilot-cli>
- **Version tested**: Latest stable (as of this writing)
- **Short description**: Standalone CLI tool for AI-powered coding with autonomous agent capabilities, replacing the older gh extension

### Task start-up commands

GitHub Copilot CLI can be started in several ways:

1. **Launch interactive session**:

   ```bash
   copilot
   ```

2. **Start with specific model**:

   ```bash
   copilot --model gpt-5
   copilot --model claude-sonnet-4
   copilot --model claude-sonnet-4.5  # default
   ```

3. **Execute prompt directly** (non-interactive):

   ```bash
   copilot -p "Fix the bug in main.js" --allow-all-tools
   ```

4. **Resume sessions**:

   ```bash
   copilot --resume              # Resume latest session
   copilot --resume session-id   # Resume specific session
   copilot --continue            # Resume most recent session
   ```

5. **Authentication**:

   ```bash
   export GH_TOKEN=ghp_...  # or GITHUB_TOKEN
   export GITHUB_TOKEN=ghp_...
   copilot
   ```

6. **Path permissions**:
   copilot --add-dir ~/workspace --add-dir /tmp
   copilot --allow-all-paths # Disable path verification

   ```
   copilot --allow-all-tools  # Auto-approve all tools (required for non-interactive)
   ```

   _Note: **Use with caution:** `--allow-all-tools` gives Copilot CLI unrestricted permission to run any command it deems necessary, without asking for approval_

   ```
   copilot --allow-tool 'shell(git:*)' --deny-tool 'shell(git push)'
   ```

7. **Logging**:
   ```bash
   copilot --log-dir ./logs
   ```

**Interactive vs non-interactive**:

- Default: Interactive TUI mode
- `-p` flag: Direct prompt execution (non-interactive)

**Session resumption**: Full session management with `--resume` and `--continue` options.

**Model specification**:

- Default: `claude-sonnet-4.5`
- Options: `gpt-5`, `claude-sonnet-4`, `claude-sonnet-4.5`
- Set via `--model` flag or `COPILOT_MODEL` env var

**Note**: This is the NEW standalone Copilot CLI (not the older `gh copilot` extension which is being deprecated).

### Support for custom hooks

**Status: UNKNOWN**

No information found about custom hooks or per-step command execution. Investigation needed to determine if Agent Time Travel integration is possible.

**Authentication via Environment Variables** (verified):

````bash
# Set GitHub token
export GH_TOKEN=ghp_...
export GITHUB_TOKEN=ghp_...

# Run copilot - will use token automatically

**Non-interactive execution** (verified):
```bash
# Direct prompt execution with auto-approvals
copilot -p "task description" --allow-all-tools
````

- `GH_TOKEN` or `GITHUB_TOKEN`: Authentication token
- `COPILOT_ALLOW_ALL`: Set to "true" to auto-approve tools
- `COPILOT_MODEL`: Set default model (gpt-5, claude-sonnet-4, claude-sonnet-4.5)
- `XDG_CONFIG_HOME`: Override config directory (default: `$HOME/.copilot`)
- `XDG_STATE_HOME`: Override state directory (default: `$HOME/.copilot`)

### Checkpointing (point-in-time restore of chat + filesystem)

**Status: NO OFFICIAL CHECKPOINTING SUPPORT**

GitHub Copilot CLI does not provide documented checkpointing functionality that restores both chat and filesystem state to a specific moment in time.

### Session Management

The GitHub Copilot CLI provides options to manage and resume conversational sessions, allowing you to maintain context between interactions.

`--continue`:
Reopens the most recent Copilot CLI session, restoring its context so you can pick up right where you left off. This is useful when continuing work or revisiting an earlier conversation.

`--session`:
Allows you to start or resume a specific named session. This helps organize different tasks or projects under separate session names, preserving context across multiple workflows.

### Where are chat sessions stored?

**Confirmed locations** (from environment help):

- **Linux/macOS**: `$HOME/.copilot/` (default)
- **Windows**: `%USERPROFILE%\.copilot\` (default)
- **Custom locations**: Override via `XDG_CONFIG_HOME` and `XDG_STATE_HOME`

**Configuration**: `$XDG_CONFIG_HOME` (defaults to `$HOME/.copilot`)
**State files**: `$XDG_STATE_HOME` (defaults to `$HOME/.copilot`)

### How to run the agent with a set of MCP servers?

Details of your configured MCP servers are stored in the `mcp-config.json` file, which is located, by default, in the `$HOME/.copilot/` (for Linux/macOS) and `%USERPROFILE%\.copilot\` (for Windows) directory. This location can be changed by setting the XDG_CONFIG_HOME environment variable. For information about the JSON structure of a MCP server definition, see [Extending GitHub Copilot coding agent with the Model Context Protocol (MCP)](https://docs.github.com/en/copilot/how-tos/use-copilot-agents/coding-agent/extend-coding-agent-with-mcp#writing-a-json-configuration-for-mcp-servers).

### Credentials

The credentials are stored in the `$HOME/.config/gh` directory. Within this directory the `hosts.yml` file can be found. That file maps GitHub hostnames (for example, github.com) to their corresponding oauth_token and other related authentication details. This structure mirrors how the GitHub CLI manages and stores authentication tokens.

**Authentication methods:**

1. **GitHub CLI (gh) authentication** (recommended):

   ```bash
   gh auth login
   # GitHub Copilot CLI inherits authentication from gh
   ```

2. **Personal Access Token (PAT)**:

   ```bash
   export GH_TOKEN="ghp_..."
   export GITHUB_TOKEN="ghp_..."  # Alternative
   ```

3. **OAuth token** (via interactive login):
   ```bash
   copilot
   /login  # Follow OAuth flow
   ```

**Prerequisites:**

- GitHub Copilot CLI installed and authenticated

### Known issues and quirks

- **Public preview**: Still in development, features may change
- **Subscription required**: Requires GitHub Copilot subscription
- **Windows**: native PowerShell support is experimental; WSL is recommended for stability
- **Deprecation note**: Replaces older `gh copilot` extension (being deprecated October 25, 2025)
- **Version compatibility**: Ensure Node.js ≥22 and npm ≥10 for stable operation
- **Platform support**: Available for macOS, Linux, Windows
- **Premium requests quota**: Each prompt consumes one premium request from monthly quota
- **API rate limits**: Subject to GitHub API rate limits and LLM provider limits
- **Model switching**: Different models may have different quota consumption rates
