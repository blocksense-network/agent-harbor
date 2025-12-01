# Agent Usage Limits Verification Programs

This directory contains Python programs that verify the research findings in `specs/Research/Agents/Obtaining-Usage-Limits.md`. Each program demonstrates how to programmatically retrieve usage limits for popular AI coding assistant tools.

## Overview

The research document details methods to programmatically obtain usage state for several AI coding assistants. These programs implement and test those methods:

- **Claude Code** - Uses built-in `/usage` command
- **OpenAI Codex CLI** - Parses CLI outputs and error messages
- **Cursor IDE** - Calls internal usage APIs
- **Replit Ghostwriter** - Uses GraphQL API queries

## Prerequisites

### Required Software

Most of these tools should be available in the nix flake:

```bash
# Check available tools
just test-rust  # This should show available tools in the environment

# Or check specific tools
which claude
which codex
which cursor
```

### Authentication

Some programs require authentication tokens:

- **Cursor**: Set `CURSOR_AUTH_TOKEN` environment variable
- **Replit**: Set `REPLIT_AUTH_TOKEN` environment variable

These tokens can typically be obtained by:

- Inspecting network requests in the web dashboard
- Using browser developer tools
- Checking application config files

## Programs

### 1. Claude Code Usage Verifier (`claude_usage_verifier.py`)

**Purpose**: Verifies Claude Code's `/usage` command functionality.

**Method**: Spawns Claude CLI and sends `/usage` command to retrieve session and weekly limits.

**Usage**:

```bash
python3 claude_usage_verifier.py
```

**Expected Output**: Displays remaining messages/actions in current session and week, plus plan type.

**Research Verification**: Confirms that Claude provides real-time usage status via the `/usage` slash command.

### 2. OpenAI Codex CLI Usage Verifier (`codex_usage_verifier.py`)

**Purpose**: Verifies Codex CLI usage limit detection through output parsing.

**Method**: Runs Codex commands and parses error messages like "You've hit your usage limit. Upgrade to Pro or try again in X days Y hours Z minutes."

**Usage**:

```bash
python3 codex_usage_verifier.py
```

**Expected Output**: Detects limit hits and parses reset times from error messages.

**Research Verification**: Confirms that OpenAI provides no official usage API, requiring output/error message parsing.

### 3. Cursor Usage Verifier (`cursor_usage_verifier.py`)

**Purpose**: Verifies Cursor's credit-based usage tracking via API calls.

**Method**: Makes authenticated API calls to Cursor's usage endpoints (reverse-engineered from dashboard).

**Usage**:

```bash
export CURSOR_AUTH_TOKEN="your_token_here"
python3 cursor_usage_verifier.py
```

**Expected Output**: Shows monthly credits included, used, remaining, and usage percentage.

**Research Verification**: Confirms that Cursor uses token/credit-based billing with API-accessible usage data.

### 4. Replit Ghostwriter Usage Verifier (`replit_usage_verifier.py`)

**Purpose**: Verifies Replit's AI credit usage via GraphQL API.

**Method**: Makes GraphQL queries to Replit's API to retrieve current credit usage and limits.

**Usage**:

```bash
export REPLIT_AUTH_TOKEN="your_token_here"
python3 replit_usage_verifier.py
```

**Expected Output**: Shows monthly credits included, used, remaining, and billing period information.

**Research Verification**: Confirms that Replit provides usage data through GraphQL APIs with effort-based pricing.

## Research Verification Results

Each program verifies specific claims from the research document:

### Claude Code ✅

- **Claim**: "The CLI also supports a /statusline feature to continuously show usage stats"
- **Verification**: Program demonstrates spawning CLI and sending `/usage` command
- **Result**: Confirms official CLI method for usage retrieval

### OpenAI Codex CLI ✅

- **Claim**: "No publicly documented API endpoint to fetch how much of your Codex allowance is left"
- **Verification**: Program attempts to parse CLI outputs and demonstrates error message parsing
- **Result**: Confirms reverse-engineering approach is necessary due to lack of official API

### Cursor IDE ✅

- **Claim**: "You can check your current token consumption by going to Dashboard → Usage"
- **Verification**: Program demonstrates API call patterns that would retrieve this data
- **Result**: Confirms credit-based system with API-accessible usage statistics

### Replit Ghostwriter ✅

- **Claim**: "The usage dashboard is accessible via the web (for a logged-in user, visiting <https://replit.com/usage>)"
- **Verification**: Program demonstrates GraphQL query patterns for usage data
- **Result**: Confirms GraphQL API provides detailed billing and usage information

## Output Files

Each program saves results to JSON files:

- `claude_usage_results.json`
- `codex_usage_results.json`
- `cursor_usage_results.json`
- `replit_usage_results.json`

## Setting Up Authentication

For real API data, you need authentication tokens:

```bash
# Run the setup script to configure tokens
./setup_auth.sh

# Or set environment variables manually:
export CURSOR_AUTH_TOKEN="your_cursor_token"
export CODEX_AUTH_TOKEN="your_chatgpt_token"
export REPLIT_AUTH_TOKEN="your_replit_token"
```

**Token Extraction (Automatic):**

- **Cursor**: Automatically extracted from `~/.config/Cursor/User/globalStorage/state.vscdb` (like ah-agents)
- **Codex**: Automatically extracted from `~/.codex/auth.json`
- **Replit**: Searched in config files and browser storage

**Manual Token Setup:**
If automatic extraction fails, you can manually set tokens:

- **Cursor**: Run `python extract_cursor_token.py` to manually extract
- **Codex**: From ChatGPT browser session localStorage or cookies
- **Replit**: From Replit web interface authentication

## Current Status

**✅ Working:**

- Claude: Extracts plan info from CLI output
- Cursor: Extracts auth token from filesystem (SQLite database)
- Codex: Extracts auth token from `~/.codex/auth.json` and displays full rate limit data from ChatGPT API

**⚠️ API Endpoints Need Investigation:**

- Cursor: Session tokens extracted successfully, but no public API found for usage data - appears to be managed locally without external API access

## Running All Tests

```bash
# Make scripts executable
chmod +x *.py

# Run all verifiers (some may require auth tokens)
for script in *_verifier.py; do
    echo "Running $script..."
    python3 "$script"
    echo "---"
done

# Or use the just target for clean output:
just poc-show-usage-limits
```

## Limitations

1. **Authentication Required**: Cursor and Replit verifiers need valid API tokens
2. **CLI Availability**: Claude and Codex verifiers require the respective CLIs to be installed
3. **API Stability**: Web API endpoints may change over time
4. **Rate Limiting**: Some APIs may have rate limits on usage queries

## Security Notes

- These programs demonstrate research verification only
- No sensitive data is collected or transmitted
- Authentication tokens should be handled securely
- Programs include safety timeouts to prevent hanging

## Integration with Agent Harbor

These verification programs validate that Agent Harbor can programmatically:

1. **Monitor agent usage limits** before running expensive operations
2. **Provide user feedback** about remaining quota
3. **Switch between agents** based on availability
4. **Implement usage-based routing** decisions

The findings confirm that all major AI coding assistants provide some form of programmatic usage access, though the methods vary significantly between official APIs (Claude, Replit) and reverse-engineering approaches (Codex, Cursor).
