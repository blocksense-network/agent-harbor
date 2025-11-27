# Agent Harbor

[![CI Status](https://img.shields.io/github/actions/workflow/status/blocksense-network/agent-harbor/ci-badge.yml?branch=main&style=for-the-badge)](https://github.com/blocksense-network/agent-harbor/actions/workflows/ci.yml?query=event%3Amerge_group)
[![License](https://img.shields.io/badge/License-AGPL_v3-blue.svg?style=for-the-badge)](https://opensource.org/license/agpl-v3)
[![Docs](https://img.shields.io/badge/Docs-Nextra-blue?style=for-the-badge&logo=nextra)](https://docs.agent-harbor.dev)

**Instantly spawn dozens of YOLO agents in the most advanced local sandbox for macOS and Linux. Steer them with precision with agent session forking.**

## What is Agent Harbor?

Agent Harbor is a **sandboxed execution environment and orchestration layer for AI coding agents**.

It lets you:

### Launch many agents in parallel

Each in its own isolated workspace — so they can autonomously refactor, test, and ship changes while you stay focused on higher-level work. Agent Harbor can drive **Claude Code**, **Codex** and most other agent types and acts as a bridge that makes them **accessible remotely** from mobile and Web UIs.

### Start tasks instantly

Tasks start in milliseconds by using **copy-on-write filesystem** snapshots (ZFS, Btrfs or own cross-platform **AgentFS**). Agents get fast **incremental builds** with much **lower disk usage** and **zero configuration hassles** caused by path differences in git worktrees.

### Enjoy secure YOLO mode that can keep going for hours

We asked ourselves the question "What would be the properties of the ideal sandbox for parallel agents?". Then we set out to build it across all major operating systems in an uncompromising way using advanced system programming techniques and modern kernel APIs.

The Agent Harbor sandbox ensures that the **agents cannot harm your computer or access your sensitive files**, while making sure that all common developer activities are still possible in the worldview of the agent - installing dependencies, launching containers and VMs, working with low-level diagnostic tools and debuggers.

Our sandbox is able to emulate the **strong isolation** of Linux namespaces on all major operating systems, ensuring that concurrent agents don't run into issues like **port conflicts** or **cross-session process killing**.

We detect common agent halting patterns, such as launching processes that never return or wait for user input. We mitigate them by delivering precise feedback to the agent that avoids repeating the same mistake. This extends to **automatic analysis and diagnosis of stuck processes** that help the agent overcome conditions like deadlocks, infinite loops and connection failures in test suites.

Given the right development plan, our **supervisor agent** can drive the completion of multiple milestones by **taking the place of the developer** in demanding quality, helping the agent overcome difficulties through **targeted web research** and providing simple "Please continue" prompts when they are necessary.

### Time-travel and fork sessions

Rewind any agent’s timeline, inspect the exact filesystem state, and branch off with new instructions when it goes off-course. These actions are non-destructive — a supervisor agent can examine all created alternatives to arrive at the best possible solution.

### Test cross-platform code effectively

Let agents run the project test suite on all targeted operating systems in parallel through our advanced leader/follower orchestration that redefines the role of the CI in the agentic world.

### Keep everything auditable

Every task, command, and transcript can be recorded for later review, debugging, and compliance. This enables everyone in your organization to learn from the tricks and practices of your best engineers.

---

You can run Agent Harbor fully locally or connect it to an **on-prem execution cluster**. You can also use our unified UI to launch tasks with your favorite cloud agents (Codex, Claude, Cursor, etc.) via browser and API automations.

## Getting Started (Quick Start)

Here is the fastest way to get an agent working on your project.

### 1\. Install Agent Harbor

> [!IMPORTANT]
> More (classic `Linux` / `MacOS` / `Windows`) packages are to come soon :tm:

<!-- TODO: bring back when we have stable packages -->
<!--
#### macOS

```bash
# Via Homebrew
brew install blocksense/tap/agent-harbor
```

#### Linux

```bash
# Via cURL/bash (installs to /usr/local/bin)
curl -sL https://install.agent-harbor.com | bash
```

#### Windows

Coming Soon
-->

#### Nix-enabled OS (`MacOS`, `Linux`)

Install [Nix](https://nixos.org/download) on your system and install the application

```bash
nix profile install github:blocksense-network/agent-harbor
```

> [!NOTE]
> Make sure you've enabled [flake support](https://wiki.nixos.org/wiki/Flakes#Setup)

### 2\. Launch the TUI Dashboard

The main entry point for Agent Harbor is the Terminal User Interface (TUI). Just run `ah` in your repo:

```bash
ah
```

This opens a dashboard where you can write a new task prompt, select your agent, and launch it.

### 3\. Run a Task (CLI)

You can also launch tasks directly from the command line. Agent Harbor will automatically create a new, isolated branch for the task.

```bash
# Example: Have Claude create a new file
ah task --agent claude --prompt "Create a new file 'hello.py' that prints 'Hello, Agent Harbor!'"
```

### 3\. Monitor and Intervene

If an agent makes a mistake, don't restart. **Intervene.**

1. Open the TUI: `ah tui`
2. Navigate the timeline to the moment before the error.
3. Provide a correcting instruction (e.g., _"Don't use that deprecated API, use X instead"_).
4. A new parallel timeline is created, and the agent resumes from that exact filesystem state.

---

## Documentation

- **[User Documentation](https://blocksense.network/agent-harbor)**: Comprehensive guides on every feature of Agent Harbor.
- **[Project Specifications](./specs)**: Agent Harbor follows a rigorous **spec-driven development** process. You can read the engineering specifications for every component (AgentFS, Protocol designs, etc.) in the `specs/` directory.

## Supported Agents and TUI environments

Agent Harbor unifies the interface for the most popular coding agents. You can swap agents without changing your workflow. We use the automation protocols of many terminal emulators and multiplexers to provide variety of options for arranging the agents and supporting tools in the best possible way, depending on your screen real estate.

<table>
<tr><th> Supported Agents </th><th> Supported Terminal Environments </th></tr>
<tr><td>

- **Claude Code**: CLI
- **OpenAI Codex**: CLI / Cloud
- **GitHub Copilot**: CLI
- **Google Gemini**: CLI
- **Cursor** : CLI / IDE

</td><td>

- **Tmux**
- **Zellij**
- **iTerm2**
- **Kitty**
- **WezTerm**
- **Tilix**

</td></tr></table>

## 🤝 Contributing & License

We welcome contributions\! Please see our `CONTRIBUTING.md` guide for details on how to submit issues, features, and pull requests.

This project is licensed under the **AGPL-3.0-only** license. You can find the full license text in the `LICENSE` file.
