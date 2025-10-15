# Sourcegraph Cody CLI — Integration Notes

## Overview

Sourcegraph Cody CLI is the command-line interface for Cody AI coding assistant, providing terminal-based access to Cody's capabilities.

- **Website**: <https://sourcegraph.com/cody>
- **Documentation**: <https://sourcegraph.com/docs/cody/clients/install-cli>
- **GitHub**: <https://github.com/sourcegraph/cody>
- **npm package**: `@sourcegraph/cody-cli`
- **Version tested**: 5.5.21 (as of documentation)
- **Short description**: Command-line AI coding assistant with code search integration; experimental for Enterprise accounts

## Important 2025 Update

**Cody CLI deprecation notice**: Starting July 23, 2025, Cody will no longer be available for Free, Pro, and Enterprise Starter plans. Users are being directed to Amp as an alternative service.

**Amp compatibility**: Amp runs in VS Code and compatible forks (Cursor, Windsurf, VSCodium) and as a CLI.

### Task start-up commands

Cody CLI usage (to be documented from `--help` output):

1. **Installation**:

   ```bash
   npm install -g @sourcegraph/cody-cli
   ```

2. **Authentication**:

   ```bash
   cody auth login
   # Or via environment variables:
   export SRC_ENDPOINT=https://sourcegraph.com
   export SRC_ACCESS_TOKEN=your-token
   ```

3. **Interactive vs non-interactive**: To be documented

4. **Machine-readable output**: Unknown; requires investigation

5. **Session resumption**: Unknown; requires investigation

6. **Model specification**: Unknown; likely determined by Sourcegraph account settings

### Support for custom hooks

**Status: UNKNOWN**

No documentation found for per-step hooks. Investigation required.

### How to skip the initial onboarding screens on first launch of the agent?

**Authentication via environment variables:**
- `SRC_ENDPOINT`: Endpoint URL
- `SRC_ACCESS_TOKEN`: Access token

**Secure storage**: Access tokens stored in OS secure storage (keychain/keyring) when using `cody auth login`.

### Checkpointing (point-in-time restore of chat + filesystem)

**Status: UNKNOWN**

Not documented in available materials.

### Session continuation (conversation resume)

**Status: UNKNOWN**

Requires testing to determine capabilities.

### Where are chat sessions stored?

**Status: UNKNOWN**

Investigation required.

### What is the format of the persistent chat sessions?

**Status: UNKNOWN**

Investigation required.

### How to run the agent with a set of MCP servers?

**Status: UNKNOWN**

MCP support not documented for Cody CLI.

### Credentials

**Authentication methods:**
- `cody auth login`: Interactive authentication
- Environment variables: `SRC_ENDPOINT`, `SRC_ACCESS_TOKEN`

**Storage:**
- Secure OS storage (keychain/keyring) for tokens
- Configuration in OS-specific app data directories

### Known issues and quirks

- **Experimental status**: Cody CLI is experimental for Enterprise accounts
- **Deprecation timeline**: Free/Pro/Starter plans being deprecated July 23, 2025
- **Amp migration**: Users being directed to Amp as alternative
- **npm installation**: Requires Node.js/npm
- **Sourcegraph integration**: Designed for use with Sourcegraph code search
- **Enterprise focus**: Primarily targeted at Enterprise customers
- **Not in nixpkgs**: Would require Node.js/npm packaging

## Research Status

**IMPORTANT**: Given the deprecation timeline and migration to Amp, this agent may not be a high priority for Agent Harbor integration.

This document is incomplete and requires:
1. Installation and `--help` documentation
2. Testing of core features
3. Session management investigation
4. Evaluation of Amp as potential replacement

**Note**: Not currently in nixpkgs. Would require npm-based packaging if pursued.

## Recommendation

Given the July 2025 deprecation for non-Enterprise plans, Agent Harbor should:
1. Evaluate Amp as potential alternative
2. Prioritize other CLI agents (Claude Code, Cursor, Codex, etc.)
3. Consider Cody CLI only if Enterprise Sourcegraph integration is required

**Amp** may be a better long-term investment for integration efforts.
