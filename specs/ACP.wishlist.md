# ACP Protocol Enhancement Wishlist

This document captures desired enhancements to the Agent Client Protocol (ACP) that would improve the developer experience, add new capabilities, or simplify common use cases. These are not part of the current ACP specification but represent features that Harbor and potentially other ACP implementations would benefit from.

## 1. Session History Population

### Problem

Currently, clients cannot create a new session pre-populated with conversation history. The `session/new` method only accepts working directory and MCP server configuration, requiring clients to send follow-up `session/prompt` requests to build conversation state.

### Desired Solution

Extend `session/new` to accept an optional `conversationHistory` parameter containing an array of pre-existing conversation turns (user messages, agent responses, tool calls, etc.).

### Benefits

- **Faster Session Restoration**: Skip individual `session/prompt` calls for historical context
- **Reduced Latency**: Populate session state in a single round-trip
- **Better UX**: Seamless transition between different client implementations
- **Testing Efficiency**: Easier to set up complex conversation scenarios

### Example API

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "session/new",
  "params": {
    "cwd": "/home/user/project",
    "mcpServers": [...],
    "conversationHistory": [
      {
        "type": "user_message",
        "content": {"type": "text", "text": "Hello, can you help me?"}
      },
      {
        "type": "agent_response",
        "content": {"type": "text", "text": "Of course! What would you like help with?"}
      }
    ]
  }
}
```

### Implementation Notes

- Agent validates history format and content
- Agent can choose to accept/reject history based on capabilities
- Should be optional feature with capability advertisement

## 2. Agent-Initiated Tool Execution

### Problem

Currently, all tool execution must be initiated by the client. When an agent wants to run a tool, it sends a `tool_call` notification and waits for the client to execute it and report results via `tool_call_update`.

### Desired Solution

Allow agents to optionally execute certain tools directly, only notifying the client of outputs and results rather than requiring client permission/execution.

### Benefits

- **Reduced Latency**: No client round-trip for tool execution
- **Better Security Model**: Agent can execute trusted/safe operations directly
- **Improved Performance**: Eliminates network overhead for simple operations
- **Enhanced Agent Autonomy**: Agents can perform routine tasks without client intervention

### Example Flow

```json
// Agent decides to execute a tool directly
{
  "jsonrpc": "2.0",
  "method": "session/update",
  "params": {
    "sessionId": "sess_123",
    "update": {
      "sessionUpdate": "agent_tool_execution",
      "toolCallId": "call_456",
      "toolName": "read_file",
      "args": {"path": "/etc/hostname"},
      "executionMode": "agent_direct"
    }
  }
}

// Agent reports completion
{
  "jsonrpc": "2.0",
  "method": "session/update",
  "params": {
    "sessionId": "sess_123",
    "update": {
      "sessionUpdate": "tool_call_update",
      "toolCallId": "call_456",
      "fields": {
        "status": "completed",
        "result": "my-server\n"
      }
    }
  }
}
```

### Implementation Notes

- Should be opt-in per tool type or agent capability
- Client still receives full visibility into tool execution
- Agent assumes responsibility for safe execution
- Could include execution policies (e.g., "safe_tools_only")

## 3. Proactive File Edit Notifications

### Problem

Currently, file edits are reactive - agents request permission to make changes, and clients either approve/execute or deny. However, some tools or agents may already perform file operations (e.g., version control tools, build systems, or agents with direct filesystem access).

### Desired Solution

Allow agents to notify clients of file changes that have already occurred, enabling clients to update their views and potentially rollback if needed.

### Benefits

- **Real-time Synchronization**: Clients stay current with actual filesystem state
- **Better Tool Integration**: Support tools that modify files as part of their operation
- **Improved Reliability**: Agents can report what actually happened vs. what was requested
- **Enhanced Debugging**: Clear visibility into all filesystem changes

### Example API

```json
// Agent notifies of completed file edit
{
  "jsonrpc": "2.0",
  "method": "session/update",
  "params": {
    "sessionId": "sess_123",
    "update": {
      "sessionUpdate": "file_edit_completed",
      "path": "/home/user/project/main.py",
      "changeType": "modified",
      "oldContent": "def hello():\n    print('Hello')",
      "newContent": "def hello():\n    print('Hello, World!')",
      "changeSource": "external_tool",
      "timestamp": "2025-01-15T10:30:00Z"
    }
  }
}

// Client can optionally rollback
{
  "jsonrpc": "2.0",
  "id": 2,
  "method": "session/rollback_edit",
  "params": {
    "sessionId": "sess_123",
    "editId": "edit_789",
    "reason": "User requested rollback"
  }
}
```

### Implementation Notes

- Should include metadata about the change source (tool, agent, external process)
- Clients can choose to accept, ignore, or rollback changes
- Could integrate with filesystem watching capabilities
- Should support partial rollbacks and conflict resolution

## Implementation Considerations

### Capability Negotiation

All wishlist items should use ACP's existing capability negotiation mechanism:

```json
{
  "agentCapabilities": {
    "_meta": {
      "agent.harbor": {
        "sessionHistoryPopulation": true,
        "agentInitiatedTools": ["read_file", "list_dir"],
        "proactiveFileEdits": true
      }
    }
  }
}
```

### Backward Compatibility

All enhancements should be optional and backward-compatible:

- Existing clients work unchanged
- New features only activate when both sides support them
- Graceful degradation when features aren't available

### Security Implications

- Agent-initiated tool execution requires careful security review
- File edit notifications should not allow unauthorized modifications
- All features should maintain the principle of client control

## Future Wishlist Items

This document can be extended with additional enhancement ideas as they arise during ACP implementation and usage.
