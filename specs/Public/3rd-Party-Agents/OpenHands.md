# OpenHands — Integration Notes

## Overview

OpenHands (formerly OpenDevin) is an open-source AI software development platform that can execute complex coding tasks autonomously.

- **Website**: <https://www.all-hands.dev/>
- **Documentation**: <https://docs.all-hands.dev>
- **GitHub**: <https://github.com/All-Hands-AI/OpenHands>
- **Version tested**: TBD (actively developed)
- **Short description**: Open-source platform for autonomous AI software development with CLI, GUI, and headless modes

### Task start-up commands

OpenHands provides multiple ways to launch:

1. **CLI mode** (recommended install method):

   ```bash
   uvx --python 3.12 --from openhands-ai openhands
   ```

2. **GUI server mode**:

   ```bash
   uvx --python 3.12 --from openhands-ai openhands serve
   # Opens GUI at http://localhost:3000
   ```

3. **Headless mode**: Available for automation (exact command to be documented)

4. **GitHub Action integration**: Can run on tagged issues automatically

**Interactive vs non-interactive**:
- CLI mode: Command-line interaction
- GUI mode: Web interface at localhost:3000
- Headless mode: Automation without UI

**Machine-readable output**: JSON output capabilities to be determined through testing.

**Session resumption**: Session management capabilities to be investigated.

**Model specification**: Supports multiple LLM providers; default recommended is Anthropic Claude Sonnet 4.5.

### Support for custom hooks

**Status: UNKNOWN**

OpenHands does not document per-step hooks in available materials. As an open-source platform, investigation needed to determine:
- Whether hooks can be implemented
- Extension points in the codebase
- MCP server integration for extensibility

Further testing required.

### How to skip the initial onboarding screens on first launch of the agent?

**Status: TO BE DETERMINED**

OpenHands requires:
- Python 3.12
- LLM provider selection and configuration
- API keys for chosen provider

Likely approaches for automation:
- Environment variables for API keys
- Configuration files for provider settings
- Command-line flags (to be documented)

Investigation needed.

### Checkpointing (point-in-time restore of chat + filesystem)

**Status: UNKNOWN**

- **Official checkpointing**: Not documented in available materials
- **Scope**: N/A
- **Restore semantics**: N/A
- **Operational notes**: N/A

Investigation required to determine if checkpointing is supported.

### Session continuation (conversation resume)

**Status: TO BE DETERMINED**

OpenHands likely supports session persistence given its multi-mode architecture. Investigation needed to document:
- How sessions are stored
- Resumption commands/procedures
- What context is preserved

### Where are chat sessions stored?

**Status: UNKNOWN**

Likely locations based on Python application patterns:
- **Linux/macOS**: `~/.openhands/` or `~/.local/share/openhands/`
- **Windows**: `%APPDATA%\openhands\` or `%LOCALAPPDATA%\openhands\`

Investigation required.

### What is the format of the persistent chat sessions?

**Status: UNKNOWN**

Session format needs investigation. As OpenHands is open-source, reviewing the codebase may reveal serialization details.

### Reverse‑engineering policy for session formats

**Recommended procedure:**

1. Install OpenHands via `uvx --python 3.12 --from openhands-ai openhands`
2. Run minimal session with benign task
3. Locate session files in likely directories
4. Examine file format (JSON/JSONL/database)
5. Review source code for session management
6. Test resumption if supported
7. Document findings

### How to run the agent with a set of MCP servers?

**Status: UNKNOWN**

OpenHands documentation does not specify MCP server configuration in available materials. Investigation needed to determine:
- Whether MCP is supported
- Configuration file format
- Command-line options for MCP

As an open-source project, MCP support may be possible to add if not present.

### Credentials

OpenHands authentication with LLM providers:

**Required configuration:**
- LLM provider selection (Anthropic, OpenAI, etc.)
- API keys for chosen provider
- Recommended: Anthropic Claude Sonnet 4.5

**Storage locations (to be confirmed):**
- Configuration likely in `~/.openhands/` or similar
- API keys via environment variables or config files

**Investigation needed** to document exact credential storage and configuration.

### Known issues and quirks

- **Python 3.12 requirement**: Must use Python 3.12 specifically
- **uv installer**: Recommended installation via `uvx` (uv Python package runner)
- **Single-user design**: Optimized for local workstation use, not multi-tenant deployments
- **GUI on localhost**: Web interface runs on `http://localhost:3000`
- **Open source**: Actively developed, community-driven
- **Platform support**: Linux, macOS, Windows
- **Capabilities**: Can modify code, run commands, browse web, call APIs
- **GitHub integration**: Supports running on GitHub issues via Actions
- **No Docker required**: Simplified installation compared to earlier versions
- **Not in nixpkgs**: Currently requires installation via uv/pip

## Deployment Modes

1. **Local CLI**: For interactive terminal-based development
2. **Local GUI**: For web-based interface and visual interaction
3. **Headless**: For automation and CI/CD pipelines
4. **GitHub Actions**: For automated issue resolution

## Research Status

This document requires extensive hands-on testing. Key next steps:

1. Install OpenHands via uv
2. Run `openhands --help` to document command-line options
3. Test CLI, GUI, and headless modes
4. Locate configuration files and session storage
5. Document LLM provider configuration
6. Test with mock LLM API server
7. Investigate MCP support
8. Determine if can be packaged for nixpkgs

**Note**: OpenHands is not in nixpkgs. May be possible to create a Nix derivation wrapping the uv installation.

## Installation Notes

**Prerequisites:**
```bash
# Install uv (Python package runner)
curl -LsSf https://astral.sh/uv/install.sh | sh

# Then run OpenHands
uvx --python 3.12 --from openhands-ai openhands
```

**Alternative - GUI server:**
```bash
uvx --python 3.12 --from openhands-ai openhands serve
```
