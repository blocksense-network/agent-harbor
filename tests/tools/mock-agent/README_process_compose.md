# Process Compose Integration Testing

This document describes how to use the `scripts/agent-test-run.py` script for running integration tests between the mock LLM server and Agent Harbor CLI.

## Overview

The `scripts/agent-test-run.py` script provides a convenient way to run orchestrated integration tests that combine:

- **Mock LLM API Server**: Simulates OpenAI/Anthropic API responses with realistic coalescing (thinking + tools + text in single responses)
- **Agent Harbor CLI**: Runs agent start commands with various configurations
- **TUI Testing Framework**: Optional screenshot and exit command automation
- **Isolated Workspace**: Creates separate repo/ and user-home/ directories for each test run

## Usage

### Basic Syntax

```bash
python3 scripts/agent-test-run.py [OPTIONS] (--scenario SCENARIO | --playbook PLAYBOOK)
```

### Required Arguments

One of the following must be specified:

- `--scenario SCENARIO`: Use a YAML scenario file from `tests/tools/mock-agent/scenarios/`
- `--playbook PLAYBOOK`: Use a JSON playbook file from `tests/tools/mock-agent/examples/`

### Server Configuration

The script supports configuration of the mock LLM API server behavior:

- `--server-llm-api-style STYLE`: LLM API style - `openai` or `anthropic` (default: `openai`)
- `--server-coalesce-thinking`: Enable coalescing thinking with tool use for Anthropic style (default: enabled)
- `--server-tools-profile PROFILE`: Tools profile name (automatically set based on `--agent` type, can be overridden if needed)
- `--strict-tools-validation`: Enable strict mode - abort on unknown tool definitions to identify missing mappings during development

**Note**: When using the mock server directly (not through agent-test-run.py), you can also specify these options:

- `--tools-profile PROFILE`: Tools profile for the target coding agent (default: codex)
- `--strict-tools-validation`: Enable strict tools validation

### Optional Arguments

- `--server-port PORT`: Port for mock LLM server (default: 18081)
- `--tui-port PORT`: Port for TUI testing IPC (default: 5555)

#### Agent Configuration
- `--agent-type TYPE`: Agent type - `mock`, `codex`, `claude`, `gemini`, `opencode`, `qwen`, `cursor-cli`, `goose` (default: codex)
- `--non-interactive`: Enable non-interactive mode (`--non-interactive` flag)
- `--output-format FORMAT`: Output format - `text`, `text-normalized`, `json`, `json-normalized` (default: json)
- `--llm-api URI`: Custom LLM API URI for agent backend
- `--llm-api-key KEY`: API key for custom LLM API

#### Working Directory
- `--working-dir PATH`: Custom working directory (default: auto-generated from scenario name)

#### TUI Testing
- `--enable-tui-testing`: Enable TUI testing integration
- `--tui-command CMD`: TUI command to send (default: "exit:0")

#### Process Compose
- `--config-only`: Only generate config, don't run process-compose
- `--config-file FILE`: Save config to specific file

## Examples

### Basic Codex Integration Test

```bash
# Run codex with test scenario in non-interactive mode
python3 scripts/agent-test-run.py --scenario test_scenario --non-interactive
```

### Claude with JSON Output

```bash
# Run claude with feature implementation scenario and JSON output
python3 scripts/agent-test-run.py \
  --scenario feature_implementation_scenario \
  --agent-type claude \
  --output-format json
```

### Gemini with Custom API

```bash
# Run gemini agent with custom Google AI API
python3 scripts/agent-test-run.py \
  --scenario realistic_development_scenario \
  --agent-type gemini \
  --llm-api https://generativelanguage.googleapis.com \
  --llm-api-key your-gemini-key
```

### OpenCode with Custom API

```bash
# Run opencode agent with custom OpenCode API
python3 scripts/agent-test-run.py \
  --scenario code_refactoring_scenario \
  --agent-type opencode \
  --llm-api https://api.opencode.example.com \
  --llm-api-key your-opencode-key
```

### With TUI Testing

```bash
# Run test with TUI screenshot capture
python3 scripts/agent-test-run.py \
  --scenario realistic_development_scenario \
  --non-interactive \
  --enable-tui-testing \
  --tui-command "screenshot:final_state"
```

### Using Playbook Instead of Scenario

```bash
# Use JSON playbook instead of YAML scenario
python3 scripts/agent-test-run.py \
  --playbook comprehensive_playbook \
  --agent-type codex \
  --non-interactive
```

