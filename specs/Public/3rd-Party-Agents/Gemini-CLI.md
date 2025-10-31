# Gemini CLI — Integration Notes

> Usually this information can be obtained by checking out command-line help screens or man pages for the agent software.

## Overview

Gemini CLI is Google's official command-line interface for interacting with Gemini AI models, providing an AI-powered coding assistant with MCP support and experimental features.

- **Website**: <https://ai.google.dev/gemini-api>
- **GitHub**: <https://github.com/google-gemini//gemini-cli>
- **Documentation**: <https://github.com/google-gemini/gemini-cli/tree/main/docs>
- **Version**: 0.8.1 (as of this writing)

### Task start-up command

Gemini CLI can be started with a specific task prompt in several ways:

1. **Direct prompt (positional)**:

   ```bash
   gemini "Implement a user authentication system"
   ```

2. **Interactive session with initial prompt**:

   ```bash
   gemini --prompt-interactive "Start building a REST API"
   ```

3. **Non-interactive mode (deprecated -p flag)**:

   ```bash
   gemini -p "Generate unit tests for this function"
   ```

   Note: The `-p, --prompt` flag is deprecated and will be removed in a future version. Use positional prompts instead.

4. **With specific model**:

   ```bash
   gemini --model gemini-pro "Refactor this legacy code"
   ```

5. **With checkpointing enabled**:

   ```bash
   gemini --checkpointing "Implement feature X"
   ```

   Note: The `--checkpointing` flag is deprecated. Use the "general.checkpointing.enabled" setting in settings.json instead.

### Checkpointing (point-in-time restore of chat + filesystem)

Gemini CLI documents an official checkpointing feature to snapshot and restore state around tool execution. Enable with `--checkpointing` (or configure in settings); an experimental ACP mode (`--experimental-acp`) is also referenced in some materials.

- Scope: Filesystem snapshots (via a shadow history area) and conversation context associated with the checkpoint event; used to re‑propose the same tool call on restore.
- Enable: `--checkpointing` flag or settings. Some docs reference ACP for advanced behavior.
- Restore: Use `/restore <checkpoint>` to revert project files to the snapshot and restore the associated conversation context for that point. Intended for point‑in‑time recovery before an operation proceeds.
- Storage: Shadow git/history under `~/.gemini/` directory. Exact paths may vary by platform/build; consult current `gemini-cli` docs.
- Notes: Behavior and flags are evolving; verify against the installed version’s `--help` and official docs.

### Session continuation (conversation resume)

Separate from checkpoints, Gemini CLI materials describe saving and resuming chat state (e.g., via `/chat save` and `/chat resume`). This does not restore filesystem state by itself.

- Save/resume: `/chat save <tag>` then `/chat resume <tag>`
- Scope: Conversation history only; does not rewind files
- Storage paths: `~/.gemini/` directory

### How is the use of MCP servers configured?

Gemini CLI provides MCP server configuration through command-line options and management commands:

**Command-line options:**

- `--allowed-mcp-server-names <names>`: Comma or space-separated list of allowed MCP server names

**MCP management commands:**

- `gemini mcp add <name> <commandOrUrl> [args...]`: Add a server (stdio or URL-based)
- `gemini mcp remove <name>`: Remove a server
- `gemini mcp list`: List all configured MCP servers

**Configuration files:**

