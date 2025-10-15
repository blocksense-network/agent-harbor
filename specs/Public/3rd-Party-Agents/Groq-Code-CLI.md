# Groq Code CLI — Integration Notes

## Overview

Groq Code CLI is a lightweight, open-source command-line AI coding assistant powered by Groq's ultra-fast inference API.

- **Website**: <https://groq.com>
- **Documentation**: <https://github.com/build-with-groq/groq-code-cli>
- **GitHub**: <https://github.com/build-with-groq/groq-code-cli>
- **nix-ai-tools**: Available as `groq-code-cli` package (version 1.0.2-unstable-2025-09-05)
- **Short description**: Fast, lightweight, open-source coding CLI designed as a customizable blueprint for AI-powered coding

## Design Philosophy

Groq Code CLI deliberately goes in the opposite direction from feature-heavy AI tools:
- **Lightweight**: Minimal, simple codebase
- **Fast**: Leverages Groq's ultra-fast inference
- **Customizable**: Designed as a blueprint/framework for user extensions
- **Open source**: Encourages community modifications

### Task start-up commands

Groq Code CLI provides simple command-line interaction:

1. **Start interactive chat**:
   ```bash
   groq
   ```

2. **With temperature parameter**:
   ```bash
   groq -t 0.7
   groq --temperature 0.7
   ```

3. **With custom system message**:
   ```bash
   groq -s "You are a Python expert"
   groq --system "You are a Python expert"
   ```

4. **Authentication via environment variable**:
   ```bash
   export GROQ_API_KEY=your-key
   groq
   ```

**Interactive vs non-interactive**: Primarily interactive chat-based interface.

**Machine-readable output**: Unknown; requires testing.

**Session resumption**: Uses `/clear` command for chat history management; persistent session support to be determined.

**Model specification**: Supports model selection via `/model` command within chat session.

### In-session Commands

**Model management:**
- `/model`: View and select different language models available on Groq platform

**Authentication:**
- `/login`: Set API key (alternative to environment variable)

**Session management:**
- `/clear`: Clear chat history and context

### Support for custom hooks

**Status: NO built-in hooks**

Groq Code CLI is intentionally minimal and does not provide hook mechanisms. As an open-source project, hooks could be added through customization:
- Fork and modify the codebase
- Add hook execution points in the code
- Contribute upstream if hooks are useful to community

The lightweight design makes it easier to add custom features compared to larger tools.

### How to skip the initial onboarding screens on first launch of the agent?

**Authentication methods:**

1. **Environment variable** (recommended for automation):
   ```bash
   export GROQ_API_KEY=your-key
   groq
   ```

2. **In-session login**:
   ```bash
   groq
   # Then use: /login
   ```

No complex onboarding; API key is the only requirement.

### Checkpointing (point-in-time restore of chat + filesystem)

**Status: NO built-in checkpointing**

- **Official checkpointing**: Not supported
- **Chat history**: Can be cleared with `/clear` command
- **Filesystem**: No filesystem state management

### Session continuation (conversation resume)

**Status: LIMITED**

- **Within session**: Chat history maintained during active session
- **Across sessions**: Persistent session storage to be determined through testing
- **Clear history**: `/clear` command resets context

Investigation needed to determine if sessions persist across restarts.

### Where are chat sessions stored?

**Status: UNKNOWN**

Likely locations if sessions are persisted:
- **Linux/macOS**: `~/.groq/` or `~/.config/groq-code-cli/`
- **Windows**: `%APPDATA%\groq\` or `%LOCALAPPDATA%\groq-code-cli\`

Investigation required.

### What is the format of the persistent chat sessions?

**Status: UNKNOWN**

As an open-source project, session format can be determined by reviewing the codebase at:
<https://github.com/build-with-groq/groq-code-cli>

### How to run the agent with a set of MCP servers?

**Status: NOT SUPPORTED**

Groq Code CLI is intentionally minimal and does not document MCP server support. Adding MCP support would require:
- Forking the repository
- Implementing MCP protocol handling
- Contributing back to upstream

### Credentials

Groq Code CLI authentication:

**API Key requirement:**
- Groq API key (from <https://console.groq.com>)

**Configuration methods:**
1. Environment variable: `GROQ_API_KEY`
2. In-session command: `/login`

**Storage:**
- Environment variable (no storage needed)
- Configuration file location to be determined if `/login` persists keys

### Known issues and quirks

- **Groq API required**: Requires Groq API key (Groq provides fast inference)
- **Lightweight design**: Intentionally minimal feature set
- **Customization expected**: Designed as blueprint for user modifications
- **Fast inference**: Leverages Groq's speed advantage
- **Model flexibility**: Supports multiple Groq-hosted models via `/model` command
- **Simple codebase**: Easy to understand and modify
- **Node.js based**: npm package, TypeScript codebase
- **Temperature control**: Fine-tune model behavior with `-t` flag
- **System prompts**: Customize AI behavior with `-s` flag
- **Available in nix-ai-tools**: Included in numtide/nix-ai-tools flake

## Installation

```bash
# Via npm (manual)
git clone https://github.com/build-with-groq/groq-code-cli
cd groq-code-cli
npm install
npm run build
npm link

# Via nix-ai-tools flake (already in Agent Harbor flake.nix)
nix run github:numtide/nix-ai-tools#groq-code-cli

# Within Agent Harbor dev shell (after flake update)
groq
```

## Groq Platform

**Groq advantages:**
- Ultra-fast inference (significantly faster than typical cloud APIs)
- Multiple model support
- Developer-friendly API

**Groq models available:**
To be documented via `/model` command in groq CLI.

## Research Status

This document requires hands-on testing. Key next steps:

1. Install from nix-ai-tools
2. Run `groq --help` to document all CLI options
3. Test `/model` command to list available models
4. Test `/login` to understand credential storage
5. Investigate session persistence
6. Review source code for customization points
7. Evaluate for Agent Harbor integration

**Note**: Groq Code CLI is available in the nix-ai-tools flake and has been added to Agent Harbor's flake.nix.

## Integration with Agent Harbor

**Available in flake.nix**: Yes, via `nix-ai-tools.packages.${system}.groq-code-cli`

**Binary name**: `groq`

**Recommended for:**
- Fast experimentation
- Custom tool development (fork and modify)
- Groq API testing

**Limitations for Agent Time Travel:**
- No built-in hooks (requires customization)
- Minimal session management
- Would need forking to add snapshot capabilities

**Customization potential**: High - intentionally designed as a starting point for building custom tools.