### Custom Ports

```bash
# Use custom ports to avoid conflicts
python3 scripts/agent-test-run.py \
  --scenario bug_fix_scenario \
  --server-port 18082 \
  --tui-port 5556 \
  --non-interactive
```

### Anthropic Style with Thinking Coalescing

```bash
# Run with Anthropic API style (exposes thinking blocks)
python3 scripts/agent-test-run.py \
  --scenario realistic_development_scenario \
  --server-llm-api-style anthropic \
  --non-interactive
```

### OpenAI Style (Thinking Internal Only)

```bash
# Run with OpenAI API style (thinking kept internal)
python3 scripts/agent-test-run.py \
  --scenario realistic_development_scenario \
  --server-llm-api-style openai \
  --non-interactive
```

### Strict Mode for Development

```bash
# Enable strict mode to catch missing tool mappings during development
# Tools profile is automatically set based on --agent type
python3 scripts/agent-test-run.py \
  --scenario test_scenario \
  --agent claude \
  --strict-tools-validation \
  --non-interactive
```

### Generate Config Only

```bash
# Generate process-compose config without running it
python3 scripts/agent-test-run.py \
  --scenario test_scenario \
  --config-only > test_config.yaml
```

## Available Scenarios

Run `python3 scripts/agent-test-run.py --scenario` to see available scenarios:

- `basic_timeline_scenario`
- `bug_fix_scenario`
- `claude_file_creation`
- `code_refactoring_scenario`
- `codex_file_creation`
- `documentation_scenario`
- `feature_implementation_scenario`
- `new_format_scenario`
- `realistic_development_scenario` (default)
- `test_scenario`
- `testing_workflow_scenario`
- `timing_test_scenario`

## Available Playbooks

Run `python3 scripts/agent-test-run.py --playbook` to see available playbooks:

- `comprehensive_playbook`
- `hello_scenario`
- `playbook`

## Process Compose Configuration

The script generates a process-compose YAML configuration with the following processes:

### mock-server
- Runs the mock LLM API server
- Waits for health check before starting dependent processes
- Configured with selected scenario/playbook
- Only sets mock API environment variables when `--llm-api` is NOT specified

### ah-agent
- Runs `ah agent start` with specified options
- Depends on mock-server being healthy
- Runs in isolated working directory with `repo/` and `user-home/` subdirectories
- Configured with environment variables for API connection and workspace isolation

### tui-testing (optional)
- Runs TUI testing commands
- Only included when `--enable-tui-testing` is specified
- Depends on ah-agent being started

## Environment Variables

The script sets the following environment variables for the ah-agent process:

### Always Set:
- `HOME`: Points to the `user-home/` subdirectory
- `AH_HOME`: Isolated home directory for Agent Harbor (within user-home)
- `TUI_TESTING_URI`: IPC endpoint for TUI testing (when enabled)

### Agent-Specific API Variables:
- **Codex**: `CODEX_API_BASE`, `CODEX_API_KEY`
- **Claude**: `ANTHROPIC_BASE_URL`, `ANTHROPIC_API_KEY`
- **Gemini**: `GOOGLE_AI_BASE_URL`, `GOOGLE_API_KEY`
- **OpenCode**: `OPENCODE_API_BASE`, `OPENCODE_API_KEY`
- **Qwen**: `QWEN_API_BASE`, `QWEN_API_KEY`
- **Cursor CLI**: `CURSOR_API_BASE`, `CURSOR_API_KEY`
- **Goose**: `GOOSE_API_BASE`, `GOOSE_API_KEY`

### Mock Server Integration:
When `--llm-api` is NOT specified, the script automatically configures the agent to use the mock server by setting the appropriate API base URL and key (typically "mock-key").

## Troubleshooting

### Process Compose Not Found

```bash
# Install process-compose
# On macOS with Homebrew:
brew install process-compose

# Or download from: https://github.com/F1bonacc1/process-compose
```

### Port Conflicts

If ports are already in use, use `--server-port` and `--tui-port` to specify different ports:

```bash
python3 launch_with_process_compose.py \
  --scenario test_scenario \
  --server-port 18082 \
  --tui-port 5556
```

### Build Issues

Make sure the Agent Harbor CLI is built:

```bash
cd /path/to/project
cargo build
```

### Scenario/Playbook Not Found

Verify the scenario/playbook file exists:

```bash
ls tests/tools/mock-agent/scenarios/
ls tests/tools/mock-agent/examples/
```

