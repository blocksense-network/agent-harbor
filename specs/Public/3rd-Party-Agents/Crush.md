# Crush — Integration Notes

## Overview

Crush is Charmbracelet's "glamorous AI coding agent" for the terminal, featuring a beautiful TUI interface and extensive AI provider support.

- **Website**: <https://charm.land/blog/crush-comes-home/>
- **Documentation**: <https://github.com/charmbracelet/crush>
- **GitHub**: <https://github.com/charmbracelet/crush>
- **nix-ai-tools**: Available as `crush` package (version 0.11.1 in nix-ai-tools)
- **Version tested**: 0.11.1 (from nix-ai-tools)
- **Short description**: Open-source terminal AI coding agent with LSP integration, MCP support, and beautiful Bubble Tea TUI

### Task start-up commands

Crush can be started with various invocations:

1. **Direct interactive mode**:

   ```bash
   crush
   ```

2. **Interactive vs non-interactive**: Crush is primarily an interactive TUI application; non-interactive modes to be determined through `--help` documentation.

3. **Machine-readable output**: JSON output capabilities unknown; requires investigation.

4. **Session resumption**: Session management capabilities to be determined.

5. **Model specification**: Supports switching between AI models mid-session; configuration via JSON config files.

**Note**: Detailed command-line options require running `crush --help` to document fully.

### Support for custom hooks

**Status: UNKNOWN**

Crush does not appear to have documented support for per-step custom hooks in the available materials. However, as an open-source project, it may be possible to:
- Contribute hook functionality
- Use MCP servers for extensibility
- Monitor log files for step-by-step tracking

Investigation required to determine if hooks can be implemented for Agent Time Travel.

### How to skip the initial onboarding screens on first launch of the agent?

**Status: TO BE DETERMINED**

As Crush uses configuration files, likely approaches:
- Pre-create configuration JSON in expected locations
- Set environment variables for API keys
- Use command-line flags (to be documented from `--help`)

Further testing needed.

### Checkpointing (point-in-time restore of chat + filesystem)

**Status: NO built-in checkpointing found**

- **Official checkpointing**: Not documented in available materials
- **Scope**: N/A
- **Restore semantics**: N/A
- **Operational notes**: Use external tooling for filesystem snapshots

### Session continuation (conversation resume)

**Status: TO BE DETERMINED**

Session management capabilities need to be investigated by:
- Examining configuration directory structure
- Testing session persistence across restarts
- Reviewing source code if needed

### Where are chat sessions stored?

Based on typical Charmbracelet application patterns:

**Likely locations:**
- **Linux/macOS**: `~/.config/crush/` or `~/.local/share/crush/`
- **Windows**: `%APPDATA%\crush\` or `%LOCALAPPDATA%\crush\`

Crush stores logs locally in project directories:
- `./.crush/logs/crush.log` (per-project logs)

Investigation required to confirm session storage paths.

### What is the format of the persistent chat sessions?

**Status: UNKNOWN**

Session format needs investigation. As Crush is open-source, reviewing the codebase at <https://github.com/charmbracelet/crush> may reveal session serialization details.

### Reverse‑engineering policy for session formats

**Recommended procedure:**

1. Install Crush and run minimal session
2. Inspect `~/.config/crush/` and `./.crush/` directories
3. Examine created files for serialization format
4. Review source code for session management logic
5. Test session resumption if supported
6. Document findings

### How to run the agent with a set of MCP servers?

Crush supports Model Context Protocol (MCP) through three transport types:

- **stdio**: Standard input/output based servers
- **http**: HTTP-based servers
- **sse**: Server-Sent Events based servers

**Configuration method:**

MCP servers are configured via JSON configuration files. The exact structure and location need to be documented by:
1. Running `crush --help` to find config file paths
2. Examining example configuration files from the repository
3. Testing MCP server integration

**Example configuration pattern** (to be confirmed):
```json
{
  "mcp_servers": [
    {
      "name": "my-server",
      "type": "stdio",
      "command": "/path/to/mcp-server"
    }
  ]
}
```

### Credentials

Crush supports multiple AI provider authentication:

**Supported providers:**
- Anthropic
- OpenAI
- Gemini (Google)
- Groq
- AWS Bedrock (Claude)
- Azure OpenAI
- OpenAI-compatible APIs (Ollama, etc.)

**Configuration storage:**

Credentials and settings are stored in JSON configuration files:
- **Linux/macOS**: `~/.config/crush/config.json` (likely)
- **Windows**: `%APPDATA%\crush\config.json` (likely)

**Privacy**: Crush emphasizes local storage; no data sent to Charm's servers beyond basic functionality.

**Investigation needed** to document exact file paths and configuration structure.

### Known issues and quirks

- **Open-source**: Actively developed on GitHub, community-driven
- **Platform support**: macOS, Linux, Windows (PowerShell and WSL), FreeBSD, OpenBSD, NetBSD
- **Built with Go**: Fast performance and cross-platform compatibility
- **TUI-based**: Beautiful terminal interface using Bubble Tea framework
- **LSP integration**: Language Server Protocol support for real-time code intelligence
- **MCP support**: Extensive Model Context Protocol integration
- **Local LLM support**: Works with Ollama and other local models
- **Community**: Large community (150,000+ GitHub stars for Charmbracelet projects)
- **Installation**: Available via Homebrew, npm, Go, Scoop, Nix
- **Not in nixpkgs**: Currently requires manual installation or nix run from external flake

## Installation Methods

**Package managers:**
```bash
# Homebrew
brew install charmbracelet/tap/crush

# NPM
npm install -g @charmland/crush

# Nix (from external flake)
nix run github:numtide/nix-ai-tools#crush

# Arch Linux
yay -S crush-bin

# Windows
winget install crush
scoop install crush
```

## Research Status

This document requires hands-on testing to complete. Key next steps:

1. Install Crush and capture `crush --help` output
2. Locate and document configuration file paths
3. Test MCP server configuration
4. Examine session management capabilities
5. Document API key configuration for various providers
6. Test with mock LLM API server
7. Investigate possibility of adding to nixpkgs or creating custom derivation

**Note**: Crush is available in the nix-ai-tools flake and has been added to Agent Harbor's flake.nix.

## Integration with Agent Harbor

**Available in flake.nix**: Yes, via `nix-ai-tools.packages.${system}.crush`

**Binary name**: `crush`
