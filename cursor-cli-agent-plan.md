# Cursor CLI Agent Integration Plan

## Overview

This document outlines the comprehensive plan to add Cursor CLI (`cursor-agent`) support to the Agent Harbor system, following the established patterns from Claude Code, Codex CLI, and Gemini CLI agents.

## Background

Cursor CLI is a proprietary AI coding agent with the following characteristics:

- **Binary name**: `cursor-agent`
- **Configuration directory**: `~/.cursor/`
- **Authentication**: API key via `CURSOR_API_KEY` env var or `--api-key` flag
- **Models**: Supports gpt-5, sonnet-4, sonnet-4-thinking
- **Output formats**: text, json, stream-json (with `--print` mode)
- **Session management**: Built-in chat resumption capabilities
- **MCP support**: Full MCP server integration via `.cursor/mcp.json`

## Implementation Plan

### Phase 1: Core Agent Implementation

#### 1.1 Feature Gate Configuration

**File**: `crates/ah-agents/Cargo.toml`

- Add `cursor-cli = []` feature gate to the `[features]` section
- Ensure it's included in default features if desired

#### 1.2 Agent Module Creation

**File**: `crates/ah-agents/src/cursor.rs` (new file)

Key components to implement:

1. **CursorAgent struct**:

   ```rust
   pub struct CursorAgent {
       binary_path: String,
   }
   ```

2. **Version Detection**:
   - Parse output from `cursor-agent --version`
   - Expected format: version strings like "2025.09.18-39624ef"

3. **Launch Configuration**:
   - Interactive mode: `cursor-agent` (default)
   - Non-interactive: `cursor-agent --print "prompt"`
   - Model selection: `--model <model>`
   - API key: `--api-key` or `CURSOR_API_KEY` env var
   - Output format: `--output-format json|text|stream-json`
   - MCP server integration (future)

4. **Configuration Setup**:
   - Create `~/.cursor/cli-config.json` to skip onboarding
   - Configure default settings for automated use
   - Set up MCP server configurations if specified

5. **Credential Management**:
   - Copy `~/.cursor/cli-config.json`
   - Copy `~/.cursor/mcp.json`
   - Handle API key storage/retrieval

6. **Session Management**:
   - Export: Archive entire `~/.cursor/` directory
   - Import: Restore from archive
   - Note: Cursor has built-in session management, but we'll provide external backup

7. **Output Parsing**:
   - Parse JSON output for tool usage tracking
   - Support stream-json format for real-time monitoring
   - Map to normalized `AgentEvent` types

#### 1.3 Library Integration

**File**: `crates/ah-agents/src/lib.rs`

Updates needed:

- Add `#[cfg(feature = "cursor-cli")]` module declaration
- Add feature-gated constructor: `pub fn cursor() -> cursor::CursorAgent`
- Add to `agent_by_name()` function
- Add to `available_agents()` list

#### 1.4 Credential Paths

**File**: `crates/ah-agents/src/credentials.rs`

Add `cursor_credential_paths()` function:

```rust
pub fn cursor_credential_paths() -> Vec<PathBuf> {
    vec![
        PathBuf::from(".cursor/cli-config.json"),
        PathBuf::from(".cursor/mcp.json"),
    ]
}
```

### Phase 2: Configuration and Onboarding

#### 2.1 CLI Configuration Setup

Create `cli-config.json` with settings to:

- Skip onboarding screens
- Disable auto-updates in automated environments
- Configure default model and output preferences
- Set up MCP server defaults

#### 2.2 MCP Integration

- Support MCP server configuration copying
- Handle MCP authentication if needed
- Provide utilities for MCP server management

### Phase 3: Testing and Validation

#### 3.1 Unit Tests

**File**: `crates/ah-agents/src/cursor.rs`

- Version parsing tests
- Configuration generation tests
- Credential path tests

#### 3.2 Integration Tests

**File**: `crates/ah-agents/tests/integration_test.rs`

- Add Cursor-specific integration tests
- Test credential copying
- Test session export/import
- Test launch configuration

#### 3.3 Mock Server Tests

**File**: `crates/ah-agents/tests/mock_server_integration.rs`

- Add Cursor mock server scenarios
- Test API key authentication
- Test model selection
- Test output format parsing

### Phase 4: Documentation and Specs

