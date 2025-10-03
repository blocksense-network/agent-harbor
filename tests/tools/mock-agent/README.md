# Mock Coding Agent

A lightweight, deterministic mock "coding agent" that can impersonate both Codex and Claude Code for testing and development:

## Features

- **🎭 Dual Format Support**: Impersonates both Codex CLI and Claude Code agents
- **📝 File Operations**: Create, overwrite, append, and replace files in workspaces
- **🖥️ Terminal Output**: Streams thinking traces and tool-use messages to stdout
- **📁 Session Recording**: Writes session files in either Codex or Claude format
- **🌐 API Server**: Mock OpenAI/Anthropic API server for testing IDE integrations
- **🎬 Session Recording**: Record agent interactions with asciinema for demonstrations
- **🧪 Test-Ready**: Comprehensive integration test suite with non-interactive verification

## Format Support

The agent can impersonate different tools via the `--format` flag:

- **Codex Format** (`--format codex`, default): Compatible with [Codex Session File Format](../../../specs/Research/Codex-Session-File-Format.md)
- **Claude Format** (`--format claude`): Compatible with [Claude Session File Format](../../../specs/Research/Claude-Session-File-Format.md)

Each format creates session files in the appropriate directory structure with tool-specific metadata, conversation threading, and environment context.

## Claude Code Integration

Claude Code supports custom API servers via environment variables, enabling testing and integration with alternative LLM providers:

```bash
# Configure Claude Code to use the mock server
export ANTHROPIC_BASE_URL="http://127.0.0.1:18080"
export ANTHROPIC_API_KEY="mock-key"

# Run Claude Code with the mock server
claude "Create hello.py that prints Hello, World!"
```

This allows you to:

- Test Claude Code integrations without API costs
- Develop and test MCP servers
- Demonstrate agent capabilities with recorded sessions
- Validate enterprise gateway configurations

### Claude Code Hooks Configuration

For testing Agent Time-Travel functionality with real Claude Code, the integration tests automatically configure hooks in temporary directories to avoid polluting your home folder.

**Test Configuration**:
- Claude Code hooks are configured in a temporary `~/.claude/settings.json` within the test environment
- The `HOME` environment variable is set to point to a temporary directory during test execution
- This ensures your real `~/.claude` directory remains untouched

**Manual Configuration** (for development/testing):
If you need to manually configure Claude Code hooks, create the settings files:

**Project-specific hooks** (`.claude/settings.json`):
```json
{
  "hooks": {
    "PostToolUse": [
      {
        "matcher": "write_file|read_file|append_file|replace_in_file",
        "hooks": [
          {
            "type": "command",
            "command": "$CLAUDE_PROJECT_DIR/hooks/simulate_snapshot.py",
            "timeout": 30
          }
        ]
      }
    ]
  }
}
```

**Global hooks** (`~/.claude/settings.json`):
```json
{
  "hooks": {
    "PostToolUse": [
      {
        "matcher": "*",
        "hooks": [
          {
            "type": "command",
            "command": "/path/to/global/snapshot/hook.py"
          }
        ]
      }
    ]
  }
}
```

The hook script will be executed after each file operation, allowing you to test filesystem snapshot creation and Agent Time-Travel branching functionality.

### Codex CLI Hooks Configuration

For testing Agent Time-Travel functionality with real Codex CLI, the integration tests automatically configure hooks and verify execution.

**Test Configuration**:
- Codex CLI hooks are configured using the `--rollout-hook` command-line option
- The `CODEX_HOME` environment variable is set to point to a temporary directory during test execution
- Hook execution is verified through evidence files created during testing
- This ensures your real `~/.codex` directory remains untouched

**Manual Configuration** (for development/testing):
If you need to manually configure Codex CLI hooks, use the `--rollout-hook` option:

```bash
# Run Codex with a hook that executes after each rollout entry
codex --rollout-hook "/path/to/snapshot/hook.py" "Create hello.py"

# Multiple arguments can be passed to the hook
codex --rollout-hook "my-hook-script.sh" "arg1" "arg2" "Create hello.py"
```

The hook command receives the JSON rollout entry as its last argument. The hook script should process this JSON to extract tool information and create filesystem snapshots.

**Example Hook Script**:
```bash
#!/bin/bash
# Last argument is the JSON rollout entry
json_entry="${@: -1}"
echo "Processing rollout entry: $json_entry" >&2
# Parse JSON and create snapshot...
```

For the mock agent testing, the hook format remains the same as Claude Code, but when testing with real Codex CLI, use the `--rollout-hook` command-line option.

## Quickstart

### Installation

The mock agent runs in the current development environment. No separate installation needed.

```bash
# Verify the agent works
python -m src.cli --help
```

### Basic Usage

