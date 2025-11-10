# Cursor CLI Agent Integration Plan

## Overview

This document outlines the comprehensive plan for integrating the Cursor CLI agent into the agent-harbor system, following the patterns established in the codebase for Claude, Codex, and Gemini agents.

## Implementation Steps

### 1. Core Agent Implementation

**File: `crates/ah-agents/src/cursor_cli.rs`**

Create the main Cursor CLI agent implementation following the established pattern:

```rust
// Copyright 2025 Schelling Point Labs Inc
// SPDX-License-Identifier: AGPL-3.0-only

/// Cursor CLI agent implementation
use crate::credentials::{copy_files, cursor_credential_paths};
use crate::session::{export_directory, import_directory};
use crate::traits::*;
use async_trait::async_trait;
use regex::Regex;
use std::path::{Path, PathBuf};
use tokio::process::{Child, Command};
use tracing::{debug, info, warn};

/// Cursor CLI agent executor
pub struct CursorCliAgent {
    binary_path: String,
}
```

Key implementation aspects:

- Implement `AgentExecutor` trait with all required methods
- Version detection via `cursor --version` command
- Credential management (OAuth tokens, API keys)
- Configuration setup to skip onboarding
- Support for interactive and non-interactive modes
- Custom API server configuration
- Model selection support
- Output parsing for normalized events

### 2. Feature Flag Configuration

**File: `crates/ah-agents/Cargo.toml`**

The `cursor-cli` feature flag is already defined in the features section.

### 3. Module Integration

**File: `crates/ah-agents/src/lib.rs`**

Add the following sections:

```rust
// Agent implementations (feature-gated)
#[cfg(feature = "cursor-cli")]
pub mod cursor_cli;

// Convenience constructors
#[cfg(feature = "cursor-cli")]
pub fn cursor_cli() -> cursor_cli::CursorCliAgent {
    cursor_cli::CursorCliAgent::new()
}

// In agent_by_name function
#[cfg(feature = "cursor-cli")]
"cursor-cli" => Some(Box::new(cursor_cli::CursorCliAgent::new())),
```

### 4. CLI Integration

**File: `crates/ah-cli/src/agent/start.rs`**

The `CursorCli` variant is already added to the `CliAgentType` enum, so no changes needed here.

### 5. Core Type Updates

**File: `crates/ah-core/src/agent_types.rs`**

The `CursorCli` variant is already added to the `AgentType` enum, so no changes needed here.

### 6. Credential Management

**File: `crates/ah-agents/src/credentials.rs`**

The `cursor_credential_paths()` function is already implemented with:

- `.cursor/cli-config.json`
- `.cursor/mcp.json`

### 7. Implementation Details

#### Version Detection

```rust
async fn detect_version(&self) -> AgentResult<AgentVersion> {
    // Execute `cursor --version` and parse output
    // Expected format: "cursor version X.Y.Z" or "X.Y.Z"
}
```

#### Credential Retrieval

Cursor CLI may use various authentication methods:

- OAuth tokens stored in platform-specific locations
- API keys from environment variables
- Configuration files in `~/.cursor/`

#### Launch Configuration

Key configuration aspects:

- Set custom HOME directory via environment variable
- Configure MCP (Model Context Protocol) servers if provided
- Support for both interactive and non-interactive modes
- Handle custom LLM API endpoints
- Model selection (if supported by cursor CLI)

#### Output Parsing

Parse cursor CLI output into normalized `AgentEvent` types:

- `Thinking`: Reasoning/planning output
- `ToolUse`: Tool invocations (file edits, commands, etc.)
- `Output`: Regular text output
- `Error`: Error messages
- `Complete`: Task completion

### 8. Testing

#### Unit Tests

Add tests in `cursor_cli.rs`:

- Version parsing tests
- Configuration directory tests
- Credential extraction tests

#### Integration Tests

- Mock server integration tests
- Credential copying tests
- Session export/import tests
- Output parsing tests

### 9. Documentation

- Add cursor-cli to the list of supported agents in README
- Document cursor-cli specific environment variables
- Add examples of cursor-cli usage

## Implementation Checklist

- [ ] Create `crates/ah-agents/src/cursor_cli.rs` with full `AgentExecutor` implementation
- [ ] Add module imports and exports in `crates/ah-agents/src/lib.rs`
- [ ] Implement version detection logic
- [ ] Implement credential retrieval (OAuth/API key handling)
- [ ] Implement onboarding skip configuration
- [ ] Add support for MCP server configuration
- [ ] Implement output parsing for normalized events
- [ ] Add comprehensive unit tests
- [ ] Add integration tests
- [ ] Update documentation
- [ ] Test with actual cursor CLI binary

## Key Considerations

1. **Authentication**: Cursor CLI may use different authentication methods than other agents. Research the specific auth flow and credential storage.

2. **Command Structure**: Verify the exact command-line interface of cursor CLI:
   - How to run in non-interactive mode
   - How to specify models
   - How to configure API endpoints
   - How to enable/disable features

3. **Output Format**: Analyze cursor CLI's output format to properly parse events and tool usage.

4. **Configuration Files**: Understand what configuration files cursor CLI uses and what settings can skip onboarding/setup.

5. **Platform Differences**: Consider platform-specific differences in credential storage and configuration paths.

6. **Error Handling**: Implement robust error handling for:
   - Binary not found
   - Version incompatibility
   - Authentication failures
   - Network issues

## Next Steps

1. Research cursor CLI's exact command-line interface and options
2. Analyze cursor CLI's output format for proper parsing
3. Understand cursor CLI's authentication and configuration system
4. Begin implementation following the established patterns
5. Test thoroughly with actual cursor CLI binary
