# Goose — Integration Notes

> Usually this information can be obtained by checking out command-line help screens or man pages for the agent software.

## Overview

Goose is an AI-powered coding assistant CLI tool developed by Block. It provides an interactive coding experience with support for various AI providers and extensions.

- **Website**: <https://block.github.io/goose/>
- **Documentation**: <https://block.github.io/goose/docs/>
- **GitHub**: <https://github.com/block/goose>
- **Version**: 1.6.0 (as of this writing)

### Task start-up command

Goose can be started with a specific task prompt in several ways:

1. **Direct text input**:

   ```bash
   goose run -t "Implement a user authentication system"
   ```

2. **From a file**:

   ```bash
   goose run -i instructions.txt
   ```

3. **Interactive session**:

   ```bash
   goose session
   ```

4. **With custom system instructions**:
   ```bash
   goose run --system "You are a senior Ruby developer" -t "Refactor this legacy code"
   ```

### Checkpointing (point-in-time restore of chat + filesystem)

No official checkpointing mechanism was found in Goose docs that restores both chat and filesystem to a specific moment in time. Documentation emphasizes sessions and recipes/extensions; file‑system snapshots or rollbacks are not documented as a built‑in feature.

- Scope: No documented file snapshots. Checkpointing not supported as defined by AH.
- Restore semantics: N/A. Use external VCS or your own tooling for file rollback.

### Session continuation (conversation resume)

Goose documents sessions that primarily track conversation and basic context. These enable resuming work but are distinct from filesystem checkpoints.

- **Session creation**: Sessions may be created automatically when running commands
- **Resume capability**: `goose session --resume` to continue the most recent session
- **Named sessions**: `--name <NAME>` to create/resume identifiable sessions
- **Path-based sessions**: `--path <PATH>` to scope session location
- **Export/list/remove**: `goose session export|list|remove`

### Where are chat sessions stored?

- Location (typical): `~/.local/share/goose/sessions/`
- Format: Internal Goose representation; export to Markdown supported
- Persistence: Sessions persist across Goose restarts

### What is the format of the persistent chat sessions?

- Internal format not specified in help; export produces Markdown. Trimming sessions manually is not documented; use built‑in export if needed.

### How is the use of MCP servers configured?

Goose supports MCP servers through both built-in and external configurations:

**Command-line options:**

- `--with-builtin <NAME>`: Add one or more builtin extensions that are bundled with goose
- `--with-extension <COMMAND>`: Add stdio extensions from full commands with environment variables
- `--with-remote-extension <URL>`: Add remote extensions from a URL
- `--with-streamable-http-extension <URL>`: Add streamable HTTP extensions from a URL

**Built-in MCP servers:**

- `goose mcp <NAME>`: Run one of the MCP servers bundled with goose

**Configuration files:**

- `~/.config/goose/config.yaml`: Main configuration file for MCP server settings

**Environment variables:**

- No specific MCP-related environment variables documented

### Support for custom hooks

For Agent Time Travel feature (commands executed after every agent step), Goose does not appear to have built-in support for custom step-level hooks. However, it does support broader customization through:

1. **Recipes**: YAML-based configuration files that define custom agent behaviors
   - Use `--recipe` to specify a recipe file
   - Recipes can include custom parameters with `--params`

2. **Extensions**: Various extension mechanisms that could potentially be adapted for hook-like functionality
   - **Built-in extensions**: `--with-builtin` to add bundled extensions
   - **Stdio extensions**: `--with-extension` for custom command-based extensions
   - **Remote extensions**: `--with-remote-extension` for URL-based extensions
   - **Streamable HTTP extensions**: `--with-streamable-http-extension` for HTTP-based extensions

Note: While extensions exist, there is no documented support for automatic execution of custom commands after every agent step as required for the Agent Time Travel feature.

### Credentials

Goose stores credentials and configuration in the following precise locations:

**Configuration files:**

- **Main config file**: `~/.config/goose/config.yaml` - Contains provider settings, API keys, and authentication details
- **Sessions directory**: `~/.local/share/goose/sessions` - Stores session data and conversation history
- **Logs directory**: `~/.local/state/goose/logs` - Contains log files

**Environment variables:**

- Various `GOOSE_*` environment variables for provider configuration (exact variables depend on configured providers)

**Provider-specific storage:**

- API keys are stored in the main config file (`~/.config/goose/config.yaml`) for different AI providers (OpenAI, Anthropic, etc.)
- Provider-specific authentication tokens and credentials are managed within the config file structure

### Known Issues

- **Provider compatibility**: Some features may vary depending on the chosen AI provider
- **Extension stability**: Custom extensions may have varying levels of stability
- **Resource usage**: Sessions and extensions can consume significant system resources
- **Network dependency**: Requires internet access for AI provider communication

## Findings — Session File Experiments (2025-09-10)

- Tooling: `specs/Research/Tools/SessionFileExperiments/goose.py` executes a short `goose session` flow (pexpect‑only).
- Storage (observed): `~/.local/share/goose/sessions/` with files named by timestamp, e.g. `20250625_135230.jsonl`, `20250625_141115.jsonl`.
- Format (observed): JSONL. Header line includes `{ working_dir, description, message_count, total_tokens, ... }`. Subsequent lines alternate `role: "user"|"assistant"` entries; tool use is encoded as `content` parts with `type: "toolRequest"`/`"toolResponse"`, including `developer__shell` and `developer__text_editor` calls. Timestamps are epoch seconds in a `created` field.
- Resume: After producing a short session, `goose session --resume` continues the last session. No built‑in filesystem checkpointing observed.

Trim test (this machine):

- Source: `~/.local/share/goose/sessions/20250625_141115.jsonl` (169 lines)
- Backup: `.bak-YYYYMMDD-HHMMSS`
- Output: `.trimmed` (84 lines)
