# Agent Request Tracking

This directory contains captured API requests from third-party coding agents to track how their tool definitions change over time.

## Directory Structure

```
agent-requests/
├── {agent_name}/
│   └── {version_range}/
│       └── request.json
```

### Naming Convention

- **agent_name**: The agent type (e.g., `claude`, `codex`, `gemini`)
- **version_range**: Version range where this request format was observed (e.g., `0.4.0`, `2024.1.0`)
- **request.json**: The captured API request that triggered validation failure

## File Format

Each `request.json` file contains the **exact raw JSON request** sent by the agent to the mock server:

```json
{
  "model": "claude-3-5-sonnet-20240620",
  "messages": [
    {
      "role": "user",
      "content": "Create a hello.py file"
    }
  ],
  "tools": [
    {
      "type": "function",
      "function": {
        "name": "run_terminal_cmd",
        "description": "Execute a terminal command",
        "parameters": {
          "type": "object",
          "properties": {
            "command": {"type": "string", "description": "The command to execute"},
            "cwd": {"type": "string", "description": "Working directory"}
          },
          "required": ["command"]
        }
      }
    }
  ]
}
```

**Note**: This example shows a **real Claude Code initial request** with tools definitions, not a synthetic test request. The tools are defined at the top level of the request, following OpenAI API format.

This preserves the exact format and content of API requests sent by third-party agents.

## Purpose

These files serve to:

- **Track Evolution**: See how agent tool definitions change across versions
- **Debug Validation**: Understand what tools agents actually send
- **Update Profiles**: Use captured data to update tool validation profiles
- **Historical Record**: Maintain git history of agent API changes

## Capturing Requests

To capture real agent requests, set the `FORCE_TOOLS_VALIDATION_FAILURE` environment variable before running `agent-test-run.py`:

```bash
# Force all tool validation to fail to capture agent requests
export FORCE_TOOLS_VALIDATION_FAILURE=1

# Run agent-test-run.py with strict validation
python3 scripts/agent-test-run.py --agent-type claude --strict-tools-validation

# This will save real Claude requests to agent-requests/claude/{version}/request.json
```

The `FORCE_TOOLS_VALIDATION_FAILURE=1` environment variable causes the mock server to:
- Treat all tools as invalid (forcing validation failures)
- Save the complete request JSON when validation fails
- Capture the exact API requests sent by real coding agents

## Version Detection

Agent versions are automatically detected by running the agent's version command:

- **claude**: `claude --version` → detected dynamically (e.g., `1.0.85`)
- **codex**: `codex --version` → `unknown` if not available
- **gemini**: `gemini --version` → `unknown` if not available
- **opencode**: `opencode --version` → `unknown` if not available
- **qwen**: `qwen --version` → `unknown` if not available
- **cursor-cli**: `cursor --version` → `unknown` if not available
- **goose**: `goose --version` → `unknown` if not available

Versions are extracted using regex pattern `(\d+\.\d+\.\d+)` from command output.

## Usage

Files are automatically created when tool validation fails in strict mode (`--strict-tools-validation`) with `FORCE_TOOLS_VALIDATION_FAILURE=1` set.

To capture requests for a specific agent:
1. Set `FORCE_TOOLS_VALIDATION_FAILURE=1` in your environment
2. Run `agent-test-run.py --agent-type {agent} --strict-tools-validation`
3. The agent will make requests that fail validation
4. Real API requests are saved to `agent-requests/{agent}/{version}/request.json`
5. Commit the files to git to track agent API evolution
