## Overview

This repository provides a highly-opinionated workflow for working
with cloud and local coding agents, such as Claude Code, Codex, GitHub
Copilot, Jules, Gemini, Goose, OpenHands and others.

## Goals

The workflow adheres to the following principles, which are implemented
both when working with local agents and when working with remote agents:

1. **The developer provides a coding task through a convenient command-line interface**

2. **The agent works in a secure sandbox environment, without asking for confirmation when using tools**

3. **The agent presents a complete patch/PR once it reaches a stage where all tests and linters are green**

4. **It's easy to start multiple tasks in parallel from the current state of your working tree**

5. **All tasks are recorded as commits/files in the history of the project**

Pushing to git becomes the primary interface for starting cloud agents.
All other interactions with the web UIs of the agents are automated.

Committing all task descriptions in git creates an auditable trail and
a knowledge base demonstrating how tasks are approached and solved.

This allows team members to learn from each other's practices and makes
`git blame` an effective tool for understanding the intention behind all
code. The workflow injects instructions that teach the agents how to
leverage this.

Local agents are started is devcontainers with rich support for different
interaction patterns:

- Start one Editor/IDE instance per task to observe the work of the agent
  and review the final

- Push to a designated branch automatically or create a PR.

## Other Practical Benefits

- Local agents can leverage ZFS and Btrfs snapshots to provide the best
  possible agent-start up time. The agent takes advantage of incremental
  compilation when building the project and its test suite.

- The same start-up time and incremental compilations are possible when
  you dispatch the coding tasks to a cluster of self-managed machines in
  an office environment or a private cloud.

- The workflow smooths out the differences between different agent tools
  and cloud environments. Everything can be handled through shared config
  and user interfaces.

  The behavior of the cloud agents is modified through prompt engineering
  and automation to implement new workflows such as automatically creating
  PRs, automatically pushing to specific branches, etc.

- The workflow provides a helpful framework for automatically downloading
  relevant internet resources before coding tasks start for agents that
  need to operate offline.

- The workflow provides a framework for working in big monorepos that speeds
  up agent start-up times (both locally and it the cloud) and helps with
  managing the context of the agent in such repositories.

## Using the Workflow

1. **Starting a Task (Developer):**

   When a developer needs to assign a task to the agent, they run
   the `agent-task` command. If a branch name is provided it starts a
   new branch, otherwise it appends a follow-up task on the current
   branch.

   ```bash
   agent-task [branch-name]
   ```

   This script will:
   - When a branch name is supplied, create the branch first and abort
     early with the VCS error message if the name is invalid.
   - Prompt the developer to enter the task description in an editor.
   - Commit the task description to a file within a `.agents/tasks/`
     directory on the new branch or append it as a follow-up task if
     no branch was given.
   - Push the branch to the default remote.

   The command accepts a few options for non-interactive use:
   - `--push-to-remote=BOOL` – automatically push to the default remote without prompting.
   - `--prompt=STRING` – use `STRING` as the task description instead of launching an editor.
   - `--prompt-file=FILE` – read the task description from `FILE`.
   - `--devshell=NAME` (`-s`) – record the given Nix dev shell in the initial commit message.

   The command also provides a `setup` subcommand that prints the versions of `codex` and `goose` available in the current `PATH`.

2. **Retrieving a Task (Coding Agent):**

   Once the developer has set up the task, they instruct the agent to
   switch to the right branch and retrieve its instructions.
   A typical prompt for an agent would be:

   ```
   Run the `get-task` command and follow the provided instructions.
   ```

   The `get-task` script will print the task description for the agent,
   along with instructions for accessing the downloaded internet resources
   and working with the git history.
   It also supports a `--get-setup-env` option which prints only the
   environment variable assignments gathered from `@agents-setup` lines.

### Workflow Commands

Task descriptions may include lines beginning with `/` (e.g. `/front-end-task`).

When `get-task` is executed, these lines are replaced with the output of a
matching programs or text files in the `.agents/workflows` folder of your
repository.

In other words, in the example above, `get-task` will look either for an
executable stored in `.agents/workflows/front-end-task` or for a text file
located at `.agents/workflows/front-end-task.txt` (the contents of this file
will take the place of the workflow command in the task description, like a
macro in a programming language).

