# OpenAI Codex CLI — Integration Notes

> Usually this information can be obtained by checking out command-line help screens or man pages for the agent software.

## Overview

OpenAI Codex CLI is a Rust-based command-line interface for OpenAI's Codex models, providing an AI-powered coding assistant with advanced sandboxing and execution capabilities.

- **Website**: <https://openai.com/blog/openai-codex/>
- **Documentation**: <https://github.com/OpenAI/openai-codex-cli>
- **GitHub**: <https://github.com/OpenAI/openai-codex-cli>
- **Version**: 0.27.0 (as of this writing)

### Task start-up command

Codex CLI can be started with a specific task prompt in several ways:

1. **Interactive session with prompt**:

   ```bash
   codex "Implement a user authentication system"
   ```

2. **Non-interactive execution**:

   ```bash
   codex exec "Generate unit tests for this function"
   ```

3. **Read from stdin**:

   ```bash
   echo "Refactor this legacy code" | codex exec -
   ```

4. **With specific model**:

   ```bash
   codex --model o3 "Implement a REST API endpoint"
   ```

5. **With sandbox mode**:
   ```bash
   codex --sandbox workspace-write "Modify these files safely"
   ```

### How to skip the initial onboarding screens on first launch of the agent?

To bypass Codex CLI's initial setup and authentication prompts for automated testing and programmatic integration:

1. **Create `.config/codex/config.toml` file** in HOME directory:

   ```toml
   [user]
   id = "test-user-id"

   [api]
   # API configuration will be overridden by environment variables
   ```

2. **Set environment variables** for API access:

   ```bash
   export OPENAI_API_KEY="your-api-key"
   export OPENAI_BASE_URL="https://api.openai.com/v1"  # or custom server
   ```

3. **For custom HOME directory** (useful for isolated testing):

   ```bash
   export HOME=/path/to/custom/home
   # Create .config/codex/config.toml as above
   codex exec "Your prompt here"
   ```

4. **Use non-interactive mode** to avoid any prompts:
   ```bash
   codex exec "Your prompt here"
   ```

**Key commands for automation:**

- `codex exec`: Non-interactive execution mode
- `--skip-git-repo-check`: Skip git repository verification (required for temporary workspaces)
- `--yes` or `-y`: Auto-confirm prompts (if supported by the version)
- `--sandbox`: Control file operation permissions

**Note:** Codex CLI is generally designed for programmatic use and has minimal onboarding requirements compared to interactive agents. The `--skip-git-repo-check` flag is essential when working with temporary directories or non-git workspaces.

### Checkpointing (point-in-time restore of chat + filesystem)

There is no evidence of an official checkpointing feature in the Codex CLI that restores both chat and filesystem to a specific moment. Some materials reference applying diffs, but this is not equivalent to AH’s checkpointing requirement.

- Scope: No official checkpoints. Diff application is not a full checkpoint/restore.
- Restore: N/A. Applying a diff is not a full restore, and requires manual Git management.

### Session continuation (conversation resume)

No built‑in persistent conversational sessions are documented. Each CLI invocation is independent.

### Where are chat sessions stored?

N/A — no persistent session format is documented.

### How is the use of MCP servers configured?

OpenAI Codex CLI has experimental MCP server support:

**Command-line options:**

- No specific MCP configuration command-line options documented (experimental feature)

**MCP management commands:**

- `codex mcp`: Experimental command to run Codex as an MCP server
- Configuration overrides via `-c, --config` options can be applied to MCP mode

**Configuration files:**

- `~/.codex/config.toml`: Main configuration file that may include MCP-related settings
- No specific MCP configuration file format documented

**Environment variables:**

- No specific MCP-related environment variables documented in help screens

Note: MCP support is marked as experimental in Codex CLI.

### Support for custom hooks

For Agent Time Travel feature (commands executed after every agent step), OpenAI Codex CLI does not appear to have built-in support for custom step-level hooks. However, it does support broader customization through:

1. **Configuration profiles**: `-p, --profile` to use predefined configuration profiles from `~/.codex/config.toml`
2. **Runtime configuration overrides**: `-c, --config` to override configuration values at runtime
   - Examples: `-c model="o3"`, `-c 'sandbox_permissions=["disk-full-read-access"]'`
3. **Experimental MCP support**: `codex mcp` for running as an MCP server

Note: While configuration profiles and MCP support exist, there is no documented support for automatic execution of custom commands after every agent step as required for the Agent Time Travel feature.

### Credentials

Codex CLI uses OpenAI authentication with the following precise storage locations:

**Authentication methods:**

- **API Key authentication**: `codex login --api-key <API_KEY>`
- **Interactive login**: `codex login` (launches browser for OAuth)
- **Login status**: `codex login status` shows current authentication state
- **Logout**: `codex logout` removes stored credentials

**Configuration files:**

- **Main config file**: `~/.codex/config.toml` - Contains API keys, authentication tokens, default model settings, sandbox permissions, and configuration profiles
- Configuration stored in TOML format with support for profiles and overrides

**Environment variables:**

- Standard OpenAI environment variables (OPENAI_API_KEY, etc.)
- Custom configuration overrides via environment variables

**Precise storage paths:**

- **Configuration directory**: `~/.codex/`
- **Config file**: `~/.codex/config.toml`
- **Authentication tokens**: Stored within the config file or OS keychain (depending on authentication method)

### Known Issues

- **Git repository dependency**: By default requires execution within a Git repository
- **Sandbox limitations**: Sandbox modes may have performance or functionality limitations
- **Experimental MCP**: MCP server functionality is marked as experimental
- **OpenAI API dependency**: Requires active OpenAI API access and is subject to rate limits
- **Limited session management**: Less sophisticated session resumption compared to other agents
- **Configuration complexity**: Advanced configuration requires understanding of TOML structure

## Findings — Session File Experiments (2025-09-10)

- Tooling: `specs/Research/Tools/SessionFileExperiments/codex.py` documents lack of persistent sessions and demonstrates diff-centric workflows.
- Recommendation: For Time Travel, rely on external session recording and filesystem snapshots; Codex execution remains stateless per invocation.
