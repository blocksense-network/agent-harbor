# Agent Workflow Commands

This specification describes the workflow commands feature implemented in the agent-harbor project. This feature allows task descriptions to include dynamic content and environment variable setup through special directives.

## Overview

The workflow commands feature enables two types of directives in task descriptions:

1. **Workflow Commands** - Lines starting with `/` that execute scripts or include text files
2. **Environment Setup Directives** - Lines starting with `@agents-setup` that configure environment variables

## Architecture

### Current Implementation (Ruby)

The workflow commands are implemented in the Ruby codebase with the following key components:

- **`process_workflows(text)`** in `lib/agent_tasks.rb`: Main processing function that handles workflow commands and environment directives
- **`handle_workflow_line(line, env_vars, diagnostics, output_lines)`**: Processes individual lines for environment directives
- **`execute_script(script_path, args)`**: Cross-platform script execution with Windows support
- **`agent_prompt_with_env`**: Returns both processed task text and environment variables
- **Platform helpers** in `lib/platform_helpers.rb`: Cross-platform detection utilities

### Future Implementation (Rust CLI)

Under the new Rust CLI architecture, workflow commands will be handled by:

- **`ah agent get-task`**: Replaces the current `get-task` command, processes workflow commands and environment directives
- **`ah agent get-setup-env`**: Dedicated command for extracting environment variables from workflow directives
- **`ah agent start-work`**: Creates tasks with workflow processing support

## Workflow Commands

### Syntax (Workflow Commands)

Task descriptions may include lines beginning with `/` (e.g. `/front-end-task`). When `get-task` is executed, these lines are replaced with the output of matching programs or text files in the `.agents/workflows` folder.

### Resolution Rules

For a workflow command `/command`:

1. **Repo Directory Scripts (with precedence)**: If no whitelisted executable is found in PATH, check for scripts in `.agents/workflows/` directory (when it exists). Scripts take precedence over text files.
2. **Text File Fallback**: If no executable script is found in `.agents/workflows/`, look for a text file at `.agents/workflows/command.txt` and include its contents.
3. **Configuration-based Whitelist**: Check if `command` is listed in the `extra_workflow_executables` configuration variable. If not listed, report an error.
4. **PATH Verification**: If listed in the whitelist, verify that the executable exists in the system `PATH` and is executable. If not found in PATH, report an error.
5. **Error Handling**: If no script/text file exists, the command is not whitelisted, or it's not found in PATH, report an error in diagnostics.

### Script Execution

#### Unix Systems

- Scripts are executed directly using the system's shell
- File permissions must allow execution (attempts `chmod +x` automatically if not executable)
- Uses `Open3.capture3(script_path, *args)` for execution

#### Windows Systems

The implementation includes comprehensive Windows support:

**File Extension Detection:**

- `.bat`, `.cmd`: Executed with `cmd /c script_path args...`
- `.rb`: Executed with `ruby script_path args...`
- `.py`: Executed with `python script_path args...`
- `.js`: Executed with `node script_path args...`
- `.ps1`: Executed with `powershell -ExecutionPolicy Bypass -File script_path args...`

**Extensionless/Shell Scripts:**

1. Check if bash is available (`bash --version` test)
2. If bash available: `bash script_path args...`
3. If not available but file exists: Parse shebang line
4. Extract interpreter from shebang patterns:
   - `#!/usr/bin/env ruby` → `ruby`
   - `#!/bin/bash` → `bash` (if available) or `sh`
   - `#!/bin/sh` → `bash` (if available) or `sh`
   - Custom interpreters supported

**Error Cases:**

- Script not found or empty: Returns error status
- No interpreter available: Returns error with diagnostic message

### Arguments

Workflow commands can accept arguments:

```
/echo-args foo "bar baz"
/echo-args qux quux
```

- Arguments are parsed using `Shellwords.split()` (Ruby) / shell-like parsing (Rust)
- Quoted arguments preserve spaces and special characters
- Arguments are passed as separate parameters to the script

## Environment Setup Directives

### Syntax

