# Copyright 2025 Schelling Point Labs Inc
# SPDX-License-Identifier: AGPL-3.0-only

"""
Mock LLM API Server Algorithm

OVERVIEW:
The mock server simulates OpenAI/Anthropic API endpoints for deterministic testing.
It does NOT execute tools/processes itself - it only returns properly formatted API responses
that suggest tool usage. Tool execution happens separately in the agent client.

The server is started with a specific TOOLS PROFILE (command-line option --tools-profile) that defines
the valid tool schemas for a particular coding agent (Codex, Claude, Gemini, etc.). This profile
determines how scenario events like agentEdits and agentToolUse are mapped to specific tool call
responses that are valid for that agent.

TOOLS MAPPING PRINCIPLE: Scenario events represent a superset of all possible tools across all agents.
The tools profile provides mappings from scenario tool names to agent-specific tool implementations.
For example:
- Scenario `grep` → Claude: native `grep` tool → Other agents: `run_terminal_cmd` with `grep` command
- Scenario `read_file` → Claude: native `read_file` tool → Other agents: `run_terminal_cmd` with `cat` command

TOOL CHANGES TRACKING: When tool validation fails in strict mode (--strict-tools-validation)
with FORCE_TOOLS_VALIDATION_FAILURE=1 set in the environment, the server automatically saves the complete
API request to agent-requests/{agent_name}/{version}/request.json. This creates a historical record
of how third-party coding agents' tool definitions change over time, maintained in git.

The FORCE_TOOLS_VALIDATION_FAILURE environment variable forces all tool validation to fail, ensuring that
real agent requests get captured even when their tools are normally considered valid.

In strict tools validation mode (command-line option --strict-tools-validation), the server
immediately aborts if it encounters an unfamiliar tool definition, helping developers quickly
identify missing tool profiles and mappings during development.

KEY PRINCIPLES:
1. Session Isolation: Each unique API key represents a separate client session
2. Timeline-Based Responses: Scenarios define deterministic sequences of agent behavior
3. Protocol Compliance: Responses follow exact OpenAI/Anthropic API schemas with proper coalescing
4. Provider-Specific Thinking: OpenAI keeps thinking internal (not in responses), Anthropic can expose thinking blocks
5. Client Tool Validation: Server validates tool definitions sent by clients in API requests
6. No Tool Execution: Server only suggests tool calls and edits, never executes them
7. llmResponse Grouping: Multiple response elements can be grouped into single API responses
8. Tool Evolution Tracking: Failed validations automatically save requests for historical tracking

ALGORITHM:
FOR each API request with api_key:
    IF api_key not seen before:
        Create new session with scenario timeline
        Reset timeline to beginning

    Get current session for api_key

    // Skip events that don't generate API responses, advance to next response-generating event/group
    WHILE there are more events AND current event is not response-generating:
        CASE event.type:
            "complete" -> Mark scenario as completed (handled by test harness)
            "merge" -> Mark session for merging (handled by test harness)
            "advanceMs" -> Advance logical time (handled by test harness)
            "userInputs" -> Simulate user input (handled by test harness)
            "userCommands" -> Execute user command (handled by test harness)
            "userEdits" -> Apply user file edits (handled by test harness)
        Advance to next event

    IF no more events:
        Return final assistant message

    // Collect all response elements for this turn (supports both grouped and individual events)
    response_parts = []
    IF current_event.type == "llmResponse":
        // Grouped response: collect all sub-events
        response_parts.extend(current_event.sub_events)
    ELSE IF current_event.type in ["think", "runCmd", "grep", "readFile", "listDir", "find", "sed", "agentEdits", "assistant"]:
        // Individual response: treat as single-element group (legacy support)
        response_parts.append(current_event)

    // Note: Tools validation is performed when the CLIENT makes API requests,
    // not during scenario processing. The server validates that tool definitions
    // sent by the coding agent in tool_calls match the current tools profile.

    // Coalesce response parts based on LLM API style (OpenAI vs Anthropic)
    // OpenAI: thinking -> internal (not in response), text + tool_calls -> assistant message
    // Anthropic: thinking + text + tool_calls -> content blocks in single response
    api_response = coalesce_response_parts(response_parts, llm_api_style)

    Return api_response

    Advance session timeline pointer past consumed event(s)

COALESCING RULES:
- OpenAI: Thinking content is kept internal and NOT included in API responses. Only text content and tool_calls appear in the assistant message. Thinking is processed but remains hidden from the agent client.
- Anthropic: Thinking content can be exposed as separate "thinking" blocks in the response content array, alongside "text" blocks and "tool_use" blocks, all within a single API response.

NOTE: The mock server skips over events that don't generate API responses.
"llmResponse" groups, "think", "agentToolUse", "agentEdits", and "assistant" events produce API responses.
Other events are processed for test harness coordination but don't return data to the agent client.

SESSION MANAGEMENT:
- API keys are arbitrary strings (any valid key works)
- Sessions persist across multiple API calls with same key
- Fresh API key = fresh scenario execution
- Enables concurrent testing without server restarts

RESPONSE FORMATS:
- OpenAI: {"choices": [{"message": {"role": "assistant", "content": "text_content_only", "tool_calls": [...]}}]} - thinking is processed internally but NOT exposed in the response
- Anthropic: {"content": [{"type": "thinking", "thinking": "thinking_content"}, {"type": "text", "text": "text_content"}, {"type": "tool_use", ...}]} - thinking is exposed as separate content blocks

TOOL CHANGES TRACKING:
When tool validation fails in strict mode, the complete API request is saved to:
agent-requests/{agent_name}/{version}/request.json

This creates a git-tracked historical record of third-party agent API evolution:
- Captures the exact tool definitions agents send
- Enables updating tool profiles as agents change
- Provides evidence for tool mapping decisions
- Tracks API schema evolution over time

This design enables deterministic, replayable testing of agent workflows with realistic LLM response patterns.
"""