#### 4.1 Update Agent Documentation

**File**: `AGENTS.md`

- Add Cursor CLI section
- Document setup requirements
- List supported features and limitations

#### 4.2 Update Specifications

**File**: `specs/Public/3rd-Party-Agents/Cursor-CLI.md`

- Mark as "implemented" in Agent Harbor
- Document integration specifics
- Update any findings from implementation

### Phase 5: Build System Integration

#### 5.1 Nix Flake Updates

**File**: `flake.nix`

- Ensure `cursor-cli` package is available
- Add to dev shell if needed

#### 5.2 CI/CD Updates

- Add Cursor tests to CI pipeline
- Update dependency management
- Add Cursor binary availability checks

## Implementation Details

### Key Cursor CLI Behaviors to Handle

1. **Authentication Priority**:
   - Environment variable `CURSOR_API_KEY` takes precedence
   - Falls back to `--api-key` flag
   - May support interactive login via `cursor-agent login`

2. **Output Format Mapping**:
   - `--output-format json` → structured parsing
   - `--output-format stream-json` → streaming event parsing
   - Default text format → basic line parsing

3. **Model Specification**:
   - `--model gpt-5`
   - `--model sonnet-4`
   - `--model sonnet-4-thinking`

4. **Interactive vs Non-Interactive**:
   - Default: Interactive TUI
   - `--print`: Non-interactive mode
   - Different stdio handling required

5. **Session Management**:
   - `cursor-agent ls`: List sessions
   - `cursor-agent --resume <id>`: Resume session
   - External archiving for backup/restore

### Configuration File Structure

**cli-config.json** (to be created):

```json
{
  "version": "1.0",
  "settings": {
    "skipOnboarding": true,
    "autoUpdate": false,
    "defaultModel": "gpt-5",
    "outputFormat": "json",
    "vimMode": false
  },
  "mcp": {
    "servers": []
  }
}
```

### Error Handling Considerations

1. **Binary Not Found**: Clear error when `cursor-agent` not in PATH
2. **Authentication Required**: Handle missing API key gracefully
3. **Version Compatibility**: Check minimum supported version
4. **MCP Server Issues**: Handle MCP configuration errors
5. **Session Corruption**: Robust session import/export error handling

### Testing Strategy

1. **Unit Tests**: Core functionality without external dependencies
2. **Integration Tests**: Full agent lifecycle with mock API
3. **Manual Testing**: Real Cursor CLI with test scenarios
4. **CI Testing**: Automated tests in CI environment

## Dependencies and Prerequisites

- **Cursor CLI**: Must be installed and available in PATH
- **API Key**: Valid Cursor API key for testing
- **MCP Servers**: Optional, for advanced testing
- **Test Infrastructure**: Mock Cursor API server for integration tests

## Risk Assessment

### High Risk

- **Proprietary Binary**: No source code access, behavior changes possible
- **API Compatibility**: Cursor may change CLI interface without notice
- **Authentication Changes**: API key format or requirements may change

### Medium Risk

- **Session Format**: Internal session storage format not documented
- **MCP Integration**: Complex MCP server setup and configuration
- **Version Detection**: Version string format may vary

### Low Risk

- **Basic Launch**: Core launch functionality follows standard patterns
- **Credential Copying**: File-based configuration is straightforward
- **Output Parsing**: JSON parsing is well-established

## Success Criteria

1. **Core Functionality**: Can launch Cursor CLI with proper configuration
2. **Authentication**: Supports API key authentication methods
3. **Model Selection**: Can specify and use different models
4. **Session Management**: Can export/import agent sessions
5. **Testing**: Comprehensive test coverage for all features
6. **Documentation**: Complete documentation and specifications
7. **Integration**: Works with existing Agent Harbor infrastructure

## Timeline Estimate

- **Phase 1**: 2-3 days (core implementation)
- **Phase 2**: 1-2 days (configuration and onboarding)
- **Phase 3**: 2-3 days (testing and validation)
- **Phase 4**: 1 day (documentation updates)
- **Phase 5**: 1 day (build system integration)

**Total**: 7-10 days for complete implementation

## Next Steps

1. Review and approve this plan
2. Begin implementation with Phase 1
3. Regular check-ins for progress and adjustments
4. Testing with real Cursor CLI environment
5. Final integration and documentation updates
