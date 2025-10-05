# Gemini CLI — Integration Notes

> Usually this information can be obtained by checking out command-line help screens or man pages for the agent software.

## Overview

Gemini CLI is Google's official command-line interface for interacting with Gemini AI models, providing an AI-powered coding assistant with MCP support and experimental features.

- **Website**: <https://ai.google.dev/gemini-api>
- **Documentation**: <https://github.com/google/gemini-cli>
- **GitHub**: <https://github.com/google/gemini-cli>
- **Version**: 0.2.2 (as of this writing)

### Task start-up command

Gemini CLI can be started with a specific task prompt in several ways:

1. **Direct prompt**:

   ```bash
   gemini --prompt "Implement a user authentication system"
   ```

2. **Interactive session with initial prompt**:

   ```bash
   gemini --prompt-interactive "Start building a REST API"
   ```

3. **Non-interactive mode** (for automation):

   ```bash
   gemini -p "Generate unit tests for this function"
   ```

4. **With specific model**:

   ```bash
   gemini --model gemini-pro "Refactor this legacy code"
   ```

5. **With checkpointing enabled**:
   ```bash
   gemini --checkpointing --prompt "Implement feature X"
   ```

### Checkpointing (point-in-time restore of chat + filesystem)

Gemini CLI documents an official checkpointing feature to snapshot and restore state around tool execution. Enable with `--checkpointing` (or configure in settings); an experimental ACP mode (`--experimental-acp`) is also referenced in some materials.

- Scope: Filesystem snapshots (via a shadow history area) and conversation context associated with the checkpoint event; used to re‑propose the same tool call on restore.
- Enable: `--checkpointing` flag or settings. Some docs reference ACP for advanced behavior.
- Restore: Use `/restore <checkpoint>` to revert project files to the snapshot and restore the associated conversation context for that point. Intended for point‑in‑time recovery before an operation proceeds.
- Storage: Shadow git/history under user config dir (platform‑specific; e.g., under `~/.config`/`~/.local/share`). Exact paths may vary by platform/build; consult current `gemini-cli` docs.
- Notes: Behavior and flags are evolving; verify against the installed version’s `--help` and official docs.

### Session continuation (conversation resume)

Separate from checkpoints, Gemini CLI materials describe saving and resuming chat state (e.g., via `/chat save` and `/chat resume`). This does not restore filesystem state by itself.

- Save/resume: `/chat save <tag>` then `/chat resume <tag>`
- Scope: Conversation history only; does not rewind files
- Storage paths: Platform‑specific under the gemini config directory

### How is the use of MCP servers configured?

Gemini CLI provides MCP server configuration through command-line options and management commands:

**Command-line options:**

- `--allowed-mcp-server-names <names>`: Comma or space-separated list of allowed MCP server names

**MCP management commands:**

- `gemini mcp add <name> <commandOrUrl> [args...]`: Add a server (stdio or URL-based)
- `gemini mcp remove <name>`: Remove a server
- `gemini mcp list`: List all configured MCP servers

**Configuration files:**

- MCP server configurations stored in Gemini CLI's configuration directory
- No specific MCP configuration file format documented

**Environment variables:**

- No specific MCP-related environment variables documented in help screens

### Support for custom hooks

For Agent Time Travel feature (commands executed after every agent step), Gemini CLI does not appear to have built-in support for custom step-level hooks. However, it does support MCP-based extensibility:

1. **MCP Server Management**:
   - **Add servers**: `gemini mcp add <name> <commandOrUrl>` to add stdio or URL-based servers
   - **Remove servers**: `gemini mcp remove <name>` to remove configured servers
   - **List servers**: `gemini mcp list` to view all configured MCP servers
   - **Allowed servers**: `--allowed-mcp-server-names` to restrict which MCP servers can be used

2. **Extensions**:
   - **Extension loading**: `-e, --extensions` to specify which extensions to use
   - **List extensions**: `-l, --list-extensions` to see all available extensions
   - **Automatic extension discovery**: If no extensions specified, all available extensions are used

3. **Additional directories**: `--include-directories` to add extra directories to the workspace context

Note: While MCP servers and extensions exist, there is no documented support for automatic execution of custom commands after every agent step as required for the Agent Time Travel feature.

### Credentials

Gemini CLI uses Google's authentication system for credentials with the following precise storage locations:

**Authentication methods:**

- **Application Default Credentials (ADC)**: Automatically uses credentials from `gcloud auth application-default login`
- **Service Account Keys**: JSON key files for service accounts
- **OAuth 2.0**: Interactive OAuth flow for user authentication

**Environment variables:**

- `GOOGLE_API_KEY`: Environment variable for Gemini API key
- `GOOGLE_CLOUD_PROJECT`: Environment variable for GCP project ID
- `GOOGLE_APPLICATION_CREDENTIALS`: Path to service account key file

**Configuration files:**

- **ADC credentials**: `~/.config/gcloud/application_default_credentials.json`
- **Service account keys**: Custom paths specified via `GOOGLE_APPLICATION_CREDENTIALS`
- **gcloud config**: `~/.config/gcloud/configurations/config_default` (if using gcloud CLI)
- **Gemini CLI config**: Stored in platform-specific user configuration directories

**Precise storage paths** (platform-dependent):

- **Linux/macOS**: `~/.config/gemini-cli/` or `~/.config/gcloud/`
- **Windows**: `%APPDATA%\gemini-cli\` or `%APPDATA%\gcloud\`
- **OAuth tokens**: Stored in OS keychain/keyring or gcloud credential files

### Known Issues

- **Experimental status**: Many features are marked as experimental (ACP mode, some extensions)
- **Google Cloud dependency**: Requires Google Cloud authentication or API keys
- **Network dependency**: Requires internet access for Gemini API communication
- **Rate limiting**: Subject to Google's API rate limits and quotas
- **Checkpoint stability**: Checkpointing feature may have stability issues in complex scenarios
- **MCP compatibility**: MCP server compatibility may vary

## Findings — Session File Experiments (2025-09-10)

- Tooling: `specs/Research/Tools/SessionFileExperiments/gemini.py` runs a minimal session (pexpect‑only) and attempts `--checkpointing`.
- Storage (observed on this machine): No new files were created under `~/.config/gemini*` or `~/.local/share/gemini*` during a short run. It is possible the installed version only persists checkpoints when edits occur or when a restore point is explicitly created.
- Recommendation: Run with `--checkpointing` in a repo and approve an edit tool so a file actually changes; then inspect recent files under likely roots (`~/.config/gemini-cli`, `~/.local/share/gemini`, project `.git` or shadow history). Use `list_recent.py` to surface recent writes.

How to produce session/checkpoint files (recommended procedure):

- Start in a writable project directory.
- Run: `gemini --checkpointing --approval-mode yolo`
- Prompt: "Create a file named `experiment.tmp` with one line, then append another line and print the file."
- Wait for edits to complete; then run `/stop`.
- Inspect recent files under `~/.config/gemini-cli` and `~/.local/share/gemini` and in the project’s VCS metadata for shadow history. Use `specs/Research/Tools/SessionFileExperiments/list_recent.py`.
