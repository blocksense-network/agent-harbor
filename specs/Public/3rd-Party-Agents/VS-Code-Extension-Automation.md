# VS Code Extension Automation — Integration Possibilities

## Overview

VS Code extensions like Cline, Kilo Code, and Roo Code CAN be automated for CLI integration through several mechanisms:

1. **VS Code Command API** - Programmatic command execution
2. **Extension Host Debugging** - Chrome DevTools Protocol access
3. **External Script Automation** - Node.js script control
4. **Local API Server** - HTTP/WebSocket server within extension
5. **File Watchers** - Filesystem-based command triggering

## VS Code Automation Mechanisms

### 1. Command-Line Interface (CLI)

VS Code provides extensive CLI options for automation:

```bash
# Launch with extensions
code /path/to/project

# Wait for window to close (useful for automation)
code --wait /path/to/file

# Go to specific line/column
code --goto package.json:10:5

# Install/manage extensions
code --install-extension <ext-id>
code --list-extensions
code --disable-extensions
code --disable-extension <ext-id>

# Extension debugging
code --inspect-extensions=<port>
code --inspect-brk-extensions=<port>
```

**Key automation flags:**

- `--wait`: Blocks until window closes (useful for CI/CD)
- `--goto`: Open file at specific location
- `--inspect-extensions`: Enable debugging protocol for extensions (connects to Chrome DevTools Protocol)
- `--enable-proposed-api`: Enable proposed APIs for extensions

### 2. Programmatic Command Execution

Extensions can expose commands that are callable programmatically:

**Via Extension API (within VS Code):**

```javascript
// Execute any registered command
await vscode.commands.executeCommand('commandId', ...args);

// Example: Execute Cline command
await vscode.commands.executeCommand('cline.startTask', 'Implement feature X');
```

**Via External Automation:**

```javascript
// Node.js script automating VS Code
const { exec } = require('child_process');

// Execute command via VS Code CLI (if extension provides CLI integration)
exec('code --command cline.startTask "Implement feature X"', (err, stdout) => {
  console.log(stdout);
});
```

### 3. Extension Host Debugging Protocol

VS Code uses Chrome DevTools Protocol for extension debugging:

```bash
# Start VS Code with extension debugging enabled
code --inspect-extensions=9229

# Connect via Chrome DevTools Protocol
# Access at: chrome://inspect or via Puppeteer/Playwright
```

**Automation via CDP:**

- Use Puppeteer or Playwright to connect to debugging port
- Execute commands through CDP
- Monitor extension state
- Capture events and responses

**Example with Puppeteer:**

```javascript
const puppeteer = require('puppeteer');

// Connect to VS Code's extension host
const browser = await puppeteer.connect({
  browserURL: 'http://localhost:9229',
});

// Execute extension commands
// Note: Requires understanding CDP protocol
```

### 4. External Script Automation Framework

Create a custom extension or wrapper that accepts external commands:

**Architecture:**

1. VS Code extension runs HTTP/WebSocket server
2. External scripts send commands as JSON
3. Extension executes commands via `vscode.commands.executeCommand`
4. Results returned to external script

**Example Extension Code:**

```javascript
// In extension.ts
const express = require('express');
const app = express();

app.post('/execute', async (req, res) => {
  const { command, args } = req.body;
  try {
    const result = await vscode.commands.executeCommand(command, ...args);
    res.json({ success: true, result });
  } catch (error) {
    res.json({ success: false, error: error.message });
  }
});

app.listen(3000);
```

**External Script:**

```bash
curl -X POST http://localhost:3000/execute \
  -H "Content-Type: application/json" \
  -d '{"command": "cline.startTask", "args": ["Implement feature X"]}'
```

### 5. File Watcher Automation

Use filesystem as communication channel:

**Extension watches for command files:**

```javascript
// Extension code
const watcher = vscode.workspace.createFileSystemWatcher('**/.vscode/commands/*.json');

watcher.onDidCreate(async uri => {
  const content = await vscode.workspace.fs.readFile(uri);
  const { command, args } = JSON.parse(content.toString());
  await vscode.commands.executeCommand(command, ...args);
  // Delete command file after execution
  await vscode.workspace.fs.delete(uri);
});
```

**External script writes command:**

```bash
# Trigger Cline task
echo '{"command": "cline.startTask", "args": ["Fix bug in auth.ts"]}' > .vscode/commands/task-$(date +%s).json
```

## Cline-Specific Automation

Based on GitHub discussion #2622, Cline community is actively developing remote control:

### Proposed Solutions

1. **Local API Server** (most feasible)
   - HTTP/WebSocket server in extension
   - JSON API for commands
   - Example: `POST /api/tasks` with task description

2. **VS Code Command Palette Integration**
   - Expose commands via `vscode.commands.registerCommand`
   - Callable via external scripts

3. **Dedicated CLI Tool**
   - Separate binary that communicates with extension
   - Similar to `claude` CLI but for Cline

### Current Community Projects

- **Agent Maestro**: Extension with partial remote control
- **Standalone versions**: Community forks with headless capabilities
- **WebSocket experiments**: Working prototypes

### Proposed API (from GitHub issue #4734)

```typescript
// Proposed methods
controller.addPromptToChat(prompt: string)
controller.addFileMentionToChat(filePath: string)

// Registered commands
cline.api.addPrompt
cline.api.addFile
```

## Integration Strategies for Agent Harbor

### Strategy 1: VS Code CLI Wrapper (Simplest)

**Pros:**

- No extension modifications needed
- Works with installed extensions
- Standard VS Code automation

**Cons:**

- Requires VS Code GUI to be running
- Limited control over extension state
- Depends on extension exposing commands

