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
