This template defines the information we collect for each agentic coding tool supported by Agents Workflow. Usually this can be obtained from `--help` screens and the tool's documentation.

We'll aim to collect the following information for all agent types supported by Agent Harbor:

* Codex CLI
* Claude Code
* Cursor CLI
* Gemeni CLI
* Cursor VS Code Extensin
* GitHub Copilot CLI
* GitHub Copilot VS Code Extension
* OpenCode
* Windsurf
* Cline CLI
* Cline VS Code Extension
* Crush
* Goose
* Kilo Code
* Roo Code
* Qwen Code
* OpenHands
* SourceGraph Cody

# <Agent Tool> — Integration Notes

## Overview

- Website:
- Documentation:
- Source/GitHub:
- Version tested:
- Short description:

### Task start-up commands

How do we start or resume the agent with a specific task prompt? Include canonical invocations for:

- Direct prompt on the CLI
- Are there options that control whether interactive or non-interactive mode will be launched?
- Are there options controlling whether human-readable or machine-readable output will be produced (e.g. json)?
- Are the options related to resuming previous sessions?
- Are there options specifying which model will be used?

Provide 1–3 concrete examples for this tool using code blocks.

### Support for custom hooks

Does the agent support custom hooks or commands to be executed during its work (e.g., before/after each file modification or tool use)? Detail how this is configured.

Please note that by custom hooks, we are not referring just to MCP tools, but specifically to the ability to configure certain commands to be executed after every agent step, so we can implement our [Agent Time Travel feature](../Agent-Time-Travel.md).

### How to skip the initial onboarding screens on first launch of the agent?

Detail the specific configuration, environment variables, or file setup needed to bypass initial setup screens, license agreements, or authentication prompts that would interfere with automated testing and integration.

### Checkpointing (point-in-time restore of chat + filesystem)

We are specifically interested in an official checkpoint feature that can restore both the chat state and the file system state to a specific moment in time. Please answer precisely:

- Does the tool have official checkpointing? If yes, how is it enabled (flags/commands/config)?
- Scope: Does checkpointing cover chat, filesystem, or both? What is the granularity (per step/message/edit)?
- Restore semantics: How do we restore from a particular checkpoint ID or moment? Is filesystem state guaranteed to be restored?
- Operational notes: Performance, stability, limits, and compatibility.

### Session continuation (conversation resume)

If the tool supports resuming a conversation/session, describe the behavior clearly. This is different from checkpointing:

- How to resume the latest/specific session
- What is persisted (conversation only vs any filesystem context)
- Limitations and differences vs checkpointing

### Where are chat sessions stored?

If the agent supports resuming chat sessions, where are their files stored? Cover all supported operating systems. If unknown from help screens, research likely locations and provide links.

### What is the format of the persistent chat sessions?

Provide an example snippet. Would it be easy to trim an existing session to a certain point in time (e.g., a specific agent thought or tool use)?

### Reverse‑engineering policy for session formats

When the on‑disk session format is undocumented or incomplete, perform careful reverse‑engineering:

- Record a very short session (benign actions like reading this repo’s docs) to generate minimal transcript/state files.
- Inspect created paths and filenames; note placeholders like `<project-id>` and how they are derived (e.g., by project root hashing or internal IDs).
- Open files to identify serialization format (JSON/JSONL/YAML/etc). Capture a minimal example in this document.
- Attempt a surgical trim to an earlier step; relaunch the tool to validate behavior. Keep backups and record tool version.
- Maintain per‑version notes in this repository, as formats may change across releases. Prefer stable, additive edits that the tool tolerates.

### How to run the agent with a set of MCP servers?

Provide the exact command-line options, configuration files, or environment variables needed to configure MCP servers for the agent, including examples of stdio-based and URL-based MCP server connections. Note project‑scoped vs global config.

### Credentials

Where are the agent login credentials stored? What are the precise paths of its settings and credentials files? If the help screens don't provide this information, use web search to find a definitive answer and provide links to the discovered resources.

### Known issues and quirks

Platform limitations, rate limits, stability notes, experimental features, and any other gotchas relevant to AH integration.