**Implementation:**

```bash
#!/bin/bash
# Launch VS Code with project
code --wait --goto file.ts:10:5 /path/to/project

# If extension registers commands, execute them
code --command cline.startTask "Implement feature X"
```

### Strategy 2: Extension Host Debugging (Advanced)

**Pros:**

- Full programmatic control
- Can inspect extension state
- Works with any extension

**Cons:**

- Complex CDP protocol
- Requires debugging enabled
- May be fragile across versions

**Implementation:**

```javascript
// Use Puppeteer/Playwright to connect to extension host
const puppeteer = require('puppeteer');

async function automateVSCodeExtension() {
  const browser = await puppeteer.connect({
    browserURL: 'http://localhost:9229',
  });

  // Execute extension commands via CDP
  // Note: Requires deep understanding of VS Code internals
}
```

### Strategy 3: Custom Automation Extension (Most Robust)

**Pros:**

- Full control over automation
- Clean API for external scripts
- Can wrap any extension

**Cons:**

- Requires custom extension development
- Maintenance overhead
- Extension marketplace approval (if public)

**Implementation:**

1. Create "Agent Harbor VS Code Bridge" extension
2. Expose HTTP/WebSocket API
3. Forward commands to target extensions (Cline, Kilo, Roo)
4. Return results to Agent Harbor

### Strategy 4: File Watcher Bridge (No Extension Mod)

**Pros:**

- No extension modifications
- Simple protocol (files)
- Works across platforms

**Cons:**

- Polling overhead
- Race conditions possible
- Requires extension file watcher support

**Implementation:**

```bash
# Agent Harbor writes command
echo '{"command": "cline.startTask", "args": ["..."]}' > .vscode/ah-commands/cmd-$$.json

# Extension watches .vscode/ah-commands/ and executes
# Extension writes results to .vscode/ah-results/
```

## Recommended Approach for Agent Harbor

### Phase 1: Command Palette Automation (MVP)

Use VS Code's existing command infrastructure:

```bash
# 1. Launch VS Code with project
code /path/to/project

# 2. Execute extension commands programmatically
# (Requires extension to expose commands)
code --command extensionId.commandName "arguments"

# 3. Monitor output/results
# (Via filesystem, logs, or extension-specific mechanisms)
```

### Phase 2: Custom Bridge Extension

Develop "Agent Harbor VS Code Bridge":

```typescript
// Bridge extension architecture
export function activate(context: vscode.ExtensionContext) {
  // Start local API server
  const server = startAPIServer(3000);

  // Register command handlers
  server.post('/execute', async (req, res) => {
    const { extension, command, args } = req.body;
    const fullCommand = `${extension}.${command}`;
    const result = await vscode.commands.executeCommand(fullCommand, ...args);
    res.json({ success: true, result });
  });

  // Health check endpoint
  server.get('/health', (req, res) => {
    res.json({ status: 'ok', extensions: getInstalledExtensions() });
  });
}
```

### Phase 3: Extension-Specific Integrations

Work with extension maintainers to add official APIs:

- **Cline**: Contribute to GitHub discussion #2622
- **Kilo Code**: Request CLI/API features
- **Roo Code**: Fork and add automation layer

## Testing with Mock LLM API Server

VS Code extensions can be tested with mock servers:

```bash
# 1. Start mock LLM API server
python tests/tools/mock-agent/start_test_server.py --port 18080

# 2. Configure extension to use mock server
# (via extension settings or environment variables)
export ANTHROPIC_BASE_URL=http://localhost:18080
export OPENAI_API_BASE=http://localhost:18080

# 3. Launch VS Code with extension
code --inspect-extensions=9229 /path/to/test-project

# 4. Execute commands via automation
curl -X POST http://localhost:3000/execute \
  -d '{"command": "cline.startTask", "args": ["Create hello.py"]}'
```

## Known Limitations

1. **GUI Dependency**: Most extensions require VS Code GUI running
2. **Command Discovery**: Not all extensions expose programmable commands
3. **State Management**: Extension state may not be fully accessible externally
4. **Version Compatibility**: Automation may break across VS Code versions
5. **Extension Activation**: Extensions may need manual activation before automation

## Community Projects & Resources

- **Cline Remote Control Discussion**: <https://github.com/cline/cline/discussions/2622>
- **Cline API Feature Request**: <https://github.com/cline/cline/issues/4734>
- **Agent Maestro**: Extension with partial automation
- **VS Code Extension Testing**: <https://code.visualstudio.com/api/working-with-extensions/testing-extension>
- **VS Code Commands API**: <https://code.visualstudio.com/api/references/commands>

## Next Steps for Agent Harbor

1. **Research Current Commands**: Document which commands Cline, Kilo, Roo expose
2. **Prototype CLI Wrapper**: Test basic automation with `code --command`
3. **Evaluate CDP Approach**: Experiment with `--inspect-extensions` and Puppeteer
4. **Develop Bridge Extension**: Create Agent Harbor VS Code Bridge if needed
5. **Contribute Upstream**: Work with extension maintainers to add official APIs
6. **Document Integration**: Create guides for each supported VS Code extension

## Conclusion

**YES, VS Code extensions ARE suitable for CLI integration** through multiple approaches:

- ✅ **Command Palette Automation** (simplest, limited)
- ✅ **Extension Debugging Protocol** (powerful, complex)
- ✅ **Custom Bridge Extension** (robust, requires development)
- ✅ **File Watcher Bridge** (simple, polling-based)

The best approach depends on:

- Required control level
- Maintenance resources
- Extension cooperation
- CI/CD integration needs

Recommended starting point: **Command Palette Automation** + **Custom Bridge Extension** for robust long-term solution.
