# Amp — Integration Notes

## Overview

Amp is Sourcegraph's autonomous AI coding agent, launched in July 2025 as the official successor to Cody. It provides agentic capabilities for complex coding tasks.

- **Website**: <https://ampcode.com/> and <https://sourcegraph.com/amp>
- **Documentation**: To be determined from CLI help output
- **GitHub**: Not yet publicly available (as of documentation)
- **nix-ai-tools**: Available as `amp` package (version 0.0.1760534862-gbac8c1)
- **Short description**: Autonomous coding agent built for teams with no token constraints, successor to Cody

## Important Context

**Cody Deprecation**: Cody Free, Pro, and Enterprise Starter plans were discontinued on July 23, 2025. Amp is the official successor with enhanced agentic capabilities.

**Design Philosophy**: Unlike traditional code completion tools (e.g., GitHub Copilot), Amp actively collaborates with users through thread-based interactions, confirming, explaining, and revising actions before applying them.

### Task start-up commands

Amp provides multiple access methods:

1. **CLI mode**:
   ```bash
   amp
   ```

2. **VS Code extension**: Available as VS Code extension for GUI-based interaction

3. **Browser-based**: Can run directly in browser

**Command-line options**: To be documented from `amp --help` output.

**Interactive vs non-interactive**: Capabilities to be determined through testing.

**Machine-readable output**: Unknown; requires investigation.

**Session resumption**: Thread-based interaction model suggests session persistence; details to be documented.

**Model specification**: To be determined from help output and configuration.

### Support for custom hooks

**Status: UNKNOWN**

Amp does not document per-step hooks in available materials. Investigation needed to determine hook support for Agent Time Travel integration.

### How to skip the initial onboarding screens on first launch of the agent?

**Status: TO BE DETERMINED**

Amp likely requires:
- Sourcegraph account or authentication
- API keys or tokens
- Configuration for first launch

Methods to investigate:
- Environment variables
- Configuration files
- Command-line authentication flags

### Checkpointing (point-in-time restore of chat + filesystem)

**Status: UNKNOWN**

- **Official checkpointing**: Not documented
- **Thread-based interactions**: May provide session history
- Investigation required

### Session continuation (conversation resume)

Amp uses thread-based interactions:

- **Threads**: Shared by default for team collaboration
- **Context preservation**: Threads maintain conversation and workflow context
- **Resume capabilities**: Exact mechanism to be determined

The thread model suggests strong session persistence capabilities.

### Where are chat sessions stored?

**Status: UNKNOWN**

Likely locations:
- **Linux/macOS**: `~/.amp/` or `~/.config/amp/`
- **Windows**: `%APPDATA%\amp\` or `%LOCALAPPDATA%\amp\`

Investigation required.

### What is the format of the persistent chat sessions?

**Status: UNKNOWN**

Thread format to be determined through testing.

### How to run the agent with a set of MCP servers?

**Status: UNKNOWN**

MCP support not documented in available materials. As a modern AI agent, MCP support is likely but needs confirmation.

### Credentials

Amp authentication:

**Requirements:**
- Sourcegraph account (likely)
- Team configuration for shared threads
- API keys or tokens

**Storage locations (to be confirmed):**
- Configuration directory: `~/.amp/` or similar
- Credentials stored via Sourcegraph authentication

**Investigation needed** for exact paths and configuration format.

### Known issues and quirks

- **Team-focused**: Designed for team collaboration with shared threads and workflows
- **No token constraints**: Unlimited token usage (significant differentiator)
- **Cody successor**: Official replacement for Cody Free/Pro/Starter
- **Autonomous operation**: Can handle complex, multi-step tasks independently
- **Thread-based interaction**: Uses conversation threads for context management
- **Multi-platform**: CLI, VS Code extension, and browser access
- **July 2025 launch**: Relatively new platform, may have evolving features
- **Available in nix-ai-tools**: Included in numtide/nix-ai-tools flake

## Capabilities

Amp can perform complex autonomous tasks:
- Read entire repositories
- Generate test files
- Refactor large modules
- Document code
- Create new components
- Step-by-step reasoning (like a junior developer)

## Research Status

This document requires hands-on testing. Key next steps:

1. Install Amp from nix-ai-tools: `nix run github:numtide/nix-ai-tools#amp`
2. Run `amp --help` to document command-line interface
3. Test authentication and initial setup
4. Document thread-based interaction model
5. Locate configuration and session storage
6. Test MCP server support if available
7. Evaluate for Agent Harbor integration

**Note**: Amp is available in the nix-ai-tools flake and has been added to Agent Harbor's flake.nix.

## Installation

```bash
# Via nix-ai-tools flake (already in Agent Harbor flake.nix)
nix run github:numtide/nix-ai-tools#amp

# Within Agent Harbor dev shell (after flake update)
amp
```

## Integration with Agent Harbor

**Available in flake.nix**: Yes, via `nix-ai-tools.packages.${system}.amp`

**Binary name**: `amp`

**Priority**: High - official Cody successor with team features and no token limits

**Investigation needed**: Complete CLI documentation, authentication flow, session management, and hook capabilities.