```bash
# Run built-in demo (creates hello.py)
python -m src.cli demo --workspace /tmp/mock-ws

# Run with Claude format
python -m src.cli demo --workspace /tmp/mock-ws --format claude

# Run a custom scenario
python -m src.cli run --scenario examples/hello_scenario.json --workspace /tmp/mock-ws

# Run with different formats
python -m src.cli run --scenario examples/hello_scenario.json --workspace /tmp/mock-ws --format codex
python -m src.cli run --scenario examples/hello_scenario.json --workspace /tmp/mock-ws --format claude
```

### Output Locations

**Codex Format:**

- Session files: `~/.codex/sessions/YYYY/MM/DD/rollout-*.jsonl`
- UI logs: `~/.codex/logs/session-*.jsonl`

**Claude Format:**

- Session files: `~/.claude/projects/<encoded-workspace-path>/<uuid>.jsonl`

## Mock API Server

Run a mock OpenAI/Anthropic-compatible API server for testing IDE integrations. The server actually executes tools and creates/modifies files in workspaces:

```bash
# Start server with comprehensive playbook
python -m src.cli server --host 127.0.0.1 --port 18080 --playbook examples/comprehensive_playbook.json

# Test with OpenAI-compatible API
curl -s http://127.0.0.1:18080/v1/chat/completions \
  -H 'content-type: application/json' \
  -d '{"model": "gpt-4o-mini", "messages": [{"role":"user","content":"Create hello.py"}]}'

# Test with Anthropic-compatible API (for Claude Code)
curl -s http://127.0.0.1:18080/v1/messages \
  -H 'content-type: application/json' \
  -H 'anthropic-version: 2023-06-01' \
  -d '{"model": "claude-3-sonnet", "messages": [{"role":"user","content":"Create hello.py"}], "max_tokens": 100}'
```

The server supports:

- **OpenAI API** (`/v1/chat/completions`): Compatible with Codex CLI and other OpenAI-based tools
- **Anthropic API** (`/v1/messages`): Compatible with Claude Code and other Anthropic-based tools
- **Tool Execution**: Actually creates, modifies, and reads files in designated workspaces
- **Session Recording**: Records all interactions in appropriate session file formats
- **Deterministic Responses**: Uses playbook rules for predictable testing scenarios

## Session Recording with Asciinema

Record actual agent terminal interactions for demonstrations using asciinema. These recordings show the real output that users see when running codex and claude commands (not the JSON testing output):

```bash
# Start the mock server
python tests/tools/mock-agent/start_test_server.py --port 18080 &

# Set environment for Claude Code
export ANTHROPIC_BASE_URL="http://127.0.0.1:18080"
export ANTHROPIC_API_KEY="mock-key"

# Run integration tests to generate recordings
just test-mock-agent-integration

# Replay recordings to see actual agent behavior
just replay-mock-agent-sessions        # Interactive fzf menu for all recordings (↑↓ navigation, type to filter)
just replay-last-mock-agent-session     # Replays most recent recording
just clear-mock-agent-recordings        # Clears all recording files
```

The integration tests include automated asciinema recording that captures:

- **Codex recordings**: Real terminal output showing interactive command execution, colored UI, and actual file operations
- **Claude recordings**: Full interactive sessions with API key confirmation and prompt processing

**Test Verification**: The integration tests verify actual functionality by checking that:

- CLI commands return success codes
- Files are created with correct content in the expected workspace locations through API-driven tool execution
- Commands complete within timeout limits
- Workspace isolation is maintained between tests
- Side effects occur as expected (no superficial exit-without-processing)

**Interactive Mode**: Tests run Claude and Codex in their standard modes (without output-formatting flags like --print) to ensure full workflow execution and proper API interaction.

## Session File Formats

The mock agent supports two session file formats via the `--format` flag, allowing it to impersonate different coding tools:

### Codex Format (`--format codex`, default)

Mimics the Codex CLI session format with rollout files and optional UI logs:

- **Location**: `~/.codex/sessions/YYYY/MM/DD/rollout-YYYY-MM-DDThh-mm-ss-<uuid>.jsonl`
- **UI Logs**: `~/.codex/logs/session-YYYYMMDDTHHMMSSZ.jsonl` (when `CODEX_TUI_RECORD_SESSION=1`)
- **Structure**: Linear sequence of events with session metadata, messages, tool calls, and results
- **Use Case**: Testing applications that integrate with Codex CLI

### Claude Format (`--format claude`)

Mimics Claude Code's session format with rich conversation threading and context:

- **Location**: `~/.claude/projects/<encoded-project-path>/<session-uuid>.jsonl`
- **Structure**: Threaded conversation with parent-child UUID relationships, detailed tool context
- **Features**: Git integration, environment tracking, usage statistics, tool result metadata
- **Use Case**: Testing applications that integrate with Claude Code

