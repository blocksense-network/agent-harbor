# How to develop code with AI agents?

Developing with AI agents is similar to being a team lead or senior developer. You are responsible for researching the problem and exploring potential solutions, as well as ensuring that the AI agents deliver exceptional quality by guiding them through code reviews and designing appropriate safeguards.

## What we aim to get out of the AI agents

- **Development Velocity**: Developing with AI agents is significantly faster. We want to be more ambitious regarding what we can ship in a given time frame.

- **Higher Codebase Quality**: With the agents being so fast at adding/refactoring code and writing documentation, all software projects can now attain a significantly higher codebase quality at all times. There are no excuses for not having:
  - Extremely well-factored code.
  - Comprehensive READMEs.
  - Fully-specified development environment (with support for GitHub codespaces/devcontainers).
  - Great CI setup for tracking both correctness and performance over time.
  - Fully automated processes for shipping software with great release notes.

  Your primary goal as a software engineer in this new world is to develop your understanding of all aspects of software quality and to act as an uncompromising supervisor for the AI agents. Please read the following as a start:

  [Guide on Software Quality from OpenAI DeepResearch](https://chatgpt.com/share/68332152-0750-8009-846d-1e1dd017fac3#:~:text=Report%20content)

  All tech debt should be addressed immediately upon being recognized. You may argue that our human-centric understanding of quality is no longer relevant when AI agents write the software, but we are taking a safer bet. We want to keep the code understandable and easy-to-debug by ourselves when things go wrong.

- **Extremely Well-Tested Software**: Having safety nets is crucial for making the agents effective. They work with limited context and they have to build their understanding of the code from scratch for every task.

  The agents will routinely produce incorrect code. The best way to get productivity out of them is to let them run through the compile-test-fix cycle in a sandbox environment, without interruption, all the way until they get working code ready for review.

  The throughput of the agent and the probability of producing correct code then largely depend on two factors:
  - **Modularity of the software and its test suite**: The agents will work faster when they have clear instructions how to test smaller components that fit in their context.
  - **Extent of test coverage**: We want only correct code to pass the tests. Thus, the test suite should have high [Mutation Coverage](https://en.wikipedia.org/wiki/Mutation_testing).

  You have to find creative ways to design tests that cover the entirety of the software. With the speed of coding agents, certain tests that used to be too laborious to create are now practical. By relying on mocks, synthesized data, vision-enabled MCP servers and other emerging techniques, we should be able to overcome this challenge.

  Besides correctness, test suites should also detect regressions in dimensions such as performance, security, accessibility and others.

  To succeed, you must familiarize yourself with a wide range of techniques for detecting defects and other software quality issues. Exploring the following resources will help identify gaps in your knowledge:

  <https://en.wikipedia.org/wiki/Static_program_analysis>
  <https://en.wikipedia.org/wiki/Software_testing>

## How does it work in practice right now?

Coding agents work best when you ask them to make small incremental changes to your codebase. Please read the following guide to get the big picture:

<https://harper.blog/2025/02/16/my-llm-codegen-workflow-atm/>

We use our own workflow that records most of the interactions with the agents in the git history of the project, so you can learn from the prompting strategies of your team mates. See the README below for more details:

<https://github.com/blocksense-network/agent-harbor>

As an example, you can see the prompts that were used to develop `agent-harbor` itself in the project’s task history: [agent-harbor/.agents/tasks/2025/05](https://github.com/blocksense-network/agent-harbor/tree/main/.agents/tasks/2025/05).

Your project should have a defined workspace in Codex and a devcontainer setup that will be used when invoking agents locally (e.g. Codex CLI, GitHub Copilot, Goose, OpenHands, etc).

### Planning phase

1. Research

For quicker/simpler questions, have a conversation with a thinking model with access to web search (e.g. Gemini 2.5 Pro, OpenAI gpt-5-thinking, Grok 4, etc) to ask questions related to the problem that you are solving.

For deeper problems where a lot of details have to be considered, use Gemini Deep Research (available when you log in with your company email) or ChatGPT DeepResearch.

2. Create a Development Plan

TBD

### Implementation phase

1. Ask a thinking model to generate prompts according to the plan or start writing them yourself.

2. Submit tasks with `ah task`.
