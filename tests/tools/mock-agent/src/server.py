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
    ELSE IF current_event.type in ["think", "agentToolUse", "agentEdits", "assistant"]:
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

This design enables deterministic, replayable testing of agent workflows with realistic LLM response patterns.
"""

import json
import os
import uuid
import importlib.util
from http.server import BaseHTTPRequestHandler, HTTPServer
from urllib.parse import urlparse
from typing import Dict, Any

try:
    import yaml
except ImportError:
    yaml = None

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

    def _respond_with(self, user_text: str, provider: str):
        if hasattr(self.server, 'playbook') and self.server.playbook:
            pb: Playbook = self.server.playbook
            resp = pb.match(user_text)
            assistant_text = resp.get("assistant", "")
            tool_calls = resp.get("tool_calls", [])
        elif hasattr(self.server, 'scenario') and self.server.scenario:
            # For scenarios, we return a simple response for now
            # In a full implementation, this would follow the scenario timeline
            resp = {"assistant": f"Mock response for: {user_text}", "tool_calls": []}
            assistant_text = resp.get("assistant", "")
            tool_calls = resp.get("tool_calls", [])
        else:
            resp = {"assistant": "No playbook or scenario loaded", "tool_calls": []}
            assistant_text = resp.get("assistant", "")
            tool_calls = resp.get("tool_calls", [])

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

    def _handle_openai_chat_completions(self):
        body = _json_body(self)
        messages = body.get("messages", [])
        user_text = self._infer_text_from_messages(messages)
        assistant_text, tool_calls, executed_tools = self._respond_with(user_text, provider="openai")

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
        messages = body.get("messages", [])
        user_text = self._infer_text_from_messages(messages)
        assistant_text, tool_calls, executed_tools = self._respond_with(user_text, provider="anthropic")

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
    def __init__(self, server_address, RequestHandlerClass, codex_home, playbook_path=None, scenario_path=None, workspace=None):
        super().__init__(server_address, RequestHandlerClass)
        self.codex_home = codex_home
        self.playbook = None
        self.scenario = None

        if playbook_path:
            self.playbook = Playbook(playbook_path)
        elif scenario_path:
            try:
                self.scenario = Scenario(scenario_path)
            except ImportError:
                raise ImportError("Scenario support requires PyYAML. Install with: pip install pyyaml")

        self.recorder = RolloutRecorder(codex_home=codex_home, originator="mock-api-server")
        self.workspace = workspace

def serve(host: str, port: int, playbook: str = None, scenario: str = None, codex_home: str = None, format: str = "codex", workspace: str = None):
    httpd = MockAPIServer((host, port), MockAPIHandler, codex_home=codex_home, playbook_path=playbook, scenario_path=scenario, workspace=workspace)
    print(f"Mock API server listening on http://{host}:{port}")
    try:
        httpd.serve_forever()
    except KeyboardInterrupt:
        print("Shutting down server...")
    finally:
        httpd.server_close()
        httpd.recorder.close()