See [Codex Session File Format](../../../specs/Research/Codex-Session-File-Format.md) and [Claude Session File Format](../../../specs/Research/Claude-Session-File-Format.md) for detailed specifications.

## Scenario Format

Create custom scenarios using JSON files. Example structure:

```json
{
  "meta": {
    "instructions": "You are a helpful coding agent.",
    "turn_context": {
      "cwd": "/workspace",
      "model": "mock-model"
    }
  },
  "turns": [
    { "user": "Create hello.py" },
    { "think": "I'll create a Python file" },
    {
      "tool": {
        "name": "write_file",
        "args": { "path": "hello.py", "text": "print('Hello!')\n" }
      }
    },
    { "assistant": "Created hello.py successfully" }
  ]
}
```

**Supported turn types:**

- `user`: User message
- `think`: Assistant reasoning/thinking
- `tool`: Tool call with name and arguments
- `assistant`: Assistant response
- `shell`: Shell command execution

## Hooks Support

The mock agent supports hooks similar to Claude Code hooks, enabling testing of Agent Time-Travel functionality and filesystem snapshot integration.

### Hook Configuration

Hooks are configured in scenario JSON files under the `hooks` key. The configuration follows the Claude Code hooks format:

```json
{
  "hooks": {
    "session_id": "test-session-123",
    "PostToolUse": [
      {
        "matcher": "write_file|read_file",
        "hooks": [
          {
            "type": "command",
            "command": "$CLAUDE_PROJECT_DIR/hooks/simulate_snapshot.py",
            "timeout": 30
          }
        ]
      }
    ]
  }
}
```

**Supported Hook Events:**
- `PostToolUse`: Executed after successful or failed tool execution

**Hook Input Format:**
Hooks receive JSON input via stdin containing:
```json
{
  "session_id": "test-session-123",
  "transcript_path": "/tmp/mock-transcript.jsonl",
  "cwd": "/workspace/path",
  "hook_event_name": "PostToolUse",
  "tool_name": "write_file",
  "tool_input": {"path": "file.txt", "text": "content"},
  "tool_response": {"success": true}
}
```

### Filesystem Snapshot Testing

The mock agent includes built-in support for testing filesystem snapshot functionality through hooks. A sample hook script `hooks/simulate_snapshot.py` simulates taking snapshots by appending evidence to `.ah/snapshots/evidence.log`.

**Running Snapshot Tests:**

```bash
# Run the snapshot test scenario
python -m src.cli run --scenario examples/snapshot_test_scenario.json --workspace /tmp/snapshot-test --format claude

# Check that snapshots were taken
cat /tmp/snapshot-test/.ah/snapshots/evidence.log
```

**Current Status**: Hook verification infrastructure is complete:
- ✅ **Codex CLI**: Hooks are enabled and verified in integration tests using `--rollout-hook`
- ⚠️ **Claude Code**: Hooks work in interactive mode but are bypassed in API client mode
- 🏗️ **Infrastructure**: Complete hook execution verification with evidence logging
- 📁 **Isolation**: Temporary directories prevent pollution of user home directories

**Evidence File Format:**
Each snapshot creates a JSON line in the evidence file:
```json
{
  "timestamp": "2025-09-22T10:30:45.123456",
  "session_id": "test-session-snapshots-123",
  "tool_name": "write_file",
  "tool_input": {"path": "hello.py", "text": "..."},
  "tool_response": {"success": true},
  "event": "PostToolUse",
  "snapshot_id": "snapshot-2025-09-22T10-30-45-123456",
  "provider": "mock-fs-snapshot"
}
```

## Tool Support

The mock agent supports these file operation tools:

- **`write_file`**: Create or overwrite files
- **`read_file`**: Read file contents
- **`append_file`**: Append to existing files
- **`replace_in_file`**: Replace text within files

Tool results are properly formatted for each session format, including success/error status and metadata.

## Testing

The mock agent includes sophisticated integration tests that exercise **real interactive CLI sessions** with automated user simulation:

### Interactive Session Testing

The integration tests run actual Codex and Claude Code CLI tools in their **standard interactive mode** using a **scenario-driven automation framework**:

#### Scenario-Driven Automation

Interactive tests use predefined JSON scenarios that describe the complete user interaction flow:

```json
{
  "description": "Interactive Claude Code session for creating a hello.py file",
  "tool": "claude",
  "prompt": "Create hello.py that prints Hello, World!",
  "steps": [
    {
      "type": "expect",
      "patterns": [
        "Do you want to use this API key\\?",
        "Enter to confirm",
        "TIMEOUT"
      ],
      "timeout": 5,
      "description": "Wait for API key confirmation dialog"
    },
    {
      "type": "send",
      "text": "1",
      "sendline": true,
      "description": "Select option 1 (Yes) for API key"
    },
    {
      "type": "expect",
      "patterns": ["Enter to confirm"],
      "timeout": 3,
      "description": "Wait for confirmation prompt"
    },
    {
      "type": "send",
      "text": "",
      "sendline": true,
      "description": "Confirm selection with Enter"
    }
  ],
  "expectations": [
    {
      "type": "file_exists",
      "path": "hello.py"
    },
    {
      "type": "file_contains",
      "path": "hello.py",
      "text": "Hello, World!"
    }
  ]
}
```

