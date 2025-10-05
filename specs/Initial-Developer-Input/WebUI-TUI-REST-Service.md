I'd like to write few more specs in the docs folder. Before we start please read the existing docs.

1. It will be possible to launch agent-harbor as a REST service. The service will be able to spawn tasks on demand and list currently active coding sessions (as explained in [fs-snapshots-overview](../Public/FS-Snapshots/FS-Snapshots-Overview.md)). We need a document describing the purpose of this service and its specification. It's intended for usage by companies that set up on-premise or private cloud clusters for running coding tasks or for developers who prefer a WebUI experience.

2. There will be a WebUI front-end that will consume the REST service. It needs something like PRD document and detailed spec of its UI.

3. There will be a similar TUI, providing a dashboard of currently running tasks and allowing the launching of new tasks. It needs a PRD document and detailed spec of its UI. The TUI will be built around tmux, zellij or screen (all of them will be supported). The default view is a dashboard of the currently running tasks, featuring an easy way to start a new task. When a new task is launched, a new tmux windows is created split in two - on the right side the user sees the AI agent working. On the left side, a terminal is launched within the newly created per-task FS mount. The user can also decide to launch a text editor like vim, emacs or another instead of a terminal (since these editors have built-in terminal).

4. The user will also be able to spawn the new task inside Visual Studio Code, Cursor or Windsurf (using their built-in agents).

5. The user will be able to spawn a terminal-based agent, such as Claude Code next to a GUI editor launched in the task-specific FS mount.

Usually the agents work in devcontainers, but it's possible to launch the agent locally with a more minimal sandbox or no sandbox at all (this is intended for environments that are already well-sandboxed, such as VMs)

The WebUI will feature a list of repositories on the left. Each repo will feature a plus button that is used for the creation of a new task. In the central feed, the user sees the list of submitted tasks according to their chnological order. Each task card has sufficient status indicators and potentially a single line that live-updates with the last action of the agent. Clicking on a task adds another pane on the right with a live log of the task or a final report including the created diff. The feed of tasks and the list of projects can be collapsed to the left to take less space.

When the new task button is pressed, this creates a new task card at the top of the task list. The card has a vertically resizable input box for the task description.

Below it, there is a combo-box for selecting a branch, which comes preconfigured to the default branch for task creating in the repo (typically the main branch). You can also specify the coding agent and the number of concurrent instances (available for some of the agents). There is a right aligned button start. Before pressing start, any edits are auto saved in a draft mode. You can have multiple tasks as drafts. There is a button for deleting a draft.