Lines starting with `@agents-setup` in either task files or workflow outputs are interpreted as environment variable assignments:

```
@agents-setup DEV_SHELL=csharp TESTED_COMPONENTS+=backend,db
```

### Assignment Rules

**Direct Assignment (`VAR=value`):**

- Sets the variable to the specified value
- Multiple assignments to the same variable are checked for conflicts
- Conflicting values result in diagnostic error: `"Conflicting assignment for VAR"`

**Append Assignment (`VAR+=value`):**

- Adds values to a comma-separated set
- Multiple appends accumulate values: `VAR+=a,b VAR+=c` → `VAR=a,b,c`
- Can be combined with direct assignments

**Multiple Operations Processing:**

- Direct assignment + appends: Final value contains assigned value plus all appended entries
- Multiple appends: Values are accumulated and deduplicated
- Order of operations doesn't affect final result
- Values are deduplicated automatically

### Environment Variable Merging

When processing multiple tasks or workflow outputs:

```ruby
env.merge!(vars) do |_k, old_v, new_v|
  (old_v.split(',') + new_v.split(',')).uniq.join(',')
end
```

- Variables from multiple sources are merged
- Comma-separated values are concatenated and deduplicated
- Later sources can override earlier ones

## Integration with Setup Scripts

### Current Implementation Details (Ruby)

All setup scripts include this pattern:

```bash
SETUP_ENV="$("$AH_DIR/bin/get-task" --get-setup-env 2>/dev/null)"
if [ -n "$SETUP_ENV" ]; then
  while IFS= read -r line; do
    export "$line"
  done <<< "$SETUP_ENV"
fi
```

### Future Implementation Details (Rust CLI)

The Rust CLI will provide:

```bash
# Get environment variables
SETUP_ENV="$(ah agent get-setup-env 2>/dev/null)"
if [ -n "$SETUP_ENV" ]; then
  while IFS= read -r line; do
    export "$line"
  done <<< "$SETUP_ENV"
fi
```

## Manual Testing

- You can manually test dynamic instruction and task prompts using `ah agent instructions`. It processes a dynamic instruction file by expanding workflow commands per the rules above and outputs the final prompt text.

### Command Line Interface

**Current (Ruby):**

- `get-task --get-setup-env`: Print environment variables in `KEY=VALUE` format
- `get-task --autopush`: Process workflows with autopush setup

**Future (Rust):**

- `ah agent get-task [--autopush] [--agent TYPE] [--model MODEL]`: Get processed task
- `ah agent get-setup-env`: Extract environment variables only
- `ah agent start-work --task-description DESC [--branch-name NAME]`: Create task with workflow support

## Processing Flow

### Task Processing Sequence

1. **Read Task File**: Load `.agents/tasks/YYYY/MM/DD-HHMM-branch` content
2. **Split Tasks**: Divide into initial task and follow-up tasks (delimiter: `--- FOLLOW UP TASK ---`)
3. **Process Each Task**:
   - Call `process_workflows(task_text)` to handle `/` commands and `@agents-setup` directives
   - Merge environment variables across all tasks
   - Format output with task prefixes for multi-task scenarios
4. **Handle Offline Mode**: Append internet access instructions when offline
5. **Return Results**: Processed text and merged environment variables

### Workflow Command Processing

For each line in task text:

```ruby
if line.start_with?('/')
  tokens = Shellwords.split(line[1..])  # Remove leading /
  cmd = tokens.shift
  script_path = ".agents/workflows/#{cmd}"
  txt_path = ".agents/workflows/#{cmd}.txt"

  if File.exist?(script_path)
    # Execute script with arguments
    stdout, stderr, status = execute_script(script_path, tokens)
    # Process stdout lines for @agents-setup directives
    # Add stderr to diagnostics if status != 0
  elsif File.exist?(txt_path)
    # Read and process text file lines
    File.read(txt_path).each_line do |l|
      handle_workflow_line(l.chomp, env_vars, diagnostics, output_lines)
    end
  else
    diagnostics << "Unknown workflow command '/#{cmd}'"
  end
else
  handle_workflow_line(line, env_vars, diagnostics, output_lines)
end
```