- MCP server configurations stored in `~/.gemini/settings.json`
- Configuration can be modified via `gemini mcp` commands or by editing settings.json directly
- See the [MCP Server Integration guide](https://github.com/google-gemini/gemini-cli/blob/HEAD/docs/tools/mcp-server.md) for setup instructions

**Environment variables:**

- No specific MCP-related environment variables documented in help screens

### Additional Command-Line Options

The following additional options are available (as of version 0.8.1):

**Sandbox options:**

- `-s, --sandbox`: Run in sandbox mode
- `--sandbox-image`: Sandbox image URI (deprecated, use settings.json)

**Tool control:**

- `--allowed-tools`: Array of tools that are allowed to run without confirmation

**UI options:**

- `--show-memory-usage`: Show memory usage in status bar (deprecated, use settings.json)
- `--screen-reader`: Enable screen reader mode for accessibility
- `-o, --output-format`: Set output format (choices: "text", "json")

**Development options:**

- `-d, --debug`: Run in debug mode

**Telemetry options** (all deprecated, use settings.json):

- `--telemetry`: Enable telemetry
- `--telemetry-target`: Set telemetry target (local or gcp)
- `--telemetry-otlp-endpoint`: Set OTLP endpoint for telemetry
- `--telemetry-otlp-protocol`: Set OTLP protocol (grpc or http)
- `--telemetry-log-prompts`: Enable/disable logging of user prompts
- `--telemetry-outfile`: Redirect telemetry output to file

**Proxy:**

- `--proxy`: Proxy for gemini client (deprecated, use settings.json)

**Legacy file inclusion:**

- `-a, --all-files`: Include ALL files in context (deprecated, use @ includes instead)

### Support for custom hooks

For Agent Time Travel feature (commands executed after every agent step), Gemini CLI does not appear to have built-in support for custom step-level hooks. However, it does support MCP-based extensibility and a rich extension system:

1. **MCP Server Management**:
   - **Add servers**: `gemini mcp add <name> <commandOrUrl>` to add stdio or URL-based servers
   - **Remove servers**: `gemini mcp remove <name>` to remove configured servers
   - **List servers**: `gemini mcp list` to view all configured MCP servers
   - **Allowed servers**: `--allowed-mcp-server-names` to restrict which MCP servers can be used

2. **Extensions**:
   - **Extension loading**: `-e, --extensions` to specify which extensions to use
   - **List extensions**: `-l, --list-extensions` to see all available extensions
   - **Automatic extension discovery**: If no extensions specified, all available extensions are used
   - **Extension management commands**:
     - `gemini extensions install <source>`: Install from git repository URL or local path
     - `gemini extensions uninstall <name>`: Uninstall an extension
     - `gemini extensions list`: List installed extensions
     - `gemini extensions update [<name>] [--all]`: Update extensions to latest version
     - `gemini extensions disable [--scope] <name>`: Disable an extension
     - `gemini extensions enable [--scope] <name>`: Enable an extension
     - `gemini extensions link <path>`: Link an extension from a local path
     - `gemini extensions new <path> [template]`: Create a new extension from boilerplate

3. **Additional directories**: `--include-directories` to add extra directories to the workspace context

4. **Approval modes**: `--approval-mode` with options:
   - `default`: Prompt for approval (default behavior)
   - `auto_edit`: Auto-approve edit tools only
   - `yolo`: Auto-approve all tools (equivalent to the deprecated `--yolo` flag)

Note: While MCP servers and extensions exist, there is no documented support for automatic execution of custom commands after every agent step as required for the Agent Time Travel feature.

### Credentials

Gemini CLI supports multiple authentication methods (as of version 0.8.1):

**Authentication methods:**

1. **USE_GEMINI** (Direct Gemini API):
   - Requires: `GEMINI_API_KEY` environment variable
   - Get your API key from: <https://aistudio.google.com/apikey>
   - Simplest method for direct Gemini API access

2. **USE_VERTEX_AI** (Vertex AI):
   - Option A: Project-based authentication
     - Requires: `GOOGLE_CLOUD_PROJECT` and `GOOGLE_CLOUD_LOCATION` environment variables
     - Uses Application Default Credentials (ADC) from gcloud
   - Option B: Express mode
     - Requires: `GOOGLE_API_KEY` environment variable

3. **LOGIN_WITH_GOOGLE**:
   - Interactive OAuth flow
   - Authenticates via Google account login

4. **CLOUD_SHELL**:
   - Automatically used when running in Google Cloud Shell environment
   - No additional configuration required

**Environment variables:**

- `GEMINI_API_KEY`: Gemini API key (for direct Gemini API access)
- `GOOGLE_API_KEY`: Google API key (for Vertex AI express mode)
- `GOOGLE_CLOUD_PROJECT`: GCP project ID (for Vertex AI)
- `GOOGLE_CLOUD_LOCATION`: GCP region (for Vertex AI, e.g., "us-central1")

**Configuration storage:**

- **Settings directory**: `~/.gemini/` (all platforms: Linux, macOS, Windows)
  - Main config file: `~/.gemini/settings.json`
  - MCP server configurations also stored here
- **OAuth tokens**: Stored in `~/.gemini/` directory when using LOGIN_WITH_GOOGLE
- **.env files**: Supported for environment variable configuration (no reload needed)

### Known Issues

- **Experimental status**: Many features are marked as experimental (ACP mode, some extensions)
- **Authentication required**: Requires either Gemini API key (`GEMINI_API_KEY`) or Vertex AI credentials
- **Network dependency**: Requires internet access for Gemini API communication
- **Rate limiting**: Subject to Google's API rate limits and quotas
- **Checkpoint stability**: Checkpointing feature may have stability issues in complex scenarios
- **MCP compatibility**: MCP server compatibility may vary
- **Deprecated flags**: Several command-line flags are deprecated in favor of settings.json configuration:
  - `--prompt` (use positional arguments)
  - `--checkpointing` (use settings.json)
  - `--yolo` (use `--approval-mode yolo`)
  - `--telemetry` flags (use settings.json)
  - `--proxy` (use settings.json)
  - `--sandbox-image` (use settings.json)
  - `--all-files` (use @ includes in the application)
  - `--show-memory-usage` (use settings.json)

## Findings — Session File Experiments (2025-10-28)

**Note**: This section has been updated to reflect current version 0.8.1 findings.

- Tooling: `specs/Research/Tools/SessionFileExperiments/gemini.py` runs a minimal session (pexpect‑only) and attempts `--checkpointing`.
- Storage (observed on this machine): No new files were created under `~/.gemini/` during a short run. It is possible the installed version only persists checkpoints when edits occur or when a restore point is explicitly created.
- Recommendation: Run with `--checkpointing` in a repo and approve an edit tool so a file actually changes; then inspect recent files under `~/.gemini/` directory, including shadow history. Use `list_recent.py` to surface recent writes.

How to produce session/checkpoint files (recommended procedure):

- Start in a writable project directory.
- Run: `gemini --checkpointing --approval-mode yolo`
- Prompt: "Create a file named `experiment.tmp` with one line, then append another line and print the file."
- Wait for edits to complete; then run `/stop`.
- Inspect recent files under `~/.config/gemini-cli` and `~/.local/share/gemini` and in the project’s VCS metadata for shadow history. Use `specs/Research/Tools/SessionFileExperiments/list_recent.py`.