import json
import os
import sys
import uuid
import importlib.util
from datetime import datetime, timezone
from http.server import BaseHTTPRequestHandler, HTTPServer
from urllib.parse import urlparse
from typing import Dict, Any
from pathlib import Path

try:
    import yaml
except (ImportError, ModuleNotFoundError):
    print("Warning: PyYAML not available. " +
          "Please run this script in the nix dev shell, provided by the nix flake at the root of the repository.")
    sys.exit(1)

try:
    from .session_io import RolloutRecorder, SessionLogger
except ImportError:
    # Fallback for direct execution
    import sys
    import os
    sys.path.insert(0, os.path.dirname(__file__))
    from session_io import RolloutRecorder, SessionLogger

class Playbook:
    """
    Deterministic mapping from user prompts to responses/tool-calls.
    Format:
    { "rules": [ { "if_contains": [...], "response": {
        "assistant": "...", "tool_calls": [ { "name": "...", "args": {...}} ] } } ] }
    """
    def __init__(self, path: str):
        with open(path, "r", encoding="utf-8") as f:
            self.data = json.load(f)
        self.rules = self.data.get("rules", [])

    def match(self, text: str) -> Dict[str, Any]:
        t = text.lower()
        for r in self.rules:
            conds = [c.lower() for c in r.get("if_contains", [])]
            if all(c in t for c in conds):
                return r.get("response", {})
        return {"assistant": "Acknowledged. (no matching rule)", "tool_calls": []}


class Scenario:
    """
    Timeline-based scenario for deterministic testing.
    Format: YAML with timeline of events (think, agentToolUse, agentEdits, etc.)
    """
    def __init__(self, path: str):
        if yaml is None:
            raise ImportError("yaml module is required for scenario support")
        with open(path, "r", encoding="utf-8") as f:
            self.data = yaml.safe_load(f)
        self.timeline = self.data.get("timeline", [])
        self.current_event = 0

    def get_next_event(self) -> Dict[str, Any]:
        """Get the next event from the timeline."""
        if self.current_event < len(self.timeline):
            event = self.timeline[self.current_event]
            self.current_event += 1
            return event
        return {}

    def has_more_events(self) -> bool:
        """Check if there are more events in the timeline."""
        return self.current_event < len(self.timeline)

    def reset(self):
        """Reset to the beginning of the timeline."""
        self.current_event = 0

def _json_body(handler: BaseHTTPRequestHandler):
    length = int(handler.headers.get("Content-Length", "0"))
    raw = handler.rfile.read(length) if length > 0 else b"{}"
    return json.loads(raw.decode("utf-8"))