### YAML Import Error

The script requires PyYAML for scenario support. If you see import errors, ensure you're in a nix environment with direnv:

```bash
# Make sure direnv is loaded
direnv reload

# Or enter the nix shell directly
nix develop
```

PyYAML should be available in the nix flake environment. If not, add it to the flake's Python dependencies.

Note: YAML is optional - you can still use JSON playbooks without PyYAML.

### Custom LLM API Issues

When using `--llm-api` and `--llm-api-key`, make sure:
- The API endpoint is accessible
- The API key is valid
- The agent type supports the API format you're connecting to

## Integration with Development Workflow

### Running Tests

```bash
# Quick integration test (uses realistic_development_scenario by default)
python3 scripts/agent-test-run.py --non-interactive

# Full development scenario with default settings
python3 scripts/agent-test-run.py \
  --non-interactive \
  --enable-tui-testing \
  --tui-command "screenshot:complete"

# Test different agent types
python3 scripts/agent-test-run.py --agent-type gemini --non-interactive
python3 scripts/agent-test-run.py --agent-type claude --output-format json
python3 scripts/agent-test-run.py --agent-type goose --llm-api https://api.goose.example.com --llm-api-key your-key
```

### Debugging

```bash
# Generate config only for inspection
python3 scripts/agent-test-run.py \
  --scenario test_scenario \
  --config-only > debug_config.yaml

# Edit and run manually
process-compose up --config debug_config.yaml

# View available options
python3 scripts/agent-test-run.py --help
```

## Architecture

The script provides a clean separation between:

1. **Configuration Generation**: Python script builds process-compose YAML with scenario configuration
2. **Process Orchestration**: process-compose manages startup/shutdown dependencies
3. **Workspace Isolation**: Creates isolated `repo/` and `user-home/` directories for each test run
4. **Realistic LLM Simulation**: Supports `llmResponse` grouping for realistic API response patterns
5. **API Style Coalescing**: Different response formats for OpenAI vs Anthropic API styles
6. **Tools Profile Validation**: Validates and maps scenario tools to agent-specific schemas
7. **Integration Testing**: Combines mock server + real CLI + optional TUI testing

### Scenario Response Grouping

The new `llmResponse` event allows grouping multiple response elements into single API responses:

```yaml
timeline:
  - llmResponse:        # Single API response containing:
      - think: [[500, "Analyzing..."]]     # Thinking content
      - agentToolUse: {...}                # Tool suggestions
      - assistant: [[200, "Done!"]]        # Final response
```

This creates **realistic LLM behavior** where thinking, tool use, and text can appear in a single API response, unlike the previous separate-query-per-event approach.

### API Style Coalescing

- **OpenAI Style**: Thinking content is processed internally but **NOT included in API responses**. Only text content and tool calls appear in the assistant message. This matches OpenAI's actual API behavior where thinking is never exposed.
- **Anthropic Style**: Thinking content can be exposed as separate "thinking" blocks in the response content array, alongside "text" blocks and "tool_use" blocks, all within a single API response. This matches Anthropic's extended thinking feature.

### Tools Profile Validation

The server validates tool definitions sent by the coding agent client in API requests:

- **Automatic Profile Selection**: The script automatically sets the tools profile based on the `--agent` type (e.g., `codex` for Codex, `claude` for Claude Code)
- **Client Request Validation**: When the coding agent sends tool_calls in API requests, the server validates that these tools are known and match the current tools profile
- **Tools Profile**: Defines valid tool schemas for each coding agent (Codex, Claude, Gemini, etc.)
- **Strict Tools Validation**: When enabled (`--strict-tools-validation`), the server aborts immediately on unknown tool definitions sent by clients, helping developers quickly identify missing tool profiles and mappings during development
- **Validation Timing**: Validation occurs during API request processing, not during scenario loading

### Workspace Structure

Each test run creates an isolated workspace:

```
test-{scenario-name}/
├── repo/              # Repository directory (populated by scenario/playbook)
└── user-home/         # User home directory (HOME environment variable)
    └── .ah/          # Agent Harbor configuration
```

This approach ensures:
- **Isolation**: Each test run has its own clean environment
- **Realism**: LLM responses match actual API behavior with proper coalescing
- **Flexibility**: Support for both scenario-driven repo setup and custom LLM APIs
- **Reproducibility**: Consistent workspace setup across runs
- **Reliability**: Proper cleanup and dependency management