Executables are typically used when the workflow command has parameters.

Lines starting with `@agents-setup` in either the task file or the workflow
output are stripped from the final message and interpreted as environment
variable assignments for the `*-setup` scripts described below.

```shell
@agent-setup DEV_SHELL=csharp TESTED_COMPONENTS+=backend,db
```

A directive may either assign a value (`VAR=value`) or append entries to a
comma‑separated set using the `VAR+=val1,val2` syntax. When multiple directives
affect the same variable, the following rules apply:

1. Conflicting direct assignments (different values for the same variable)
   result in an error.
2. A direct assignment can be combined with one or more appends. The final value
   contains the assigned value plus all appended entries, regardless of their
   order.
3. One or more append operations without a direct assignment simply combine
   their entries.

Duplicate directives or values are ignored.

## Supported Agent Systems

This workflow supports setup for multiple AI coding agent systems:

- **[Codex](https://openai.com/codex)** - OpenAI's code generation model
- **[Jules](https://jules.google.com/)** - AI pair programming assistant
- **[Goose](https://github.com/square/goose)** - AI-powered development tool
- **[Open Hands](https://github.com/All-Hands-AI/OpenHands)** - Open-source AI coding assistant
- **[GitHub Copilot](https://github.com/features/copilot)** - GitHub's AI pair programmer

## Setup Script Architecture

Each agent system has a dedicated setup script (e.g., `codex-setup`, `jules-setup`) that follows a three-phase setup process:

1. `.agents/common-pre-setup` runs first (if it exists in your project)
2. `.agents/{agent}-setup` runs for agent-specific configuration (if it exists)
3. `.agents/common-post-setup` runs last for finalization tasks (if it exists)

This architecture allows you to share common setup logic across all agent systems while customizing setup for specific agents when needed.

## Usage by Agent System

### Codex

In your Codex environment's Advanced settings, enter the following setup script:

```
git clone https://github.com/blocksense-network/agent-harbor
agent-harbor/codex-setup
```

### Jules

In the Jules web-interface, select a codebase in the left-hand-side panel, click
"Configuration" and enter the following Initial Script:

```
git clone https://github.com/blocksense-network/agent-harbor
agent-harbor/codex-setup
```

### Goose

**TBD** - Usage instructions for Goose will be added once integration is tested.

### Open Hands

**TBD** - Usage instructions for Open Hands will be added once integration is tested.

### GitHub Copilot

**TBD** - Usage instructions for GitHub Copilot will be added once integration is tested.

## Environment Variables

- `NIX=1` - Set this to enable Nix installation during common-pre-setup

### Installing as a Ruby gem

The scripts can be installed as a gem for easier reuse:

```bash
gem install --local agent-harbor.gem
```

This will provide the `agent-task`, `get-task`, and `download-internet-resources` executables in your `PATH`.

To enable bash completion for `agent-task`, source the script `scripts/agent-task-completion.bash` in your shell profile.

### Installing with Nix

This repository also provides a Nix flake. The default package installs the `agent-task` binary with `codex` and `goose` available in its `PATH`. An additional `agent-utils` package bundles the `get-task` and `start-work` binaries.

```bash
nix run github:blocksense-network/agent-harbor
```

Or install the utilities package:

```bash
nix profile install github:blocksense-network/agent-harbor#agent-utils
```

### What's included?

The core components include:

- `codex-setup`: A script to initialize the workspace, download necessary internet resources, and run project-specific setup.
- `agent-task`: A script for developers to begin a new task, automatically creating a dedicated branch and storing the task description.
- `agent-task setup`: Prints the versions of `codex` and `goose` available in `PATH`.
- `get-task`: A script for the coding agent to retrieve its current task instructions.
- `start-work`: A helper that configures a freshly checked-out repository for development.
- `download-internet-resources`: A helper script that scans task descriptions for URLs and downloads them (or clones Git repositories) for offline access.

## Future Direction

We envision that the manual step of prompting the agent to run `get-task` could be automated in the future through:

- An API integration with Codex.
- (interim) A browser extension that drives the Codex WebUI.