class MockAPIHandler(BaseHTTPRequestHandler):
    server_version = "MockAgentServer/0.1"

    def _send_json(self, code: int, obj: Dict[str, Any]):
        body = json.dumps(obj).encode("utf-8")
        self.send_response(code)
        self.send_header("content-type", "application/json; charset=utf-8")
        self.send_header("content-length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def _log_request(self, body):
        """Log the complete request with headers and body."""
        if not self.server.request_log_template:
            return

        # Extract API key from headers
        api_key = "unknown"
        auth_header = self.headers.get("authorization", "")
        if auth_header.startswith("Bearer "):
            api_key = auth_header[7:]

        # Get scenario name from server
        scenario = "unknown"
        if hasattr(self.server, 'scenario') and self.server.scenario:
            scenario = Path(self.server.scenario_path).stem if self.server.scenario_path else "unknown"

        # Format log path with template
        log_path = self.server.request_log_template.format(scenario=scenario, key=api_key)

        # Create log entry
        log_entry = {
            "timestamp": datetime.now(timezone.utc).isoformat(),
            "method": self.command,
            "path": self.path,
            "headers": dict(self.headers),
            "body": body
        }

        # Write to file or stdout
        if log_path == "stdout":
            print(json.dumps(log_entry, indent=2), file=sys.stdout)
        else:
            # Ensure directory exists
            log_file_path = Path(log_path)
            log_file_path.parent.mkdir(parents=True, exist_ok=True)
            with open(log_file_path, 'a', encoding='utf-8') as f:
                json.dump(log_entry, f, indent=2, ensure_ascii=False)
                f.write('\n')

    def do_GET(self):
        parsed = urlparse(self.path)
        if parsed.path == "/health":
            self._send_json(200, {"status": "ok", "server": "MockAgentServer"})
        else:
            self.send_response(404)
            self.end_headers()

    def do_POST(self):
        parsed = urlparse(self.path)
        if parsed.path == "/v1/chat/completions":
            self._handle_openai_chat_completions()
        elif parsed.path == "/v1/messages":
            self._handle_anthropic_messages()
        else:
            # Debug: log unknown endpoints
            print(f"DEBUG: Unknown POST endpoint: {parsed.path}", file=sys.stderr)
            self._send_json(404, {"error":"not found"})

    def _infer_text_from_messages(self, messages) -> str:
        for m in reversed(messages):
            if m.get("role") == "user":
                content = m.get("content")
                if isinstance(content, list):
                    # For Claude Code, collect all text content blocks, excluding system reminders
                    text_parts = []
                    for b in content:
                        if b.get("type") == "text":
                            text = b.get("text", "")
                            # Skip system reminders
                            if not text.startswith("<system-reminder>"):
                                text_parts.append(text)
                    # Return the concatenated text (excluding system reminders)
                    return " ".join(text_parts)
                return str(content)
        return ""

    def _respond_with(self, user_text: str, provider: str, api_key: str = "default"):
        executed_tools = []  # Track executed tools for response

        # Scenario-based response following the timeline algorithm (required)
        session = self.server._get_session(api_key)
        # Follow the algorithm: skip non-response events, collect response parts
        response_parts = self._collect_response_parts(session)
        assistant_text, tool_calls = self._process_response_parts(response_parts, provider)

        return assistant_text, tool_calls, executed_tools

    def _collect_response_parts(self, session: Scenario) -> list:
        """Collect response parts from the current scenario event following the algorithm."""
        response_parts = []

        # Skip events that don't generate API responses, advance to next response-generating event/group
        while session.has_more_events():
            current_event = session.get_next_event()

            # Check if this is a response-generating event
            event_type = current_event.get("type") if isinstance(current_event, dict) and "type" in current_event else next(iter(current_event.keys())) if current_event else None

            if event_type in ["complete", "merge", "advanceMs", "userInputs", "userCommands", "userEdits"]:
                # Skip non-response-generating events (handled by test harness)
                continue
            elif event_type in ["think", "runCmd", "grep", "readFile", "listDir", "find", "sed", "editFile", "writeFile", "task", "webFetch", "webSearch", "todoWrite", "notebookEdit", "exitPlanMode", "bashOutput", "killShell", "slashCommand", "agentEdits", "agentToolUse", "assistant"]:
                # Individual response event
                response_parts.append(current_event)
                break
            elif event_type == "llmResponse":
                # Grouped response: collect all sub-events
                if isinstance(current_event, dict) and "llmResponse" in current_event:
                    response_parts.extend(current_event["llmResponse"])
                break

        return response_parts

    def _process_response_parts(self, response_parts: list, provider: str) -> tuple:
        """Process collected response parts into assistant text and tool calls."""
        assistant_text = ""
        tool_calls = []

        for part in response_parts:
            # Handle both typed events (legacy) and direct event objects
            if isinstance(part, dict) and "type" in part:
                # Legacy typed format
                event_type = part["type"]
                event_data = part
            else:
                # New format where the key is the event type
                event_type = next(iter(part.keys()))
                event_data = part[event_type]

            if event_type == "think":
                # Thinking content - handled differently by provider
                if provider == "anthropic":
                    # For Anthropic, thinking can be exposed as a content block
                    # But we'll keep it internal for now as per OpenAI behavior
                    pass
                # For OpenAI, thinking is kept internal (not in response)
            elif event_type == "assistant":
                # Assistant message
                if isinstance(event_data, dict) and "text" in event_data:
                    assistant_text += event_data["text"]
                elif isinstance(event_data, list):
                    # Array of [milliseconds, text] pairs
                    for _, text in event_data:
                        assistant_text += text
                else:
                    assistant_text += str(event_data)
            elif event_type in ["runCmd", "grep", "readFile", "listDir", "find", "sed", "editFile", "writeFile", "task", "webFetch", "webSearch", "todoWrite", "notebookEdit", "exitPlanMode", "bashOutput", "killShell", "slashCommand"]:
                # Tool use events - map to agent-specific tool calls
                tool_call = self.server._map_tool_call(event_type, event_data if isinstance(event_data, dict) else {})
                if tool_call:
                    tool_calls.append({
                        "id": f"call_{len(tool_calls)}",
                        "name": tool_call["name"],
                        "args": tool_call["args"]
                    })
            elif event_type == "agentEdits":
                # File editing - map to appropriate editing tool
                # For now, map to a generic edit tool (will be refined through empirical testing)
                tool_calls.append({
                    "id": f"call_{len(tool_calls)}",
                    "name": "edit_file",
                    "args": event_data if isinstance(event_data, dict) else {}
                })
            elif event_type == "agentToolUse":
                # Generic tool use - extract toolName and args from event data
                if isinstance(event_data, dict) and "toolName" in event_data:
                    tool_name = event_data["toolName"]
                    tool_args = event_data.get("args", {})
                    tool_call = self.server._map_tool_call(tool_name, tool_args)
                    if tool_call:
                        tool_calls.append({
                            "id": f"call_{len(tool_calls)}",
                            "name": tool_call["name"],
                            "args": tool_call["args"]
                        })

        return assistant_text, tool_calls

        # Execute tools immediately for mock server
        executed_tools = []
        for tc in tool_calls:
            tool_name = tc["name"]
            tool_args = tc.get("args", {})
            try:
                # Import tools directly to avoid relative import issues
                import sys
                import os
                tools_path = os.path.join(os.path.dirname(__file__), 'tools.py')
                spec = importlib.util.spec_from_file_location("tools", tools_path)
                tools_module = importlib.util.module_from_spec(spec)
                spec.loader.exec_module(tools_module)

                if hasattr(tools_module, tool_name):
                    tool_func = getattr(tools_module, tool_name)
                    # Get workspace from server or from a file set by the test
                    workspace = self.server.workspace
                    if not workspace:
                        # Try to read workspace from a file
                        workspace_file = os.path.join(os.path.dirname(__file__), "..", "MOCK_AGENT_WORKSPACE.txt")
                        try:
                            with open(workspace_file, "r") as f:
                                workspace = f.read().strip()
                        except FileNotFoundError:
                            workspace = "/tmp"

                    # Add workspace to tool args
                    tool_args_with_workspace = {"workspace": workspace, **tool_args}
                    result = tool_func(**tool_args_with_workspace)
                    executed_tools.append({
                        "name": tool_name,
                        "args": tool_args,
                        "result": result
                    })
                else:
                    print(f"DEBUG: Tool {tool_name} not found in tools module", file=sys.stderr)
            except Exception as e:
                print(f"DEBUG: Tool execution failed: {e}", file=sys.stderr)

        recorder: RolloutRecorder = self.server.recorder  # type: ignore
        recorder.record_message("user", user_text)
        if assistant_text:
            recorder.record_reasoning(summary_text=f"[{provider}] planning response for: {user_text}")
            recorder.record_message("assistant", assistant_text)
        for tc in tool_calls:
            recorder.record_function_call(name=tc["name"], arguments=json.dumps(tc.get("args", {})))
        return assistant_text, tool_calls, executed_tools

    def _handle_force_validation_failure(self, body):
        force_validation_failure = os.environ.get("FORCE_TOOLS_VALIDATION_FAILURE", "").lower() in ("1", "true", "yes")
        if body and force_validation_failure:
            self.server._save_agent_request(body, f"{self.server.tools_profile}_request", f"Capturing real {self.server.tools_profile} request")

    def _handle_openai_chat_completions(self):
        body = _json_body(self)
        self._log_request(body)
        self._handle_force_validation_failure(body)

        messages = body.get("messages", [])

        # Extract API key for session management
        api_key = self.headers.get("authorization", "").replace("Bearer ", "") or self.headers.get("api-key", "default")

        # Validate tool definitions in requests (for Claude and other agents that define tools upfront)
        if body.get("tools") and self.server.tools_profile in ["claude"]:
            self.server._validate_tool_definitions(body["tools"], body)

        # For Claude, save requests that define tools to capture tool definitions
        if self.server.tools_profile == "claude" and body.get("tools"):
            self.server._save_agent_request(body, "claude_tools_request", "Capturing Claude request with tools")
            # Continue processing the request

        # Check for tool calls/results in the request messages
        request_tool_calls = []
        for msg in messages:
            if msg.get("role") == "assistant" and "tool_calls" in msg:
                request_tool_calls.extend(msg["tool_calls"])
            elif msg.get("role") == "tool":
                # Tool result message - extract the tool_call_id to validate
                tool_call_id = msg.get("tool_call_id")
                if tool_call_id:
                    # Create a synthetic tool call for validation
                    request_tool_calls.append({"id": tool_call_id, "function": {"name": "unknown_tool"}})

        # Validate any tool calls found in the request
        if request_tool_calls:
            self.server._validate_tools(request_tool_calls, body)

        user_text = self._infer_text_from_messages(messages)
        assistant_text, tool_calls, executed_tools = self._respond_with(user_text, provider="openai", api_key=api_key)

        tc = []
        for _idx, t in enumerate(tool_calls):
            tc.append({
                "id": f"call_{uuid.uuid4().hex[:8]}",
                "type": "function",
                "function": {
                    "name": t["name"],
                    "arguments": json.dumps(t.get("args", {}))
                }
            })
        obj = {
            "id": f"chatcmpl-{uuid.uuid4().hex}",
            "object": "chat.completion",
            "created": int(uuid.uuid1().time/1e7),
            "model": body.get("model", "mock-model"),
            "choices": [{
                "index": 0,
                "message": {
                    "role": "assistant",
                    "content": assistant_text,
                    "tool_calls": tc if tc else None
                },
                "finish_reason": "stop"
            }],
            "usage": {"prompt_tokens": 0, "completion_tokens": 0, "total_tokens": 0}
        }
        if obj["choices"][0]["message"]["tool_calls"] is None:
            del obj["choices"][0]["message"]["tool_calls"]
        self._send_json(200, obj)

    def _handle_anthropic_messages(self):
        body = _json_body(self)
        self._log_request(body)
        self._handle_force_validation_failure(body)

        messages = body.get("messages", [])

        # Extract API key for session management (Anthropic uses x-api-key header)
        api_key = self.headers.get("x-api-key", "default")

        # Check for tool calls/results in the request messages (Anthropic format)
        request_tool_calls = []
        for msg in messages:
            if msg.get("role") == "assistant" and isinstance(msg.get("content"), list):
                # Check content blocks for tool_use
                for block in msg["content"]:
                    if block.get("type") == "tool_use":
                        request_tool_calls.append({
                            "name": block.get("name"),
                            "id": block.get("id")
                        })
            elif msg.get("role") == "user" and isinstance(msg.get("content"), list):
                # Check for tool_result blocks
                for block in msg["content"]:
                    if block.get("type") == "tool_result":
                        tool_call_id = block.get("tool_use_id")
                        if tool_call_id:
                            request_tool_calls.append({"id": tool_call_id, "name": "unknown_tool"})

        # Validate any tool calls found in the request
        if request_tool_calls:
            self.server._validate_tools(request_tool_calls, body)

        user_text = self._infer_text_from_messages(messages)
        assistant_text, tool_calls, executed_tools = self._respond_with(user_text, provider="anthropic", api_key=api_key)

        content = []
        if assistant_text:
            content.append({"type": "text", "text": assistant_text})
        for t in tool_calls:
            content.append({
                "type": "tool_use",
                "id": f"toolu_{uuid.uuid4().hex[:8]}",
                "name": t["name"],
                "input": t.get("args", {})
            })
        obj = {
            "id": f"msg_{uuid.uuid4().hex}",
            "type": "message",
            "role": "assistant",
            "model": body.get("model", "mock-model"),
            "content": content,
            "stop_reason": "end_turn",
            "usage": {"input_tokens": 0, "output_tokens": 0}
        }
        self._send_json(200, obj)

class MockAPIServer(HTTPServer):
    def __init__(self, server_address, RequestHandlerClass, codex_home, scenario_path, workspace=None, tools_profile=None, strict_tools_validation=False, agent_version=None, request_log_template=None):
        super().__init__(server_address, RequestHandlerClass)
        self.codex_home = codex_home
        self.scenario = None
        self.scenario_path = scenario_path  # Store scenario path for session creation
        self.tools_profile = tools_profile or "codex"  # Default to codex if not specified
        self.strict_tools_validation = strict_tools_validation
        self.agent_version = agent_version or "unknown"
        self.request_log_template = request_log_template

        # Load scenario (required)
        try:
            self.scenario = Scenario(scenario_path)
        except ImportError:
            raise ImportError("Scenario support requires PyYAML. Install with: pip install pyyaml")

        self.recorder = RolloutRecorder(codex_home=codex_home, originator="mock-api-server")
        self.workspace = workspace

        # Session management for scenario-based responses
        self.sessions = {}  # api_key -> Scenario instance

        # Load tools profile
        self._load_tools_profile()

    def _get_session(self, api_key: str) -> Scenario:
        """Get or create a scenario session for the given API key."""
        if api_key not in self.sessions:
            # Create a fresh scenario instance for this session
            try:
                self.sessions[api_key] = Scenario(self.scenario_path)
            except ImportError:
                raise ImportError("Scenario support requires PyYAML. Install with: pip install pyyaml")
        return self.sessions[api_key]

    def _load_tools_profile(self):
        """Load the tools profile for the specified agent type."""
        # Tool profiles empirically determined through testing
        self.valid_tools = {
            "codex": {
                # Codex tool names (empirical verification needed)
                "write_file",
                "read_file",
                "run_command",
                "append_file",
                "replace_in_file",
            },
            "claude": {
                # Updated to match actual Claude 2.0.5 tool definitions
                "Bash",              # Terminal command execution
                "Grep",              # Advanced search tool
                "Read",              # File reading
                "Glob",              # File pattern matching
                "Edit",              # File editing with string replacements
                "Write",             # File writing
                "Task",              # Launch specialized agents
                "WebFetch",          # URL content fetching
                "WebSearch",         # Web search functionality
                "TodoWrite",         # Task management
                "NotebookEdit",      # Jupyter notebook editing
                "ExitPlanMode",      # Exit plan mode
                "BashOutput",        # Background bash output retrieval
                "KillShell",         # Kill background shells
                "SlashCommand",      # Slash command execution
            },
            "gemini": set(),  # TODO: Verify with actual Gemini CLI
            "opencode": set(),  # TODO: Verify with actual OpenCode
            "qwen": set(),  # TODO: Verify with actual Qwen
            "cursor-cli": set(),  # TODO: Verify with actual Cursor CLI
            "goose": set(),  # TODO: Verify with actual Goose
        }

        # Tools mapping: scenario tool event names -> agent-specific tool implementations
        # This allows scenarios to use a superset of tools that get mapped to agent capabilities
        # Mappings are determined through empirical testing with strict-tools-validation
        self.tools_mapping = {
                    "codex": {
                        # Codex mappings (needs empirical verification with actual Codex CLI)
                        # Canonical tool names from Scenario Format
                        "writeFile": {"name": "write_file", "direct": True, "args_map": {"path": "path", "content": "text"}},
                        "readFile": {"name": "read_file", "direct": True, "args_map": {"path": "path"}},
                        "runCmd": {"name": "run_command", "direct": True, "args_map": {"cmd": "command", "cwd": "cwd"}},
                        "appendFile": {"name": "append_file", "direct": True, "args_map": {"path": "path", "content": "text"}},
                        "replaceInFile": {"name": "replace_in_file", "direct": True, "args_map": {"path": "path", "old_string": "old", "new_string": "new"}},

                        # Backward compatibility for old playbook tool names (snake_case)
                        "write_file": {"name": "write_file", "direct": True, "args_map": {"path": "path", "text": "text"}},
                        "read_file": {"name": "read_file", "direct": True, "args_map": {"path": "path"}},
                        "run_command": {"name": "run_command", "direct": True, "args_map": {"command": "command", "cwd": "cwd"}},
                        "append_file": {"name": "append_file", "direct": True, "args_map": {"path": "path", "text": "text"}},
                        "replace_in_file": {"name": "replace_in_file", "direct": True, "args_map": {"path": "path", "old": "old", "new": "new"}},
                    },
                    "claude": {
                        # Claude empirically verified tools - updated to match Claude 2.0.5 actual tool definitions
                        # Canonical tool names from Scenario Format
                        "runCmd": {"name": "Bash", "direct": True, "args_map": {"cmd": "command", "cwd": "cwd", "timeout": "timeout", "description": "description", "run_in_background": "run_in_background"}},
                        "grep": {"name": "Grep", "direct": True, "args_map": {"pattern": "pattern", "path": "path", "glob": "glob", "output_mode": "output_mode", "-B": "-B", "-A": "-A", "-C": "-C", "-n": "-n", "-i": "-i", "type": "type", "head_limit": "head_limit", "multiline": "multiline"}},
                        "readFile": {"name": "Read", "direct": True, "args_map": {"path": "file_path", "offset": "offset", "limit": "limit"}},
                        "writeFile": {"name": "Write", "direct": True, "args_map": {"path": "file_path", "content": "content"}},
                        "editFile": {"name": "Edit", "direct": True, "args_map": {"path": "file_path", "old_string": "old_string", "new_string": "new_string", "replace_all": "replace_all"}},
                        "appendFile": {"name": "Edit", "direct": True, "args_map": {"path": "file_path"}},  # Note: Claude doesn't have native append
                        "replaceInFile": {"name": "Edit", "direct": True, "args_map": {"path": "file_path", "old_string": "old_string", "new_string": "new_string"}},
                        "listDir": {"name": "Glob", "direct": True, "args_map": {"pattern": "pattern", "path": "path"}},
                        "task": {"name": "Task", "direct": True, "args_map": {"description": "description", "prompt": "prompt", "subagent_type": "subagent_type"}},
                        "webFetch": {"name": "WebFetch", "direct": True, "args_map": {"url": "url", "prompt": "prompt"}},
                        "webSearch": {"name": "WebSearch", "direct": True, "args_map": {"query": "query", "allowed_domains": "allowed_domains", "blocked_domains": "blocked_domains"}},
                        "todoWrite": {"name": "TodoWrite", "direct": True, "args_map": {"todos": "todos"}},
                        "notebookEdit": {"name": "NotebookEdit", "direct": True, "args_map": {"notebook_path": "notebook_path", "cell_id": "cell_id", "new_source": "new_source", "cell_type": "cell_type", "edit_mode": "edit_mode"}},
                        "exitPlanMode": {"name": "ExitPlanMode", "direct": True, "args_map": {"plan": "plan"}},
                        "bashOutput": {"name": "BashOutput", "direct": True, "args_map": {"bash_id": "bash_id", "filter": "filter"}},
                        "killShell": {"name": "KillShell", "direct": True, "args_map": {"shell_id": "shell_id"}},
                        "slashCommand": {"name": "SlashCommand", "direct": True, "args_map": {"command": "command"}},

                        # Direct mappings for agent tool names (for scenarios that use them directly)
                        "Write": {"name": "Write", "direct": True, "args_map": {"path": "file_path", "text": "content"}},
                        "Edit": {"name": "Edit", "direct": True, "args_map": {"path": "file_path", "old_string": "old_string", "new_string": "new_string"}},

                        # Backward compatibility mappings for old playbook tool names (snake_case)
                        "write_file": {"name": "Write", "direct": True, "args_map": {"path": "file_path", "text": "content"}},
                        "read_file": {"name": "Read", "direct": True, "args_map": {"path": "file_path"}},
                        "append_file": {"name": "Edit", "direct": True, "args_map": {"path": "file_path"}},
                        "replace_in_file": {"name": "Edit", "direct": True, "args_map": {"path": "file_path", "old_string": "old", "new_string": "new"}},
                        "run_command": {"name": "Bash", "direct": True, "args_map": {"command": "command", "cwd": "cwd"}},
                    },
                    "gemini": {
                        # Gemini mappings (needs empirical verification)
                    },
                    "opencode": {
                        # OpenCode mappings (needs empirical verification)
                    },
                    "qwen": {
                        # Qwen mappings (needs empirical verification)
                    },
                    "cursor-cli": {
                        # Cursor CLI mappings (needs empirical verification)
                    },
                    "goose": {
                        # Goose mappings (needs empirical verification)
                    },
                }

    def _validate_tool_definitions(self, tool_definitions, request_body=None):
        """Validate that tool definitions match the current tools profile."""
        if not tool_definitions:
            return  # No tools to validate

        profile_tools = self.valid_tools.get(self.tools_profile, set())

        for tool_def in tool_definitions:
            tool_name = tool_def.get("name")
            if not tool_name:
                continue

            if tool_name not in profile_tools:
                error_msg = f"Tool '{tool_name}' is not in the valid tools profile for {self.tools_profile}"
                if hasattr(self, 'strict_tools_validation') and self.strict_tools_validation:
                    raise ValueError(f"Strict tools validation failed: {error_msg}")
                else:
                    # In non-strict mode, just log the issue but continue
                    print(f"WARNING: {error_msg}")
                    if hasattr(self, '_save_agent_request'):
                        self._save_agent_request(request_body, f"unknown_tool_{tool_name}",
                                               f"Unknown tool definition: {tool_name}")

    def _validate_tools(self, tool_calls, request_body=None):
        """Validate that tool calls match the current tools profile."""
        if not tool_calls:
            return  # No tools to validate

        profile_tools = self.valid_tools.get(self.tools_profile, set())

        for tool_call in tool_calls:
            tool_name = tool_call.get("name") or tool_call.get("function", {}).get("name")
            if not tool_name:
                continue

            # Force validation failure if FORCE_TOOLS_VALIDATION_FAILURE is set
            if tool_name not in profile_tools:
                error_msg = f"Unknown tool '{tool_name}' for profile '{self.tools_profile}'. Valid tools: {sorted(profile_tools)}"
                print(f"TOOLS VALIDATION ERROR: {error_msg}", file=sys.stderr)
                print(f"TOOL CALL DUMP: {json.dumps(tool_call, indent=2)}", file=sys.stderr)

                # Save the request for tracking tool definition changes
                if request_body:
                    self._save_agent_request(request_body, tool_name, error_msg)

                if self.strict_tools_validation:
                    raise ValueError(error_msg)
                else:
                    print(f"WARNING: {error_msg} (continuing due to non-strict mode)", file=sys.stderr)

    def _save_agent_request(self, request_body, unknown_tool, error_msg):
        """Save the raw agent request JSON to track tool definition changes over time."""
        # Create directory structure: agent-requests/{agent_name}/{version_range}/
        agent_requests_dir = Path(__file__).parent.parent / "agent-requests"
        agent_dir = agent_requests_dir / self.tools_profile
        version_dir = agent_dir / self.agent_version

        # Create directories if they don't exist
        version_dir.mkdir(parents=True, exist_ok=True)

        # Use simple filename: request.json
        request_file = version_dir / "request.json"

        # Save just the raw request JSON as sent by the agent
        with open(request_file, 'w', encoding='utf-8') as f:
            json.dump(request_body, f, indent=2, ensure_ascii=False)

        print(f"SAVED AGENT REQUEST: {request_file}", file=sys.stderr)

    def _map_tool_call(self, scenario_event_type, scenario_args):
        """Map a scenario tool event type and args to agent-specific tool implementation."""
        agent_mapping = self.tools_mapping.get(self.tools_profile, {})

        if scenario_event_type not in agent_mapping:
            # If no mapping exists, try to find a reasonable default
            # For unknown tools, assume they map to terminal commands
            return {
                "name": "run_terminal_cmd",
                "args": {
                    "command": f"{scenario_event_type} {' '.join(str(v) for v in scenario_args.values())}",
                    "cwd": "."
                }
            }

        mapping = agent_mapping[scenario_event_type]

        if mapping.get("direct", False):
            # Direct mapping - use the mapped tool name and remap arguments
            args_map = mapping.get("args_map", {})
            mapped_args = {}
            for scenario_key, agent_key in args_map.items():
                if scenario_key in scenario_args:
                    mapped_args[agent_key] = scenario_args[scenario_key]

            # Include any unmapped arguments
            for key, value in scenario_args.items():
                if key not in args_map:
                    mapped_args[key] = value

            return {
                "name": mapping["name"],
                "args": mapped_args
            }
        else:
            # Template-based mapping (typically run_terminal_cmd with command templates)
            mapped_name = mapping["name"]
            template_args = mapping.get("args", {})

            # Substitute scenario args into the template
            mapped_args = {}
            for key, value in template_args.items():
                if isinstance(value, str):
                    # String template substitution
                    try:
                        mapped_args[key] = value.format(**scenario_args)
                    except KeyError:
                        # If template substitution fails, keep the original template
                        mapped_args[key] = value
                else:
                    mapped_args[key] = value

            # Merge any additional scenario args that weren't templated
            for key, value in scenario_args.items():
                if key not in mapped_args:
                    mapped_args[key] = value

            return {
                "name": mapped_name,
                "args": mapped_args
            }

def serve(host: str, port: int, scenario: str, codex_home: str = None, format: str = "codex", workspace: str = None, tools_profile: str = None, strict_tools_validation: bool = False, agent_version: str = None, request_log_template: str = None):
    httpd = MockAPIServer((host, port), MockAPIHandler, codex_home=codex_home, scenario_path=scenario, workspace=workspace, tools_profile=tools_profile, strict_tools_validation=strict_tools_validation, agent_version=agent_version, request_log_template=request_log_template)
    print(f"Mock API server listening on http://{host}:{port}")
    print(f"Tools profile: {httpd.tools_profile}")
    print(f"Strict tools validation: {httpd.strict_tools_validation}")
    try:
        httpd.serve_forever()
    except KeyboardInterrupt:
        print("Shutting down server...")
    finally:
        httpd.server_close()
        httpd.recorder.close()
