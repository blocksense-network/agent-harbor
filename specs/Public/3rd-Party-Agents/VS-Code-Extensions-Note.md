# VS Code Extensions — Integration Note

## Overview

Several popular AI coding agents are primarily available as VS Code extensions rather than standalone CLI tools. This document addresses their integration status with Agent Harbor.

## VS Code Extensions in Agent List

The following tools from the template list are VS Code extensions:

1. **Cursor VS Code Extension** - Built into Cursor IDE (fork of VS Code)
2. **GitHub Copilot VS Code Extension** - Extension for VS Code/IDEs
3. **Cline** (formerly Claude Dev) - VS Code extension
4. **Kilo Code** - VS Code/JetBrains extension
5. **Roo Code** - VS Code extension (fork of Cline)

## Integration Possibilities ✅

**UPDATE**: VS Code extensions ARE suitable for CLI integration through automation!

VS Code provides multiple mechanisms for programmatic extension control:

### Automation Approaches

1. **Command-Line Interface**
   - `code --wait`: Block until window closes (CI/CD friendly)
   - `code --goto`: Open files at specific locations
   - `code --inspect-extensions=<port>`: Enable debugging protocol
   - `code --command`: Execute extension commands (if exposed)

2. **Chrome DevTools Protocol (CDP)**
   - VS Code extension host supports CDP
   - Can connect via Puppeteer/Playwright
   - Full programmatic control over extension state
   - Execute commands, monitor events, capture output

3. **Extension API Automation**
   - `vscode.commands.executeCommand()`: Programmatic command execution
   - Custom bridge extension: HTTP/WebSocket API for external control
   - File watchers: Filesystem-based command triggering

4. **Community Solutions**
   - Cline: Active development of remote control API (GitHub #2622)
   - Agent Maestro: Extension with partial automation support
   - WebSocket-based control: Working prototypes exist

### Recommended Integration Strategy

**For Agent Harbor:**

1. **Phase 1 (MVP)**: Command Palette automation via `code --command`
2. **Phase 2 (Robust)**: Custom "Agent Harbor VS Code Bridge" extension
   - Local HTTP/WebSocket server
   - Forward commands to Cline/Kilo/Roo Code
   - Return results to Agent Harbor
3. **Phase 3 (Long-term)**: Contribute upstream APIs to extension projects

See [VS-Code-Extension-Automation.md](./VS-Code-Extension-Automation.md) for detailed implementation strategies.

## Specific Extension Notes

### Cline

- **Type**: VS Code extension
- **GitHub**: <https://github.com/cline/cline>
- **Description**: Autonomous coding agent for VS Code
- **CLI availability**: No standalone CLI; VS Code extension only
- **Installation**: VS Code Marketplace
- **Modes**: Plan mode (read-only) and Act mode (read/write)
- **Open source**: Yes, highly popular (3.2M+ users)

**Agent Harbor integration**: Not feasible without significant reverse-engineering or upstream CLI development.

### Kilo Code

- **Type**: VS Code/JetBrains extension
- **GitHub**: <https://github.com/Kilo-Org/kilocode>
- **Description**: Open-source AI agent for VS Code (fork of Roo/Cline)
- **CLI availability**: No standalone CLI
- **Installation**: VS Code Marketplace, Open VSX
- **Features**: 400+ LLMs, MCP support
- **Open source**: Yes

**Agent Harbor integration**: Not feasible without CLI interface.

### Roo Code

- **Type**: VS Code extension
- **GitHub**: <https://github.com/RooCodeInc/Roo-Code>
- **Description**: VS Code extension, fork of Cline
- **CLI availability**: No standalone CLI
- **Installation**: VS Code Marketplace
- **Modes**: Code, Architect, Ask modes
- **Open source**: Yes

**Agent Harbor integration**: Not feasible without CLI interface.

### Cursor VS Code Extension

- **Note**: Cursor is a full IDE (VS Code fork), not just an extension
- **Cursor CLI**: Available separately as `cursor-cli` / `cursor-agent`
- **Integration**: Use cursor-cli (already documented in Cursor-CLI.md)

**Agent Harbor integration**: Use cursor-cli, not the VS Code extension.

### GitHub Copilot VS Code Extension

- **Note**: GitHub Copilot has both extension and CLI versions
- **Copilot CLI**: Available separately (documented in GitHub-Copilot-CLI.md)
- **Integration**: Use the standalone Copilot CLI, not VS Code extension

**Agent Harbor integration**: Use standalone CLI, not extension.

## Recommendation for Agent Harbor

### High Priority (Standalone CLI)

These have standalone CLI tools - immediate integration:
- ✅ **Claude Code** (in nixpkgs)
- ✅ **Cursor CLI** (in nixpkgs-unstable)
- ✅ **Codex CLI** (custom flake)
- ✅ **Goose** (in nixpkgs)
- ✅ **Gemini CLI** (in nixpkgs)
- ✅ **OpenCode** (in nixpkgs)
- ✅ **Qwen Code** (in nixpkgs)
- ✅ **Windsurf** (in nixpkgs - IDE with CLI)
- ✅ **GitHub Copilot CLI** (nix-ai-tools)
- ✅ **Crush** (nix-ai-tools)
- ✅ **Amp** (nix-ai-tools)
- ✅ **Groq Code CLI** (nix-ai-tools)
- ⚠️ **OpenHands** (uv/Python, not in nixpkgs)

### Medium Priority (VS Code Extensions via Automation)

These can be integrated through VS Code automation:
- 🔄 **Cline** - VS Code extension, automation via CDP/bridge extension
- 🔄 **Kilo Code** - VS Code extension, automation via CDP/bridge extension
- 🔄 **Roo Code** - VS Code extension, automation via CDP/bridge extension

**Integration approach**: Develop "Agent Harbor VS Code Bridge" extension that exposes HTTP/WebSocket API for controlling these extensions programmatically.

### Low Priority (Deprecated)

- ⚠️ **Sourcegraph Cody CLI** - Deprecated July 2025, replaced by Amp

## Summary

**Of the original agent types listed:**
- **12** have standalone CLI interfaces (all in Agent Harbor flake.nix)
- **3** are VS Code extensions that CAN be automated via VS Code's programmatic APIs
- **1** is deprecated (Cody CLI → replaced by Amp)

**Integration Status:**
- ✅ **Immediate use**: 12 CLI agents in flake.nix
- 🔄 **Automation possible**: 3 VS Code extensions (Cline, Kilo, Roo)
- 📋 **Future work**: Develop VS Code Bridge extension for robust automation

**Key Finding**: VS Code extensions are NOT blocked from integration - they require a different approach using VS Code's automation capabilities (CDP, command API, custom bridge extension).

For Agent Harbor:
1. **Short term**: Use the 12 CLI agents already integrated
2. **Medium term**: Prototype VS Code automation with existing tools
3. **Long term**: Develop Agent Harbor VS Code Bridge for robust extension control
