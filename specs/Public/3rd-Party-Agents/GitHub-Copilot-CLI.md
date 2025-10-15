# GitHub Copilot CLI — Integration Notes

## Overview

GitHub Copilot CLI is GitHub's command-line interface that brings AI-powered coding assistance directly to the terminal, featuring autonomous agentic capabilities.

- **Website**: <https://docs.github.com/en/copilot/how-tos/set-up/install-copilot-cli>
- **Documentation**: <https://docs.github.com/en/copilot/using-github-copilot/using-github-copilot-in-the-command-line>
- **GitHub**: <https://github.com/github/copilot-cli>
- **Version tested**: TBD (in public preview as of 2025)
- **Short description**: Standalone CLI tool for AI-powered coding with autonomous agent capabilities, replacing the older gh extension

### Task start-up commands

GitHub Copilot CLI provides agentic coding assistance (verified from --help):

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
   ```bash
   copilot --add-dir ~/workspace --add-dir /tmp
   copilot --allow-all-paths  # Disable path verification
   ```

7. **Tool permissions**:
   ```bash
   copilot --allow-all-tools  # Auto-approve all tools (required for non-interactive)
   copilot --allow-tool 'shell(git:*)' --deny-tool 'shell(git push)'
   copilot --allow-tool 'write'  # Allow all file editing
   copilot --deny-tool 'MyMCP(denied_tool)' --allow-tool 'MyMCP'
   ```

8. **Logging**:
   ```bash
   copilot --log-dir ./logs
   copilot --log-level debug  # none, error, warning, info, debug, all
   ```

**Interactive vs non-interactive**:
- Default: Interactive TUI mode
- `-p` flag: Direct prompt execution (non-interactive)
- `--allow-all-tools` required for non-interactive mode

**Machine-readable output**: To be determined through testing.

**Session resumption**: Full session management with `--resume` and `--continue` options.

**Model specification**:
- Default: `claude-sonnet-4.5`
- Options: `gpt-5`, `claude-sonnet-4`, `claude-sonnet-4.5`
- Set via `--model` flag or `COPILOT_MODEL` env var
- Change in-session via `/model` command

**Note**: This is the NEW standalone Copilot CLI (not the older `gh copilot` extension which is being deprecated).

### Support for custom hooks

**Status: UNKNOWN**

No information found about custom hooks or per-step command execution. Investigation needed to determine if Agent Time Travel integration is possible.

### How to skip the initial onboarding screens on first launch of the agent?

**Authentication via Environment Variables** (verified):

```bash
# Set GitHub token
export GH_TOKEN=ghp_...
# or
export GITHUB_TOKEN=ghp_...

# Run copilot - will use token automatically
copilot
```

**Non-interactive execution** (verified):

```bash
# Direct prompt execution with auto-approvals
copilot -p "task description" --allow-all-tools
```

**Environment variables for automation** (from help):
- `GH_TOKEN` or `GITHUB_TOKEN`: Authentication token
- `COPILOT_ALLOW_ALL`: Set to "true" to auto-approve tools
- `COPILOT_MODEL`: Set default model (gpt-5, claude-sonnet-4, claude-sonnet-4.5)
- `COPILOT_CUSTOM_INSTRUCTIONS_DIRS`: Comma-separated custom instruction dirs
- `XDG_CONFIG_HOME`: Override config directory (default: `$HOME/.copilot`)
- `XDG_STATE_HOME`: Override state directory (default: `$HOME/.copilot`)

### Checkpointing (point-in-time restore of chat + filesystem)

**Status: UNKNOWN**

- **Official checkpointing**: Not documented in available materials
- **Scope**: Investigation required
- **Restore semantics**: N/A until checkpointing capability is confirmed
- **Operational notes**: N/A

### Session continuation (conversation resume)

**Status: TO BE DETERMINED**

Session management needs investigation through:
- Running the CLI and examining created files
- Reviewing help documentation
- Testing persistence across invocations

### Where are chat sessions stored?

**Confirmed locations** (from environment help):

- **Linux/macOS**: `$HOME/.copilot/` (default)
- **Windows**: `%USERPROFILE%\.copilot\` (default)
- **Custom locations**: Override via `XDG_CONFIG_HOME` and `XDG_STATE_HOME`

**Configuration**: `$XDG_CONFIG_HOME` (defaults to `$HOME/.copilot`)
**State files**: `$XDG_STATE_HOME` (defaults to `$HOME/.copilot`)

Session management confirmed through `--resume` and `--continue` options.

### What is the format of the persistent chat sessions?

**Status: UNKNOWN**

Session format to be determined through testing.

### Reverse‑engineering policy for session formats

**Recommended procedure:**

1. Install GitHub Copilot CLI
2. Authenticate with GitHub
3. Run minimal session
4. Locate session files in configuration directories
5. Examine file format and structure
6. Test resumption capabilities
7. Document findings

### How to run the agent with a set of MCP servers?

**Status: UNKNOWN**

MCP server support not documented in available materials. Investigation required.

### Credentials

GitHub Copilot CLI uses GitHub authentication:

**Authentication methods:**
- GitHub account with Copilot subscription
- OAuth flow through browser
- Token-based authentication (possibly)

**Storage locations (to be confirmed):**
- Uses GitHub CLI authentication infrastructure
- Likely shares credentials with `gh` CLI tool
- **Linux/macOS**: `~/.config/gh/` or dedicated Copilot config directory
- **Windows**: `%APPDATA%\GitHub CLI\` or dedicated directory

**Prerequisites:**
- Active GitHub Copilot subscription
- GitHub CLI installed and authenticated (possibly)

### Known issues and quirks

- **Public preview**: Still in development, features may change
- **Subscription required**: Requires GitHub Copilot subscription
- **Deprecation note**: Replaces older `gh copilot` extension (being deprecated October 25, 2025)
- **Agentic features**: Designed as autonomous coding agent, not just a suggestion tool
- **Platform support**: Available for macOS, Linux, Windows
- **GitHub integration**: Deep integration with GitHub ecosystem
- **Network dependency**: Requires internet access and GitHub API connectivity

## Important Transition Note

**Legacy vs New CLI:**
- **OLD**: `gh extension install github/gh-copilot` (deprecated Oct 25, 2025)
- **NEW**: Standalone `copilot` CLI tool (current, in public preview)

This document covers the NEW standalone Copilot CLI tool, not the deprecated gh extension.

## Research Status

This document is incomplete and requires hands-on testing. Key next steps:

1. Install GitHub Copilot CLI following official installation instructions
2. Capture full `copilot --help` output
3. Document command-line options and subcommands
4. Test session management and persistence
5. Locate configuration and credential files
6. Test with mock LLM API server (if compatible with custom endpoints)
7. Document any MCP or hook capabilities
8. Determine nixpkgs availability or create custom derivation

**Note**: GitHub Copilot CLI is available in the nix-ai-tools flake and has been added to Agent Harbor's flake.nix.

## Installation

```bash
# Via npm (manual)
npm install -g @github/copilot

# Via nix-ai-tools flake (already in Agent Harbor flake.nix)
nix run github:numtide/nix-ai-tools#copilot-cli

# Within Agent Harbor dev shell (after flake update)
copilot
```

## Integration with Agent Harbor

**Available in flake.nix**: Yes, via `nix-ai-tools.packages.${system}.copilot-cli`

**Binary name**: `copilot`