#### Automation Framework Features

- **Pattern Matching**: Wait for specific UI prompts and outputs using regex patterns
- **Input Simulation**: Send keystrokes and commands to interact with CLI interfaces
- **State Verification**: Check file system state and content after interactions
- **Error Handling**: Robust cleanup and timeout handling for reliable automation
- **Multi-Tool Support**: Unified framework for both Codex and Claude Code testing

This approach provides **fully automated end-to-end testing** of the complete user experience, from CLI launch through interactive prompts to final task completion.

**Current Status**: Interactive testing framework is implemented with scenario-driven automation for both Codex and Claude. Codex interactive tests work fully, while Claude interactive tests handle API key confirmation but require further refinement for complete prompt processing.

**Note**: Interactive testing provides end-to-end validation of real CLI workflows, with `--print` mode available as a reliable fallback for CI/CD environments.

```bash
# Run integration tests (recommended)
just test-mock-agent-integration

# Or run directly
python tests/test_agent_integration.py

# Run specific test
python -m unittest tests.tools.mock-agent.tests.test_agent_integration.MockAgentIntegrationTest.test_claude_file_creation -v

# Replay specific agent recordings
just replay-last-mock-agent-codex-session
just replay-last-mock-agent-claude-session

# Legacy simple tests
python tests/test_agent_simple.py
```

### Test Architecture

The tests use a sophisticated automation framework that:

- **Spawns real CLI processes** in interactive mode using `pexpect`
- **Monitors terminal output** for prompts, menus, and UI elements
- **Simulates user input** (keystrokes, menu selections, confirmations)
- **Coordinates with mock server** for API request/response cycles
- **Validates final results** (files created, content correct, session state)

This creates fully automated end-to-end tests that mirror real user interactions while remaining deterministic and reliable.

The integration tests verify:

- **Complete interactive workflows**: From CLI launch through UI prompts to task completion
- **UI interaction handling**: Menu selections, prompt responses, error recovery
- **API integration**: Proper request formatting, response parsing, tool execution
- **File system operations**: Correct workspace usage, file creation/modification
- **Hook execution**: Filesystem snapshot hooks are triggered and create evidence files
- **Session management**: Recording, state persistence, cleanup

### Files Created During Testing

The integration tests create the following files (automatically cleaned up after completion):

**Workspace Files** (in temporary test directories):

- `hello.py` - Simple Python file created by file creation tests
- `calculator.py` - Calculator module with add/subtract functions
- `test_calculator.py` - Unit tests for the calculator module
- Various other files created by playbook rules

**Session Recordings** (persistent in `tests/tools/mock-agent/recordings/`):

- `{tool}_{scenario}_{timestamp}.json` - Descriptive recordings from interactive test sessions
- Use `just clear-mock-agent-recordings` to remove all recordings

**Temporary Files** (cleaned up after each test):

- `MOCK_AGENT_WORKSPACE.txt` - Inter-process communication file for workspace paths

**Hook Evidence Files** (created during hook testing):

- `.ah/snapshots/evidence.log` - JSONL file containing snapshot evidence entries
- Each entry proves a hook was executed with tool details and timestamps

**Interactive Scenarios** (in `tests/tools/mock-agent/scenarios/`):

- `claude_file_creation.json` - Scenario for Claude Code file creation with API key confirmation
- `codex_file_creation.json` - Scenario for Codex file creation workflow

**Session Files** (in `tests/tools/mock-agent/sessions/` during testing):

- Codex format: `rollout-YYYY-MM-DDTHH-MM-SS-<uuid>.jsonl`
- Claude format: `<uuid>.jsonl` in project-specific subdirectories

Session recordings and scenarios are preserved for development, while workspace and temporary files are cleaned up after each test run.

See [INTEGRATION_TESTING.md](INTEGRATION_TESTING.md) for detailed testing documentation.

## Implementation Details

- **`src/session_io.py`**: Session file writers for both Codex and Claude formats
- **`src/agent.py`**: Core agent logic and scenario execution
- **`src/cli.py`**: Command-line interface with format selection
- **`src/tools.py`**: File operation implementations with workspace support
- **`src/server.py`**: Mock API server supporting OpenAI and Anthropic APIs with tool execution
- **`tests/test_agent_integration.py`**: Comprehensive integration tests for both CLI tools
- **`examples/comprehensive_playbook.json`**: Extensive playbook with multi-step workflows

## License

MIT