### Environment Directive Processing

```ruby
def handle_workflow_line(line, env_vars, diagnostics, output_lines)
  if line =~ /^@agents-setup\s+(.*)$/
    Shellwords.split(Regexp.last_match(1)).each do |pair|
      op = pair.include?('+=') ? '+=' : '='
      var, val = pair.split(op, 2)
      env_vars[var] ||= { direct: nil, append: [] }
      entry = env_vars[var]

      if op == '='
        if entry[:direct] && entry[:direct] != val
          diagnostics << "Conflicting assignment for #{var}"
        else
          entry[:direct] = val
        end
      else
        entry[:append].concat(val.split(','))
      end
    end
  else
    output_lines << line  # Non-directive lines pass through
  end
end
```

## Examples

### Basic Workflow Usage

```
Implement a new feature for the web application.

/front-end-task
/back-end-task

This involves updating both the UI and API components.
```

Where `.agents/workflows/front-end-task.txt` contains:

```
Focus on the user interface components:
- Update the dashboard layout
- Add new form validation
- Implement responsive design

@agents-setup DEV_SHELL=typescript
```

### Script with Arguments

```
Set up the development environment.

/configure-env production database=mysql
```

Where `.agents/workflows/configure-env` is an executable script that processes the arguments.

### Environment Variable Setup

```
Prepare the build environment.

@agents-setup BUILD_ENV=production
@agents-setup TESTED_COMPONENTS+=frontend,api
@agents-setup DEBUG_LEVEL=verbose
```

## Error Handling and Diagnostics

### Diagnostic Categories

1. **Unknown Commands**: `"Unknown workflow command '/command'"`
2. **Script Failures**: `"$ command args\nstderr_output"`
3. **Permission Issues**: `"Workflow command 'command' not executable"`
4. **Environment Conflicts**: `"Conflicting assignment for VAR"`
5. **Windows Execution**: `"Cannot execute script on Windows without bash: path"`

### Error Propagation

- Script execution failures are captured in diagnostics but don't stop processing
- Environment conflicts are reported but processing continues
- Missing workflow files generate errors but don't abort the entire task
- All diagnostics are collected and can be inspected by calling code

## Cross-Platform Compatibility

### Platform Detection

Uses Ruby's `RbConfig::CONFIG['host_os']` with regex patterns:

```ruby
def windows?
  RbConfig::CONFIG['host_os'] =~ /mswin|mingw|cygwin/
end

def linux?
  RbConfig::CONFIG['host_os'] =~ /linux/
end

def macos?
  RbConfig::CONFIG['host_os'] =~ /darwin|mac os/
end
```

### Windows-Specific Behavior

- **Interpreter Priority**: PowerShell → cmd → bash (if available) → shebang extraction
- **Extension Mapping**: Comprehensive mapping for common script types
- **Bash Fallback**: Graceful degradation when bash is not available
- **Shebang Parsing**: Robust extraction of interpreters from `#!/usr/bin/env` and direct paths

## Testing

The implementation includes comprehensive test coverage in `test/test_workflows.rb`:

- **Basic workflow expansion**: Script execution and text file inclusion
- **Environment variable processing**: Direct assignment, append operations, conflicts
- **Cross-platform execution**: Windows-specific script handling
- **Multi-task scenarios**: Follow-up task processing
- **Error conditions**: Missing commands, script failures, permission issues
- **Integration testing**: End-to-end workflow processing with setup scripts

## Benefits

1. **Modular Task Descriptions**: Break down complex tasks into reusable workflow components
2. **Dynamic Content Generation**: Scripts can generate context-specific task content
3. **Environment Consistency**: Centralized environment variable management across agents
4. **Cross-Platform Compatibility**: Robust script execution on Windows, Linux, and macOS
5. **Error Resilience**: Comprehensive error handling with detailed diagnostics
6. **Extensibility**: Easy to add new workflow commands without modifying core code
